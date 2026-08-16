use crate::{
    error::{CommandError, CommandResult},
    models::{Asset, AssetVersion, ExportResult, GenerationManifest},
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use image::{ColorType, ImageReader};
use rusqlite::{params, OptionalExtension};
use std::path::{Path, PathBuf};
use tauri::{Manager, State};
use uuid::Uuid;

fn asset_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Asset> {
    Ok(Asset {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        name: row.get(2)?,
        path: row.get(3)?,
        relative_path: row.get(4)?,
        category: row.get(5)?,
        format: row.get(6)?,
        width: row.get(7)?,
        height: row.get(8)?,
        file_size: row.get::<_, i64>(9)? as u64,
        has_alpha: row.get(10)?,
        created_at: row.get(11)?,
    })
}

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

pub(crate) fn inspect(
    workspace_id: &str,
    root: &Path,
    path: &Path,
    existing_id: Option<String>,
) -> CommandResult<Asset> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let format = reader
        .format()
        .map(|value| format!("{value:?}").to_ascii_lowercase())
        .unwrap_or_else(|| "image".into());
    let image = reader.decode()?;
    let has_alpha = matches!(
        image.color(),
        ColorType::La8
            | ColorType::La16
            | ColorType::Rgba8
            | ColorType::Rgba16
            | ColorType::Rgba32F
    );
    let relative = path.strip_prefix(root).map_err(|_| {
        CommandError::new(
            "asset_outside_workspace",
            "Asset path is outside the workspace",
        )
    })?;
    let category = relative
        .components()
        .nth(1)
        .and_then(|value| value.as_os_str().to_str())
        .unwrap_or("uncategorized")
        .to_string();
    Ok(Asset {
        id: existing_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        workspace_id: workspace_id.to_string(),
        name: path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Untitled")
            .to_string(),
        path: path.to_string_lossy().into_owned(),
        relative_path: relative.to_string_lossy().into_owned(),
        category,
        format,
        width: image.width(),
        height: image.height(),
        file_size: std::fs::metadata(path)?.len(),
        has_alpha,
        created_at: Utc::now().to_rfc3339(),
    })
}

fn content_hash(path: &Path) -> CommandResult<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hasher.finalize().to_hex().to_string())
}

fn snapshot_version(root: &Path, asset: &Asset, version_number: i64) -> CommandResult<PathBuf> {
    let extension = Path::new(&asset.path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or(asset.format.as_str());
    let directory = root
        .join(".sprite-studio")
        .join("versions")
        .join(&asset.id);
    std::fs::create_dir_all(&directory)?;
    let destination = directory.join(format!("v{version_number}.{extension}"));
    if !destination.is_file() || content_hash(&destination)? != content_hash(Path::new(&asset.path))? {
        std::fs::copy(&asset.path, &destination)?;
    }
    Ok(destination)
}

pub(crate) fn upsert(state: &AppState, asset: &Asset, change_kind: &str) -> CommandResult<()> {
    let hash = content_hash(Path::new(&asset.path))?;
    let root = workspace_path(state, &asset.workspace_id)?;
    let mut connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let transaction = connection.transaction()?;
    transaction.execute(
        r#"INSERT INTO assets(id, workspace_id, name, path, relative_path, category, format, width, height, file_size, has_alpha, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        ON CONFLICT(path) DO UPDATE SET name=excluded.name, relative_path=excluded.relative_path, category=excluded.category,
        format=excluded.format, width=excluded.width, height=excluded.height, file_size=excluded.file_size, has_alpha=excluded.has_alpha"#,
        params![asset.id, asset.workspace_id, asset.name, asset.path, asset.relative_path, asset.category, asset.format,
            asset.width, asset.height, asset.file_size as i64, asset.has_alpha, asset.created_at],
    )?;
    let latest: Option<(String, i64, String, String)> = transaction
        .query_row(
            "SELECT id, version_number, content_hash, path FROM asset_versions WHERE asset_id = ?1 ORDER BY version_number DESC LIMIT 1",
            [&asset.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    match latest {
        None => {
            let snapshot = snapshot_version(&root, asset, 1)?;
            transaction.execute(
                "INSERT INTO asset_versions(id, asset_id, version_number, path, format, width, height, file_size, has_alpha, content_hash, change_kind, available, selected, created_at) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, 1, ?11)",
                params![Uuid::new_v4().to_string(), asset.id, snapshot.to_string_lossy(), asset.format, asset.width, asset.height, asset.file_size as i64, asset.has_alpha, hash, change_kind, asset.created_at],
            )?;
        }
        Some((version_id, version_number, previous_hash, _)) if previous_hash.is_empty() => {
            let snapshot = snapshot_version(&root, asset, version_number)?;
            transaction.execute(
                "UPDATE asset_versions SET path=?1, format=?2, width=?3, height=?4, file_size=?5, has_alpha=?6, content_hash=?7, available=1, selected=1 WHERE id=?8",
                params![snapshot.to_string_lossy(), asset.format, asset.width, asset.height, asset.file_size as i64, asset.has_alpha, hash, version_id],
            )?;
        }
        Some((version_id, version_number, previous_hash, previous_path))
            if previous_hash != hash =>
        {
            let previous_available = Path::new(&previous_path).is_file();
            let snapshot = snapshot_version(&root, asset, version_number + 1)?;
            transaction.execute(
                "UPDATE asset_versions SET selected=0, available=?1 WHERE id=?2",
                params![previous_available, version_id],
            )?;
            transaction.execute(
                "INSERT INTO asset_versions(id, asset_id, version_number, parent_version_id, generation_id, path, format, width, height, file_size, has_alpha, content_hash, change_kind, available, selected, created_at) VALUES (?1, ?2, ?3, ?4, (SELECT generation_id FROM assets WHERE id=?2), ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1, 1, ?13)",
                params![Uuid::new_v4().to_string(), asset.id, version_number + 1, version_id, snapshot.to_string_lossy(), asset.format, asset.width, asset.height, asset.file_size as i64, asset.has_alpha, hash, change_kind, Utc::now().to_rfc3339()],
            )?;
        }
        Some((version_id, version_number, _, version_path)) => {
            let snapshot = if Path::new(&version_path).is_file() && version_path != asset.path {
                PathBuf::from(version_path)
            } else {
                snapshot_version(&root, asset, version_number)?
            };
            transaction.execute(
                "UPDATE asset_versions SET path=?1, format=?2, width=?3, height=?4, file_size=?5, has_alpha=?6, available=1, selected=1 WHERE id=?7",
                params![snapshot.to_string_lossy(), asset.format, asset.width, asset.height, asset.file_size as i64, asset.has_alpha, version_id],
            )?;
        }
    }
    transaction.commit()?;
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
    app.asset_protocol_scope()
        .allow_directory(&assets_root, true)
        .map_err(|error| CommandError::new("asset_scope_error", error.to_string()))?;
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

fn safe_category(category: &str) -> CommandResult<String> {
    let value = category.trim().to_ascii_lowercase();
    if !matches!(
        value.as_str(),
        "characters" | "creatures" | "terrain" | "props" | "effects"
    ) {
        return Err(CommandError::new(
            "invalid_category",
            "Choose characters, creatures, terrain, props, or effects",
        ));
    }
    Ok(value)
}

#[tauri::command]
pub fn import_asset(
    workspace_id: String,
    source_path: String,
    category: String,
    state: State<'_, AppState>,
) -> CommandResult<Asset> {
    let root = workspace_path(&state, &workspace_id)?;
    let source = PathBuf::from(source_path);
    if !source.is_file() {
        return Err(CommandError::new(
            "asset_missing",
            "The selected asset file no longer exists",
        ));
    }
    // Decode before copying so invalid files never enter the workspace.
    ImageReader::open(&source)?
        .with_guessed_format()?
        .decode()?;
    let category = safe_category(&category)?;
    let directory = root.join("assets").join(&category);
    std::fs::create_dir_all(&directory)?;
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("asset");
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("png");
    let mut destination = directory.join(format!("{stem}.{extension}"));
    let mut version = 2;
    while destination.exists() {
        destination = directory.join(format!("{stem}-{version}.{extension}"));
        version += 1;
    }
    std::fs::copy(&source, &destination)?;
    let asset = inspect(&workspace_id, &root, &destination, None)?;
    upsert(&state, &asset, "imported")?;
    Ok(asset)
}

#[tauri::command]
pub fn rename_asset(id: String, name: String, state: State<'_, AppState>) -> CommandResult<Asset> {
    let name = name.trim();
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        return Err(CommandError::new(
            "invalid_name",
            "Enter a valid filename without path separators",
        ));
    }
    let (workspace_id, old_path): (String, String) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                "SELECT workspace_id, path FROM assets WHERE id = ?1",
                [&id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| CommandError::new("asset_not_found", "Asset no longer exists"))?
    };
    let old_path = PathBuf::from(old_path);
    if !old_path.is_file() {
        return Err(CommandError::new(
            "asset_missing",
            "The asset was moved or deleted outside Sprite Studio",
        ));
    }
    let extension = old_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("png");
    let new_path = old_path.with_file_name(format!("{name}.{extension}"));
    if new_path.exists() {
        return Err(CommandError::new(
            "asset_exists",
            "An asset with that name already exists",
        ));
    }
    std::fs::rename(&old_path, &new_path)?;
    let root = workspace_path(&state, &workspace_id)?;
    let asset = inspect(&workspace_id, &root, &new_path, Some(id.clone()))?;
    upsert(&state, &asset, "renamed")?;
    Ok(asset)
}

#[tauri::command]
pub fn delete_asset(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let path: String = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row("SELECT path FROM assets WHERE id = ?1", [&id], |row| {
                row.get(0)
            })
            .optional()?
            .ok_or_else(|| CommandError::new("asset_not_found", "Asset no longer exists"))?
    };
    if Path::new(&path).is_file() {
        std::fs::remove_file(&path)?;
    }
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute("DELETE FROM assets WHERE id = ?1", [id])?;
    Ok(())
}

#[tauri::command]
pub fn export_asset(id: String, state: State<'_, AppState>) -> CommandResult<ExportResult> {
    let asset = get_asset(&state, &id)?;
    let source = PathBuf::from(&asset.path);
    if !source.is_file() {
        return Err(CommandError::new(
            "asset_missing",
            "The asset was moved or deleted outside Sprite Studio",
        ));
    }
    let output_directory = workspace_path(&state, &asset.workspace_id)?.join("exports");
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
    })
}

pub fn get_asset(state: &AppState, id: &str) -> CommandResult<Asset> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.query_row(
        "SELECT id, workspace_id, name, path, relative_path, category, format, width, height, file_size, has_alpha, created_at FROM assets WHERE id = ?1",
        [id], asset_row,
    ).optional()?.ok_or_else(|| CommandError::new("asset_not_found", "Asset no longer exists"))
}

fn asset_version_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssetVersion> {
    Ok(AssetVersion {
        id: row.get(0)?,
        asset_id: row.get(1)?,
        version_number: row.get(2)?,
        parent_version_id: row.get(3)?,
        generation_id: row.get(4)?,
        path: row.get(5)?,
        format: row.get(6)?,
        width: row.get(7)?,
        height: row.get(8)?,
        file_size: row.get::<_, i64>(9)? as u64,
        has_alpha: row.get(10)?,
        content_hash: row.get(11)?,
        change_kind: row.get(12)?,
        available: row.get(13)?,
        selected: row.get(14)?,
        created_at: row.get(15)?,
    })
}

#[tauri::command]
pub fn list_asset_versions(
    asset_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<AssetVersion>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut statement = connection.prepare(
        "SELECT id, asset_id, version_number, parent_version_id, generation_id, path, format, width, height, file_size, has_alpha, content_hash, change_kind, available, selected, created_at FROM asset_versions WHERE asset_id = ?1 ORDER BY version_number DESC",
    )?;
    let rows = statement.query_map([asset_id], asset_version_row)?;
    Ok(rows.filter_map(Result::ok).collect())
}

#[tauri::command]
pub fn restore_asset_version(
    asset_id: String,
    version_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Asset> {
    restore_asset_version_record(&asset_id, &version_id, &state)
}

fn restore_asset_version_record(
    asset_id: &str,
    version_id: &str,
    state: &AppState,
) -> CommandResult<Asset> {
    let asset = get_asset(state, asset_id)?;
    let version: AssetVersion = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                "SELECT id, asset_id, version_number, parent_version_id, generation_id, path, format, width, height, file_size, has_alpha, content_hash, change_kind, available, selected, created_at FROM asset_versions WHERE id=?1 AND asset_id=?2",
                params![version_id, asset_id],
                asset_version_row,
            )
            .optional()?
            .ok_or_else(|| CommandError::new("version_not_found", "Asset version no longer exists"))?
    };
    let source = PathBuf::from(&version.path);
    if !version.available || !source.is_file() {
        return Err(CommandError::new(
            "version_unavailable",
            "The pixels for this historical version are unavailable",
        ));
    }
    if version.selected {
        return Ok(asset);
    }
    let active = PathBuf::from(&asset.path);
    let parent = active.parent().ok_or_else(|| {
        CommandError::new("unsafe_restore", "Asset has no writable parent directory")
    })?;
    let temporary = parent.join(format!(".sprite-studio-restore-{}.tmp", Uuid::new_v4()));
    let safety = parent.join(format!(".sprite-studio-current-{}.tmp", Uuid::new_v4()));
    std::fs::copy(&source, &temporary)?;
    ImageReader::open(&temporary)?.with_guessed_format()?.decode()?;
    std::fs::rename(&active, &safety)?;
    if let Err(error) = std::fs::rename(&temporary, &active) {
        let _ = std::fs::rename(&safety, &active);
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    let mut asset_root = active.clone();
    for _ in Path::new(&asset.relative_path).components() {
        asset_root.pop();
    }
    let restored = inspect(&asset.workspace_id, &asset_root, &active, Some(asset.id.clone()))?;
    if let Err(error) = upsert(state, &restored, "restored") {
        let _ = std::fs::remove_file(&active);
        let _ = std::fs::rename(&safety, &active);
        return Err(error);
    }
    let _ = std::fs::remove_file(safety);
    Ok(restored)
}

#[tauri::command]
pub fn get_generation_manifest(
    workspace_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Option<GenerationManifest>> {
    let root = workspace_path(&state, &workspace_id)?;
    let path = root.join(".sprite-studio/last-generation.json");
    if !path.is_file() {
        return Ok(None);
    }
    let manifest: GenerationManifest = serde_json::from_str(&std::fs::read_to_string(path)?)
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
    Ok(Some(manifest))
}

#[cfg(test)]
mod tests {
    use super::{inspect, restore_asset_version_record, safe_category, upsert};
    use crate::{database, AppState};
    use image::{Rgba, RgbaImage};
    use std::{collections::HashMap, sync::Mutex};
    use uuid::Uuid;

    #[test]
    fn accepts_only_workspace_asset_categories() {
        assert_eq!(
            safe_category("Characters").expect("valid category"),
            "characters"
        );
        assert!(safe_category("../../outside").is_err());
        assert!(safe_category("misc").is_err());
    }

    #[test]
    fn invalid_image_fails_without_becoming_an_asset() {
        let root =
            std::env::temp_dir().join(format!("sprite-studio-invalid-image-{}", Uuid::new_v4()));
        let asset_dir = root.join("assets/characters");
        std::fs::create_dir_all(&asset_dir).expect("asset directory should exist");
        let path = asset_dir.join("broken.png");
        std::fs::write(&path, b"not an image").expect("invalid fixture should write");
        let error = inspect("workspace", &root, &path, None).expect_err("invalid image must fail");
        assert!(matches!(
            error.code.as_str(),
            "invalid_image" | "filesystem_error"
        ));
        std::fs::remove_dir_all(root).expect("temporary fixture should be removable");
    }

    #[test]
    fn scanning_changed_pixels_creates_a_new_asset_version() {
        let root =
            std::env::temp_dir().join(format!("sprite-studio-version-test-{}", Uuid::new_v4()));
        let asset_dir = root.join("assets/characters");
        std::fs::create_dir_all(&asset_dir).expect("asset directory should exist");
        let connection = database::open(&root.join("studio.sqlite3")).expect("database opens");
        connection.execute(
            "INSERT INTO projects(id,name,path,created_at,last_opened_at) VALUES ('project','Test',?1,'now','now')",
            [root.to_string_lossy().as_ref()],
        ).expect("project inserts");
        let state = AppState {
            db: Mutex::new(connection),
            cancellers: Mutex::new(HashMap::new()),
        };
        let path = asset_dir.join("hero.png");
        RgbaImage::from_pixel(8, 8, Rgba([255, 0, 0, 255]))
            .save(&path)
            .expect("first image saves");
        let first = inspect("project", &root, &path, None).expect("first image inspects");
        upsert(&state, &first, "scanned").expect("first image indexes");
        upsert(&state, &first, "scanned").expect("unchanged image reindexes");
        RgbaImage::from_pixel(8, 8, Rgba([0, 255, 0, 255]))
            .save(&path)
            .expect("changed image saves");
        let changed = inspect("project", &root, &path, Some(first.id.clone()))
            .expect("changed image inspects");
        upsert(&state, &changed, "scanned").expect("changed image indexes");
        let versions: i64 = state
            .db
            .lock()
            .expect("database lock")
            .query_row(
                "SELECT COUNT(*) FROM asset_versions WHERE asset_id=?1",
                [&first.id],
                |row| row.get(0),
            )
            .expect("versions count");
        assert_eq!(versions, 2);
        let available: i64 = state
            .db
            .lock()
            .expect("database lock")
            .query_row(
                "SELECT COUNT(*) FROM asset_versions WHERE asset_id=?1 AND available=1",
                [&first.id],
                |row| row.get(0),
            )
            .expect("available version count");
        assert_eq!(available, 2);
        let first_version: String = state
            .db
            .lock()
            .expect("database lock")
            .query_row(
                "SELECT id FROM asset_versions WHERE asset_id=?1 ORDER BY version_number LIMIT 1",
                [&first.id],
                |row| row.get(0),
            )
            .expect("first version id");
        restore_asset_version_record(&first.id, &first_version, &state)
            .expect("historical version restores");
        let restored_pixel = image::open(&path)
            .expect("restored image opens")
            .to_rgba8()
            .get_pixel(0, 0)
            .0;
        assert_eq!(restored_pixel, [255, 0, 0, 255]);
        let restored_versions: i64 = state
            .db
            .lock()
            .expect("database lock")
            .query_row(
                "SELECT COUNT(*) FROM asset_versions WHERE asset_id=?1",
                [&first.id],
                |row| row.get(0),
            )
            .expect("restored versions count");
        assert_eq!(restored_versions, 3);
        drop(state);
        std::fs::remove_dir_all(root).expect("temporary fixture should be removable");
    }
}
