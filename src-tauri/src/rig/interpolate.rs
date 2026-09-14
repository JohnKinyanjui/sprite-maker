use crate::{
    animations::{load_animation_by_id, save_animation_inner},
    assets::{self, extract_palette, inspect, normalize_sprite_alpha, upsert},
    error::{CommandError, CommandResult},
    jobs::{set_job_state, JobProgress},
    models::{AnimationFrame, AnimationInput},
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use image::RgbaImage;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use uuid::Uuid;

use super::commands::rig_input_to_rig;
use super::render::render_rig_frames_blocking;
use super::types::{Rig, RigFrame, RigInput, RigTransform};

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn lerp_transform(left: &RigTransform, right: &RigTransform, t: f64) -> RigTransform {
    RigTransform {
        bone: left.bone.clone(),
        dx: lerp(left.dx, right.dx, t),
        dy: lerp(left.dy, right.dy, t),
        rotate: lerp(left.rotate, right.rotate, t),
        scale_x: lerp(left.scale_x, right.scale_x, t),
        scale_y: lerp(left.scale_y, right.scale_y, t),
    }
}

fn interpolate_transforms(
    left: &[RigTransform],
    right: &[RigTransform],
    t: f64,
) -> Vec<RigTransform> {
    let mut merged = Vec::new();
    for transform in left {
        let counterpart = right.iter().find(|value| value.bone == transform.bone);
        merged.push(match counterpart {
            Some(right_transform) => lerp_transform(transform, right_transform, t),
            None => transform.clone(),
        });
    }
    for transform in right {
        if !merged.iter().any(|value| value.bone == transform.bone) {
            merged.push(transform.clone());
        }
    }
    merged
}

fn interpolate_contacts(
    left: &[super::types::RigContact],
    right: &[super::types::RigContact],
    t: f64,
) -> Vec<super::types::RigContact> {
    let mut merged = Vec::new();
    for contact in left {
        let counterpart = right.iter().find(|value| value.bone == contact.bone);
        merged.push(match counterpart {
            Some(right_contact) => super::types::RigContact {
                bone: contact.bone.clone(),
                x: lerp(contact.x, right_contact.x, t),
                y: lerp(contact.y, right_contact.y, t),
                bend: lerp(contact.bend, right_contact.bend, t),
            },
            None => contact.clone(),
        });
    }
    for contact in right {
        if !merged.iter().any(|value| value.bone == contact.bone) {
            merged.push(contact.clone());
        }
    }
    merged
}

pub fn interpolate_rig_frame_at(left: &RigFrame, right: &RigFrame, t: f64) -> RigFrame {
    RigFrame {
        phase: left.phase.clone().or_else(|| right.phase.clone()),
        hold: false,
        root_dx: lerp(left.root_dx, right.root_dx, t),
        root_dy: lerp(left.root_dy, right.root_dy, t),
        transforms: interpolate_transforms(&left.transforms, &right.transforms, t),
        contacts: interpolate_contacts(&left.contacts, &right.contacts, t),
    }
}

pub fn interpolate_rig_frame(left: &RigFrame, right: &RigFrame) -> RigFrame {
    interpolate_rig_frame_at(left, right, 0.5)
}

pub fn interpolate_rig_frames_between(
    left: &RigFrame,
    right: &RigFrame,
    steps: u32,
) -> Vec<RigFrame> {
    if steps == 0 {
        return Vec::new();
    }
    (1..=steps)
        .map(|index| {
            let t = index as f64 / (steps as f64 + 1.0);
            interpolate_rig_frame_at(left, right, t)
        })
        .collect()
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InterpolateRigFramesInput {
    pub rig: RigInput,
    pub from_index: usize,
    pub to_index: usize,
    pub steps: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterpolateRigFramesResult {
    pub frames: Vec<RigFrame>,
    pub preview_paths: Vec<String>,
    pub inserted_count: u32,
}

pub(crate) fn interpolate_rig_frames_inner(
    input: InterpolateRigFramesInput,
    state: &AppState,
) -> CommandResult<InterpolateRigFramesResult> {
    let rig = rig_input_to_rig(input.rig, state)?;
    if rig.frames.is_empty() {
        return Err(CommandError::new(
            "empty_rig",
            "Add pose frames before interpolating",
        ));
    }
    if input.from_index >= rig.frames.len() || input.to_index >= rig.frames.len() {
        return Err(CommandError::new(
            "invalid_frame_index",
            "Keyframe indices are out of range",
        ));
    }
    if input.from_index >= input.to_index {
        return Err(CommandError::new(
            "invalid_frame_order",
            "fromIndex must be less than toIndex",
        ));
    }
    let steps = input.steps.clamp(1, 16);
    let inserted = interpolate_rig_frames_between(
        &rig.frames[input.from_index],
        &rig.frames[input.to_index],
        steps,
    );
    let mut frames = rig.frames.clone();
    let insert_at = input.from_index + 1;
    frames.splice(insert_at..insert_at, inserted.clone());

    let preview_paths = render_interpolated_preview_paths(state, &rig, &inserted)?;

    Ok(InterpolateRigFramesResult {
        frames,
        preview_paths,
        inserted_count: inserted.len() as u32,
    })
}

fn render_interpolated_preview_paths(
    state: &AppState,
    rig: &Rig,
    inserted: &[RigFrame],
) -> CommandResult<Vec<String>> {
    if inserted.is_empty() {
        return Ok(Vec::new());
    }
    let asset_id = rig.asset_id.clone().ok_or_else(|| {
        CommandError::new("missing_master", "Choose a source sprite before interpolating")
    })?;
    let asset = assets::get_asset(state, &asset_id)?;
    let workspace = workspace_path(state, &rig.workspace_id)?;
    let master_path = asset.path.clone();
    let mut render_rig = rig.clone();
    render_rig.frames = inserted.to_vec();
    let rendered =
        render_rig_frames_blocking(master_path, render_rig)?.into_iter().collect::<Vec<RgbaImage>>();
    let directory = workspace
        .join(".sprite-studio")
        .join("rig-interpolate")
        .join(&rig.id);
    std::fs::create_dir_all(&directory)?;
    let mut paths = Vec::with_capacity(rendered.len());
    for (index, frame) in rendered.iter().enumerate() {
        let path = directory.join(format!("interp_{:02}.png", index + 1));
        frame.save(&path)?;
        paths.push(path.to_string_lossy().into_owned());
    }
    Ok(paths)
}

#[tauri::command]
pub fn interpolate_rig_frames(
    input: InterpolateRigFramesInput,
    state: State<'_, AppState>,
) -> CommandResult<InterpolateRigFramesResult> {
    interpolate_rig_frames_inner(input, &state)
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InterpolateRigAnimationFramesInput {
    pub rig: RigInput,
    pub from_index: usize,
    pub to_index: usize,
    pub steps: u32,
    #[serde(default)]
    pub animation_id: Option<String>,
    pub workspace_id: String,
    #[serde(default)]
    pub worktree_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterpolateRigAnimationFramesResult {
    pub animation_id: String,
    pub inserted_count: u32,
    pub asset_ids: Vec<String>,
    #[serde(default)]
    pub job_id: Option<String>,
}

pub(crate) fn interpolate_rig_animation_frames_inner(
    input: InterpolateRigAnimationFramesInput,
    app: Option<AppHandle>,
    state: &AppState,
) -> CommandResult<InterpolateRigAnimationFramesResult> {
    if input.steps == 0 {
        return Err(CommandError::new(
            "invalid_steps",
            "Provide at least one in-between step",
        ));
    }
    let animation_id = input.animation_id.clone().ok_or_else(|| {
        CommandError::new(
            "animation_required",
            "Provide animationId to insert rendered in-between frames",
        )
    })?;
    if input.steps > 8 {
        let job_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let animation = load_animation_by_id(state, &animation_id)?;
        {
            let connection = state
                .db
                .lock()
                .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
            connection.execute(
                r#"INSERT INTO background_jobs(
                    id,project_id,worktree_id,kind,target_type,target_id,status,
                    progress,stage,created_at,updated_at,started_at
                ) VALUES (?1,?2,?3,'rig_interpolate_render','animation',?4,'queued',0.0,'Queued',?5,?5,?5)"#,
                params![
                    job_id,
                    animation.workspace_id,
                    animation.worktree_id,
                    animation_id,
                    now
                ],
            )?;
        }
        let task_app = app.clone();
        let task_state = state.clone();
        let task_input = input.clone();
        let task_job_id = job_id.clone();
        tauri::async_runtime::spawn(async move {
            let result = run_interpolate_rig_animation_job(&task_input, &task_state);
            let (status, stage, progress, error) = match result {
                Ok(()) => ("completed", "Completed", 1.0, None),
                Err(error) => ("failed", "Failed", 0.0, Some(error.message)),
            };
            let _ = set_job_state(
                task_app.as_ref(),
                &task_state,
                &task_job_id,
                JobProgress {
                    status,
                    progress,
                    stage,
                    error_message: error.as_deref(),
                    result_path: None,
                },
            );
        });
        return Ok(InterpolateRigAnimationFramesResult {
            animation_id,
            inserted_count: 0,
            asset_ids: Vec::new(),
            job_id: Some(job_id),
        });
    }
    let result = run_interpolate_rig_animation_sync(&input, state)?;
    Ok(result)
}

fn run_interpolate_rig_animation_job(
    input: &InterpolateRigAnimationFramesInput,
    state: &AppState,
) -> CommandResult<()> {
    run_interpolate_rig_animation_sync(input, state)?;
    Ok(())
}

fn run_interpolate_rig_animation_sync(
    input: &InterpolateRigAnimationFramesInput,
    state: &AppState,
) -> CommandResult<InterpolateRigAnimationFramesResult> {
    let animation_id = input
        .animation_id
        .clone()
        .ok_or_else(|| CommandError::new("animation_required", "animationId is required"))?;
    let animation = load_animation_by_id(state, &animation_id)?;
    let rig = rig_input_to_rig(input.rig.clone(), state)?;
    if input.from_index >= rig.frames.len() || input.to_index >= rig.frames.len() {
        return Err(CommandError::new(
            "invalid_frame_index",
            "Keyframe indices are out of range",
        ));
    }
    if input.from_index >= input.to_index {
        return Err(CommandError::new(
            "invalid_frame_order",
            "fromIndex must be less than toIndex",
        ));
    }
    let steps = input.steps.clamp(1, 16);
    let inserted_frames = interpolate_rig_frames_between(
        &rig.frames[input.from_index],
        &rig.frames[input.to_index],
        steps,
    );
    let asset_id = rig.asset_id.clone().ok_or_else(|| {
        CommandError::new("missing_master", "Choose a source sprite before rendering in-betweens")
    })?;
    let master = assets::get_asset(state, &asset_id)?;
    let workspace = workspace_path(state, &animation.workspace_id)?;
    let master_image = image::open(&master.path)?.to_rgba8();
    let master_palette = extract_palette(&master_image, 64);
    let mut render_rig = rig.clone();
    render_rig.frames = inserted_frames.clone();
    let rendered =
        render_rig_frames_blocking(master.path.clone(), render_rig)?.into_iter().collect::<Vec<RgbaImage>>();
    let category = if master.category.is_empty() {
        "characters".to_string()
    } else {
        master.category.clone()
    };
    let output_directory = workspace.join("assets").join(&category);
    std::fs::create_dir_all(&output_directory)?;
    let mut asset_ids = Vec::new();
    let mut new_animation_frames = Vec::new();
    for (index, frame) in rendered.iter().enumerate() {
        let path = output_directory.join(format!(
            "{}-rig-between-{:02}.png",
            animation.name.replace(' ', "-").to_lowercase(),
            index + 1
        ));
        let normalized = normalize_sprite_alpha(frame, Some(&master_palette));
        normalized.save(&path)?;
        let registered = inspect(
            &animation.workspace_id,
            &workspace,
            &path,
            None,
        )?;
        upsert(state, &registered, "rig_interpolate_render")?;
        asset_ids.push(registered.id.clone());
        new_animation_frames.push(AnimationFrame {
            asset_id: registered.id,
            duration_ms: None,
            offset_x: 0,
            offset_y: 0,
        });
    }
    let mut frames = animation.frames.clone();
    let insert_at = input.from_index + 1;
    frames.splice(insert_at..insert_at, new_animation_frames);
    save_animation_inner(
        AnimationInput {
            id: Some(animation_id.clone()),
            workspace_id: animation.workspace_id.clone(),
            worktree_id: animation.worktree_id.clone(),
            name: animation.name.clone(),
            fps: animation.fps,
            looping: animation.looping,
            frames,
            motion_plan: None,
            review_status: Some(animation.review_status.clone()),
        },
        state,
    )?;
    Ok(InterpolateRigAnimationFramesResult {
        animation_id,
        inserted_count: asset_ids.len() as u32,
        asset_ids,
        job_id: None,
    })
}

#[tauri::command]
pub fn interpolate_rig_animation_frames(
    input: InterpolateRigAnimationFramesInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<InterpolateRigAnimationFramesResult> {
    interpolate_rig_animation_frames_inner(input, Some(app), &state)
}
