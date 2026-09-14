use crate::{
    animations::{
        load_animation_frames_from_table, resolve_animation_frames, save_animation_inner,
    },
    assets::{get_asset, inspect, upsert},
    error::{CommandError, CommandResult},
    models::{Animation, AnimationInput},
    workspace::workspace_path,
    AppState,
};
use image::RgbaImage;
use rusqlite::OptionalExtension;
use tauri::State;

fn snap_value(value: i32, grid_size: u32) -> i32 {
    if grid_size <= 1 {
        return value;
    }
    let grid = grid_size as i32;
    ((value as f64 / grid as f64).round() * grid as f64) as i32
}

pub(crate) fn quantize_channel(value: u8, grid_size: u32) -> u8 {
    if grid_size <= 1 {
        return value;
    }
    let grid = grid_size as i32;
    let quantized = ((value as i32 + grid / 2) / grid * grid).clamp(0, 255);
    quantized as u8
}

pub(crate) fn quantize_rgba_palette(image: &RgbaImage, grid_size: u32) -> RgbaImage {
    if grid_size <= 1 {
        return image.clone();
    }
    let mut output = image.clone();
    for pixel in output.pixels_mut() {
        if pixel[3] == 0 {
            continue;
        }
        pixel[0] = quantize_channel(pixel[0], grid_size);
        pixel[1] = quantize_channel(pixel[1], grid_size);
        pixel[2] = quantize_channel(pixel[2], grid_size);
    }
    output
}

pub(crate) fn snap_animation_offsets_inner(
    state: &AppState,
    animation_id: &str,
    grid_size: u32,
) -> CommandResult<Animation> {
    let grid_size = grid_size.clamp(1, 16);
    let (workspace_id, worktree_id, name, fps, looping, review_status, frames_json) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let row: (String, Option<String>, String, f64, bool, String, String) = connection
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
            .ok_or_else(|| CommandError::new("animation_not_found", "Animation no longer exists"))?;
        row
    };
    let frames = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let table_frames = load_animation_frames_from_table(&connection, animation_id)?;
        if table_frames.is_empty() {
            resolve_animation_frames(&connection, animation_id, &frames_json)?
        } else {
            table_frames
        }
    };
    if frames.is_empty() {
        return Err(CommandError::new(
            "empty_animation",
            "Add frames before snapping offsets to the pixel grid",
        ));
    }

    if grid_size > 1 {
        let root = workspace_path(state, &workspace_id)?;
        for frame in &frames {
            let asset = get_asset(state, &frame.asset_id)?;
            let image = image::open(&asset.path)?.to_rgba8();
            let quantized = quantize_rgba_palette(&image, grid_size);
            if quantized.as_raw() != image.as_raw() {
                quantized
                    .save(&asset.path)
                    .map_err(|error| CommandError::new("snap_grid_write_failed", error.to_string()))?;
                let updated = inspect(
                    &workspace_id,
                    &root,
                    std::path::Path::new(&asset.path),
                    Some(asset.id.clone()),
                )?;
                upsert(state, &updated, "snap-grid")?;
            }
        }
    }

    let snapped_frames = frames
        .into_iter()
        .map(|frame| {
            let mut frame = frame;
            frame.offset_x = snap_value(frame.offset_x, grid_size);
            frame.offset_y = snap_value(frame.offset_y, grid_size);
            frame
        })
        .collect();

    save_animation_inner(
        AnimationInput {
            id: Some(animation_id.to_string()),
            workspace_id,
            worktree_id,
            name,
            fps,
            looping,
            frames: snapped_frames,
            motion_plan: None,
            review_status: Some(review_status),
        },
        state,
    )
}

#[tauri::command]
pub fn snap_to_pixel_grid(
    animation_id: String,
    grid_size: Option<u32>,
    state: State<'_, AppState>,
) -> CommandResult<Animation> {
    snap_animation_offsets_inner(&state, &animation_id, grid_size.unwrap_or(1))
}
