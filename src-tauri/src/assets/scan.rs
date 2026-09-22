use crate::{
    error::{CommandError, CommandResult},
    models::{Asset, ExportResult, GenerationManifest, WorkspaceRigSpec},
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use std::path::{Path, PathBuf};
use tauri::State;

use super::inspect::{content_hash, get_asset, inspect, safe_category, upsert};
use super::pixel_normalize::normalize_sprite_file;

fn image_paths(directory: &Path, output: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            image_paths(&path, output)?;
        } else if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|ext| {
                matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "png" | "jpg" | "jpeg" | "gif" | "webp"
                )
            })
        {
            output.push(path);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn scan_assets(
    workspace_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Asset>> {
    let root = workspace_path(&state, &workspace_id)?;
    let assets_root = root.join("assets");
    std::fs::create_dir_all(&assets_root)?;
    crate::allow_asset_directory(Some(&app), &assets_root, true)?;
    let mut paths = Vec::new();
    image_paths(&assets_root, &mut paths)?;
    let mut assets = Vec::new();
    for path in paths {
        let existing_id = {
            let connection = state
                .db
                .lock()
                .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
            connection
                .query_row(
                    "SELECT id FROM assets WHERE path = ?1",
                    [path.to_string_lossy().as_ref()],
                    |row| row.get(0),
                )
                .optional()?
        };
        if let Ok(asset) = inspect(&workspace_id, &root, &path, existing_id) {
            upsert(&state, &asset, "scanned")?;
            assets.push(asset);
        }
    }
    {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let mut statement =
            connection.prepare("SELECT id, path FROM assets WHERE workspace_id = ?1")?;
        let registered = statement.query_map([&workspace_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for entry in registered.filter_map(Result::ok) {
            if !Path::new(&entry.1).is_file() {
                // Keep the asset row so historical animations and exported sheets retain
                // their frame identity. The live browser is built from `assets` above, so
                // missing files stay hidden while their versions become explicitly unavailable.
                connection.execute(
                    "UPDATE asset_versions SET available = 0, selected = 0 WHERE asset_id = ?1 AND path = ?2",
                    params![entry.0, entry.1],
                )?;
            }
        }
    }
    assets.sort_by(|a, b| a.category.cmp(&b.category).then(a.name.cmp(&b.name)));
    Ok(assets)
}

#[tauri::command]
pub fn export_asset(id: String, state: State<'_, AppState>) -> CommandResult<ExportResult> {
    export_asset_inner(&id, &state)
}

pub(crate) fn export_asset_inner(id: &str, state: &AppState) -> CommandResult<ExportResult> {
    let asset = get_asset(state, id)?;
    let source = PathBuf::from(&asset.path);
    if !source.is_file() {
        return Err(CommandError::new(
            "asset_missing",
            "The asset was moved or deleted outside Sprite Studio",
        ));
    }
    let output_directory = workspace_path(state, &asset.workspace_id)?.join("exports");
    std::fs::create_dir_all(&output_directory)?;
    let slug: String = asset
        .name
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() {
                value.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("png");
    let image_path = output_directory.join(format!("{slug}.{extension}"));
    let metadata_path = output_directory.join(format!("{slug}.json"));
    std::fs::copy(&source, &image_path)?;
    let metadata = serde_json::json!({
        "name": asset.name,
        "image": image_path.file_name().and_then(|value| value.to_str()).unwrap_or("sprite.png"),
        "source": asset.relative_path,
        "category": asset.category,
        "width": asset.width,
        "height": asset.height,
        "frameCount": 1
    });
    std::fs::write(
        &metadata_path,
        serde_json::to_vec_pretty(&metadata)
            .map_err(|error| CommandError::new("serialization_error", error.to_string()))?,
    )?;
    Ok(ExportResult {
        png_path: image_path.to_string_lossy().into_owned(),
        metadata_path: metadata_path.to_string_lossy().into_owned(),
        width: asset.width,
        height: asset.height,
        format: "asset".into(),
    })
}

pub(crate) fn collect_workspace_rig_specs(root: &std::path::Path) -> CommandResult<Vec<WorkspaceRigSpec>> {
    let rigs_dir = root.join(".sprite-studio/rigs");
    if !rigs_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut specs = Vec::new();
    for entry in std::fs::read_dir(&rigs_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map(|value| value.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| path.to_string_lossy().into_owned());
        let updated_at = entry
            .metadata()?
            .modified()
            .map(|value| chrono::DateTime::<Utc>::from(value).to_rfc3339())
            .unwrap_or_else(|_| Utc::now().to_rfc3339());
        let parsed = serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(&path).unwrap_or_default(),
        )
        .ok();
        let name = parsed
            .as_ref()
            .and_then(|value| value.get("name"))
            .and_then(|value| value.as_str())
            .unwrap_or_else(|| path.file_stem().and_then(|value| value.to_str()).unwrap_or("rig"));
        let source = parsed
            .as_ref()
            .and_then(|value| value.get("source"))
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let fps = parsed
            .as_ref()
            .and_then(|value| value.get("fps"))
            .and_then(|value| value.as_f64())
            .unwrap_or(8.0);
        let frame_count = parsed
            .as_ref()
            .and_then(|value| value.get("frames"))
            .and_then(|value| value.as_array())
            .map(|frames| frames.len() as u32)
            .unwrap_or(0);
        specs.push(WorkspaceRigSpec {
            relative_path: relative,
            name: name.to_string(),
            source,
            fps,
            frame_count,
            updated_at,
        });
    }
    specs.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(specs)
}

#[tauri::command]
pub fn list_workspace_rig_specs(
    workspace_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<WorkspaceRigSpec>> {
    let root = workspace_path(&state, &workspace_id)?;
    collect_workspace_rig_specs(&root)
}

#[tauri::command]
pub fn get_generation_manifest(
    workspace_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Option<GenerationManifest>> {
    let root = workspace_path(&state, &workspace_id)?;
    read_generation_manifest(&root)
}

#[tauri::command]
pub fn get_generation_fingerprint(
    workspace_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Option<String>> {
    let root = workspace_path(&state, &workspace_id)?;
    read_generation_manifest(&root)?
        .map(|manifest| generation_fingerprint(&root, &manifest))
        .transpose()
}

pub(super) fn generation_fingerprint(
    root: &Path,
    manifest: &GenerationManifest,
) -> CommandResult<String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(manifest.name.as_bytes());
    hasher.update(manifest.category.as_bytes());
    hasher.update(manifest.fps.to_string().as_bytes());
    for relative in &manifest.files {
        hasher.update(relative.as_bytes());
        hasher.update(content_hash(&root.join(relative))?.as_bytes());
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn png_paths(directory: &Path, output: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            png_paths(&path, output)?;
        } else if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
        {
            output.push(path);
        }
    }
    Ok(())
}

fn newest_png_under(directory: &Path) -> std::io::Result<Option<PathBuf>> {
    if !directory.is_dir() {
        return Ok(None);
    }
    let mut candidates = Vec::new();
    png_paths(directory, &mut candidates)?;
    if candidates.is_empty() {
        return Ok(None);
    }
    candidates.sort_by_key(|path| {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });
    Ok(candidates.last().cloned())
}

fn materialize_recovered_asset(root: &Path, source: &Path) -> CommandResult<(String, String)> {
    let source_relative = match source.strip_prefix(root) {
        Ok(path) => path.to_string_lossy().replace('\\', "/"),
        Err(_) => {
            return Err(CommandError::new(
                "invalid_generation",
                "Asset path escapes workspace",
            ));
        }
    };
    if source_relative.starts_with("assets/") {
        let category = source_relative
            .split('/')
            .nth(1)
            .unwrap_or("characters")
            .to_string();
        return Ok((source_relative, safe_category(&category)?));
    }
    let file_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| CommandError::new("invalid_generation", "Recovered PNG has no file name"))?;
    let category = safe_category("characters")?;
    let destination = root.join("assets").join(&category).join(file_name);
    std::fs::create_dir_all(
        destination
            .parent()
            .ok_or_else(|| CommandError::new("invalid_generation", "Recovered asset has no parent"))?,
    )?;
    std::fs::copy(source, &destination)?;
    let asset_relative = match destination.strip_prefix(root) {
        Ok(path) => path.to_string_lossy().replace('\\', "/"),
        Err(_) => {
            return Err(CommandError::new(
                "invalid_generation",
                "Recovered asset path escapes workspace",
            ));
        }
    };
    Ok((asset_relative, category))
}

pub(crate) fn recover_manifest_from_imagegen_sources(
    root: &Path,
) -> CommandResult<Option<GenerationManifest>> {
    let imagegen_root = root.join(".sprite-studio/imagegen-sources");
    let (source, source_relative) = if let Some(imagegen_png) = newest_png_under(&imagegen_root)
        .map_err(|error| CommandError::new("filesystem_error", error.to_string()))?
    {
        let source_relative = match imagegen_png.strip_prefix(root) {
            Ok(path) => path.to_string_lossy().replace('\\', "/"),
            Err(_) => {
                return Err(CommandError::new(
                    "invalid_generation",
                    "ImageGen source escapes workspace",
                ));
            }
        };
        (imagegen_png, source_relative)
    } else {
        let assets_root = root.join("assets");
        let asset_png = newest_png_under(&assets_root)
            .map_err(|error| CommandError::new("filesystem_error", error.to_string()))?;
        match asset_png {
            Some(path) => {
                let relative = match path.strip_prefix(root) {
                    Ok(value) => value.to_string_lossy().replace('\\', "/"),
                    Err(_) => {
                        return Err(CommandError::new(
                            "invalid_generation",
                            "Asset path escapes workspace",
                        ));
                    }
                };
                (path, relative)
            }
            None => return Ok(None),
        }
    };
    let (asset_relative, category) = materialize_recovered_asset(root, &source)?;
    let name = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("sprite")
        .to_string();
    let manifest = GenerationManifest {
        kind: Some("sprite".into()),
        name,
        category,
        fps: 1.0,
        files: vec![asset_relative],
        generated_at: Utc::now().to_rfc3339(),
        rig: None,
        rig_id: None,
        source: Some(source_relative),
        quality: None,
        direction_family: None,
        facing: None,
        mirrored_from: None,
        anchor_slug: None,
    };
    write_generation_manifest(root, &manifest, None)?;
    Ok(Some(manifest))
}

pub(crate) fn write_generation_manifest(
    root: &Path,
    manifest: &GenerationManifest,
    session: Option<&crate::pipeline::ManifestSessionHook>,
) -> CommandResult<()> {
    let directory = root.join(".sprite-studio");
    std::fs::create_dir_all(&directory)?;
    std::fs::write(
        directory.join("last-generation.json"),
        serde_json::to_vec_pretty(manifest)
            .map_err(|error| CommandError::new("serialization_error", error.to_string()))?,
    )?;
    if let Some(hook) = session {
        crate::pipeline::append_session_from_manifest(root, manifest, hook)?;
    }
    Ok(())
}

pub(crate) fn read_generation_manifest(root: &Path) -> CommandResult<Option<GenerationManifest>> {
    let path = root.join(".sprite-studio/last-generation.json");
    if !path.is_file() {
        return Ok(None);
    }
    let mut manifest: GenerationManifest =
        serde_json::from_str(&std::fs::read_to_string(&path)?)
            .map_err(|error| CommandError::new("invalid_generation", error.to_string()))?;
    if manifest.files.is_empty()
        || !manifest.files.iter().all(|relative| {
            let candidate = root.join(relative);
            candidate.is_file()
                && candidate.starts_with(root.join("assets"))
                && Path::new(relative)
                    .components()
                    .all(|component| !matches!(component, std::path::Component::ParentDir))
        })
    {
        return Err(CommandError::new(
            "invalid_generation",
            "Codex returned an invalid sprite generation manifest",
        ));
    }
    // Provider-authored manifests occasionally contain a rounded or copied
    // timestamp. The file modification time is the reliable local handoff time
    // and prevents a completed generation from being rejected until restart.
    if let Ok(modified) = std::fs::metadata(&path).and_then(|metadata| metadata.modified()) {
        let modified = chrono::DateTime::<Utc>::from(modified);
        let declared = chrono::DateTime::parse_from_rfc3339(&manifest.generated_at)
            .map(|value| value.with_timezone(&Utc));
        if declared.map_or(true, |declared| modified > declared) {
            manifest.generated_at = modified.to_rfc3339();
        }
    }
    Ok(Some(manifest))
}

/// Registers only the files named by the provider's validated generation
/// manifest. This keeps chat completion proportional to the new output instead
/// of decoding and hashing every image in a large project.
#[tauri::command]
pub fn scan_generation_assets(
    workspace_id: String,
    worktree_id: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Asset>> {
    scan_generation_assets_inner(&workspace_id, worktree_id.as_deref(), Some(&app), &state)
}

pub(crate) fn scan_generation_assets_inner(
    workspace_id: &str,
    worktree_id: Option<&str>,
    app: Option<&tauri::AppHandle>,
    state: &AppState,
) -> CommandResult<Vec<Asset>> {
    let root = workspace_path(state, workspace_id)?;
    let assets_root = root.join("assets");
    crate::allow_asset_directory(app, &assets_root, true)?;
    let manifest = match read_generation_manifest(&root) {
        Ok(Some(manifest)) => Some(manifest),
        Ok(None) | Err(_) => recover_manifest_from_imagegen_sources(&root)?,
    };
    let Some(manifest) = manifest else {
        return Ok(Vec::new());
    };
    let existing_ids = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let mut statement =
            connection.prepare("SELECT path, id FROM assets WHERE workspace_id = ?1")?;
        let rows = statement.query_map([&workspace_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.filter_map(Result::ok)
            .collect::<std::collections::HashMap<_, _>>()
    };
    let master_path = manifest
        .source
        .as_ref()
        .map(|relative| root.join(relative))
        .filter(|path| path.is_file());
    let mut generated = Vec::with_capacity(manifest.files.len());
    for relative in &manifest.files {
        let path = root.join(relative);
        if path.extension().and_then(|value| value.to_str()) == Some("png") {
            normalize_sprite_file(&path, master_path.as_deref())?;
        }
        let existing_id = existing_ids.get(path.to_string_lossy().as_ref()).cloned();
        let asset = inspect(workspace_id, &root, &path, existing_id)?;
        upsert(state, &asset, "generated")?;
        generated.push(asset);
    }
    if !generated.is_empty() {
        if let Some(worktree_id) = worktree_id {
            let kind = manifest
                .kind
                .clone()
                .unwrap_or_else(|| "generation".into());
            crate::pipeline::append_session_from_manifest(
                &root,
                &manifest,
                &crate::pipeline::ManifestSessionHook {
                    worktree_id: worktree_id.to_string(),
                    kind,
                    animation_id: None,
                    score: None,
                },
            )?;
        }
    }
    Ok(generated)
}
