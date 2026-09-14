use crate::{
    assets,
    error::{CommandError, CommandResult},
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use image::RgbaImage;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use std::path::Path;
use tauri::State;
use uuid::Uuid;

use super::fit::{capsule_coverage, detect_morphology, shape_features, RigFitReport};
use super::suggest::suggest_points;
use super::suggestion::parse_rig_suggestion_text;
use super::types::{normalize_morphology, Rig, RigInput, RigSpec, RigSuggestion};
use super::validate::validate_rig;

pub(crate) fn capture_chat_suggestion(
    state: &AppState,
    workspace_id: &str,
    worktree_id: Option<&str>,
    response: &str,
) -> CommandResult<Option<String>> {
    let Some(suggestion) = parse_rig_suggestion_text(response, u32::MAX, u32::MAX) else {
        return Ok(None);
    };
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    let name = format!("AI rig — {}", suggestion.morphology);
    let spec = RigSpec {
        points: suggestion.points.clone(),
        bones: suggestion.bones.clone(),
        frames: suggestion.frames.clone(),
    };
    let spec_json = serde_json::to_string(&spec)
        .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        r#"INSERT INTO rigs(id, workspace_id, worktree_id, asset_id, name, morphology, fps, looping, spec_json, created_at, updated_at)
           VALUES (?1, ?2, ?3, NULL, ?4, ?5, 8, 1, ?6, ?7, ?7)"#,
        params![id, workspace_id, worktree_id, name, suggestion.morphology, spec_json, now],
    )?;
    Ok(Some(name))
}

pub(crate) fn analyze_rig_fit_inner(
    state: &AppState,
    asset_id: &str,
) -> CommandResult<RigFitReport> {
    let (_, master) = load_master_asset(state, asset_id)?;
    let detections = detect_morphology(&master)?;
    let best = detections.first().ok_or_else(|| {
        CommandError::new("morphology_failed", "The sprite could not be profiled")
    })?;
    let suggestion = suggest_points(&master, &best.morphology)?;
    let capsule_fit = capsule_coverage(&master, &suggestion);
    let mut warnings = Vec::new();
    if best.confidence < 0.45 {
        warnings.push(format!(
            "No rig profile fits this sprite well (best: {} at {}%). A cleaner master with a readable silhouette and separated limbs will rig and animate better.",
            best.morphology,
            (best.confidence * 100.0).round() as u32
        ));
    }
    let needs_legs = matches!(best.morphology.as_str(), "biped" | "quadruped");
    if needs_legs {
        if let Some(features) = shape_features(&master) {
            if !features.lower_separated {
                warnings.push(
                    "The legs never separate in this silhouette, so leg poses will read poorly. Regenerate the master in a wide stance, or widen the leg capsules after applying."
                        .to_string(),
                );
            }
        }
    }
    if capsule_fit < 0.5 {
        warnings.push(format!(
            "Template capsules claim only {}% of the silhouette directly; fine-tune the bone radii after applying.",
            (capsule_fit * 100.0).round() as u32
        ));
    }
    Ok(RigFitReport {
        detections,
        recommended: suggestion,
        capsule_fit,
        warnings,
    })
}

#[tauri::command]
pub fn analyze_rig_fit(
    asset_id: String,
    state: State<'_, AppState>,
) -> CommandResult<RigFitReport> {
    analyze_rig_fit_inner(&state, &asset_id)
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

const RIG_SELECT: &str =
    "SELECT id, workspace_id, worktree_id, asset_id, name, morphology, fps, looping, spec_json, created_at, updated_at FROM rigs";

pub(crate) fn load_rig_by_id(state: &AppState, rig_id: &str) -> CommandResult<Option<Rig>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection
        .query_row(&format!("{RIG_SELECT} WHERE id=?1"), [rig_id], rig_row)
        .optional()
        .map_err(Into::into)
}

fn rig_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Rig> {
    let spec_json: String = row.get(8)?;
    let spec: RigSpec = serde_json::from_str(&spec_json).unwrap_or_default();
    Ok(Rig {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        worktree_id: row.get(2)?,
        asset_id: row.get(3)?,
        name: row.get(4)?,
        morphology: row.get(5)?,
        fps: row.get(6)?,
        looping: row.get(7)?,
        points: spec.points,
        bones: spec.bones,
        frames: spec.frames,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

#[tauri::command]
pub fn list_rigs(
    workspace_id: String,
    worktree_id: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Rig>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut statement = if worktree_id.is_some() {
        connection.prepare(&format!(
            "{RIG_SELECT} WHERE workspace_id = ?1 AND worktree_id = ?2 ORDER BY updated_at DESC"
        ))?
    } else {
        connection.prepare(&format!(
            "{RIG_SELECT} WHERE workspace_id = ?1 ORDER BY updated_at DESC"
        ))?
    };
    let rows = if let Some(value) = worktree_id.as_ref() {
        statement.query_map(params![workspace_id, value], rig_row)?
    } else {
        statement.query_map([&workspace_id], rig_row)?
    };
    Ok(rows.filter_map(Result::ok).collect())
}

pub(super) fn rig_input_to_rig(input: RigInput, state: &AppState) -> CommandResult<Rig> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(CommandError::new(
            "invalid_name",
            "Rig name cannot be empty",
        ));
    }
    if !(1.0..=60.0).contains(&input.fps) {
        return Err(CommandError::new(
            "invalid_fps",
            "Rig FPS must be between 1 and 60",
        ));
    }
    let now = Utc::now().to_rfc3339();
    let id = input.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let existing_created_at: Option<String> = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row("SELECT created_at FROM rigs WHERE id = ?1", [&id], |row| {
                row.get(0)
            })
            .optional()?
    };
    Ok(Rig {
        id,
        workspace_id: input.workspace_id,
        worktree_id: input.worktree_id,
        asset_id: input.asset_id,
        name: name.to_string(),
        morphology: normalize_morphology(Some(&input.morphology)),
        fps: input.fps,
        looping: input.looping,
        points: input.points,
        bones: input.bones,
        frames: input.frames,
        created_at: existing_created_at.unwrap_or_else(|| now.clone()),
        updated_at: now,
    })
}

#[tauri::command]
pub fn save_rig(input: RigInput, state: State<'_, AppState>) -> CommandResult<Rig> {
    save_rig_inner(input, &state)
}

pub(crate) fn render_rig_animation_blocking(
    input: RigInput,
    state: &AppState,
) -> CommandResult<super::types::RigRenderResult> {
    use super::render::{render_rig_animation_inner, render_rig_frames_blocking};
    use super::validate::validate_perceptible_rig_motion;

    let rig = rig_input_to_rig(input, state)?;
    let asset_id = rig.asset_id.clone().ok_or_else(|| {
        CommandError::new("missing_master", "Choose a source sprite before rendering")
    })?;
    let asset = assets::get_asset(state, &asset_id)?;
    let workspace = workspace_path(state, &rig.workspace_id)?;
    if rig.bones.is_empty() {
        return Err(CommandError::new(
            "empty_rig",
            "Add at least one bone before rendering",
        ));
    }
    validate_perceptible_rig_motion(&rig)?;
    let master_path = asset.path.clone();
    let render_rig = rig.clone();
    let frames = tauri::async_runtime::block_on(async move {
        tauri::async_runtime::spawn_blocking(move || render_rig_frames_blocking(master_path, render_rig))
            .await
    })
    .map_err(|error| CommandError::new("render_failed", error.to_string()))??;
    render_rig_animation_inner(&frames, &rig, &asset, &workspace, state)
}

pub(crate) fn save_rig_inner(input: RigInput, state: &AppState) -> CommandResult<Rig> {
    let rig = rig_input_to_rig(input, state)?;
    let spec = RigSpec {
        points: rig.points.clone(),
        bones: rig.bones.clone(),
        frames: rig.frames.clone(),
    };
    let spec_json = serde_json::to_string(&spec)
        .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        r#"INSERT INTO rigs(id, workspace_id, worktree_id, asset_id, name, morphology, fps, looping, spec_json, created_at, updated_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
           ON CONFLICT(id) DO UPDATE SET worktree_id=excluded.worktree_id, asset_id=excluded.asset_id,
             name=excluded.name, morphology=excluded.morphology, fps=excluded.fps, looping=excluded.looping,
             spec_json=excluded.spec_json, updated_at=excluded.updated_at"#,
        params![
            rig.id,
            rig.workspace_id,
            rig.worktree_id,
            rig.asset_id,
            rig.name,
            rig.morphology,
            rig.fps,
            rig.looping,
            spec_json,
            rig.created_at,
            rig.updated_at
        ],
    )?;
    Ok(rig)
}

#[tauri::command]
pub fn validate_rig_spec(
    input: RigInput,
    state: State<'_, AppState>,
) -> CommandResult<Vec<String>> {
    let rig = rig_input_to_rig(input, &state)?;
    let asset_id = rig
        .asset_id
        .clone()
        .ok_or_else(|| CommandError::new("missing_master", "Choose a source sprite first"))?;
    let asset = assets::get_asset(&state, &asset_id)?;
    validate_rig(&rig, asset.width, asset.height)
}

#[tauri::command]
pub fn delete_rig(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute("DELETE FROM rigs WHERE id = ?1", [id])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Suggestions and rendering commands
// ---------------------------------------------------------------------------

fn load_master_asset(
    state: &AppState,
    asset_id: &str,
) -> CommandResult<(crate::models::Asset, RgbaImage)> {
    let asset = assets::get_asset(state, asset_id)?;
    let image = image::open(&asset.path)?.to_rgba8();
    if image.width() < 8 || image.height() < 8 || image.width() > 512 || image.height() > 512 {
        return Err(CommandError::new(
            "invalid_master",
            "Rig masters must be between 8×8 and 512×512 pixels",
        ));
    }
    Ok((asset, image))
}

pub(crate) fn suggest_rig_points_inner(
    state: &AppState,
    asset_id: &str,
    morphology: Option<&str>,
) -> CommandResult<RigSuggestion> {
    let (_, master) = load_master_asset(state, asset_id)?;
    suggest_points(&master, &normalize_morphology(morphology))
}

#[tauri::command]
pub fn suggest_rig_points(
    asset_id: String,
    morphology: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<RigSuggestion> {
    suggest_rig_points_inner(&state, &asset_id, morphology.as_deref())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRigSuggestionInput {
    pub asset_id: String,
    pub morphology: Option<String>,
    pub motion: Option<String>,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
}

#[tauri::command]
pub async fn ai_suggest_rig_points(
    input: AiRigSuggestionInput,
    state: State<'_, AppState>,
) -> CommandResult<RigSuggestion> {
    let (asset, master) = load_master_asset(&state, &input.asset_id)?;
    let workspace = workspace_path(&state, &asset.workspace_id)?;
    let provider = input.provider_id.unwrap_or_else(|| "codex".to_string());
    let morphology = normalize_morphology(input.morphology.as_deref());
    let motion_intent = input.motion.as_deref().unwrap_or("").trim().to_string();
    let mut prompt = crate::sprite_harness::rig_suggestion_prompt(
        &motion_intent,
        &morphology,
        master.width(),
        master.height(),
    );
    let mut image_paths: Vec<String> = Vec::new();
    if provider == "codex" {
        image_paths.push(asset.path.clone());
    } else {
        // Non-Codex CLIs read images from the workspace by path.
        let relative = Path::new(&asset.path)
            .strip_prefix(&workspace)
            .unwrap_or(Path::new(&asset.path));
        prompt.push_str(&format!(
            "\n\nATTACHED REFERENCE FILE\nInspect this local file before answering:\n- `{}`",
            relative.to_string_lossy()
        ));
    }
    let response = crate::providers::run_agent_text_request(
        &provider,
        input.model.as_deref(),
        input.reasoning_effort.as_deref(),
        &workspace,
        &prompt,
        &image_paths,
    )
    .await?;
    parse_rig_suggestion_text(&response, master.width(), master.height()).ok_or_else(|| {
        CommandError::new(
            "rig_suggestion_missing",
            "The provider replied without a rig-suggestion JSON block. Retry, or use Auto-suggest and drag the points into place.",
        )
    })
}
