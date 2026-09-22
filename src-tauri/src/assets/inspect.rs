use crate::{
    error::{CommandError, CommandResult},
    models::{Asset, AssetVersion},
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use image::{ColorType, ImageReader};
use rusqlite::{params, OptionalExtension};
use std::path::{Path, PathBuf};
use tauri::State;
use uuid::Uuid;

pub(super) fn asset_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Asset> {
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

pub(super) fn content_hash(path: &Path) -> CommandResult<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hasher.finalize().to_hex().to_string())
}

pub(crate) fn upsert(state: &AppState, asset: &Asset, change_kind: &str) -> CommandResult<()> {
    let hash = content_hash(Path::new(&asset.path))?;
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
            transaction.execute(
                "INSERT INTO asset_versions(id, asset_id, version_number, path, format, width, height, file_size, has_alpha, content_hash, change_kind, available, selected, created_at) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, 1, ?11)",
                params![Uuid::new_v4().to_string(), asset.id, asset.path, asset.format, asset.width, asset.height, asset.file_size as i64, asset.has_alpha, hash, change_kind, asset.created_at],
            )?;
        }
        Some((version_id, _, previous_hash, _)) if previous_hash.is_empty() => {
            transaction.execute(
                "UPDATE asset_versions SET path=?1, format=?2, width=?3, height=?4, file_size=?5, has_alpha=?6, content_hash=?7, available=1, selected=1 WHERE id=?8",
                params![asset.path, asset.format, asset.width, asset.height, asset.file_size as i64, asset.has_alpha, hash, version_id],
            )?;
        }
        Some((version_id, version_number, previous_hash, previous_path))
            if previous_hash != hash =>
        {
            let previous_available =
                previous_path != asset.path && Path::new(&previous_path).is_file();
            transaction.execute(
                "UPDATE asset_versions SET selected=0, available=?1 WHERE id=?2",
                params![previous_available, version_id],
            )?;
            transaction.execute(
                "INSERT INTO asset_versions(id, asset_id, version_number, parent_version_id, generation_id, path, format, width, height, file_size, has_alpha, content_hash, change_kind, available, selected, created_at) VALUES (?1, ?2, ?3, ?4, (SELECT generation_id FROM assets WHERE id=?2), ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1, 1, ?13)",
                params![Uuid::new_v4().to_string(), asset.id, version_number + 1, version_id, asset.path, asset.format, asset.width, asset.height, asset.file_size as i64, asset.has_alpha, hash, change_kind, Utc::now().to_rfc3339()],
            )?;
        }
        Some((version_id, _, _, _)) => {
            transaction.execute(
                "UPDATE asset_versions SET path=?1, format=?2, width=?3, height=?4, file_size=?5, has_alpha=?6, available=1, selected=1 WHERE id=?7",
                params![asset.path, asset.format, asset.width, asset.height, asset.file_size as i64, asset.has_alpha, version_id],
            )?;
        }
    }
    transaction.commit()?;
    Ok(())
}

#[tauri::command]
pub fn list_assets(
    workspace_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Asset>> {
    let root = workspace_path(&state, &workspace_id)?;
    let assets_root = root.join("assets");
    std::fs::create_dir_all(&assets_root)?;
    crate::allow_asset_directory(Some(&app), &assets_root, true)?;
    list_assets_inner(&workspace_id, &state)
}

pub(crate) fn list_assets_inner(workspace_id: &str, state: &AppState) -> CommandResult<Vec<Asset>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut statement = connection.prepare(
        "SELECT id, workspace_id, name, path, relative_path, category, format, width, height, file_size, has_alpha, created_at FROM assets WHERE workspace_id = ?1 ORDER BY category, name",
    )?;
    let rows = statement.query_map([workspace_id], asset_row)?;
    Ok(rows
        .filter_map(Result::ok)
        .filter(|asset| Path::new(&asset.path).is_file())
        .collect())
}

pub(super) fn safe_category(category: &str) -> CommandResult<String> {
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
