use crate::{
    animations::save_animation_inner,
    error::{CommandError, CommandResult},
    models::{Animation, AnimationFrame, AnimationInput, NudgeAnimationFramesInput},
    AppState,
};
use image::RgbaImage;
use rusqlite::OptionalExtension;
use tauri::State;

pub(crate) fn blit_with_offset(
    canvas: &mut RgbaImage,
    image: &RgbaImage,
    dest_x: i32,
    dest_y: i32,
) {
    for y in 0..image.height() {
        for x in 0..image.width() {
            let target_x = dest_x + x as i32;
            let target_y = dest_y + y as i32;
            if target_x < 0 || target_y < 0 {
                continue;
            }
            let target_x = target_x as u32;
            let target_y = target_y as u32;
            if target_x < canvas.width() && target_y < canvas.height() {
                canvas.put_pixel(target_x, target_y, *image.get_pixel(x, y));
            }
        }
    }
}

fn load_animation_for_nudge(
    state: &AppState,
    animation_id: &str,
) -> CommandResult<(
    String,
    Option<String>,
    String,
    f64,
    bool,
    String,
    Vec<AnimationFrame>,
)> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let (
        project_id,
        worktree_id,
        name,
        fps,
        looping,
        review_status,
        frames_json,
    ): (
        String,
        Option<String>,
        String,
        f64,
        bool,
        String,
        String,
    ) = connection
        .query_row(
            "SELECT workspace_id, worktree_id, name, fps, looping, review_status, frames_json FROM animations WHERE id=?1",
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
                ))
            },
        )
        .optional()?
        .ok_or_else(|| CommandError::new("animation_not_found", "The animation no longer exists"))?;
    let source_frames =
        crate::animations::resolve_animation_frames(&connection, animation_id, &frames_json)?;
    Ok((
        project_id,
        worktree_id,
        name,
        fps,
        looping,
        review_status,
        source_frames,
    ))
}

pub(crate) fn nudge_animation_frames_inner(
    state: &AppState,
    input: NudgeAnimationFramesInput,
) -> CommandResult<Animation> {
    let apply_to_all = input.apply_to_all.unwrap_or(false);
    let (project_id, worktree_id, name, fps, looping, review_status, source_frames) =
        load_animation_for_nudge(state, &input.animation_id)?;
    if source_frames.is_empty() {
        return Err(CommandError::new(
            "empty_animation",
            "There are no frames to nudge",
        ));
    }
    let mut next_frames = source_frames;
    if apply_to_all && input.deltas.len() == 1 {
        let delta = &input.deltas[0];
        for frame in &mut next_frames {
            frame.offset_x += delta.offset_x;
            frame.offset_y += delta.offset_y;
        }
    } else {
        for delta in &input.deltas {
            let index = delta.frame_index as usize;
            if index >= next_frames.len() {
                return Err(CommandError::new(
                    "invalid_frame_index",
                    format!("Frame index {} is out of range", delta.frame_index),
                ));
            }
            next_frames[index].offset_x += delta.offset_x;
            next_frames[index].offset_y += delta.offset_y;
        }
    }
    save_animation_inner(
        AnimationInput {
            id: Some(input.animation_id.clone()),
            workspace_id: project_id,
            worktree_id,
            name,
            fps,
            looping,
            frames: next_frames,
            motion_plan: None,
            review_status: Some(review_status),
        },
        state,
    )
}

pub(crate) fn reset_animation_alignment_offsets_inner(
    state: &AppState,
    animation_id: &str,
) -> CommandResult<Animation> {
    let (project_id, worktree_id, name, fps, looping, review_status, source_frames) =
        load_animation_for_nudge(state, animation_id)?;
    let reset_frames = source_frames
        .into_iter()
        .map(|frame| AnimationFrame {
            asset_id: frame.asset_id,
            duration_ms: frame.duration_ms,
            offset_x: 0,
            offset_y: 0,
        })
        .collect();
    save_animation_inner(
        AnimationInput {
            id: Some(animation_id.to_string()),
            workspace_id: project_id,
            worktree_id,
            name,
            fps,
            looping,
            frames: reset_frames,
            motion_plan: None,
            review_status: Some(review_status),
        },
        state,
    )
}

#[tauri::command]
pub fn nudge_animation_frames(
    input: NudgeAnimationFramesInput,
    state: State<'_, AppState>,
) -> CommandResult<Animation> {
    nudge_animation_frames_inner(&state, input)
}

#[tauri::command]
pub fn reset_animation_alignment_offsets(
    animation_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Animation> {
    reset_animation_alignment_offsets_inner(&state, &animation_id)
}
