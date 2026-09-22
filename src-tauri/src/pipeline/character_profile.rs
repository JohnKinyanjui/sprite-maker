use crate::{
    assets::{extract_palette, get_asset},
    error::{CommandError, CommandResult},
    models::CharacterProfile,
    worktrees::list_worktrees_inner,
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use std::path::{Path, PathBuf};
use tauri::State;

use super::anchors::get_anchor_inner;
use super::direction_meta::list_direction_meta_for_worktree;
use super::qc::perceptual_hash;

const PROFILE_VERSION: u32 = 1;

pub(crate) fn profiles_directory(root: &Path) -> PathBuf {
    root.join(".sprite-studio").join("character-profiles")
}

pub(crate) fn profile_path(root: &Path, slug: &str) -> PathBuf {
    profiles_directory(root).join(format!("{slug}.json"))
}

fn rgb_to_hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

fn lookup_rig_id(state: &AppState, workspace_id: &str, asset_id: &str) -> Option<String> {
    let connection = match state.db.lock() {
        Ok(value) => value,
        Err(_) => return None,
    };
    connection
        .query_row(
            "SELECT id FROM rigs WHERE workspace_id=?1 AND asset_id=?2 ORDER BY updated_at DESC LIMIT 1",
            params![workspace_id, asset_id],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten()
}

fn collect_direction_families(
    state: &AppState,
    workspace_id: &str,
    anchor_slug: &str,
) -> Vec<String> {
    let worktrees = list_worktrees_inner(workspace_id, state).unwrap_or_default();
    let mut families = Vec::new();
    for worktree in worktrees {
        if worktree.kind != "character" {
            continue;
        }
        let metas = list_direction_meta_for_worktree(state, workspace_id, &worktree.id)
            .unwrap_or_default();
        for meta in metas {
            if meta.anchor_slug.as_deref() == Some(anchor_slug) {
                if !families.iter().any(|family| family == &meta.direction_family) {
                    families.push(meta.direction_family.clone());
                }
            }
        }
    }
    families.sort();
    families
}

pub(crate) fn build_character_profile_inner(
    workspace_id: &str,
    slug: &str,
    worktree_id: Option<&str>,
    state: &AppState,
) -> CommandResult<CharacterProfile> {
    let anchor = get_anchor_inner(workspace_id, slug, state)?;
    let asset = get_asset(state, &anchor.asset_id)?;
    let image = image::open(&asset.path)?.to_rgba8();
    let palette = extract_palette(&image, 16)
        .into_iter()
        .map(rgb_to_hex)
        .collect::<Vec<_>>();
    let d_hash = format!("{:016x}", perceptual_hash(&image));
    Ok(CharacterProfile {
        version: PROFILE_VERSION,
        anchor_slug: anchor.slug.clone(),
        asset_id: anchor.asset_id.clone(),
        workspace_id: workspace_id.to_string(),
        worktree_id: worktree_id.map(str::to_string),
        palette,
        d_hash,
        rig_id: lookup_rig_id(state, workspace_id, &anchor.asset_id),
        direction_families: collect_direction_families(state, workspace_id, slug),
        updated_at: Utc::now().to_rfc3339(),
    })
}

pub(crate) fn write_character_profile(
    root: &Path,
    profile: &CharacterProfile,
) -> CommandResult<()> {
    let path = profile_path(root, &profile.anchor_slug);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_vec_pretty(profile)
        .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
    std::fs::write(path, payload)?;
    Ok(())
}

pub(crate) fn sync_character_profile_for_anchor(
    workspace_id: &str,
    slug: &str,
    worktree_id: Option<&str>,
    state: &AppState,
) -> CommandResult<CharacterProfile> {
    let profile = build_character_profile_inner(workspace_id, slug, worktree_id, state)?;
    let root = workspace_path(state, workspace_id)?;
    write_character_profile(&root, &profile)?;
    Ok(profile)
}

pub(crate) fn get_character_profile_inner(
    workspace_id: &str,
    slug: &str,
    state: &AppState,
) -> CommandResult<CharacterProfile> {
    let root = workspace_path(state, workspace_id)?;
    let path = profile_path(&root, slug);
    if path.is_file() {
        let bytes = std::fs::read(&path)?;
        return serde_json::from_slice(&bytes).map_err(|error| {
            CommandError::new(
                "invalid_character_profile",
                format!("Character profile is invalid: {error}"),
            )
        });
    }
    sync_character_profile_for_anchor(workspace_id, slug, None, state)
}

pub(crate) fn export_character_profile_inner(
    workspace_id: &str,
    slug: &str,
    destination: &str,
    state: &AppState,
) -> CommandResult<CharacterProfile> {
    let profile = get_character_profile_inner(workspace_id, slug, state)?;
    let destination_path = PathBuf::from(destination);
    if let Some(parent) = destination_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_vec_pretty(&profile)
        .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
    std::fs::write(&destination_path, payload)?;
    Ok(profile)
}

pub(crate) fn import_character_profile_inner(
    workspace_id: &str,
    source_path: &str,
    state: &AppState,
) -> CommandResult<CharacterProfile> {
    let bytes = std::fs::read(source_path)?;
    let profile = serde_json::from_slice::<CharacterProfile>(&bytes).map_err(|error| {
        CommandError::new(
            "invalid_character_profile",
            format!("Character profile import is invalid: {error}"),
        )
    })?;
    if profile.workspace_id != workspace_id {
        return Err(CommandError::new(
            "workspace_mismatch",
            "Imported profile workspaceId does not match the target workspace",
        ));
    }
    let root = workspace_path(state, workspace_id)?;
    write_character_profile(&root, &profile)?;
    Ok(profile)
}

#[tauri::command]
pub fn get_character_profile(
    workspace_id: String,
    slug: String,
    state: State<'_, AppState>,
) -> CommandResult<CharacterProfile> {
    get_character_profile_inner(&workspace_id, &slug, &state)
}

#[tauri::command]
pub fn refresh_character_profile(
    workspace_id: String,
    slug: String,
    worktree_id: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<CharacterProfile> {
    sync_character_profile_for_anchor(
        &workspace_id,
        &slug,
        worktree_id.as_deref(),
        &state,
    )
}

#[tauri::command]
pub fn export_character_profile(
    input: crate::models::ExportCharacterProfileInput,
    state: State<'_, AppState>,
) -> CommandResult<CharacterProfile> {
    export_character_profile_inner(
        &input.workspace_id,
        &input.slug,
        &input.destination,
        &state,
    )
}

#[tauri::command]
pub fn import_character_profile(
    input: crate::models::ImportCharacterProfileInput,
    state: State<'_, AppState>,
) -> CommandResult<CharacterProfile> {
    import_character_profile_inner(&input.workspace_id, &input.source_path, &state)
}
