use crate::{
    assets::{get_asset, inspect, upsert},
    error::{CommandError, CommandResult},
    models::{SplitStripResult, StripScoreReport, SubsampleVideoFramesInput, SubsampleVideoFramesResult},
    quality::pixel_difference,
    workspace::{resolve_ffmpeg_executable, workspace_path, FFMPEG_MISSING_DETAIL},
    AppState,
};
use image::{GenericImageView, RgbaImage};
use std::path::PathBuf;
use tauri::State;

use super::anchors::portable_slug;
use super::qc::{
    profile_horizontal_segments, score_strip_image, vertical_alpha_profile,
};
use super::segment::{dp_horizontal_segments, infer_frame_count_from_image, segments_cover_width};

pub(crate) fn foreground_bounds(image: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let (width, height) = image.dimensions();
    let mut minimum_x = width;
    let mut minimum_y = height;
    let mut maximum_x = 0;
    let mut maximum_y = 0;
    let mut found = false;
    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel[3] > 8 {
            found = true;
            minimum_x = minimum_x.min(x);
            minimum_y = minimum_y.min(y);
            maximum_x = maximum_x.max(x);
            maximum_y = maximum_y.max(y);
        }
    }
    if found {
        Some((minimum_x, minimum_y, maximum_x, maximum_y))
    } else {
        None
    }
}

pub(crate) fn recover_foreground(image: &RgbaImage) -> RgbaImage {
    let Some((minimum_x, minimum_y, maximum_x, maximum_y)) = foreground_bounds(image) else {
        return image.clone();
    };
    let width = maximum_x - minimum_x + 1;
    let height = maximum_y - minimum_y + 1;
    let mut recovered = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            recovered.put_pixel(x, y, *image.get_pixel(minimum_x + x, minimum_y + y));
        }
    }
    recovered
}

struct SegmentPlan {
    segments: Vec<(u32, u32)>,
    layout_used: String,
    suggested_frame_count: Option<u32>,
    inference_confidence: Option<f64>,
}

fn resolve_segment_plan(
    image: &RgbaImage,
    layout: &str,
    frame_count: u32,
) -> CommandResult<SegmentPlan> {
    let (width, _) = image.dimensions();
    let profile = vertical_alpha_profile(image);
    let layout = layout.trim().to_ascii_lowercase();

    let (resolved_count, suggested, confidence) = if layout == "auto" && frame_count == 0 {
        let inference = infer_frame_count_from_image(image);
        (
            inference.suggested_frame_count.max(1),
            Some(inference.suggested_frame_count),
            Some(inference.confidence),
        )
    } else if frame_count == 0 {
        return Err(CommandError::new(
            "invalid_frame_count",
            "Frame count must be at least 1 unless layout is \"auto\"",
        ));
    } else {
        (frame_count, None, None)
    };

    if layout == "horizontal" || layout == "strip" {
        let cell_width = width / resolved_count;
        if cell_width == 0 {
            return Err(CommandError::new(
                "invalid_strip",
                "Strip image is too narrow for the requested frame count",
            ));
        }
        let segments = (0..resolved_count)
            .map(|index| (index * cell_width, cell_width))
            .collect();
        return Ok(SegmentPlan {
            segments,
            layout_used: layout,
            suggested_frame_count: suggested,
            inference_confidence: confidence,
        });
    }

    if layout == "vertical" || layout == "grid" {
        return Err(CommandError::new(
            "invalid_layout",
            "Vertical and grid layouts are handled directly in split_cells",
        ));
    }

    if layout == "dp" {
        let segments = dp_horizontal_segments(width, &profile, resolved_count);
        if !segments_cover_width(&segments, width) {
            return Err(CommandError::new(
                "dp_split_failed",
                "Could not derive the requested number of frames with DP segmentation",
            ));
        }
        return Ok(SegmentPlan {
            segments,
            layout_used: "dp".into(),
            suggested_frame_count: suggested,
            inference_confidence: confidence,
        });
    }

    let profile_segments = profile_horizontal_segments(width, &profile, resolved_count);
    if segments_cover_width(&profile_segments, width) {
        return Ok(SegmentPlan {
            segments: profile_segments,
            layout_used: if layout == "auto" {
                "auto".into()
            } else {
                "profile".into()
            },
            suggested_frame_count: suggested,
            inference_confidence: confidence,
        });
    }

    let dp_segments = dp_horizontal_segments(width, &profile, resolved_count);
    if segments_cover_width(&dp_segments, width) {
        return Ok(SegmentPlan {
            segments: dp_segments,
            layout_used: "dp".into(),
            suggested_frame_count: suggested,
            inference_confidence: confidence,
        });
    }

    Err(CommandError::new(
        "profile_split_failed",
        "Could not derive the requested number of frames from the alpha profile",
    ))
}

pub(crate) fn split_cells(
    image: &RgbaImage,
    layout: &str,
    frame_count: u32,
    columns: Option<u32>,
) -> CommandResult<Vec<RgbaImage>> {
    let layout = layout.trim().to_ascii_lowercase();
    if layout == "grid" {
        if frame_count == 0 {
            return Err(CommandError::new(
                "invalid_frame_count",
                "Frame count must be at least 1 for grid layout",
            ));
        }
        let (width, height) = image.dimensions();
        let columns = columns.filter(|value| *value > 0).unwrap_or(frame_count);
        let rows = frame_count.div_ceil(columns);
        let cell_width = width / columns;
        let cell_height = height / rows;
        if cell_width == 0 || cell_height == 0 {
            return Err(CommandError::new(
                "invalid_grid",
                "Grid image is too small for the requested layout",
            ));
        }
        let mut frames = Vec::with_capacity(frame_count as usize);
        for index in 0..frame_count {
            let column = index % columns;
            let row = index / columns;
            let x = column * cell_width;
            let y = row * cell_height;
            frames.push(image.view(x, y, cell_width, cell_height).to_image());
        }
        return Ok(frames);
    }

    if layout == "vertical" {
        let (width, height) = image.dimensions();
        let cell_height = height / frame_count;
        if cell_height == 0 {
            return Err(CommandError::new(
                "invalid_strip",
                "Strip image is too short for the requested frame count",
            ));
        }
        let mut frames = Vec::with_capacity(frame_count as usize);
        for index in 0..frame_count {
            let y = index * cell_height;
            frames.push(image.view(0, y, width, cell_height).to_image());
        }
        return Ok(frames);
    }

    let plan = resolve_segment_plan(image, &layout, frame_count)?;
    let (_, height) = image.dimensions();
    let mut frames = Vec::with_capacity(plan.segments.len());
    for (x, segment_width) in plan.segments {
        frames.push(image.view(x, 0, segment_width, height).to_image());
    }
    Ok(frames)
}

pub(crate) fn score_strip_inner(source_path: &str, frame_count: Option<u32>, layout: Option<&str>) -> CommandResult<StripScoreReport> {
    let source = PathBuf::from(source_path);
    if !source.is_file() {
        return Err(CommandError::new(
            "strip_not_found",
            "The source strip image does not exist",
        ));
    }
    let image = image::open(&source)?.to_rgba8();
    Ok(score_strip_image(&image, frame_count, layout))
}

pub(crate) fn split_sprite_strip_inner(
    workspace_id: &str,
    source_path: &str,
    layout: &str,
    frame_count: u32,
    columns: Option<u32>,
    should_recover_foreground: bool,
    category: &str,
    state: &AppState,
) -> CommandResult<SplitStripResult> {
    let source = PathBuf::from(source_path);
    if !source.is_file() {
        return Err(CommandError::new(
            "strip_not_found",
            "The source strip image does not exist",
        ));
    }
    let image = image::open(&source)?.to_rgba8();
    let score = score_strip_image(
        &image,
        if frame_count == 0 { None } else { Some(frame_count) },
        Some(layout),
    );
    let resolved_count = if frame_count == 0 {
        score.frame_count_used
    } else {
        frame_count
    };
    let layout_normalized = layout.trim().to_ascii_lowercase();
    let plan = if layout_normalized == "grid" || layout_normalized == "vertical" {
        None
    } else {
        Some(resolve_segment_plan(&image, layout, resolved_count)?)
    };
    let warnings = score.warnings;
    let cells = split_cells(&image, layout, resolved_count, columns)?;
    let root = workspace_path(state, workspace_id)?;
    let batch = portable_slug(
        source
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("strip"),
    );
    let output_directory = root.join("assets").join(category).join(format!("{batch}-split"));
    std::fs::create_dir_all(&output_directory)?;
    let mut frame_paths = Vec::new();
    let mut relative_paths = Vec::new();
    let mut asset_ids = Vec::new();
    for (index, cell) in cells.into_iter().enumerate() {
        let frame = if should_recover_foreground {
            recover_foreground(&cell)
        } else {
            cell
        };
        let file_name = format!("frame-{:02}.png", index + 1);
        let output_path = output_directory.join(&file_name);
        frame.save(&output_path)?;
        let relative_path = format!(
            "assets/{}/{}/{}",
            category,
            output_directory
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("split"),
            file_name
        );
        let asset = inspect(workspace_id, &root, &output_path, None)?;
        upsert(state, &asset, "imported")?;
        frame_paths.push(asset.path);
        relative_paths.push(relative_path);
        asset_ids.push(asset.id);
    }
    Ok(SplitStripResult {
        frame_paths,
        asset_ids,
        relative_paths,
        warnings,
        suggested_frame_count: plan
            .as_ref()
            .and_then(|value| value.suggested_frame_count)
            .or(Some(score.suggested_frame_count)),
        frame_count_used: Some(resolved_count),
        inference_confidence: plan
            .as_ref()
            .and_then(|value| value.inference_confidence)
            .or(Some(score.inference_confidence)),
        layout_used: plan.map(|value| value.layout_used),
    })
}

pub(crate) fn extract_video_frames_inner(
    state: &AppState,
    workspace_id: &str,
    video_path: &str,
    fps: Option<f64>,
) -> CommandResult<SplitStripResult> {
    let video = PathBuf::from(video_path);
    if !video.is_file() {
        return Err(CommandError::new(
            "video_not_found",
            "The source video file does not exist",
        ));
    }
    let root = workspace_path(state, workspace_id)?;
    let output_directory = root.join("assets").join("imports").join(format!(
        "{}-frames",
        portable_slug(
            video
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("video")
        )
    ));
    std::fs::create_dir_all(&output_directory)?;
    let fps_value = fps.unwrap_or(10.0).max(1.0);
    let output_pattern = output_directory.join("frame-%03d.png");
    let ffmpeg = resolve_ffmpeg_executable().ok_or_else(|| {
        CommandError::new("ffmpeg_missing", FFMPEG_MISSING_DETAIL)
    })?;
    let status = std::process::Command::new(&ffmpeg)
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(video)
        .arg("-vf")
        .arg(format!("fps={fps_value}"))
        .arg(output_pattern)
        .status();
    match status {
        Ok(result) if result.success() => {}
        Ok(_) => {
            return Err(CommandError::new(
                "ffmpeg_failed",
                "ffmpeg could not extract frames from the video",
            ));
        }
        Err(error) => {
            return Err(CommandError::new(
                "ffmpeg_missing",
                format!("{FFMPEG_MISSING_DETAIL} ({error})"),
            ));
        }
    }
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&output_directory)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) == Some("png") {
            paths.push(path);
        }
    }
    paths.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    if paths.is_empty() {
        return Err(CommandError::new(
            "ffmpeg_no_frames",
            "ffmpeg completed but no PNG frames were written",
        ));
    }
    let mut frame_paths = Vec::with_capacity(paths.len());
    let mut relative_paths = Vec::with_capacity(paths.len());
    let mut asset_ids = Vec::with_capacity(paths.len());
    for path in paths {
        let asset = inspect(workspace_id, &root, &path, None)?;
        upsert(state, &asset, "imported")?;
        frame_paths.push(asset.path);
        relative_paths.push(asset.relative_path);
        asset_ids.push(asset.id);
    }
    let frame_count_used = asset_ids.len() as u32;
    Ok(SplitStripResult {
        frame_paths,
        asset_ids,
        relative_paths,
        warnings: Vec::new(),
        suggested_frame_count: None,
        frame_count_used: Some(frame_count_used),
        inference_confidence: None,
        layout_used: None,
    })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitStripCommandInput {
    pub workspace_id: String,
    pub source_path: String,
    pub layout: String,
    pub frame_count: u32,
    pub columns: Option<u32>,
    pub recover_foreground: Option<bool>,
    pub category: Option<String>,
}

#[tauri::command]
pub fn split_sprite_strip(
    input: SplitStripCommandInput,
    state: State<'_, AppState>,
) -> CommandResult<SplitStripResult> {
    split_sprite_strip_inner(
        &input.workspace_id,
        &input.source_path,
        &input.layout,
        input.frame_count,
        input.columns,
        input.recover_foreground.unwrap_or(true),
        input.category.as_deref().unwrap_or("characters"),
        &state,
    )
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreStripCommandInput {
    pub source_path: String,
    pub frame_count: Option<u32>,
    pub layout: Option<String>,
}

#[tauri::command]
pub fn score_sprite_strip(
    input: ScoreStripCommandInput,
    _state: State<'_, AppState>,
) -> CommandResult<StripScoreReport> {
    score_strip_inner(&input.source_path, input.frame_count, input.layout.as_deref())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractVideoFramesCommandInput {
    pub workspace_id: String,
    pub video_path: String,
    pub fps: Option<f64>,
}

#[tauri::command]
pub fn extract_video_frames(
    input: ExtractVideoFramesCommandInput,
    state: State<'_, AppState>,
) -> CommandResult<SplitStripResult> {
    extract_video_frames_inner(&state, &input.workspace_id, &input.video_path, input.fps)
}

pub(crate) fn subsample_video_frames_inner(
    state: &AppState,
    workspace_id: &str,
    asset_ids: &[String],
    min_delta: f64,
    stride: u32,
) -> CommandResult<SubsampleVideoFramesResult> {
    if asset_ids.is_empty() {
        return Err(CommandError::new(
            "empty_asset_list",
            "Provide at least one extracted frame asset id",
        ));
    }
    let stride = stride.max(1);
    let mut images = Vec::with_capacity(asset_ids.len());
    for asset_id in asset_ids {
        let asset = get_asset(state, asset_id)?;
        if asset.workspace_id != workspace_id {
            return Err(CommandError::new(
                "invalid_asset_workspace",
                "All frame assets must belong to the requested workspace",
            ));
        }
        images.push(image::open(&asset.path)?.to_rgba8());
    }
    let total = images.len();
    let mut kept_indices = Vec::new();
    let mut dropped_indices = Vec::new();
    let mut last_kept: Option<usize> = None;
    for index in 0..total {
        let keep = if index == 0 || index + 1 == total {
            true
        } else if stride > 1 && index % stride as usize == 0 {
            true
        } else if let Some(previous) = last_kept {
            pixel_difference(&images[previous], &images[index]) > min_delta
        } else {
            false
        };
        if keep {
            kept_indices.push(index as u32);
            last_kept = Some(index);
        } else {
            dropped_indices.push(index as u32);
        }
    }
    let kept_asset_ids = kept_indices
        .iter()
        .map(|index| asset_ids[*index as usize].clone())
        .collect();
    let dropped_asset_ids = dropped_indices
        .iter()
        .map(|index| asset_ids[*index as usize].clone())
        .collect();
    Ok(SubsampleVideoFramesResult {
        kept_asset_ids,
        dropped_asset_ids,
        kept_indices,
        dropped_indices,
    })
}

#[tauri::command]
pub fn subsample_video_frames(
    input: SubsampleVideoFramesInput,
    state: State<'_, AppState>,
) -> CommandResult<SubsampleVideoFramesResult> {
    subsample_video_frames_inner(
        &state,
        &input.workspace_id,
        &input.asset_ids,
        input.min_delta.unwrap_or(0.02),
        input.stride.unwrap_or(1),
    )
}
