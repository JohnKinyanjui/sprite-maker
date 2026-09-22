use super::inspect::get_asset;
use super::scan::write_generation_manifest;
use crate::models::GenerationManifest;
use crate::animations::{delete_animation_inner, detach_asset_from_animations};
use crate::error::{CommandError, CommandResult};
use crate::models::AssetUsage;
use crate::pipeline::{list_anchors_inner, remove_anchors_for_asset_inner};
use crate::workspace::workspace_path;
use crate::AppState;
use std::path::Path;
use tauri::State;

pub(crate) fn collect_asset_usage(state: &AppState, asset_id: &str) -> CommandResult<AssetUsage> {
    let asset = get_asset(state, asset_id)?;
    let (animation_names, sprite_sheet_items) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let mut animation_names = Vec::new();
        let mut statement = connection.prepare(
            r#"SELECT DISTINCT a.name
               FROM animations a
               JOIN animation_frames af ON af.animation_id = a.id
               WHERE af.asset_id = ?1
               ORDER BY a.name"#,
        )?;
        let rows = statement.query_map([asset_id], |row| row.get::<_, String>(0))?;
        animation_names.extend(rows.filter_map(Result::ok));
        let sprite_sheet_items: u32 = connection
            .query_row(
                "SELECT COUNT(*) FROM sprite_sheet_items WHERE asset_id = ?1",
                [asset_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count.max(0) as u32)?;
        (animation_names, sprite_sheet_items)
    };
    let anchor_slugs = list_anchors_inner(&asset.workspace_id, state)?
        .into_iter()
        .filter(|anchor| anchor.asset_id == asset_id)
        .map(|anchor| anchor.slug)
        .collect();
    Ok(AssetUsage {
        animation_names,
        anchor_slugs,
        sprite_sheet_items,
    })
}

fn usage_blocks_delete(usage: &AssetUsage) -> bool {
    !usage.animation_names.is_empty() || !usage.anchor_slugs.is_empty() || usage.sprite_sheet_items > 0
}

fn format_usage_message(usage: &AssetUsage) -> String {
    let mut parts = Vec::new();
    if !usage.animation_names.is_empty() {
        parts.push(format!(
            "used by {} animation(s): {}",
            usage.animation_names.len(),
            usage.animation_names.join(", ")
        ));
    }
    if !usage.anchor_slugs.is_empty() {
        parts.push(format!(
            "linked to character anchor(s): {}",
            usage.anchor_slugs.join(", ")
        ));
    }
    if usage.sprite_sheet_items > 0 {
        parts.push(format!(
            "referenced by {} sprite sheet cell(s)",
            usage.sprite_sheet_items
        ));
    }
    parts.join("; ")
}

fn delete_version_files(state: &AppState, asset_id: &str) -> CommandResult<()> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut statement = connection.prepare(
        "SELECT path FROM asset_versions WHERE asset_id = ?1 AND available = 1",
    )?;
    let paths = statement
        .query_map([asset_id], |row| row.get::<_, String>(0))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    for path in paths {
        let file = Path::new(&path);
        if file.is_file() {
            let _ = std::fs::remove_file(file);
        }
    }
    Ok(())
}

fn read_generation_manifest_loose(root: &Path) -> CommandResult<Option<GenerationManifest>> {
    let path = root.join(".sprite-studio/last-generation.json");
    if !path.is_file() {
        return Ok(None);
    }
    let manifest = serde_json::from_str(&std::fs::read_to_string(&path)?)
        .map_err(|error| CommandError::new("invalid_generation", error.to_string()))?;
    Ok(Some(manifest))
}

fn prune_generation_manifest(
    state: &AppState,
    workspace_id: &str,
    relative_path: &str,
) -> CommandResult<()> {
    let root = workspace_path(state, workspace_id)?;
    let manifest = read_generation_manifest_loose(&root)?;
    let Some(mut manifest) = manifest else {
        return Ok(());
    };
    let before = manifest.files.len();
    manifest.files.retain(|file| file != relative_path);
    if manifest.source.as_deref() == Some(relative_path) {
        manifest.source = None;
    }
    if manifest.files.len() != before || manifest.source.is_none() {
        write_generation_manifest(&root, &manifest, None)?;
    }
    Ok(())
}

pub(crate) fn delete_asset_inner(
    state: &AppState,
    asset_id: &str,
    force: bool,
) -> CommandResult<()> {
    let asset = get_asset(state, asset_id)?;
    let usage = collect_asset_usage(state, asset_id)?;
    if usage_blocks_delete(&usage) && !force {
        return Err(CommandError::new(
            "asset_in_use",
            format!(
                "This asset is still referenced ({}). Remove it from those tools first, or force delete to detach it everywhere.",
                format_usage_message(&usage)
            ),
        ));
    }
    if force {
        detach_asset_from_animations(state, asset_id)?;
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.execute(
            "DELETE FROM sprite_sheet_items WHERE asset_id = ?1",
            [asset_id],
        )?;
    }
    remove_anchors_for_asset_inner(&asset.workspace_id, asset_id, state)?;
    prune_generation_manifest(state, &asset.workspace_id, &asset.relative_path)?;
    delete_version_files(state, asset_id)?;
    if Path::new(&asset.path).is_file() {
        std::fs::remove_file(&asset.path)?;
    }
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let changed = connection.execute("DELETE FROM assets WHERE id = ?1", [asset_id])?;
    if changed == 0 {
        return Err(CommandError::new("asset_not_found", "Asset no longer exists"));
    }
    Ok(())
}

#[tauri::command]
pub fn get_asset_usage(id: String, state: State<'_, AppState>) -> CommandResult<AssetUsage> {
    collect_asset_usage(&state, &id)
}

#[tauri::command]
pub fn delete_asset(
    id: String,
    force: Option<bool>,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    delete_asset_inner(&state, &id, force.unwrap_or(false))
}
