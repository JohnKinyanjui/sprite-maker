use crate::{
    animations::{
        export_metadata::{
            anchor_meta_from_character, build_metadata_payload, default_anchor_meta,
            godot_texture_path_for_workspace, metadata_extension, normalize_export_format,
            write_metadata_file, PreparedFrame, PreparedStripExport, SpriteSheetGridMeta,
        },
        resolve_animation_frames,
    },
    assets::get_asset,
    error::{CommandError, CommandResult},
    models::{AnimationFrame, SpriteSheet, SpriteSheetInput},
    pipeline::{blit_with_offset, get_anchor_inner, list_anchors_inner},
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use image::{imageops::FilterType, Rgba, RgbaImage};
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use super::persist::{cancellation_requested, set_job_state, JobProgress};
use super::spritesheet::{
    checked_sheet_edge, fit_image, frame_offset, layout_dimensions, portable_slug, validate_input,
};

pub(super) fn render_sprite_sheet(
    app: Option<&tauri::AppHandle>,
    state: &AppState,
    job_id: &str,
    input: SpriteSheetInput,
) -> CommandResult<SpriteSheet> {
    validate_input(&input)?;
    let (animation_name, fps, looping, frames, animation_project, animation_worktree): (
        String,
        f64,
        bool,
        Vec<AnimationFrame>,
        String,
        Option<String>,
    ) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let (animation_name, fps, looping, frames_json, animation_project, animation_worktree): (
            String,
            f64,
            bool,
            String,
            String,
            Option<String>,
        ) = connection
            .query_row(
                "SELECT name, fps, looping, frames_json, workspace_id, worktree_id FROM animations WHERE id=?1",
                [&input.animation_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "The animation no longer exists"))?;
        let frames = resolve_animation_frames(
            &connection,
            &input.animation_id,
            &frames_json,
        )?;
        (
            animation_name,
            fps,
            looping,
            frames,
            animation_project,
            animation_worktree,
        )
    };
    if animation_project != input.project_id {
        return Err(CommandError::new(
            "invalid_sheet_project",
            "The animation and sprite sheet must belong to the same project",
        ));
    }
    if let Some(worktree_id) = &input.worktree_id {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let valid: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM worktrees WHERE id=?1 AND project_id=?2)",
            params![worktree_id, input.project_id],
            |row| row.get(0),
        )?;
        if !valid {
            return Err(CommandError::new(
                "invalid_sheet_worktree",
                "The sprite-sheet worktree must belong to the same project",
            ));
        }
    }
    if frames.is_empty() {
        return Err(CommandError::new(
            "empty_animation",
            "Add at least one frame before building a sprite sheet",
        ));
    }
    let frame_count = frames.len() as u32;
    let (columns, rows) = layout_dimensions(&input.layout, frame_count, input.columns);
    let output_width = checked_sheet_edge(
        columns,
        input.frame_width,
        input.padding,
        input.spacing,
        input.scale,
    )?;
    let output_height = checked_sheet_edge(
        rows,
        input.frame_height,
        input.padding,
        input.spacing,
        input.scale,
    )?;
    let base_width = output_width / input.scale;
    let base_height = output_height / input.scale;
    let background = if input.transparent {
        Rgba([0, 0, 0, 0])
    } else {
        Rgba([0, 0, 0, 255])
    };
    let metadata_format = normalize_export_format(input.metadata_format.as_deref())?;
    let mut anchor_meta = {
        let anchors = list_anchors_inner(&input.project_id, state)?;
        if let Some(summary) = anchors.first() {
            let anchor = get_anchor_inner(&input.project_id, &summary.slug, state)?;
            anchor_meta_from_character(&anchor)
        } else {
            default_anchor_meta(input.frame_width, input.frame_height)
        }
    };
    anchor_meta.pivot.x =
        input.pivot_x.clamp(0.0, 1.0) * (input.frame_width * input.scale) as f64;
    anchor_meta.pivot.y =
        input.pivot_y.clamp(0.0, 1.0) * (input.frame_height * input.scale) as f64;
    let mut canvas = RgbaImage::from_pixel(base_width, base_height, background);
    let mut prepared_frames = Vec::with_capacity(frames.len());

    set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "running",
            progress: 0.05,
            stage: "Loading frames",
            error_message: None,
            result_path: None,
        },
    )?;
    for (index, frame) in frames.iter().enumerate() {
        if cancellation_requested(state, job_id)? {
            return Err(CommandError::new(
                "job_cancelled",
                "Sprite-sheet build cancelled",
            ));
        }
        let asset = get_asset(state, &frame.asset_id)?;
        let image = fit_image(
            image::open(&asset.path)?.to_rgba8(),
            input.frame_width,
            input.frame_height,
        );
        let column = index as u32 % columns;
        let row = index as u32 / columns;
        let cell_x = input.padding + column * (input.frame_width + input.spacing);
        let cell_y = input.padding + row * (input.frame_height + input.spacing);
        let (offset_x, offset_y) = frame_offset(
            &input.alignment,
            input.frame_width,
            input.frame_height,
            image.width(),
            image.height(),
        );
        let trim_x = offset_x as i32 + frame.offset_x;
        let trim_y = offset_y as i32 + frame.offset_y;
        let draw_x = cell_x as i32 + trim_x;
        let draw_y = cell_y as i32 + trim_y;
        blit_with_offset(&mut canvas, &image, draw_x, draw_y);
        let duration = frame
            .duration_ms
            .unwrap_or_else(|| (1000.0 / fps.max(1.0)).round() as u32)
            .max(1);
        prepared_frames.push(PreparedFrame {
            frame: frame.clone(),
            asset_id: asset.id.clone(),
            relative_path: asset.relative_path.clone(),
            source_width: image.width() * input.scale,
            source_height: image.height() * input.scale,
            sheet_x: cell_x * input.scale,
            sheet_y: cell_y * input.scale,
            draw_x: trim_x * input.scale as i32,
            draw_y: trim_y * input.scale as i32,
            duration_ms: duration,
        });
        let progress = 0.08 + 0.72 * ((index + 1) as f64 / frames.len() as f64);
        set_job_state(
            app,
            state,
            job_id,
            JobProgress {
                status: "running",
                progress,
                stage: &format!("Packing frame {} of {}", index + 1, frames.len()),
                error_message: None,
                result_path: None,
            },
        )?;
    }

    if cancellation_requested(state, job_id)? {
        return Err(CommandError::new(
            "job_cancelled",
            "Sprite-sheet build cancelled",
        ));
    }
    set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "running",
            progress: 0.84,
            stage: "Scaling with nearest neighbour",
            error_message: None,
            result_path: None,
        },
    )?;
    let output = if input.scale == 1 {
        canvas
    } else {
        image::imageops::resize(&canvas, output_width, output_height, FilterType::Nearest)
    };
    let sheet_id = Uuid::new_v4().to_string();
    let root = workspace_path(state, &input.project_id)?;
    let output_directory = root.join("exports").join("sprite-sheets");
    std::fs::create_dir_all(&output_directory)?;
    let slug = portable_slug(input.name.trim());
    let revision = &sheet_id[..8];
    let extension = metadata_extension(metadata_format);
    let png_path = output_directory.join(format!("{slug}-{revision}.png"));
    let metadata_path = output_directory.join(format!("{slug}-{revision}.{extension}"));
    output.save(&png_path)?;

    let image_file_name = png_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("sprite-sheet.png");
    let prepared = PreparedStripExport {
        frame_width: input.frame_width * input.scale,
        frame_height: input.frame_height * input.scale,
        sheet_width: output_width,
        sheet_height: output_height,
        frames: prepared_frames,
    };
    let grid_meta = SpriteSheetGridMeta {
        layout: input.layout.clone(),
        padding: input.padding * input.scale,
        spacing: input.spacing * input.scale,
        rows,
        columns,
        scale: input.scale,
        transparent: input.transparent,
        alignment: input.alignment.clone(),
        source_animation_id: input.animation_id.clone(),
        source_animation_name: animation_name.clone(),
    };
    let godot_texture_path = godot_texture_path_for_workspace(&png_path, &root);
    let (metadata_body, _) = build_metadata_payload(
        metadata_format,
        input.name.trim(),
        image_file_name,
        &godot_texture_path,
        fps,
        looping,
        &anchor_meta,
        &prepared,
        Some(&grid_meta),
    )?;
    write_metadata_file(&metadata_path, &metadata_body)?;

    let now = Utc::now().to_rfc3339();
    let sheet = SpriteSheet {
        id: sheet_id,
        project_id: input.project_id,
        worktree_id: input.worktree_id.or(animation_worktree),
        animation_id: input.animation_id,
        name: input.name.trim().to_string(),
        layout: input.layout,
        frame_width: input.frame_width,
        frame_height: input.frame_height,
        padding: input.padding,
        spacing: input.spacing,
        rows,
        columns,
        scale: input.scale,
        transparent: input.transparent,
        alignment: input.alignment,
        pivot_x: input.pivot_x.clamp(0.0, 1.0),
        pivot_y: input.pivot_y.clamp(0.0, 1.0),
        png_path: png_path.to_string_lossy().into_owned(),
        metadata_path: metadata_path.to_string_lossy().into_owned(),
        width: output_width,
        height: output_height,
        frame_count,
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
            r#"INSERT INTO sprite_sheets(
                id, project_id, worktree_id, animation_id, job_id, name, layout,
                frame_width, frame_height, padding, spacing, rows, columns, scale,
                transparent, alignment, pivot_x, pivot_y, png_path, metadata_path,
                width, height, frame_count, created_at, updated_at
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)"#,
            params![
                sheet.id, sheet.project_id, sheet.worktree_id, sheet.animation_id,
                job_id, sheet.name, sheet.layout, sheet.frame_width, sheet.frame_height,
                sheet.padding, sheet.spacing, sheet.rows, sheet.columns, sheet.scale,
                sheet.transparent, sheet.alignment, sheet.pivot_x, sheet.pivot_y,
                sheet.png_path, sheet.metadata_path, sheet.width, sheet.height,
                sheet.frame_count, sheet.created_at, sheet.updated_at
            ],
        )?;
        for (index, frame) in frames.iter().enumerate() {
            let column = index as u32 % columns;
            let row = index as u32 / columns;
            let duration = frame
                .duration_ms
                .unwrap_or_else(|| (1000.0 / fps.max(1.0)).round() as u32)
                .max(1);
            transaction.execute(
                r#"INSERT INTO sprite_sheet_items(
                    id, sprite_sheet_id, animation_id, asset_id, position,
                    row_index, column_index, x, y, width, height, duration_ms
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"#,
                params![
                    Uuid::new_v4().to_string(),
                    sheet.id,
                    sheet.animation_id,
                    frame.asset_id,
                    index as u32,
                    row,
                    column,
                    (input.padding + column * (input.frame_width + input.spacing)) * input.scale,
                    (input.padding + row * (input.frame_height + input.spacing)) * input.scale,
                    input.frame_width * input.scale,
                    input.frame_height * input.scale,
                    duration
                ],
            )?;
        }
        transaction.commit()?;
    }
    crate::allow_asset_file(app, &png_path)?;
    set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "analyzing",
            progress: 0.95,
            stage: "Validating output",
            error_message: None,
            result_path: Some(sheet.png_path.as_str()),
        },
    )?;
    let verified = image::open(&png_path)?;
    if verified.width() != output_width || verified.height() != output_height {
        return Err(CommandError::new(
            "sheet_validation_failed",
            "The exported sprite-sheet dimensions did not match the plan",
        ));
    }
    Ok(sheet)
}
