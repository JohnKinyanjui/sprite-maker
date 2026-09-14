use crate::{
    error::{CommandError, CommandResult},
    models::{ListMissingMotionsResult, MotionPreset, MotionPresetCatalog},
    workspace::workspace_path,
    AppState,
};
use std::path::{Path, PathBuf};
use tauri::State;

use super::anchors::get_anchor_inner;
use super::contract::size_contract_check_inner;
use super::direction_meta::list_direction_meta_for_worktree;
use super::facing::canonical_facing_for_view;

fn matches_anchor(meta: &crate::models::AnimationDirectionMeta, anchor_slug: &str) -> bool {
    meta.anchor_slug
        .as_deref()
        .map(|slug| slug == anchor_slug)
        .unwrap_or(true)
}

const CATALOG_VERSION: u32 = 2;
const CATALOG_FILE: &str = "motion-presets.json";

pub(crate) fn motion_presets_path(root: &Path) -> PathBuf {
    root.join(".sprite-studio").join(CATALOG_FILE)
}

fn category_for_id(id: &str) -> &'static str {
    match id {
        "attack" | "cast" | "block" => "combat",
        "hurt" | "death" => "reaction",
        "pickup" | "throw" => "interaction",
        _ => "locomotion",
    }
}

fn preset(
    id: &str,
    label: &str,
    motion: &str,
    category: &str,
    frame_count: u32,
    fps: f64,
    looping: bool,
) -> MotionPreset {
    MotionPreset {
        id: id.into(),
        label: label.into(),
        motion: motion.into(),
        category: category.into(),
        frame_count,
        fps,
        looping,
        enabled: true,
    }
}

pub(crate) fn default_motion_presets() -> MotionPresetCatalog {
    MotionPresetCatalog {
        version: CATALOG_VERSION,
        presets: vec![
            preset("idle", "Idle", "idle", "locomotion", 4, 8.0, true),
            preset("walk", "Walk", "walk", "locomotion", 6, 10.0, true),
            preset("run", "Run", "run", "locomotion", 6, 12.0, true),
            preset("crouch", "Crouch", "crouch", "locomotion", 4, 8.0, true),
            preset("climb", "Climb", "climb", "locomotion", 6, 10.0, true),
            preset("jump", "Jump", "jump", "locomotion", 6, 10.0, false),
            preset("attack", "Attack", "attack", "combat", 6, 12.0, false),
            preset("cast", "Cast", "cast", "combat", 6, 12.0, false),
            preset("block", "Block", "block", "combat", 4, 10.0, false),
            preset("hurt", "Hurt", "hurt", "reaction", 4, 10.0, false),
            preset("death", "Death", "death", "reaction", 6, 8.0, false),
            preset("pickup", "Pickup", "pickup", "interaction", 4, 10.0, false),
            preset("throw", "Throw", "throw", "interaction", 6, 12.0, false),
        ],
    }
}

fn migrate_catalog(mut catalog: MotionPresetCatalog) -> MotionPresetCatalog {
    if catalog.version >= CATALOG_VERSION {
        for preset in &mut catalog.presets {
            if preset.category.is_empty() {
                preset.category = category_for_id(&preset.id).into();
            }
        }
        return catalog;
    }

    for preset in &mut catalog.presets {
        preset.category = category_for_id(&preset.id).into();
    }

    let defaults = default_motion_presets();
    for default in defaults.presets {
        if !catalog.presets.iter().any(|entry| entry.id == default.id) {
            catalog.presets.push(default);
        }
    }
    catalog.version = CATALOG_VERSION;
    catalog
}

pub(crate) fn load_motion_presets_inner(
    state: &AppState,
    workspace_id: &str,
) -> CommandResult<MotionPresetCatalog> {
    let root = workspace_path(state, workspace_id)?;
    let path = motion_presets_path(&root);
    if !path.is_file() {
        return Ok(default_motion_presets());
    }
    let bytes = std::fs::read(&path)?;
    let catalog = serde_json::from_slice::<MotionPresetCatalog>(&bytes).map_err(|error| {
        CommandError::new(
            "invalid_motion_presets",
            format!("Motion preset catalog is invalid: {error}"),
        )
    })?;
    if catalog.presets.is_empty() {
        return Ok(default_motion_presets());
    }
    let original_version = catalog.version;
    let migrated = migrate_catalog(catalog);
    if original_version < CATALOG_VERSION {
        let _ = save_motion_presets_inner(state, workspace_id, &migrated);
    }
    Ok(migrated)
}

pub(crate) fn save_motion_presets_inner(
    state: &AppState,
    workspace_id: &str,
    catalog: &MotionPresetCatalog,
) -> CommandResult<MotionPresetCatalog> {
    let root = workspace_path(state, workspace_id)?;
    let path = motion_presets_path(&root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_vec_pretty(catalog)
        .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
    std::fs::write(path, payload)?;
    Ok(catalog.clone())
}

pub(crate) fn resolve_batch_presets(
    catalog: &MotionPresetCatalog,
    motion_ids: &[String],
) -> Vec<MotionPreset> {
    if motion_ids.is_empty() {
        return catalog
            .presets
            .iter()
            .filter(|preset| preset.enabled)
            .cloned()
            .collect();
    }
    catalog
        .presets
        .iter()
        .filter(|preset| {
            preset.enabled
                && motion_ids
                    .iter()
                    .any(|id| id == &preset.id || id == &preset.motion)
        })
        .cloned()
        .collect()
}

fn motion_has_passing_canonical(
    state: &AppState,
    workspace_id: &str,
    worktree_id: &str,
    anchor_slug: &str,
    motion: &str,
    canonical_facing: &str,
) -> bool {
    let metas = list_direction_meta_for_worktree(state, workspace_id, worktree_id)
        .unwrap_or_default();
    let animation_id = metas
        .iter()
        .find(|meta| {
            meta.direction_family == motion
                && meta.facing == canonical_facing
                && matches_anchor(meta, anchor_slug)
        })
        .map(|meta| meta.animation_id.clone());
    if let Some(animation_id) = animation_id {
        return size_contract_check_inner(state, &animation_id, Some(anchor_slug))
            .map(|report| report.passed)
            .unwrap_or(false);
    }
    false
}

pub(crate) fn list_missing_motions_inner(
    state: &AppState,
    workspace_id: &str,
    worktree_id: &str,
    anchor_slug: &str,
) -> CommandResult<ListMissingMotionsResult> {
    let anchor = get_anchor_inner(workspace_id, anchor_slug, state)?;
    let view = anchor.view.as_deref().unwrap_or("side");
    let canonical_facing = canonical_facing_for_view(view).to_string();
    let catalog = load_motion_presets_inner(state, workspace_id)?;
    let mut missing_preset_ids = Vec::new();
    let mut missing_motions = Vec::new();
    for preset in catalog.presets.iter().filter(|entry| entry.enabled) {
        if motion_has_passing_canonical(
            state,
            workspace_id,
            worktree_id,
            anchor_slug,
            &preset.motion,
            &canonical_facing,
        ) {
            continue;
        }
        missing_preset_ids.push(preset.id.clone());
        if !missing_motions.iter().any(|motion| motion == &preset.motion) {
            missing_motions.push(preset.motion.clone());
        }
    }
    Ok(ListMissingMotionsResult {
        anchor_slug: anchor_slug.to_string(),
        canonical_facing,
        missing_preset_ids,
        missing_motions,
    })
}

#[tauri::command]
pub fn list_motion_presets(
    workspace_id: String,
    state: State<'_, AppState>,
) -> CommandResult<MotionPresetCatalog> {
    load_motion_presets_inner(&state, &workspace_id)
}

#[tauri::command]
pub fn save_motion_presets(
    workspace_id: String,
    catalog: MotionPresetCatalog,
    state: State<'_, AppState>,
) -> CommandResult<MotionPresetCatalog> {
    save_motion_presets_inner(&state, &workspace_id, &catalog)
}

#[tauri::command]
pub fn list_missing_motions(
    workspace_id: String,
    worktree_id: String,
    anchor_slug: String,
    state: State<'_, AppState>,
) -> CommandResult<ListMissingMotionsResult> {
    list_missing_motions_inner(&state, &workspace_id, &worktree_id, &anchor_slug)
}
