use crate::{
    animations::save_animation_inner,
    assets::{get_asset, upsert},
    error::{CommandError, CommandResult},
    models::{Animation, AnimationFrame, AnimationInput, Asset, NormalizeAnimationInput},
    quality::compute_metrics,
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use image::{Rgba, RgbaImage};
use rusqlite::OptionalExtension;
use tauri::{AppHandle, State};
use uuid::Uuid;

use super::align::blit_with_offset;
use super::anchors::get_anchor_inner;

pub(crate) fn foot_y_on_canvas(image: &RgbaImage) -> Option<u32> {
    let mut maximum_y = None;
    for (_x, y, pixel) in image.enumerate_pixels() {
        if pixel[3] > 8 {
            maximum_y = Some(maximum_y.map_or(y, |current: u32| current.max(y)));
        }
    }
    maximum_y
}

pub(crate) fn shift_canvas_vertical(image: &RgbaImage, delta_y: i32) -> RgbaImage {
    let mut shifted = RgbaImage::new(image.width(), image.height());
    blit_with_offset(&mut shifted, image, 0, delta_y);
    shifted
}

fn blit_subject_scaled(
    canvas: &mut RgbaImage,
    image: &RgbaImage,
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
    dest_left: i32,
    dest_top: i32,
    scale: f64,
) {
    let subject_width = max_x - min_x + 1;
    let subject_height = max_y - min_y + 1;
    let scaled_width = ((subject_width as f64) * scale).round().max(1.0) as u32;
    let scaled_height = ((subject_height as f64) * scale).round().max(1.0) as u32;
    for dest_y in 0..scaled_height {
        for dest_x in 0..scaled_width {
            let source_x = min_x + (dest_x as f64 / scale).floor() as u32;
            let source_y = min_y + (dest_y as f64 / scale).floor() as u32;
            if source_x > max_x || source_y > max_y {
                continue;
            }
            let pixel = image.get_pixel(source_x, source_y);
            if pixel[3] <= 8 {
                continue;
            }
            let target_x = dest_left + dest_x as i32;
            let target_y = dest_top + dest_y as i32;
            if target_x < 0 || target_y < 0 {
                continue;
            }
            let target_x = target_x as u32;
            let target_y = target_y as u32;
            if target_x < canvas.width() && target_y < canvas.height() {
                canvas.put_pixel(target_x, target_y, *pixel);
            }
        }
    }
}

pub(crate) fn align_frame_to_baseline(
    image: &RgbaImage,
    bounds: Option<(u32, u32, u32, u32)>,
    centroid: Option<(f64, f64)>,
    canvas_width: u32,
    canvas_height: u32,
    pivot_x: f64,
    baseline_y: u32,
    scale: f64,
    padding: u32,
) -> RgbaImage {
    let scale = scale.max(0.01);
    let mut canvas = RgbaImage::from_pixel(canvas_width, canvas_height, Rgba([0, 0, 0, 0]));
    let effective_baseline = baseline_y.min(canvas_height.saturating_sub(1 + padding));
    let horizontal_min = padding as f64;
    let horizontal_max = canvas_width.saturating_sub(1 + padding) as f64;
    let pivot_x = pivot_x.clamp(horizontal_min, horizontal_max);

    let Some((min_x, min_y, max_x, max_y)) = bounds else {
        let subject_width = image.width();
        let subject_height = image.height();
        let _scaled_width = ((subject_width as f64) * scale).round().max(1.0) as u32;
        let scaled_height = ((subject_height as f64) * scale).round().max(1.0) as u32;
        let subject_center_x = centroid.map(|(cx, _)| cx).unwrap_or(image.width() as f64 / 2.0);
        let dest_left = (pivot_x - (subject_center_x - 0.0) * scale).round() as i32;
        let dest_top = effective_baseline as i32 - scaled_height as i32 + 1;
        blit_subject_scaled(
            &mut canvas,
            image,
            0,
            0,
            subject_width.saturating_sub(1),
            subject_height.saturating_sub(1),
            dest_left,
            dest_top,
            scale,
        );
        return canvas;
    };

    let subject_width = max_x - min_x + 1;
    let _scaled_width = ((subject_width as f64) * scale).round().max(1.0) as u32;
    let scaled_height = ((max_y - min_y + 1) as f64 * scale).round().max(1.0) as u32;
    let subject_center_x = centroid
        .map(|(cx, _)| cx)
        .unwrap_or((min_x + max_x) as f64 / 2.0);
    let dest_left = (pivot_x - (subject_center_x - min_x as f64) * scale).round() as i32;
    let dest_top = effective_baseline as i32 - scaled_height as i32 + 1;
    blit_subject_scaled(
        &mut canvas,
        image,
        min_x,
        min_y,
        max_x,
        max_y,
        dest_left,
        dest_top,
        scale,
    );
    canvas
}

pub(crate) fn compute_shared_scale(
    decoded: &[(AnimationFrame, RgbaImage, Option<(u32, u32, u32, u32)>, Option<(f64, f64)>)],
    canvas_width: u32,
    baseline_y: u32,
    padding: u32,
) -> f64 {
    let available_width = canvas_width.saturating_sub(padding * 2).max(1);
    let available_height = baseline_y.saturating_sub(padding).saturating_add(1).max(1);
    let mut scale = 1.0f64;
    for (_, _, bounds, _) in decoded {
        if let Some((min_x, min_y, max_x, max_y)) = bounds {
            let subject_width = max_x - min_x + 1;
            let subject_height = max_y - min_y + 1;
            scale = scale.min(available_width as f64 / subject_width as f64);
            scale = scale.min(available_height as f64 / subject_height as f64);
        }
    }
    scale.min(1.0).max(0.01)
}

pub(crate) fn normalize_animation_inner(
    app: Option<&AppHandle>,
    state: &AppState,
    input: NormalizeAnimationInput,
) -> CommandResult<Animation> {
    let lock_first_frame = input.lock_first_frame.unwrap_or(false);
    let shared_scale = input.shared_scale.unwrap_or(true);
    let padding = input.padding.unwrap_or(0);
    let (project_id, worktree_id, name, fps, looping, review_status, source_frames, _created_at): (
        String,
        Option<String>,
        String,
        f64,
        bool,
        String,
        Vec<AnimationFrame>,
        String,
    ) = {
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
            created_at,
        ): (
            String,
            Option<String>,
            String,
            f64,
            bool,
            String,
            String,
            String,
        ) = connection
            .query_row(
                "SELECT workspace_id, worktree_id, name, fps, looping, review_status, frames_json, created_at FROM animations WHERE id=?1",
                [&input.animation_id],
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
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "The animation no longer exists"))?;
        let source_frames = crate::animations::resolve_animation_frames(
            &connection,
            &input.animation_id,
            &frames_json,
        )?;
        (
            project_id,
            worktree_id,
            name,
            fps,
            looping,
            review_status,
            source_frames,
            created_at,
        )
    };
    if source_frames.is_empty() {
        return Err(CommandError::new(
            "empty_animation",
            "There are no frames to normalize",
        ));
    }
    let (canvas_width, canvas_height, pivot_x, baseline_y) =
        if let Some(slug) = input.anchor_slug.as_deref() {
            let anchor = get_anchor_inner(&project_id, slug, state)?;
            (
                anchor.frame_width,
                anchor.frame_height,
                anchor.pivot.x,
                anchor.baseline_y,
            )
        } else {
            let first = get_asset(state, &source_frames[0].asset_id)?;
            (
                first.width,
                first.height,
                first.width as f64 / 2.0,
                first.height.saturating_sub(1),
            )
        };
    let root = workspace_path(state, &project_id)?;
    let repair_id = Uuid::new_v4().to_string();
    let slug = super::anchors::portable_slug(&name);
    let output_directory = root.join("assets").join("normalized").join(format!(
        "{}-{}",
        slug,
        &repair_id[..8]
    ));
    std::fs::create_dir_all(&output_directory)?;
    if let Some(app) = app {
        crate::allow_asset_directory(Some(app), &output_directory, true)?;
    }
    let mut decoded = Vec::with_capacity(source_frames.len());
    for frame in &source_frames {
        let asset = get_asset(state, &frame.asset_id)?;
        let image = image::open(&asset.path)?.to_rgba8();
        let metrics = compute_metrics(&asset.id, &asset.path)?;
        decoded.push((frame.clone(), image, metrics.metrics.bounds, metrics.metrics.centroid));
    }
    let scale = if shared_scale {
        compute_shared_scale(&decoded, canvas_width, baseline_y, padding)
    } else {
        1.0
    };
    let mut reference_foot: Option<u32> = None;
    let mut repaired_frames = Vec::with_capacity(decoded.len());
    for (index, (frame, image, bounds, centroid)) in decoded.into_iter().enumerate() {
        let mut normalized = align_frame_to_baseline(
            &image,
            bounds,
            centroid,
            canvas_width,
            canvas_height,
            pivot_x,
            baseline_y,
            scale,
            padding,
        );
        if lock_first_frame {
            let current_foot = foot_y_on_canvas(&normalized);
            if index == 0 {
                reference_foot = current_foot;
            } else if let (Some(reference), Some(current)) = (reference_foot, current_foot) {
                let delta = reference as i32 - current as i32;
                if delta != 0 {
                    normalized = shift_canvas_vertical(&normalized, delta);
                }
            }
        }
        let file_name = format!("frame-{:02}.png", index + 1);
        let output_path = output_directory.join(&file_name);
        normalized.save(&output_path)?;
        let relative_path = format!(
            "assets/normalized/{}/{}",
            output_directory
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("frames"),
            file_name
        );
        let asset = Asset {
            id: Uuid::new_v4().to_string(),
            workspace_id: project_id.clone(),
            name: format!("{} frame {}", name, index + 1),
            path: output_path.to_string_lossy().into_owned(),
            relative_path,
            category: "characters".to_string(),
            format: "png".to_string(),
            width: normalized.width(),
            height: normalized.height(),
            file_size: std::fs::metadata(&output_path)?.len(),
            has_alpha: true,
            created_at: Utc::now().to_rfc3339(),
        };
        upsert(state, &asset, "normalized")?;
        repaired_frames.push(AnimationFrame {
            asset_id: asset.id,
            duration_ms: frame.duration_ms,
            offset_x: 0,
            offset_y: 0,
        });
    }
    save_animation_inner(
        AnimationInput {
            id: Some(input.animation_id.clone()),
            workspace_id: project_id,
            worktree_id,
            name,
            fps,
            looping,
            frames: repaired_frames,
            motion_plan: None,
            review_status: Some(review_status),
        },
        state,
    )
}

#[tauri::command]
pub async fn normalize_animation(
    input: NormalizeAnimationInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<Animation> {
    let task_app = app.clone();
    let task_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        normalize_animation_inner(Some(&task_app), &task_state, input)
    })
    .await
    .map_err(|error| CommandError::new("normalize_task_failed", error.to_string()))?
}
