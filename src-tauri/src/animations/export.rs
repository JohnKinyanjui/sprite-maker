use crate::{
    assets::get_asset,
    error::{CommandError, CommandResult},
    models::ExportResult,
    pipeline::{ensure_export_allowed, get_anchor_inner, list_anchors_inner},
    workspace::{resolve_export_directory, workspace_path},
    AppState,
};
use crate::pipeline::blit_with_offset;
use image::RgbaImage;
use rusqlite::OptionalExtension;
use tauri::State;

use super::export_metadata::{
    anchor_meta_from_character, build_metadata_payload, default_anchor_meta,
    godot_texture_path_for_workspace, metadata_extension, normalize_export_format,
    write_metadata_file, PreparedFrame, PreparedStripExport,
};
use super::{animation_row, load_animation_frames_from_table};

#[tauri::command]
pub fn export_animation(
    id: String,
    destination: Option<String>,
    format: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<ExportResult> {
    export_animation_inner(&id, destination, format.as_deref(), &state)
}

pub(crate) fn export_animation_inner(
    id: &str,
    destination: Option<String>,
    format: Option<&str>,
    state: &AppState,
) -> CommandResult<ExportResult> {
    let export_format = normalize_export_format(format)?;
    ensure_export_allowed(state, id)?;
    let animation = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let mut animation = connection
            .query_row(
                "SELECT id, workspace_id, worktree_id, name, fps, looping, frames_json, review_status, created_at, updated_at FROM animations WHERE id = ?1",
                [id],
                animation_row,
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "Animation no longer exists"))?;
        if animation.frames.is_empty() {
            animation.frames = load_animation_frames_from_table(&connection, id)?;
        }
        animation
    };
    if animation.frames.is_empty() {
        return Err(CommandError::new(
            "empty_animation",
            "Add at least one frame before exporting",
        ));
    }

    let mut prepared_frames = Vec::new();
    let mut frame_width = 0;
    let mut frame_height = 0;
    for frame in &animation.frames {
        let asset = get_asset(state, &frame.asset_id)?;
        let image = image::open(&asset.path)?.to_rgba8();
        frame_width = frame_width.max(image.width());
        frame_height = frame_height.max(image.height());
        prepared_frames.push((frame.clone(), asset, image));
    }

    let anchor_meta = {
        let anchors = list_anchors_inner(&animation.workspace_id, state)?;
        if let Some(summary) = anchors.first() {
            let anchor = get_anchor_inner(&animation.workspace_id, &summary.slug, state)?;
            anchor_meta_from_character(&anchor)
        } else {
            default_anchor_meta(frame_width, frame_height)
        }
    };
    frame_width = frame_width.max(anchor_meta.source_width);
    frame_height = frame_height.max(anchor_meta.source_height);

    let sheet_width = frame_width
        .checked_mul(prepared_frames.len() as u32)
        .ok_or_else(|| {
            CommandError::new("export_too_large", "Spritesheet dimensions are too large")
        })?;
    let mut sheet = RgbaImage::new(sheet_width, frame_height);
    let default_duration = (1000.0 / animation.fps.max(1.0)).round() as u32;
    let mut export_frames = Vec::with_capacity(prepared_frames.len());
    for (index, (frame, asset, image)) in prepared_frames.iter().enumerate() {
        let sheet_x = index as u32 * frame_width;
        let draw_x = sheet_x as i32 + frame.offset_x;
        let draw_y = frame.offset_y;
        blit_with_offset(&mut sheet, image, draw_x, draw_y);
        export_frames.push(PreparedFrame {
            frame: frame.clone(),
            asset_id: asset.id.clone(),
            relative_path: asset.relative_path.clone(),
            source_width: image.width(),
            source_height: image.height(),
            sheet_x,
            sheet_y: 0,
            draw_x: frame.offset_x,
            draw_y: frame.offset_y,
            duration_ms: frame.duration_ms.unwrap_or(default_duration),
        });
    }

    let prepared = PreparedStripExport {
        frame_width,
        frame_height,
        sheet_width,
        sheet_height: frame_height,
        frames: export_frames,
    };

    let workspace_root = workspace_path(state, &animation.workspace_id)?;
    let output_directory =
        resolve_export_directory(&workspace_root, destination.as_deref())?;
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
    let extension = metadata_extension(export_format);
    let png_path = output_directory.join(format!("{slug}.png"));
    let metadata_path = output_directory.join(format!("{slug}.{extension}"));
    sheet.save(&png_path)?;

    let image_file_name = png_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("spritesheet.png");
    let godot_texture_path = godot_texture_path_for_workspace(&png_path, &workspace_root);
    let (metadata_body, _) = build_metadata_payload(
        export_format,
        &animation.name,
        image_file_name,
        &godot_texture_path,
        animation.fps,
        animation.looping,
        &anchor_meta,
        &prepared,
        None,
    )?;
    write_metadata_file(&metadata_path, &metadata_body)?;

    Ok(ExportResult {
        png_path: png_path.to_string_lossy().into_owned(),
        metadata_path: metadata_path.to_string_lossy().into_owned(),
        width: sheet_width,
        height: frame_height,
        format: export_format.to_string(),
    })
}
