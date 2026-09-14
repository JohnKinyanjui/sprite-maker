use crate::{
    animations::save_animation_inner,
    assets::{extract_palette, normalize_sprite_alpha},
    assets,
    error::{CommandError, CommandResult},
    models::{AnimationFrame, GenerationManifest},
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use image::RgbaImage;
use rusqlite::OptionalExtension;
use tauri::State;

use super::commands::rig_input_to_rig;
use super::skin::render_frames;
use super::types::{Rig, RigInput, RigRenderResult};
use super::validate::validate_perceptible_rig_motion;

pub(crate) fn render_rig_frames_blocking(master_path: String, rig: Rig) -> CommandResult<Vec<RgbaImage>> {
    let master = image::open(&master_path)?.to_rgba8();
    Ok(render_frames(&master, &rig))
}

#[tauri::command]
pub async fn render_rig_preview(
    input: RigInput,
    state: State<'_, AppState>,
) -> CommandResult<Vec<String>> {
    let rig = rig_input_to_rig(input, &state)?;
    let asset_id = rig.asset_id.clone().ok_or_else(|| {
        CommandError::new("missing_master", "Choose a source sprite before previewing")
    })?;
    let asset = assets::get_asset(&state, &asset_id)?;
    let workspace = workspace_path(&state, &rig.workspace_id)?;
    if rig.bones.is_empty() {
        return Err(CommandError::new(
            "empty_rig",
            "Add at least one bone before previewing",
        ));
    }
    let master_path = asset.path.clone();
    let asset_id_for_directory = asset.id.clone();
    let frames =
        tauri::async_runtime::spawn_blocking(move || render_rig_frames_blocking(master_path, rig))
            .await
            .map_err(|error| CommandError::new("render_failed", error.to_string()))??;
    let master_palette = extract_palette(
        &image::open(&asset.path)?.to_rgba8(),
        64,
    );
    let directory = workspace
        .join(".sprite-studio")
        .join("rig-previews")
        .join(&asset_id_for_directory);
    std::fs::create_dir_all(&directory)?;
    let mut paths = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        let path = directory.join(format!("frame_{:02}.png", index + 1));
        normalize_sprite_alpha(frame, Some(&master_palette)).save(&path)?;
        paths.push(path.to_string_lossy().into_owned());
    }
    Ok(paths)
}

fn existing_asset_id(state: &AppState, path: &str) -> CommandResult<Option<String>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    Ok(connection
        .query_row("SELECT id FROM assets WHERE path = ?1", [path], |row| {
            row.get(0)
        })
        .optional()?)
}

pub(crate) fn render_rig_animation_inner(
    frames: &[RgbaImage],
    rig: &Rig,
    asset: &crate::models::Asset,
    workspace: &std::path::Path,
    state: &AppState,
) -> CommandResult<RigRenderResult> {
    let render_name = rig.name.clone();
    let category = if asset.category.is_empty() {
        "props".to_string()
    } else {
        asset.category.clone()
    };
    let output_directory = workspace.join("assets").join(&category);
    std::fs::create_dir_all(&output_directory)?;
    let slug: String = render_name
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
    let slug = if slug.is_empty() {
        "rig-animation".to_string()
    } else {
        slug.to_string()
    };
    let mut frame_paths = Vec::new();
    let mut asset_ids = Vec::new();
    let mut animation_frames = Vec::new();
    let now = Utc::now().to_rfc3339();
    let master_image = image::open(&asset.path)?.to_rgba8();
    let master_palette = extract_palette(&master_image, 64);
    for (index, frame) in frames.iter().enumerate() {
        let path = output_directory.join(format!("{}_{:02}.png", slug, index + 1));
        let normalized = normalize_sprite_alpha(frame, Some(&master_palette));
        normalized.save(&path)?;
        let path_string = path.to_string_lossy().into_owned();
        let registered = assets::inspect(
            &asset.workspace_id,
            workspace,
            &path,
            existing_asset_id(state, &path_string)?,
        )?;
        assets::upsert(state, &registered, "rig-render")?;
        let relative = path.strip_prefix(workspace).unwrap_or(&path);
        frame_paths.push(relative.to_string_lossy().into_owned());
        asset_ids.push(registered.id.clone());
        animation_frames.push(AnimationFrame {
            asset_id: registered.id,
            duration_ms: None,
            offset_x: 0,
            offset_y: 0,
        });
    }
    let source_relative = std::path::Path::new(&asset.path)
        .strip_prefix(workspace)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| asset.relative_path.clone());
    let manifest = GenerationManifest {
        kind: Some("sprite".to_string()),
        name: render_name.clone(),
        category: category.clone(),
        fps: rig.fps,
        files: frame_paths.clone(),
        generated_at: now.clone(),
        rig: None,
        rig_id: Some(rig.id.clone()),
        source: Some(source_relative),
        quality: None,
        direction_family: None,
        facing: None,
        mirrored_from: None,
        anchor_slug: None,
    };
    std::fs::write(
        workspace
            .join(".sprite-studio")
            .join("last-generation.json"),
        serde_json::to_vec_pretty(&manifest)
            .map_err(|error| CommandError::new("serialization_error", error.to_string()))?,
    )?;
    let animation = save_animation_inner(
        crate::models::AnimationInput {
            id: None,
            workspace_id: asset.workspace_id.clone(),
            worktree_id: rig.worktree_id.clone(),
            name: render_name,
            fps: rig.fps,
            looping: rig.looping,
            frames: animation_frames,
            motion_plan: None,
            review_status: Some("draft".to_string()),
        },
        state,
    )?;
    Ok(RigRenderResult {
        animation,
        frame_paths,
        asset_ids,
        rig_id: rig.id.clone(),
    })
}

#[tauri::command]
pub async fn render_rig_animation(
    input: RigInput,
    state: State<'_, AppState>,
) -> CommandResult<RigRenderResult> {
    let rig = rig_input_to_rig(input, &state)?;
    let asset_id = rig.asset_id.clone().ok_or_else(|| {
        CommandError::new("missing_master", "Choose a source sprite before rendering")
    })?;
    let asset = assets::get_asset(&state, &asset_id)?;
    let workspace = workspace_path(&state, &rig.workspace_id)?;
    if rig.bones.is_empty() {
        return Err(CommandError::new(
            "empty_rig",
            "Add at least one bone before rendering",
        ));
    }
    validate_perceptible_rig_motion(&rig)?;
    let master_path = asset.path.clone();
    let render_rig = rig.clone();
    let frames = tauri::async_runtime::spawn_blocking(move || {
        render_rig_frames_blocking(master_path, render_rig)
    })
    .await
    .map_err(|error| CommandError::new("render_failed", error.to_string()))??;
    render_rig_animation_inner(&frames, &rig, &asset, &workspace, &state)
}
