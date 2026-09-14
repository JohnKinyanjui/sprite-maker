use crate::{
    assets::get_asset,
    error::{CommandError, CommandResult},
    models::{Animation, AnimationFrame, AnimationInput, MotionPhase, MotionPlan},
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use tauri::State;
use uuid::Uuid;

type StoredMotionPlan = (String, String, u32, u32, u32, u32, bool, bool, bool, String);

pub(crate) fn load_animation_frames_from_table(
    connection: &rusqlite::Connection,
    animation_id: &str,
) -> CommandResult<Vec<AnimationFrame>> {
    let mut statement = connection.prepare(
        r#"SELECT asset_id, duration_ms, offset_x, offset_y
           FROM animation_frames
           WHERE animation_id=?1
           ORDER BY position"#,
    )?;
    let frames = statement
        .query_map([animation_id], |row| {
            Ok(AnimationFrame {
                asset_id: row.get(0)?,
                duration_ms: row.get(1)?,
                offset_x: row.get(2)?,
                offset_y: row.get(3)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(frames)
}

pub(crate) fn resolve_animation_frames(
    connection: &rusqlite::Connection,
    animation_id: &str,
    frames_json: &str,
) -> CommandResult<Vec<AnimationFrame>> {
    match serde_json::from_str::<Vec<AnimationFrame>>(frames_json) {
        Ok(frames) if !frames.is_empty() => Ok(frames),
        Ok(_) => {
            let table_frames = load_animation_frames_from_table(connection, animation_id)?;
            Ok(table_frames)
        }
        Err(error) => {
            let table_frames = load_animation_frames_from_table(connection, animation_id)?;
            if !table_frames.is_empty() {
                Ok(table_frames)
            } else {
                Err(CommandError::new(
                    "invalid_frames_json",
                    format!("Animation frames are corrupted: {error}"),
                ))
            }
        }
    }
}

fn hydrate_animation_frames(
    connection: &rusqlite::Connection,
    animations: &mut [Animation],
) -> CommandResult<()> {
    for animation in animations {
        if animation.frames.is_empty() {
            animation.frames = load_animation_frames_from_table(connection, &animation.id)?;
        }
    }
    Ok(())
}

fn animation_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Animation> {
    let frames: String = row.get(6)?;
    Ok(Animation {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        worktree_id: row.get(2)?,
        name: row.get(3)?,
        fps: row.get(4)?,
        looping: row.get(5)?,
        frames: serde_json::from_str::<Vec<AnimationFrame>>(&frames).unwrap_or_default(),
        motion_plan: None,
        review_status: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn load_motion_plan(
    connection: &rusqlite::Connection,
    animation_id: &str,
) -> CommandResult<Option<MotionPlan>> {
    let value: Option<StoredMotionPlan> = connection
        .query_row(
            r#"SELECT id, frame_mode, selected_frame_count, minimum_frame_count,
                      maximum_frame_count, fps, looping, allow_interpolation,
                      allow_auto_adjust, explanation
               FROM animation_plans WHERE animation_id=?1"#,
            [animation_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            },
        )
        .optional()?;
    let Some((
        plan_id,
        frame_mode,
        selected,
        minimum,
        maximum,
        fps,
        looping,
        interpolation,
        auto_adjust,
        explanation,
    )) = value
    else {
        return Ok(None);
    };
    let mut statement = connection.prepare(
        r#"SELECT name, description, frame_count, timing_weight
           FROM animation_plan_phases WHERE plan_id=?1 ORDER BY position"#,
    )?;
    let phases = statement
        .query_map([plan_id], |row| {
            Ok(MotionPhase {
                name: row.get(0)?,
                description: row.get(1)?,
                frame_count: row.get(2)?,
                timing_weight: row.get(3)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(Some(MotionPlan {
        frame_mode,
        selected_frame_count: selected,
        minimum_frame_count: minimum,
        maximum_frame_count: maximum,
        fps,
        looping,
        allow_interpolation: interpolation,
        allow_auto_adjust: auto_adjust,
        explanation,
        phases,
    }))
}

fn hydrate_motion_plans(
    connection: &rusqlite::Connection,
    animations: &mut [Animation],
) -> CommandResult<()> {
    for animation in animations {
        animation.motion_plan = load_motion_plan(connection, &animation.id)?;
    }
    Ok(())
}

pub(crate) fn load_animation_by_id(
    state: &AppState,
    animation_id: &str,
) -> CommandResult<Animation> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut animation = connection
        .query_row(
            "SELECT id, workspace_id, worktree_id, name, fps, looping, frames_json, review_status, created_at, updated_at FROM animations WHERE id=?1",
            [animation_id],
            animation_row,
        )
        .optional()?
        .ok_or_else(|| CommandError::new("animation_not_found", "The animation no longer exists"))?;
    animation.motion_plan = load_motion_plan(&connection, animation_id)?;
    if animation.frames.is_empty() {
        animation.frames = load_animation_frames_from_table(&connection, animation_id)?;
    }
    Ok(animation)
}

#[tauri::command]
pub fn list_animations(
    workspace_id: String,
    worktree_id: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Animation>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let select = "SELECT id, workspace_id, worktree_id, name, fps, looping, frames_json, review_status, created_at, updated_at FROM animations";
    let mut animations: Vec<Animation> = if let Some(worktree_id) = worktree_id {
        let mut statement = connection.prepare(&format!(
            "{select} WHERE workspace_id = ?1 AND worktree_id = ?2 ORDER BY updated_at DESC"
        ))?;
        let rows = statement.query_map(params![workspace_id, worktree_id], animation_row)?;
        rows.filter_map(Result::ok).collect()
    } else {
        let mut statement = connection.prepare(&format!(
            "{select} WHERE workspace_id = ?1 ORDER BY updated_at DESC"
        ))?;
        let rows = statement.query_map([workspace_id], animation_row)?;
        rows.filter_map(Result::ok).collect()
    };
    hydrate_motion_plans(&connection, &mut animations)?;
    hydrate_animation_frames(&connection, &mut animations)?;
    Ok(animations)
}

#[tauri::command]
pub fn save_animation(
    input: AnimationInput,
    state: State<'_, AppState>,
) -> CommandResult<Animation> {
    save_animation_inner(input, &state)
}

pub(crate) fn save_animation_inner(
    input: AnimationInput,
    state: &AppState,
) -> CommandResult<Animation> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(CommandError::new(
            "invalid_name",
            "Animation name cannot be empty",
        ));
    }
    if !(1.0..=60.0).contains(&input.fps) {
        return Err(CommandError::new(
            "invalid_fps",
            "Animation FPS must be between 1 and 60",
        ));
    }
    for frame in &input.frames {
        let asset = get_asset(state, &frame.asset_id)?;
        if asset.workspace_id != input.workspace_id {
            return Err(CommandError::new(
                "invalid_animation_frame",
                "Animation frames must belong to the same project",
            ));
        }
    }
    let now = Utc::now().to_rfc3339();
    let id = input.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let (existing_created_at, existing_motion_plan, existing_review_status): (
        Option<String>,
        Option<MotionPlan>,
        Option<String>,
    ) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let created_at = connection
            .query_row(
                "SELECT created_at FROM animations WHERE id = ?1",
                [&id],
                |row| row.get(0),
            )
            .optional()?;
        let review_status = connection
            .query_row(
                "SELECT review_status FROM animations WHERE id = ?1",
                [&id],
                |row| row.get(0),
            )
            .optional()?;
        let motion_plan = load_motion_plan(&connection, &id)?;
        (created_at, motion_plan, review_status)
    };
    let review_status = input
        .review_status
        .or(existing_review_status)
        .unwrap_or_else(|| "draft".to_string());
    let animation = Animation {
        id,
        workspace_id: input.workspace_id,
        worktree_id: input.worktree_id,
        name: name.to_string(),
        fps: input.fps,
        looping: input.looping,
        frames: input.frames,
        motion_plan: input.motion_plan.or(existing_motion_plan),
        review_status,
        created_at: existing_created_at.unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    let frames_json = serde_json::to_string(&animation.frames)
        .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
    {
        let mut connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let transaction = connection.transaction()?;
        if let Some(worktree_id) = &animation.worktree_id {
            let valid: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM worktrees WHERE id = ?1 AND project_id = ?2)",
                params![worktree_id, animation.workspace_id],
                |row| row.get(0),
            )?;
            if !valid {
                return Err(CommandError::new(
                    "invalid_worktree",
                    "Animation worktree must belong to the same project",
                ));
            }
        }
        transaction.execute(
            r#"INSERT INTO animations(id, workspace_id, worktree_id, name, fps, looping, frames_json, review_status, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(id) DO UPDATE SET worktree_id=excluded.worktree_id, name=excluded.name, fps=excluded.fps, looping=excluded.looping, frames_json=excluded.frames_json, review_status=excluded.review_status, updated_at=excluded.updated_at"#,
            params![animation.id, animation.workspace_id, animation.worktree_id, animation.name, animation.fps, animation.looping, frames_json, animation.review_status, animation.created_at, animation.updated_at],
        )?;
        transaction.execute(
            r#"INSERT OR IGNORE INTO animation_revisions(
                id,animation_id,change_kind,summary,created_at
            ) VALUES (?1,?2,'created','Created in the animation editor',?3)"#,
            params![
                Uuid::new_v4().to_string(),
                animation.id,
                animation.created_at
            ],
        )?;
        transaction.execute(
            "DELETE FROM animation_frames WHERE animation_id = ?1",
            [&animation.id],
        )?;
        for (position, frame) in animation.frames.iter().enumerate() {
            transaction.execute(
                r#"INSERT INTO animation_frames(
                    id, animation_id, asset_id, position, duration_ms, offset_x, offset_y, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
                params![
                    Uuid::new_v4().to_string(),
                    animation.id,
                    frame.asset_id,
                    position as i64,
                    frame.duration_ms,
                    frame.offset_x,
                    frame.offset_y,
                    animation.updated_at
                ],
            )?;
        }
        if let Some(plan) = &animation.motion_plan {
            transaction.execute(
                "DELETE FROM animation_plans WHERE animation_id=?1",
                [&animation.id],
            )?;
            let plan_id = Uuid::new_v4().to_string();
            transaction.execute(
                r#"INSERT INTO animation_plans(
                    id, animation_id, frame_mode, selected_frame_count, minimum_frame_count,
                    maximum_frame_count, fps, looping, allow_interpolation, allow_auto_adjust,
                    explanation, created_at, updated_at
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"#,
                params![
                    plan_id,
                    animation.id,
                    plan.frame_mode,
                    plan.selected_frame_count,
                    plan.minimum_frame_count,
                    plan.maximum_frame_count,
                    plan.fps,
                    plan.looping,
                    plan.allow_interpolation,
                    plan.allow_auto_adjust,
                    plan.explanation,
                    animation.created_at,
                    animation.updated_at
                ],
            )?;
            for (position, phase) in plan.phases.iter().enumerate() {
                transaction.execute(
                    r#"INSERT INTO animation_plan_phases(
                        id, plan_id, position, name, description, frame_count, timing_weight
                    ) VALUES (?1,?2,?3,?4,?5,?6,?7)"#,
                    params![
                        Uuid::new_v4().to_string(),
                        plan_id,
                        position as i64,
                        phase.name,
                        phase.description,
                        phase.frame_count,
                        phase.timing_weight
                    ],
                )?;
            }
        }
        transaction.commit()?;
    }
    let root = workspace_path(state, &animation.workspace_id)?;
    let file = root
        .join("animations")
        .join(format!("{}.json", animation.id));
    std::fs::write(
        file,
        serde_json::to_vec_pretty(&animation)
            .map_err(|error| CommandError::new("serialization_error", error.to_string()))?,
    )?;
    Ok(animation)
}

#[tauri::command]
pub fn delete_animation(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let workspace_id: Option<String> = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                "SELECT workspace_id FROM animations WHERE id = ?1",
                [&id],
                |row| row.get(0),
            )
            .optional()?
    };
    if let Some(workspace_id) = workspace_id {
        let file = workspace_path(&state, &workspace_id)?
            .join("animations")
            .join(format!("{id}.json"));
        if file.exists() {
            std::fs::remove_file(file)?;
        }
    }
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute("DELETE FROM animations WHERE id = ?1", [id])?;
    Ok(())
}

mod export;
mod preview;
pub(crate) mod export_metadata;
#[cfg(test)]
mod export_metadata_tests;
pub(crate) use export::export_animation_inner;
pub(crate) use preview::write_animation_gif;
pub use export::{
    __cmd__export_animation, __tauri_command_name_export_animation, export_animation,
};
pub use preview::{
    __cmd__export_animation_preview, __tauri_command_name_export_animation_preview,
    export_animation_preview,
};
