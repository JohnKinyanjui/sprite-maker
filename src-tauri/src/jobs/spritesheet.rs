use crate::{
    animations::export_metadata::normalize_export_format,
    error::{CommandError, CommandResult},
    models::{BackgroundJob, SpriteSheet, SpriteSheetInput},
    pipeline::ensure_export_allowed,
    AppState,
};
use chrono::Utc;
use image::{imageops::FilterType, RgbaImage};
use rusqlite::{params, OptionalExtension};
use std::path::{Path, PathBuf};
use tauri::State;
use uuid::Uuid;

use super::persist::{emit_job, set_job_state, JobProgress};
use super::spritesheet_render::render_sprite_sheet;

const MAX_SHEET_EDGE: u32 = 32_768;
fn sheet_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SpriteSheet> {
    Ok(SpriteSheet {
        id: row.get(0)?,
        project_id: row.get(1)?,
        worktree_id: row.get(2)?,
        animation_id: row.get(3)?,
        name: row.get(4)?,
        layout: row.get(5)?,
        frame_width: row.get(6)?,
        frame_height: row.get(7)?,
        padding: row.get(8)?,
        spacing: row.get(9)?,
        rows: row.get(10)?,
        columns: row.get(11)?,
        scale: row.get(12)?,
        transparent: row.get(13)?,
        alignment: row.get(14)?,
        pivot_x: row.get(15)?,
        pivot_y: row.get(16)?,
        png_path: row.get(17)?,
        metadata_path: row.get(18)?,
        width: row.get(19)?,
        height: row.get(20)?,
        frame_count: row.get(21)?,
        created_at: row.get(22)?,
        updated_at: row.get(23)?,
    })
}

fn select_sheet() -> &'static str {
    r#"SELECT id, project_id, worktree_id, animation_id, name, layout,
              frame_width, frame_height, padding, spacing, rows, columns, scale,
              transparent, alignment, pivot_x, pivot_y, png_path, metadata_path,
              width, height, frame_count, created_at, updated_at
       FROM sprite_sheets"#
}
pub(super) fn validate_input(input: &SpriteSheetInput) -> CommandResult<()> {
    if input.name.trim().is_empty() {
        return Err(CommandError::new(
            "invalid_sheet_name",
            "Sprite-sheet name cannot be empty",
        ));
    }
    if !matches!(input.layout.as_str(), "horizontal" | "vertical" | "grid") {
        return Err(CommandError::new(
            "invalid_sheet_layout",
            "Choose Horizontal, Vertical, or Grid layout",
        ));
    }
    if input.frame_width == 0 || input.frame_height == 0 {
        return Err(CommandError::new(
            "invalid_frame_size",
            "Frame width and height must be greater than zero",
        ));
    }
    if input.frame_width > 4096 || input.frame_height > 4096 {
        return Err(CommandError::new(
            "invalid_frame_size",
            "Frame width and height must be 4096 pixels or smaller",
        ));
    }
    if !(1..=8).contains(&input.scale) {
        return Err(CommandError::new(
            "invalid_export_scale",
            "Export scale must be between 1× and 8×",
        ));
    }
    if !matches!(
        input.alignment.as_str(),
        "top_left" | "center" | "bottom_center"
    ) {
        return Err(CommandError::new(
            "invalid_alignment",
            "Choose Top left, Center, or Bottom center alignment",
        ));
    }
    if let Some(format) = &input.metadata_format {
        normalize_export_format(Some(format))?;
    }
    Ok(())
}

pub(super) fn layout_dimensions(
    layout: &str,
    frame_count: u32,
    requested_columns: u32,
) -> (u32, u32) {
    match layout {
        "horizontal" => (frame_count, 1),
        "vertical" => (1, frame_count),
        _ => {
            let columns = requested_columns.clamp(1, frame_count.max(1));
            let rows = frame_count.div_ceil(columns);
            (columns, rows)
        }
    }
}

pub(super) fn checked_sheet_edge(
    cells: u32,
    frame_edge: u32,
    padding: u32,
    spacing: u32,
    scale: u32,
) -> CommandResult<u32> {
    let base = frame_edge
        .checked_mul(cells)
        .and_then(|value| value.checked_add(spacing.saturating_mul(cells.saturating_sub(1))))
        .and_then(|value| value.checked_add(padding.saturating_mul(2)))
        .ok_or_else(|| CommandError::new("sheet_too_large", "Sprite-sheet dimensions overflow"))?;
    let edge = base
        .checked_mul(scale)
        .ok_or_else(|| CommandError::new("sheet_too_large", "Sprite-sheet dimensions overflow"))?;
    if edge > MAX_SHEET_EDGE {
        return Err(CommandError::new(
            "sheet_too_large",
            format!("Sprite-sheet edge {edge}px exceeds the {MAX_SHEET_EDGE}px limit"),
        ));
    }
    Ok(edge)
}

pub(super) fn portable_slug(name: &str) -> String {
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
    if slug.is_empty() {
        "sprite-sheet".into()
    } else {
        slug.into()
    }
}

pub(super) fn fit_image(image: RgbaImage, width: u32, height: u32) -> RgbaImage {
    if image.width() <= width && image.height() <= height {
        return image;
    }
    let ratio = (width as f64 / image.width() as f64).min(height as f64 / image.height() as f64);
    let target_width = ((image.width() as f64 * ratio).floor() as u32).max(1);
    let target_height = ((image.height() as f64 * ratio).floor() as u32).max(1);
    image::imageops::resize(&image, target_width, target_height, FilterType::Nearest)
}

pub(super) fn frame_offset(
    alignment: &str,
    cell_width: u32,
    cell_height: u32,
    image_width: u32,
    image_height: u32,
) -> (u32, u32) {
    match alignment {
        "top_left" => (0, 0),
        "center" => (
            cell_width.saturating_sub(image_width) / 2,
            cell_height.saturating_sub(image_height) / 2,
        ),
        _ => (
            cell_width.saturating_sub(image_width) / 2,
            cell_height.saturating_sub(image_height),
        ),
    }
}
#[tauri::command]
pub fn list_sprite_sheets(
    project_id: String,
    worktree_id: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<Vec<SpriteSheet>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut sheets = Vec::new();
    if let Some(worktree_id) = worktree_id {
        let mut statement = connection.prepare(&format!(
            "{} WHERE project_id=?1 AND worktree_id=?2 ORDER BY updated_at DESC",
            select_sheet()
        ))?;
        let rows = statement.query_map(params![project_id, worktree_id], sheet_row)?;
        sheets.extend(rows.filter_map(Result::ok));
    } else {
        let mut statement = connection.prepare(&format!(
            "{} WHERE project_id=?1 ORDER BY updated_at DESC",
            select_sheet()
        ))?;
        let rows = statement.query_map([project_id], sheet_row)?;
        sheets.extend(rows.filter_map(Result::ok));
    }
    for sheet in &sheets {
        if Path::new(&sheet.png_path).is_file() {
            crate::allow_asset_file(Some(&app), Path::new(&sheet.png_path))?;
        }
    }
    Ok(sheets)
}

#[tauri::command]
pub fn queue_sprite_sheet(
    input: SpriteSheetInput,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<BackgroundJob> {
    queue_sprite_sheet_inner(input, Some(app), &state)
}

pub(crate) fn queue_sprite_sheet_inner(
    input: SpriteSheetInput,
    app: Option<tauri::AppHandle>,
    state: &AppState,
) -> CommandResult<BackgroundJob> {
    validate_input(&input)?;
    ensure_export_allowed(state, &input.animation_id)?;
    let animation_exists: bool = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM animations WHERE id=?1 AND workspace_id=?2)",
            params![input.animation_id, input.project_id],
            |row| row.get(0),
        )?
    };
    if !animation_exists {
        return Err(CommandError::new(
            "animation_not_found",
            "The selected animation no longer exists in this project",
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
                id, project_id, worktree_id, kind, target_type, target_id,
                status, progress, stage, created_at, updated_at
            ) VALUES (?1,?2,?3,'sprite_sheet','animation',?4,'queued',0.0,'Queued',?5,?5)"#,
            params![
                job_id,
                input.project_id,
                input.worktree_id,
                input.animation_id,
                now
            ],
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
            render_sprite_sheet(render_app.as_ref(), &render_state, &render_job_id, input)
        })
        .await;
        match result {
            Ok(Ok(sheet)) => {
                let _ = set_job_state(
                    task_app.as_ref(),
                    &task_state,
                    &task_job_id,
                    JobProgress {
                        status: "completed",
                        progress: 1.0,
                        stage: "Completed",
                        error_message: None,
                        result_path: Some(sheet.png_path.as_str()),
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
#[tauri::command]
pub fn delete_sprite_sheet(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let paths: Option<(String, String)> = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                "SELECT png_path, metadata_path FROM sprite_sheets WHERE id=?1",
                [&id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
    };
    let Some((png_path, metadata_path)) = paths else {
        return Err(CommandError::new(
            "sprite_sheet_not_found",
            "The sprite sheet no longer exists",
        ));
    };
    {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.execute("DELETE FROM sprite_sheets WHERE id=?1", [&id])?;
    }
    for path in [PathBuf::from(png_path), PathBuf::from(metadata_path)] {
        if path.is_file() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}
