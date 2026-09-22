use crate::{
    animations::load_animation_by_id,
    assets::{get_asset, read_generation_manifest, scan_generation_assets_inner},
    error::{CommandError, CommandResult},
    jobs::{cancellation_requested, set_job_state, JobProgress},
    models::{BrushStamp, QueueRegionRegenInput, RegionMaskRect, RegionRegenResult},
    providers::start_provider_run,
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use image::{GenericImageView, Rgba, RgbaImage};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use uuid::Uuid;

use super::contract_retry_job::wait_for_generation;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MaskOverlayRecord {
    animation_id: String,
    frame_index: u32,
    regions: Vec<RegionMaskRect>,
    brush_strokes: Vec<BrushStamp>,
}

fn paint_brush_disk(mask: &mut RgbaImage, stamp: &BrushStamp, width: u32, height: u32) -> u32 {
    let radius = stamp.radius.max(1);
    let radius_sq = (radius as i64).pow(2);
    let center_x = stamp.x as i64;
    let center_y = stamp.y as i64;
    let min_x = (center_x - radius as i64).max(0);
    let max_x = (center_x + radius as i64).min(width as i64 - 1);
    let min_y = (center_y - radius as i64).max(0);
    let max_y = (center_y + radius as i64).min(height as i64 - 1);
    let mut painted = 0u32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x - center_x;
            let dy = y - center_y;
            if dx * dx + dy * dy <= radius_sq {
                mask.put_pixel(x as u32, y as u32, Rgba([255, 255, 255, 255]));
                painted += 1;
            }
        }
    }
    painted
}

pub(crate) fn build_region_mask(
    width: u32,
    height: u32,
    regions: &[RegionMaskRect],
    brush_strokes: &[BrushStamp],
) -> CommandResult<RgbaImage> {
    if regions.is_empty() && brush_strokes.is_empty() {
        return Err(CommandError::new(
            "empty_mask",
            "Select at least one region to regenerate",
        ));
    }
    let mut mask = RgbaImage::new(width, height);
    for pixel in mask.pixels_mut() {
        *pixel = Rgba([0, 0, 0, 255]);
    }
    let mut painted = 0u32;
    for region in regions {
        if region.width == 0 || region.height == 0 {
            continue;
        }
        let x_end = region.x.saturating_add(region.width).min(width);
        let y_end = region.y.saturating_add(region.height).min(height);
        for y in region.y..y_end {
            for x in region.x..x_end {
                mask.put_pixel(x, y, Rgba([255, 255, 255, 255]));
                painted += 1;
            }
        }
    }
    for stamp in brush_strokes {
        painted += paint_brush_disk(&mut mask, stamp, width, height);
    }
    if painted == 0 {
        return Err(CommandError::new(
            "empty_mask",
            "Mask region is empty after clipping to the frame bounds",
        ));
    }
    Ok(mask)
}

fn persist_mask_overlay_json(
    root: &Path,
    animation_id: &str,
    frame_index: u32,
    regions: &[RegionMaskRect],
    brush_strokes: &[BrushStamp],
) {
    let directory = root.join(".sprite-studio").join("masks");
    if std::fs::create_dir_all(&directory).is_err() {
        return;
    }
    let path = directory.join(format!("{animation_id}-{frame_index}.json"));
    let record = MaskOverlayRecord {
        animation_id: animation_id.to_string(),
        frame_index,
        regions: regions.to_vec(),
        brush_strokes: brush_strokes.to_vec(),
    };
    if let Ok(payload) = serde_json::to_vec_pretty(&record) {
        let _ = std::fs::write(path, payload);
    }
}

pub(crate) fn composite_with_mask(
    base: &RgbaImage,
    patch: &RgbaImage,
    mask: &RgbaImage,
) -> CommandResult<RgbaImage> {
    if base.dimensions() != patch.dimensions() || base.dimensions() != mask.dimensions() {
        return Err(CommandError::new(
            "dimension_mismatch",
            "Regenerated frame dimensions do not match the source frame",
        ));
    }
    let mut output = base.clone();
    for (x, y, mask_pixel) in mask.enumerate_pixels() {
        if mask_pixel[0] > 127 {
            output.put_pixel(x, y, *patch.get_pixel(x, y));
        }
    }
    Ok(output)
}

fn find_regenerated_frame_path(
    root: &Path,
    manifest: &crate::models::GenerationManifest,
    width: u32,
    height: u32,
    exclude: &Path,
) -> Option<PathBuf> {
    let mut best: Option<(PathBuf, u64)> = None;
    for relative in &manifest.files {
        let path = root.join(relative);
        if !path.is_file() || path == exclude {
            continue;
        }
        if let Ok(image) = image::open(&path) {
            let (w, h) = image.dimensions();
            if w != width || h != height {
                continue;
            }
            let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            if best.as_ref().map_or(true, |(_, best_size)| size >= *best_size) {
                best = Some((path, size));
            }
        }
    }
    best.map(|(path, _)| path)
}

fn build_region_regen_prompt(
    animation_name: &str,
    frame_index: u32,
    frame_path: &str,
    mask_path: &str,
    user_prompt: Option<&str>,
) -> String {
    let custom = user_prompt
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| format!("\nUser note: {value}"))
        .unwrap_or_default();
    format!(
        "REGIONAL FRAME REGEN\n\
Animation: {animation_name}\n\
Frame index: {frame_index}\n\
Source frame (keep unmasked pixels identical): `{frame_path}`\n\
Mask (white = regenerate, black = keep): `{mask_path}`\n\
\n\
Regenerate ONLY the masked region of this pixel-art sprite frame. Preserve palette, outline weight, and transparency outside the mask. Output a single PNG with the same dimensions as the source frame.{custom}"
    )
}

pub(crate) fn queue_region_regen_inner(
    input: QueueRegionRegenInput,
    app: Option<AppHandle>,
    state: &AppState,
) -> CommandResult<RegionRegenResult> {
    let conversation_id = input.conversation_id.trim();
    if conversation_id.is_empty() {
        return Err(CommandError::new(
            "conversation_required",
            "A conversationId is required for regional regeneration",
        ));
    }
    let animation = load_animation_by_id(state, &input.animation_id)?;
    let frame_index = input.frame_index as usize;
    if frame_index >= animation.frames.len() {
        return Err(CommandError::new(
            "invalid_frame_index",
            "Frame index is out of range",
        ));
    }
    let frame = &animation.frames[frame_index];
    let asset = get_asset(state, &frame.asset_id)?;
    let source = image::open(&asset.path)?.to_rgba8();
    let (width, height) = source.dimensions();
    build_region_mask(width, height, &input.regions, &input.brush_strokes)?;
    let root = workspace_path(state, &animation.workspace_id)?;
    persist_mask_overlay_json(
        &root,
        &input.animation_id,
        input.frame_index,
        &input.regions,
        &input.brush_strokes,
    );

    let active_job_id: Option<String> = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                r#"SELECT id FROM background_jobs
                   WHERE kind='region_regen' AND target_id=?1
                     AND status IN ('queued','running')
                   ORDER BY created_at DESC LIMIT 1"#,
                [&input.animation_id],
                |row| row.get(0),
            )
            .optional()?
    };
    if active_job_id.is_some() {
        return Err(CommandError::new(
            "region_regen_active",
            "A regional regeneration job is already running for this animation",
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
                id,project_id,worktree_id,kind,target_type,target_id,status,
                progress,stage,created_at,updated_at,started_at
            ) VALUES (?1,?2,?3,'region_regen','animation',?4,'queued',0.0,'Queued',?5,?5,?5)"#,
            params![
                job_id,
                animation.workspace_id,
                animation.worktree_id,
                input.animation_id,
                now
            ],
        )?;
    }

    let task_app = app.clone();
    let task_state = state.clone();
    let task_input = input.clone();
    let task_job_id = job_id.clone();
    tauri::async_runtime::spawn(async move {
        run_region_regen_async(task_app, task_state, task_job_id, task_input).await;
    });

    Ok(RegionRegenResult {
        job_id,
        frame_index: input.frame_index,
    })
}

async fn run_region_regen_async(
    app: Option<AppHandle>,
    state: AppState,
    job_id: String,
    input: QueueRegionRegenInput,
) {
    let result = run_region_regen_job(app.as_ref(), &state, &job_id, &input).await;
    let (status, stage, progress, error) = match result {
        Ok(()) => ("completed", "Completed", 1.0, None),
        Err(error) => {
            if error.code == "job_cancelled" {
                ("cancelled", "Cancelled", 0.0, Some(error.message))
            } else {
                ("failed", "Failed", 0.0, Some(error.message))
            }
        }
    };
    let _ = set_job_state(
        app.as_ref(),
        &state,
        &job_id,
        JobProgress {
            status,
            progress,
            stage,
            error_message: error.as_deref(),
            result_path: None,
        },
    );
    if status == "completed" {
        super::sessions::append_animation_job_session(
            &state,
            &input.animation_id,
            "region_regen",
            None,
            None,
        );
    }
}

async fn run_region_regen_job(
    app: Option<&AppHandle>,
    state: &AppState,
    job_id: &str,
    input: &QueueRegionRegenInput,
) -> CommandResult<()> {
    let animation = load_animation_by_id(state, &input.animation_id)?;
    let frame_index = input.frame_index as usize;
    let frame = animation
        .frames
        .get(frame_index)
        .ok_or_else(|| CommandError::new("invalid_frame_index", "Frame index is out of range"))?;
    let asset = get_asset(state, &frame.asset_id)?;
    let root = workspace_path(state, &animation.workspace_id)?;
    let source = image::open(&asset.path)?.to_rgba8();
    let (width, height) = source.dimensions();
    let mask = build_region_mask(width, height, &input.regions, &input.brush_strokes)?;

    let work_dir = root
        .join(".sprite-studio")
        .join("region-regen")
        .join(job_id);
    std::fs::create_dir_all(&work_dir)?;
    let source_path = work_dir.join("source.png");
    let mask_path = work_dir.join("mask.png");
    let backup_path = work_dir.join("backup.png");
    source.save(&source_path)?;
    mask.save(&mask_path)?;
    std::fs::copy(&asset.path, &backup_path)?;

    let _ = set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "running",
            progress: 0.1,
            stage: "Preparing mask",
            error_message: None,
            result_path: None,
        },
    )?;

    let source_relative = source_path
        .strip_prefix(&root)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| source_path.to_string_lossy().into_owned());
    let mask_relative = mask_path
        .strip_prefix(&root)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| mask_path.to_string_lossy().into_owned());

    let prompt = build_region_regen_prompt(
        &animation.name,
        input.frame_index,
        &source_relative,
        &mask_relative,
        input.prompt.as_deref(),
    );

    let _ = set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "running",
            progress: 0.2,
            stage: "Requesting regeneration",
            error_message: None,
            result_path: None,
        },
    )?;
    let request_id = start_provider_run(
        input.conversation_id.clone(),
        prompt,
        None,
        None,
        app.cloned(),
        state,
    )?;

    wait_for_generation(state, &request_id, job_id).await?;

    if cancellation_requested(state, job_id)? {
        return Err(CommandError::new("job_cancelled", "Regional regeneration was cancelled"));
    }

    let _ = set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "running",
            progress: 0.75,
            stage: "Importing result",
            error_message: None,
            result_path: None,
        },
    )?;
    scan_generation_assets_inner(
        &animation.workspace_id,
        animation.worktree_id.as_deref(),
        app,
        state,
    )?;
    let manifest = read_generation_manifest(&root)?
        .ok_or_else(|| CommandError::new("manifest_missing", "Generation manifest was not updated"))?;
    let regenerated = find_regenerated_frame_path(&root, &manifest, width, height, &source_path)
        .ok_or_else(|| {
            CommandError::new(
                "regen_output_missing",
                "No regenerated frame with matching dimensions was found in the generation manifest",
            )
        })?;

    let patch = image::open(&regenerated)?.to_rgba8();
    let composite = composite_with_mask(&source, &patch, &mask)?;
    composite.save(&asset.path)?;

    let _ = set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "running",
            progress: 0.95,
            stage: "Frame updated",
            error_message: None,
            result_path: None,
        },
    )?;
    Ok(())
}

#[tauri::command]
pub fn queue_region_regen(
    input: QueueRegionRegenInput,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> CommandResult<RegionRegenResult> {
    queue_region_regen_inner(input, Some(app), &state)
}
