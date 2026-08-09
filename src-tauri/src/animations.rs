use crate::{
    assets::get_asset,
    error::{CommandError, CommandResult},
    models::{Animation, AnimationFrame, AnimationInput, ExportResult, MotionPhase, MotionPlan},
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use image::{GenericImage, RgbaImage};
use rusqlite::{params, OptionalExtension};
use std::path::PathBuf;
use tauri::State;
use uuid::Uuid;

type StoredMotionPlan = (String, String, u32, u32, u32, u32, bool, bool, bool, String);

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
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
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
            "SELECT id, workspace_id, worktree_id, name, fps, looping, frames_json, created_at, updated_at FROM animations WHERE id=?1",
            [animation_id],
            animation_row,
        )
        .optional()?
        .ok_or_else(|| CommandError::new("animation_not_found", "The animation no longer exists"))?;
    animation.motion_plan = load_motion_plan(&connection, animation_id)?;
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
    let select = "SELECT id, workspace_id, worktree_id, name, fps, looping, frames_json, created_at, updated_at FROM animations";
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
    let (existing_created_at, existing_motion_plan): (Option<String>, Option<MotionPlan>) = {
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
        let motion_plan = load_motion_plan(&connection, &id)?;
        (created_at, motion_plan)
    };
    let animation = Animation {
        id,
        workspace_id: input.workspace_id,
        worktree_id: input.worktree_id,
        name: name.to_string(),
        fps: input.fps,
        looping: input.looping,
        frames: input.frames,
        motion_plan: input.motion_plan.or(existing_motion_plan),
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
            r#"INSERT INTO animations(id, workspace_id, worktree_id, name, fps, looping, frames_json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(id) DO UPDATE SET worktree_id=excluded.worktree_id, name=excluded.name, fps=excluded.fps, looping=excluded.looping, frames_json=excluded.frames_json, updated_at=excluded.updated_at"#,
            params![animation.id, animation.workspace_id, animation.worktree_id, animation.name, animation.fps, animation.looping, frames_json, animation.created_at, animation.updated_at],
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
                    id, animation_id, asset_id, position, duration_ms, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
                params![
                    Uuid::new_v4().to_string(),
                    animation.id,
                    frame.asset_id,
                    position as i64,
                    frame.duration_ms,
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

#[tauri::command]
pub fn export_animation(
    id: String,
    destination: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<ExportResult> {
    let animation = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.query_row(
            "SELECT id, workspace_id, worktree_id, name, fps, looping, frames_json, created_at, updated_at FROM animations WHERE id = ?1", [&id], animation_row
        ).optional()?.ok_or_else(|| CommandError::new("animation_not_found", "Animation no longer exists"))?
    };
    if animation.frames.is_empty() {
        return Err(CommandError::new(
            "empty_animation",
            "Add at least one frame before exporting",
        ));
    }
    let mut decoded = Vec::new();
    let mut frame_width = 0;
    let mut frame_height = 0;
    for frame in &animation.frames {
        let asset = get_asset(&state, &frame.asset_id)?;
        let image = image::open(&asset.path)?.to_rgba8();
        frame_width = frame_width.max(image.width());
        frame_height = frame_height.max(image.height());
        decoded.push((asset, image));
    }
    let sheet_width = frame_width
        .checked_mul(decoded.len() as u32)
        .ok_or_else(|| {
            CommandError::new("export_too_large", "Spritesheet dimensions are too large")
        })?;
    let mut sheet = RgbaImage::new(sheet_width, frame_height);
    for (index, (_, frame)) in decoded.iter().enumerate() {
        sheet.copy_from(frame, index as u32 * frame_width, 0)?;
    }
    let output_directory = if let Some(path) = destination {
        PathBuf::from(path)
    } else {
        workspace_path(&state, &animation.workspace_id)?.join("exports")
    };
    std::fs::create_dir_all(&output_directory)?;
    let slug: String = animation
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
    let png_path = output_directory.join(format!("{slug}.png"));
    let metadata_path = output_directory.join(format!("{slug}.json"));
    sheet.save(&png_path)?;
    let frames: Vec<_> = decoded.iter().enumerate().map(|(index, (asset, _))| serde_json::json!({
        "index": index, "assetId": asset.id, "source": asset.relative_path,
        "x": index as u32 * frame_width, "y": 0, "width": frame_width, "height": frame_height,
        "durationMs": animation.frames[index].duration_ms.unwrap_or_else(|| (1000.0 / animation.fps).round() as u32)
    })).collect();
    let metadata = serde_json::json!({
        "name": animation.name, "image": png_path.file_name().and_then(|value| value.to_str()).unwrap_or("spritesheet.png"),
        "frameWidth": frame_width, "frameHeight": frame_height, "frameCount": frames.len(), "fps": animation.fps,
        "loop": animation.looping, "frames": frames
    });
    std::fs::write(
        &metadata_path,
        serde_json::to_vec_pretty(&metadata)
            .map_err(|error| CommandError::new("serialization_error", error.to_string()))?,
    )?;
    Ok(ExportResult {
        png_path: png_path.to_string_lossy().into_owned(),
        metadata_path: metadata_path.to_string_lossy().into_owned(),
        width: sheet_width,
        height: frame_height,
    })
}
