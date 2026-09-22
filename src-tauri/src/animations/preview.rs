use crate::{
    animations::{animation_row, load_animation_frames_from_table},
    assets::get_asset,
    error::{CommandError, CommandResult},
    models::{ExportAnimationPreviewInput, ExportAnimationPreviewResult},
    pipeline::blit_with_offset,
    workspace::{resolve_export_directory, workspace_path},
    AppState,
};
use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame, RgbaImage};
use rusqlite::OptionalExtension;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use tauri::State;

pub(crate) fn export_animation_preview_inner(
    input: &ExportAnimationPreviewInput,
    state: &AppState,
) -> CommandResult<ExportAnimationPreviewResult> {
    let format = input
        .format
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "gif".to_string());
    if format != "gif" {
        return Err(CommandError::new(
            "invalid_preview_format",
            "Only gif preview export is supported",
        ));
    }

    let (workspace_id, name, frame_count, frame_width, frame_height) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let mut animation = connection
            .query_row(
                "SELECT id, workspace_id, worktree_id, name, fps, looping, frames_json, review_status, created_at, updated_at FROM animations WHERE id = ?1",
                [&input.animation_id],
                animation_row,
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "Animation no longer exists"))?;
        if animation.frames.is_empty() {
            animation.frames =
                load_animation_frames_from_table(&connection, &input.animation_id)?;
        }
        if animation.frames.is_empty() {
            return Err(CommandError::new(
                "empty_animation",
                "Add at least one frame before exporting a preview",
            ));
        }
        let mut frame_width = 0u32;
        let mut frame_height = 0u32;
        for frame in &animation.frames {
            let asset = get_asset(state, &frame.asset_id)?;
            let image = image::open(&asset.path)?.to_rgba8();
            frame_width = frame_width.max(image.width());
            frame_height = frame_height.max(image.height());
        }
        (
            animation.workspace_id.clone(),
            animation.name.clone(),
            animation.frames.len() as u32,
            frame_width,
            frame_height,
        )
    };

    let workspace_root = workspace_path(state, &workspace_id)?;
    let slug: String = name
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
    let output_directory =
        resolve_export_directory(&workspace_root, input.destination.as_deref())?;
    std::fs::create_dir_all(&output_directory)?;
    let gif_path = output_directory.join(format!("{slug}.gif"));
    write_animation_gif(&input.animation_id, &gif_path, state)?;

    Ok(ExportAnimationPreviewResult {
        gif_path: gif_path.to_string_lossy().into_owned(),
        frame_count,
        width: frame_width,
        height: frame_height,
    })
}

#[tauri::command]
pub fn export_animation_preview(
    input: ExportAnimationPreviewInput,
    state: State<'_, AppState>,
) -> CommandResult<ExportAnimationPreviewResult> {
    export_animation_preview_inner(&input, &state)
}

pub(crate) fn write_animation_gif(
    animation_id: &str,
    gif_path: &Path,
    state: &AppState,
) -> CommandResult<()> {
    let animation = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let mut animation = connection
            .query_row(
                "SELECT id, workspace_id, worktree_id, name, fps, looping, frames_json, review_status, created_at, updated_at FROM animations WHERE id = ?1",
                [animation_id],
                animation_row,
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "Animation no longer exists"))?;
        if animation.frames.is_empty() {
            animation.frames = load_animation_frames_from_table(&connection, animation_id)?;
        }
        animation
    };
    if animation.frames.is_empty() {
        return Err(CommandError::new(
            "empty_animation",
            "Add at least one frame before exporting a preview",
        ));
    }

    let mut frame_width = 0u32;
    let mut frame_height = 0u32;
    let mut rgba_frames = Vec::with_capacity(animation.frames.len());
    for frame in &animation.frames {
        let asset = get_asset(state, &frame.asset_id)?;
        let image = image::open(&asset.path)?.to_rgba8();
        frame_width = frame_width.max(image.width());
        frame_height = frame_height.max(image.height());
        rgba_frames.push((frame.clone(), image));
    }

    if let Some(parent) = gif_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let delay_ms = (1000.0 / animation.fps.max(1.0)).round().max(2.0) as u32;
    let file = File::create(gif_path)?;
    let mut encoder = GifEncoder::new(BufWriter::new(file));
    encoder.set_repeat(Repeat::Infinite)?;
    for (frame, image) in rgba_frames {
        let mut canvas = RgbaImage::new(frame_width, frame_height);
        blit_with_offset(&mut canvas, &image, frame.offset_x, frame.offset_y);
        let duration = frame.duration_ms.unwrap_or(delay_ms);
        let gif_frame = Frame::from_parts(
            canvas,
            0,
            0,
            Delay::from_numer_denom_ms(duration.max(2), 1),
        );
        encoder.encode_frame(gif_frame)?;
    }
    Ok(())
}
