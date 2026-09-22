use crate::{
    assets::{inspect, upsert},
    error::{CommandError, CommandResult},
    models::{AnimationFrame, BackgroundJob, ProceduralVfxInput, VfxEffect},
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use rusqlite::params;
use tauri::State;
use uuid::Uuid;

use super::persist::{cancellation_requested, emit_job, load_job, set_job_state, JobProgress};
use super::spritesheet::portable_slug;
use super::vfx_draw::{render_vfx_frame, select_vfx, validate_vfx_input, vfx_row};

struct PartialVfxGuard<'a> {
    state: &'a AppState,
    output_directory: std::path::PathBuf,
    generated_assets: Vec<crate::models::Asset>,
    committed: bool,
}

impl Drop for PartialVfxGuard<'_> {
    fn drop(&mut self) {
        if !self.committed {
            rollback_partial_vfx(
                self.state,
                &self.output_directory,
                &self.generated_assets,
            );
        }
    }
}

fn rollback_partial_vfx(
    state: &AppState,
    output_directory: &std::path::Path,
    generated_assets: &[crate::models::Asset],
) {
    if let Ok(connection) = state.db.lock() {
        for asset in generated_assets {
            let _ = connection.execute(
                "DELETE FROM asset_worktrees WHERE asset_id=?1",
                rusqlite::params![asset.id],
            );
            let _ = connection.execute("DELETE FROM assets WHERE id=?1", rusqlite::params![asset.id]);
            if std::path::Path::new(&asset.path).is_file() {
                let _ = std::fs::remove_file(&asset.path);
            }
        }
    }
    if output_directory.exists() {
        let _ = std::fs::remove_dir_all(output_directory);
    }
}

fn render_procedural_vfx(
    app: Option<&tauri::AppHandle>,
    state: &AppState,
    job_id: &str,
    input: ProceduralVfxInput,
) -> CommandResult<VfxEffect> {
    validate_vfx_input(&input)?;
    let valid_worktree: bool = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM worktrees WHERE id=?1 AND project_id=?2 AND kind='vfx')",
            params![input.worktree_id, input.project_id],
            |row| row.get(0),
        )?
    };
    if !valid_worktree {
        return Err(CommandError::new(
            "invalid_vfx_worktree",
            "Procedural effects must be created inside a VFX worktree",
        ));
    }
    let effect_id = Uuid::new_v4().to_string();
    let animation_id = Uuid::new_v4().to_string();
    let root = workspace_path(state, &input.project_id)?;
    let slug = portable_slug(input.name.trim());
    let output_directory =
        root.join("assets")
            .join("vfx")
            .join(format!("{}-{}", slug, &effect_id[..8]));
    std::fs::create_dir_all(&output_directory)?;
    crate::allow_asset_directory(app, &output_directory, true)?;
    let mut partial_guard = PartialVfxGuard {
        state,
        output_directory: output_directory.clone(),
        generated_assets: Vec::with_capacity(input.frames as usize),
        committed: false,
    };
    set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "running",
            progress: 0.04,
            stage: "Preparing procedural renderer",
            error_message: None,
            result_path: None,
        },
    )?;
    for frame_index in 0..input.frames {
        if cancellation_requested(state, job_id)? {
            return Err(CommandError::new(
                "job_cancelled",
                "Procedural VFX generation cancelled",
            ));
        }
        let frame = render_vfx_frame(&input, frame_index);
        let path =
            output_directory.join(format!("{}_{:02}.png", slug, frame_index.saturating_add(1)));
        frame.save(&path)?;
        let asset = inspect(&input.project_id, &root, &path, None)?;
        upsert(state, &asset, "procedural_vfx")?;
        {
            let connection = state
                .db
                .lock()
                .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
            connection.execute(
                "INSERT OR REPLACE INTO asset_worktrees(asset_id, worktree_id, relationship, created_at) VALUES (?1,?2,'owned',?3)",
                params![asset.id, input.worktree_id, Utc::now().to_rfc3339()],
            )?;
        }
        partial_guard.generated_assets.push(asset);
        set_job_state(
            app,
            state,
            job_id,
            JobProgress {
                status: "running",
                progress: 0.08 + 0.76 * ((frame_index + 1) as f64 / input.frames as f64),
                stage: &format!(
                    "Rendering effect frame {} of {}",
                    frame_index + 1,
                    input.frames
                ),
                error_message: None,
                result_path: None,
            },
        )?;
    }
    set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "analyzing",
            progress: 0.90,
            stage: "Validating alpha and frame dimensions",
            error_message: None,
            result_path: None,
        },
    )?;
    for asset in &partial_guard.generated_assets {
        let frame = image::open(&asset.path)?.to_rgba8();
        if frame.width() != input.width || frame.height() != input.height {
            return Err(CommandError::new(
                "vfx_validation_failed",
                "A procedural frame did not match the requested dimensions",
            ));
        }
        if !frame.pixels().any(|pixel| pixel[3] == 0) {
            return Err(CommandError::new(
                "vfx_validation_failed",
                "A procedural frame lost its transparent background",
            ));
        }
    }
    let now = Utc::now().to_rfc3339();
    let duration_ms = (1000.0 / input.fps as f64).round() as u32;
    let animation_frames: Vec<_> = partial_guard.generated_assets
        .iter()
        .map(|asset| AnimationFrame {
            asset_id: asset.id.clone(),
            duration_ms: Some(duration_ms.max(1)),
            offset_x: 0,
            offset_y: 0,
        })
        .collect();
    let frames_json = serde_json::to_string(&animation_frames)
        .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
    let effect = VfxEffect {
        id: effect_id,
        project_id: input.project_id,
        worktree_id: input.worktree_id,
        animation_id: Some(animation_id.clone()),
        name: input.name.trim().to_string(),
        effect_type: input.effect_type,
        blend_mode: input.blend_mode,
        center_x: 0.5,
        center_y: 0.5,
        opacity: 1.0,
        looping: input.looping,
        fps: input.fps as f64,
        created_at: now.clone(),
        updated_at: now,
    };
    {
        let mut connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let transaction = connection.transaction()?;
        transaction.execute(
            r#"INSERT INTO animations(
                id, workspace_id, worktree_id, name, fps, looping, frames_json, review_status, created_at, updated_at
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,'draft',?8,?8)"#,
            params![
                animation_id,
                effect.project_id,
                effect.worktree_id,
                effect.name,
                effect.fps,
                effect.looping,
                frames_json,
                effect.created_at
            ],
        )?;
        for (position, frame) in animation_frames.iter().enumerate() {
            transaction.execute(
                r#"INSERT INTO animation_frames(
                    id, animation_id, asset_id, position, duration_ms, pivot_x, pivot_y, created_at
                ) VALUES (?1,?2,?3,?4,?5,0.5,0.5,?6)"#,
                params![
                    Uuid::new_v4().to_string(),
                    animation_id,
                    frame.asset_id,
                    position as u32,
                    frame.duration_ms,
                    effect.created_at
                ],
            )?;
        }
        transaction.execute(
            r#"INSERT INTO vfx_effects(
                id, project_id, worktree_id, animation_id, name, effect_type,
                blend_mode, center_x, center_y, opacity, looping, fps, created_at, updated_at
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)"#,
            params![
                effect.id,
                effect.project_id,
                effect.worktree_id,
                effect.animation_id,
                effect.name,
                effect.effect_type,
                effect.blend_mode,
                effect.center_x,
                effect.center_y,
                effect.opacity,
                effect.looping,
                effect.fps,
                effect.created_at,
                effect.updated_at
            ],
        )?;
        transaction.execute(
            "UPDATE background_jobs SET target_type='vfx', target_id=?2 WHERE id=?1",
            params![job_id, effect.id],
        )?;
        transaction.commit()?;
    }
    let result_path = partial_guard
        .generated_assets
        .first()
        .map(|asset| asset.path.as_str());
    partial_guard.committed = true;
    set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "analyzing",
            progress: 0.97,
            stage: "Registering animation and effect",
            error_message: None,
            result_path,
        },
    )?;
    Ok(effect)
}
#[tauri::command]
pub fn list_vfx_effects(
    project_id: String,
    worktree_id: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<VfxEffect>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut effects = Vec::new();
    if let Some(worktree_id) = worktree_id {
        let mut statement = connection.prepare(&format!(
            "{} WHERE project_id=?1 AND worktree_id=?2 ORDER BY updated_at DESC",
            select_vfx()
        ))?;
        let rows = statement.query_map(params![project_id, worktree_id], vfx_row)?;
        effects.extend(rows.filter_map(Result::ok));
    } else {
        let mut statement = connection.prepare(&format!(
            "{} WHERE project_id=?1 ORDER BY updated_at DESC",
            select_vfx()
        ))?;
        let rows = statement.query_map([project_id], vfx_row)?;
        effects.extend(rows.filter_map(Result::ok));
    }
    Ok(effects)
}

#[tauri::command]
pub fn queue_procedural_vfx(
    input: ProceduralVfxInput,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<BackgroundJob> {
    queue_procedural_vfx_inner(input, Some(app), &state)
}

pub(crate) fn queue_procedural_vfx_inner(
    input: ProceduralVfxInput,
    app: Option<tauri::AppHandle>,
    state: &AppState,
) -> CommandResult<BackgroundJob> {
    validate_vfx_input(&input)?;
    let valid_worktree: bool = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM worktrees WHERE id=?1 AND project_id=?2 AND kind='vfx')",
            params![input.worktree_id, input.project_id],
            |row| row.get(0),
        )?
    };
    if !valid_worktree {
        return Err(CommandError::new(
            "invalid_vfx_worktree",
            "Choose a VFX worktree before creating a procedural effect",
        ));
    }
    let job_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.execute(
            r#"INSERT INTO background_jobs(
                id, project_id, worktree_id, kind, target_type, status,
                progress, stage, created_at, updated_at
            ) VALUES (?1,?2,?3,'procedural_vfx','vfx','queued',0.0,'Queued',?4,?4)"#,
            params![job_id, input.project_id, input.worktree_id, now],
        )?;
    }
    let queued = emit_job(app.as_ref(), state, &job_id)?;
    let task_app = app.clone();
    let task_state = state.clone();
    let task_job_id = job_id.clone();
    tauri::async_runtime::spawn(async move {
        let render_app = task_app.clone();
        let render_state = task_state.clone();
        let render_job_id = task_job_id.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            render_procedural_vfx(render_app.as_ref(), &render_state, &render_job_id, input)
        })
        .await;
        match result {
            Ok(Ok(_)) => {
                let completed = load_job(&task_state, &task_job_id).ok();
                let result_path = completed
                    .as_ref()
                    .and_then(|job| job.result_path.as_deref());
                let _ = set_job_state(
                    task_app.as_ref(),
                    &task_state,
                    &task_job_id,
                    JobProgress {
                        status: "completed",
                        progress: 1.0,
                        stage: "Completed",
                        error_message: None,
                        result_path,
                    },
                );
            }
            Ok(Err(error)) if error.code == "job_cancelled" => {
                let _ = set_job_state(
                    task_app.as_ref(),
                    &task_state,
                    &task_job_id,
                    JobProgress {
                        status: "cancelled",
                        progress: 0.0,
                        stage: "Cancelled",
                        error_message: None,
                        result_path: None,
                    },
                );
            }
            Ok(Err(error)) => {
                let _ = set_job_state(
                    task_app.as_ref(),
                    &task_state,
                    &task_job_id,
                    JobProgress {
                        status: "failed",
                        progress: 0.0,
                        stage: "Failed",
                        error_message: Some(&error.message),
                        result_path: None,
                    },
                );
            }
            Err(error) => {
                let message = error.to_string();
                let _ = set_job_state(
                    task_app.as_ref(),
                    &task_state,
                    &task_job_id,
                    JobProgress {
                        status: "failed",
                        progress: 0.0,
                        stage: "Failed",
                        error_message: Some(&message),
                        result_path: None,
                    },
                );
            }
        }
    });
    Ok(queued)
}
