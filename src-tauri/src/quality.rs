use crate::{
    animations::{load_animation_by_id, save_animation_inner},
    assets::{get_asset, inspect, upsert},
    error::{CommandError, CommandResult},
    jobs::{cancellation_requested, load_job, set_job_state, JobProgress},
    models::{
        Animation, AnimationFrame, AnimationInput, BackgroundJob, FrameOptimizationInput,
        FrameOptimizationResult, MotionPlan, QualityCheck, QualityReport,
    },
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use image::{imageops::FilterType, RgbaImage};
use rusqlite::{params, OptionalExtension};
use std::path::Path;
use tauri::{Emitter, Manager, State};
use uuid::Uuid;

const ANALYZER_VERSION: &str = "native-v1";

#[derive(Clone)]
struct FrameMetrics {
    asset_id: String,
    content_hash: String,
    width: u32,
    height: u32,
    bounds: Option<(u32, u32, u32, u32)>,
    centroid: Option<(f64, f64)>,
    alpha_coverage: f64,
    opaque_edge_pixels: u32,
    perceptual_hash: u64,
    palette: (f64, f64, f64),
}

struct AnalyzedFrame {
    metrics: FrameMetrics,
    image: RgbaImage,
}

struct PendingCheck {
    check_type: &'static str,
    frame_index: Option<u32>,
    comparison_frame_index: Option<u32>,
    severity: &'static str,
    score: f64,
    message: String,
    metric_value: Option<f64>,
    metric_unit: Option<&'static str>,
    repair_action: Option<&'static str>,
}

fn report_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<QualityReport> {
    Ok(QualityReport {
        id: row.get(0)?,
        project_id: row.get(1)?,
        worktree_id: row.get(2)?,
        animation_id: row.get(3)?,
        job_id: row.get(4)?,
        status: row.get(5)?,
        overall_score: row.get(6)?,
        character_consistency_score: row.get(7)?,
        motion_continuity_score: row.get(8)?,
        frame_alignment_score: row.get(9)?,
        weapon_consistency_score: row.get(10)?,
        loop_quality_score: row.get(11)?,
        transparency_score: row.get(12)?,
        frame_count: row.get(13)?,
        analyzer_version: row.get(14)?,
        checks: Vec::new(),
        created_at: row.get(15)?,
        completed_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}

fn select_report() -> &'static str {
    r#"SELECT id, project_id, worktree_id, animation_id, job_id, status,
              overall_score, character_consistency_score, motion_continuity_score,
              frame_alignment_score, weapon_consistency_score, loop_quality_score,
              transparency_score, frame_count, analyzer_version, created_at,
              completed_at, updated_at
       FROM quality_reports"#
}

fn load_checks(
    connection: &rusqlite::Connection,
    report_id: &str,
) -> CommandResult<Vec<QualityCheck>> {
    let mut statement = connection.prepare(
        r#"SELECT qc.id, qc.report_id, qc.position, qc.check_type, qc.frame_index,
                  qc.comparison_frame_index, qc.severity, qc.score, qc.message,
                  qc.metric_value, qc.metric_unit, qc.repair_action,
                  COALESCE(qw.acknowledged,0), COALESCE(qw.ignored,0), qc.created_at
           FROM quality_checks qc
           LEFT JOIN quality_warnings qw ON qw.check_id=qc.id
           WHERE qc.report_id=?1 ORDER BY qc.position"#,
    )?;
    let rows = statement.query_map([report_id], |row| {
        Ok(QualityCheck {
            id: row.get(0)?,
            report_id: row.get(1)?,
            position: row.get(2)?,
            check_type: row.get(3)?,
            frame_index: row.get(4)?,
            comparison_frame_index: row.get(5)?,
            severity: row.get(6)?,
            score: row.get(7)?,
            message: row.get(8)?,
            metric_value: row.get(9)?,
            metric_unit: row.get(10)?,
            repair_action: row.get(11)?,
            acknowledged: row.get(12)?,
            ignored: row.get(13)?,
            created_at: row.get(14)?,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn hydrate_report(
    connection: &rusqlite::Connection,
    report: &mut QualityReport,
) -> CommandResult<()> {
    report.checks = load_checks(connection, &report.id)?;
    Ok(())
}

fn content_hash(path: &str) -> CommandResult<String> {
    Ok(blake3::hash(&std::fs::read(path)?).to_hex().to_string())
}

fn compute_metrics(asset_id: &str, path: &str) -> CommandResult<AnalyzedFrame> {
    let image = image::open(path)?.to_rgba8();
    let (width, height) = image.dimensions();
    let mut minimum_x = width;
    let mut minimum_y = height;
    let mut maximum_x = 0;
    let mut maximum_y = 0;
    let mut alpha_pixels = 0_u64;
    let mut edge_pixels = 0_u32;
    let mut weighted_x = 0_f64;
    let mut weighted_y = 0_f64;
    let mut alpha_weight = 0_f64;
    let mut red = 0_f64;
    let mut green = 0_f64;
    let mut blue = 0_f64;
    for (x, y, pixel) in image.enumerate_pixels() {
        let alpha = pixel[3];
        if alpha > 8 {
            alpha_pixels += 1;
            minimum_x = minimum_x.min(x);
            minimum_y = minimum_y.min(y);
            maximum_x = maximum_x.max(x);
            maximum_y = maximum_y.max(y);
            if x == 0 || y == 0 || x + 1 == width || y + 1 == height {
                edge_pixels += 1;
            }
            let weight = alpha as f64 / 255.0;
            weighted_x += x as f64 * weight;
            weighted_y += y as f64 * weight;
            alpha_weight += weight;
            red += pixel[0] as f64 * weight;
            green += pixel[1] as f64 * weight;
            blue += pixel[2] as f64 * weight;
        }
    }
    let bounds = (alpha_pixels > 0).then_some((minimum_x, minimum_y, maximum_x, maximum_y));
    let centroid =
        (alpha_weight > 0.0).then_some((weighted_x / alpha_weight, weighted_y / alpha_weight));
    let palette = if alpha_weight > 0.0 {
        (
            red / alpha_weight,
            green / alpha_weight,
            blue / alpha_weight,
        )
    } else {
        (0.0, 0.0, 0.0)
    };
    let gray = image::imageops::grayscale(&image);
    let small = image::imageops::resize(&gray, 8, 8, FilterType::Triangle);
    let average = small.pixels().map(|pixel| pixel[0] as u64).sum::<u64>() / 64;
    let mut perceptual_hash = 0_u64;
    for (index, pixel) in small.pixels().enumerate() {
        if pixel[0] as u64 >= average {
            perceptual_hash |= 1_u64 << index;
        }
    }
    Ok(AnalyzedFrame {
        metrics: FrameMetrics {
            asset_id: asset_id.to_string(),
            content_hash: content_hash(path)?,
            width,
            height,
            bounds,
            centroid,
            alpha_coverage: alpha_pixels as f64 / (width as f64 * height as f64).max(1.0),
            opaque_edge_pixels: edge_pixels,
            perceptual_hash,
            palette,
        },
        image,
    })
}

fn cache_metrics(state: &AppState, metrics: &FrameMetrics) -> CommandResult<()> {
    let now = Utc::now().to_rfc3339();
    let bounds = metrics.bounds;
    let centroid = metrics.centroid;
    let palette = format!(
        "{:.2},{:.2},{:.2}",
        metrics.palette.0, metrics.palette.1, metrics.palette.2
    );
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        r#"INSERT INTO frame_quality_cache(
            asset_id, content_hash, width, height, alpha_min_x, alpha_min_y,
            alpha_max_x, alpha_max_y, centroid_x, centroid_y, alpha_coverage,
            opaque_edge_pixels, perceptual_hash, palette_signature, updated_at
        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
        ON CONFLICT(asset_id) DO UPDATE SET content_hash=excluded.content_hash,
            width=excluded.width, height=excluded.height, alpha_min_x=excluded.alpha_min_x,
            alpha_min_y=excluded.alpha_min_y, alpha_max_x=excluded.alpha_max_x,
            alpha_max_y=excluded.alpha_max_y, centroid_x=excluded.centroid_x,
            centroid_y=excluded.centroid_y, alpha_coverage=excluded.alpha_coverage,
            opaque_edge_pixels=excluded.opaque_edge_pixels,
            perceptual_hash=excluded.perceptual_hash,
            palette_signature=excluded.palette_signature, updated_at=excluded.updated_at"#,
        params![
            metrics.asset_id,
            metrics.content_hash,
            metrics.width,
            metrics.height,
            bounds.map(|value| value.0),
            bounds.map(|value| value.1),
            bounds.map(|value| value.2),
            bounds.map(|value| value.3),
            centroid.map(|value| value.0),
            centroid.map(|value| value.1),
            metrics.alpha_coverage,
            metrics.opaque_edge_pixels,
            format!("{:016x}", metrics.perceptual_hash),
            palette,
            now
        ],
    )?;
    Ok(())
}

fn load_cached_metrics(
    state: &AppState,
    asset_id: &str,
    expected_hash: &str,
) -> CommandResult<Option<FrameMetrics>> {
    type CachedFrame = (
        String,
        u32,
        u32,
        Option<u32>,
        Option<u32>,
        Option<u32>,
        Option<u32>,
        Option<f64>,
        Option<f64>,
        f64,
        u32,
        String,
        String,
    );
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let cached: Option<CachedFrame> = connection
        .query_row(
            r#"SELECT content_hash,width,height,alpha_min_x,alpha_min_y,alpha_max_x,
                      alpha_max_y,centroid_x,centroid_y,alpha_coverage,
                      opaque_edge_pixels,perceptual_hash,palette_signature
               FROM frame_quality_cache WHERE asset_id=?1"#,
            [asset_id],
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
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                ))
            },
        )
        .optional()?;
    let Some((
        hash,
        width,
        height,
        min_x,
        min_y,
        max_x,
        max_y,
        centroid_x,
        centroid_y,
        coverage,
        edge_pixels,
        perceptual,
        palette,
    )) = cached
    else {
        return Ok(None);
    };
    if hash != expected_hash {
        return Ok(None);
    }
    let palette: Vec<_> = palette
        .split(',')
        .filter_map(|value| value.parse::<f64>().ok())
        .collect();
    let bounds = match (min_x, min_y, max_x, max_y) {
        (Some(minimum_x), Some(minimum_y), Some(maximum_x), Some(maximum_y)) => {
            Some((minimum_x, minimum_y, maximum_x, maximum_y))
        }
        _ => None,
    };
    let centroid = centroid_x.zip(centroid_y);
    Ok(Some(FrameMetrics {
        asset_id: asset_id.to_string(),
        content_hash: hash,
        width,
        height,
        bounds,
        centroid,
        alpha_coverage: coverage,
        opaque_edge_pixels: edge_pixels,
        perceptual_hash: u64::from_str_radix(&perceptual, 16).unwrap_or_default(),
        palette: (
            palette.first().copied().unwrap_or_default(),
            palette.get(1).copied().unwrap_or_default(),
            palette.get(2).copied().unwrap_or_default(),
        ),
    }))
}

fn pixel_difference(first: &RgbaImage, second: &RgbaImage) -> f64 {
    let width = first.width().max(second.width()).max(1);
    let height = first.height().max(second.height()).max(1);
    let first = image::imageops::resize(first, width, height, FilterType::Nearest);
    let second = image::imageops::resize(second, width, height, FilterType::Nearest);
    let total = first
        .pixels()
        .zip(second.pixels())
        .map(|(left, right)| {
            (0..4)
                .map(|channel| (left[channel] as f64 - right[channel] as f64).abs())
                .sum::<f64>()
        })
        .sum::<f64>();
    total / (width as f64 * height as f64 * 4.0 * 255.0)
}

fn palette_distance(first: &FrameMetrics, second: &FrameMetrics) -> f64 {
    ((first.palette.0 - second.palette.0).powi(2)
        + (first.palette.1 - second.palette.1).powi(2)
        + (first.palette.2 - second.palette.2).powi(2))
    .sqrt()
}

fn centroid_distance(first: &FrameMetrics, second: &FrameMetrics) -> f64 {
    match (first.centroid, second.centroid) {
        (Some(first), Some(second)) => {
            ((first.0 - second.0).powi(2) + (first.1 - second.1).powi(2)).sqrt()
        }
        _ => 0.0,
    }
}

fn bounds_area(metrics: &FrameMetrics) -> f64 {
    metrics
        .bounds
        .map(|(minimum_x, minimum_y, maximum_x, maximum_y)| {
            (maximum_x - minimum_x + 1) as f64 * (maximum_y - minimum_y + 1) as f64
        })
        .unwrap_or(0.0)
}

fn score_with_penalty(penalty: f64) -> f64 {
    (100.0 - penalty).clamp(0.0, 100.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LimbBlob {
    min_x: u32,
    max_x: u32,
    pixels: u32,
    luminance: f64,
}

/// Classification of a frame's lower body: two separated legs, an occupied
/// band whose legs cannot be told apart (fused or shredded), or no lower
/// body at all.
#[derive(Debug, PartialEq)]
enum LowerBodyView {
    TwoBlobs(Vec<LimbBlob>),
    Indistinct,
    Empty,
}

/// Segments the lower body band into leg blobs, left to right. A single
/// column run means the legs fused (or a trailing scarf connected them);
/// three or more runs are equally unmeasurable. Both become `Indistinct`.
fn lower_body_view(image: &RgbaImage, bounds: Option<(u32, u32, u32, u32)>) -> LowerBodyView {
    let Some((min_x, min_y, max_x, max_y)) = bounds else {
        return LowerBodyView::Empty;
    };
    if max_x < min_x || max_y <= min_y + 2 {
        return LowerBodyView::Empty;
    }
    let band_start = min_y + ((max_y - min_y) as f64 * 0.66) as u32;
    if band_start >= max_y {
        return LowerBodyView::Empty;
    }
    let width = (max_x - min_x + 1) as usize;
    let mut occupied = vec![false; width];
    let mut occupied_columns = 0_usize;
    for y in band_start..=max_y {
        for x in min_x..=max_x {
            if image.get_pixel(x, y)[3] > 8 {
                let index = (x - min_x) as usize;
                if !occupied[index] {
                    occupied[index] = true;
                    occupied_columns += 1;
                }
            }
        }
    }
    if occupied_columns == 0 {
        return LowerBodyView::Empty;
    }
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    for (index, is_occupied) in occupied.iter().copied().enumerate().take(width) {
        if is_occupied && start.is_none() {
            start = Some(index);
        }
        if start.is_some() && (!is_occupied || index + 1 == width) {
            let begin = start.take().expect("run start");
            let end = if is_occupied { index } else { index - 1 };
            runs.push((begin, end));
        }
    }
    let runs: Vec<_> = runs
        .into_iter()
        .filter(|(begin, end)| end - begin + 1 >= 2)
        .collect();
    if runs.len() != 2 {
        return LowerBodyView::Indistinct;
    }
    let mut blobs = Vec::with_capacity(2);
    for (begin, end) in runs {
        let mut weight = 0.0;
        let mut luminance = 0.0;
        let mut pixels = 0_u32;
        for y in band_start..=max_y {
            for x in (min_x + begin as u32)..=(min_x + end as u32) {
                let pixel = image.get_pixel(x, y);
                let alpha = pixel[3];
                if alpha > 8 {
                    let alpha_weight = alpha as f64 / 255.0;
                    let value = 0.299 * pixel[0] as f64
                        + 0.587 * pixel[1] as f64
                        + 0.114 * pixel[2] as f64;
                    weight += alpha_weight;
                    luminance += value * alpha_weight;
                    pixels += 1;
                }
            }
        }
        if pixels < 8 || weight <= 0.0 {
            return LowerBodyView::Indistinct;
        }
        blobs.push(LimbBlob {
            min_x: min_x + begin as u32,
            max_x: min_x + end as u32,
            pixels,
            luminance: luminance / weight,
        });
    }
    LowerBodyView::TwoBlobs(blobs)
}

/// Detects a broken far-limb shading lock. In a correct paired-limb cycle the
/// far leg stays visibly darker than the near leg in every frame where both
/// legs are separate; frames that drop the distinction read as a limb swap.
fn limb_shading_checks(analyzed: &[AnalyzedFrame]) -> Vec<PendingCheck> {
    let gaps: Vec<Option<(usize, f64)>> = analyzed
        .iter()
        .enumerate()
        .map(|(index, frame)| match lower_body_view(&frame.image, frame.metrics.bounds) {
            LowerBodyView::TwoBlobs(blobs) => {
                Some((index, (blobs[0].luminance - blobs[1].luminance).abs()))
            }
            _ => None,
        })
        .collect();
    let measurable: Vec<(usize, f64)> = gaps.into_iter().flatten().collect();
    if measurable.len() < 3 {
        return Vec::new();
    }
    // 8/255 is the smallest gap that still reads as two shades at 1× scale;
    // measured real cycles sit near 9 (subtle palettes) to 100 (high contrast).
    let strong = measurable
        .iter()
        .filter(|(_, gap)| *gap >= 8.0)
        .count();
    if strong < 2 || (strong as f64 / measurable.len() as f64) < 0.6 {
        return Vec::new();
    }
    measurable
        .iter()
        .filter(|(_, gap)| *gap < 4.0)
        .map(|(index, gap)| PendingCheck {
            check_type: "limb_identity",
            frame_index: Some(*index as u32),
            comparison_frame_index: None,
            severity: "warning",
            score: score_with_penalty(24.0),
            message: format!(
                "Frame {} lost the far-limb shading lock: both legs read as the same limb (shading gap {:.1}/255). Regenerate it with the far leg visibly darker.",
                index + 1,
                gap
            ),
            metric_value: Some(*gap),
            metric_unit: Some("luminance gap"),
            repair_action: Some("regenerate"),
        })
        .collect()
}

/// Detects a hop masquerading as a run: a paired-limb cycle needs visible leg
/// alternation, so frames where the legs fuse into one silhouette should be
/// the minority (passing or gathered poses). When at least one frame proves
/// the character's legs do separate, an indistinct majority means the cycle
/// collapsed into synchronized legs and must be regenerated.
fn leg_alternation_checks(analyzed: &[AnalyzedFrame]) -> Vec<PendingCheck> {
    let mut separated = 0_usize;
    let mut indistinct = 0_usize;
    for frame in analyzed {
        match lower_body_view(&frame.image, frame.metrics.bounds) {
            LowerBodyView::TwoBlobs(_) => separated += 1,
            LowerBodyView::Indistinct => indistinct += 1,
            LowerBodyView::Empty => {}
        }
    }
    let total = separated + indistinct;
    if total < 5 || separated == 0 {
        return Vec::new();
    }
    let ratio = indistinct as f64 / total as f64;
    if ratio <= 0.6 {
        return Vec::new();
    }
    let severe = ratio > 0.75;
    vec![PendingCheck {
        check_type: "leg_separation",
        frame_index: None,
        comparison_frame_index: None,
        severity: if severe { "error" } else { "warning" },
        score: score_with_penalty(if severe { 35.0 } else { 18.0 }),
        message: format!(
            "The legs read as one silhouette in {indistinct} of {total} frames, so the cycle lost leg alternation and plays as a hop. Regenerate it with two wide split stances half a cycle apart (NEAR contact, then FAR contact) and keep the far leg visible as the darker shape behind the near leg in every gathered pose. If AI frames keep failing here, rig the sprite in the Rig tab and render deterministically."
        ),
        metric_value: Some(ratio * 100.0),
        metric_unit: Some("% frames with fused legs"),
        repair_action: Some("regenerate"),
    }]
}

fn run_analysis(
    app: &tauri::AppHandle,
    state: &AppState,
    job_id: &str,
    report_id: &str,
    animation_id: &str,
) -> CommandResult<QualityReport> {
    let (project_id, worktree_id, looping, frames_json): (String, Option<String>, bool, String) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                "SELECT workspace_id, worktree_id, looping, frames_json FROM animations WHERE id=?1",
                [animation_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "The animation no longer exists"))?
    };
    let frames: Vec<AnimationFrame> = serde_json::from_str(&frames_json).unwrap_or_default();
    if frames.is_empty() {
        return Err(CommandError::new(
            "empty_animation",
            "Add frames before running quality analysis",
        ));
    }
    set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "running",
            progress: 0.03,
            stage: "Inspecting frame pixels",
            error_message: None,
            result_path: None,
        },
    )?;
    let mut analyzed = Vec::with_capacity(frames.len());
    for (index, frame) in frames.iter().enumerate() {
        if cancellation_requested(state, job_id)? {
            return Err(CommandError::new(
                "job_cancelled",
                "Quality analysis cancelled",
            ));
        }
        let asset = get_asset(state, &frame.asset_id)?;
        let hash = content_hash(&asset.path)?;
        if let Some(metrics) = load_cached_metrics(state, &asset.id, &hash)? {
            analyzed.push(AnalyzedFrame {
                metrics,
                image: image::open(&asset.path)?.to_rgba8(),
            });
        } else {
            let frame = compute_metrics(&asset.id, &asset.path)?;
            cache_metrics(state, &frame.metrics)?;
            analyzed.push(frame);
        }
        set_job_state(
            app,
            state,
            job_id,
            JobProgress {
                status: "running",
                progress: 0.05 + 0.40 * ((index + 1) as f64 / frames.len() as f64),
                stage: &format!("Inspecting frame {} of {}", index + 1, frames.len()),
                error_message: None,
                result_path: None,
            },
        )?;
    }
    set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "analyzing",
            progress: 0.48,
            stage: "Comparing motion and silhouettes",
            error_message: None,
            result_path: None,
        },
    )?;
    let mut checks = Vec::new();
    let mut alignment_penalty = 0.0;
    let mut continuity_penalty = 0.0;
    let mut consistency_penalty = 0.0;
    let mut weapon_penalty = 0.0;
    let mut transparency_penalty = 0.0;
    let expected_width = analyzed[0].metrics.width;
    let expected_height = analyzed[0].metrics.height;
    for (index, frame) in analyzed.iter().enumerate() {
        let metrics = &frame.metrics;
        if metrics.width != expected_width || metrics.height != expected_height {
            consistency_penalty += 25.0;
            checks.push(PendingCheck {
                check_type: "dimensions",
                frame_index: Some(index as u32),
                comparison_frame_index: None,
                severity: "error",
                score: 30.0,
                message: format!(
                    "Frame {} is {}×{} instead of {}×{}.",
                    index + 1,
                    metrics.width,
                    metrics.height,
                    expected_width,
                    expected_height
                ),
                metric_value: None,
                metric_unit: None,
                repair_action: Some("normalize_dimensions"),
            });
        }
        if metrics.alpha_coverage > 0.96 {
            transparency_penalty += 35.0;
            checks.push(PendingCheck {
                check_type: "transparency",
                frame_index: Some(index as u32),
                comparison_frame_index: None,
                severity: "error",
                score: 35.0,
                message: format!(
                    "Frame {} is almost fully opaque; a background may be baked in.",
                    index + 1
                ),
                metric_value: Some(metrics.alpha_coverage * 100.0),
                metric_unit: Some("% opaque area"),
                repair_action: Some("inspect_transparency"),
            });
        }
        if metrics.opaque_edge_pixels > 0 {
            transparency_penalty += (metrics.opaque_edge_pixels as f64 / 8.0).min(12.0);
            checks.push(PendingCheck {
                check_type: "boundary",
                frame_index: Some(index as u32),
                comparison_frame_index: None,
                severity: if metrics.opaque_edge_pixels > 8 {
                    "error"
                } else {
                    "warning"
                },
                score: score_with_penalty(metrics.opaque_edge_pixels as f64 * 2.0),
                message: format!(
                    "Frame {} touches the canvas boundary at {} pixels.",
                    index + 1,
                    metrics.opaque_edge_pixels
                ),
                metric_value: Some(metrics.opaque_edge_pixels as f64),
                metric_unit: Some("edge pixels"),
                repair_action: Some("add_padding"),
            });
        }
    }
    for index in 1..analyzed.len() {
        if cancellation_requested(state, job_id)? {
            return Err(CommandError::new(
                "job_cancelled",
                "Quality analysis cancelled",
            ));
        }
        let previous = &analyzed[index - 1];
        let current = &analyzed[index];
        let difference = pixel_difference(&previous.image, &current.image);
        let hash_distance =
            (previous.metrics.perceptual_hash ^ current.metrics.perceptual_hash).count_ones();
        let drift = centroid_distance(&previous.metrics, &current.metrics);
        let diagonal = ((expected_width.pow(2) + expected_height.pow(2)) as f64)
            .sqrt()
            .max(1.0);
        let palette = palette_distance(&previous.metrics, &current.metrics);
        let previous_area = bounds_area(&previous.metrics).max(1.0);
        let area_ratio = bounds_area(&current.metrics) / previous_area;
        if difference < 0.012 || hash_distance <= 1 {
            continuity_penalty += 12.0;
            checks.push(PendingCheck {
                check_type: "duplicate",
                frame_index: Some(index as u32),
                comparison_frame_index: Some((index - 1) as u32),
                severity: "warning",
                score: 45.0,
                message: format!(
                    "Frames {} and {} appear nearly identical.",
                    index,
                    index + 1
                ),
                metric_value: Some(difference * 100.0),
                metric_unit: Some("% pixel difference"),
                repair_action: Some("remove_duplicate"),
            });
        }
        if difference > 0.38 {
            continuity_penalty += (difference * 35.0).min(22.0);
            weapon_penalty += 8.0;
            checks.push(PendingCheck {
                check_type: "sudden_change",
                frame_index: Some(index as u32),
                comparison_frame_index: Some((index - 1) as u32),
                severity: if difference > 0.55 {
                    "error"
                } else {
                    "warning"
                },
                score: score_with_penalty(difference * 100.0),
                message: format!(
                    "Large visual change detected between Frames {} and {}.",
                    index,
                    index + 1
                ),
                metric_value: Some(difference * 100.0),
                metric_unit: Some("% pixel difference"),
                repair_action: Some("regenerate_transition"),
            });
        }
        if drift / diagonal > 0.10 {
            alignment_penalty += (drift / diagonal * 100.0).min(25.0);
            checks.push(PendingCheck {
                check_type: "alignment",
                frame_index: Some(index as u32),
                comparison_frame_index: Some((index - 1) as u32),
                severity: if drift / diagonal > 0.20 {
                    "error"
                } else {
                    "warning"
                },
                score: score_with_penalty(drift / diagonal * 180.0),
                message: format!(
                    "Subject drifted {:.1} px between Frames {} and {}.",
                    drift,
                    index,
                    index + 1
                ),
                metric_value: Some(drift),
                metric_unit: Some("pixels"),
                repair_action: Some("auto_align"),
            });
        }
        if !(0.72..=1.38).contains(&area_ratio) {
            consistency_penalty += ((1.0 - area_ratio).abs() * 24.0).min(18.0);
            checks.push(PendingCheck {
                check_type: "scale_consistency",
                frame_index: Some(index as u32),
                comparison_frame_index: Some((index - 1) as u32),
                severity: "warning",
                score: score_with_penalty((1.0 - area_ratio).abs() * 100.0),
                message: format!("Subject scale changes sharply at Frame {}.", index + 1),
                metric_value: Some(area_ratio),
                metric_unit: Some("area ratio"),
                repair_action: Some("normalize_scale"),
            });
        }
        if palette > 58.0 {
            consistency_penalty += (palette / 12.0).min(12.0);
            weapon_penalty += 4.0;
            checks.push(PendingCheck {
                check_type: "palette_consistency",
                frame_index: Some(index as u32),
                comparison_frame_index: Some((index - 1) as u32),
                severity: "warning",
                score: score_with_penalty(palette * 0.7),
                message: format!("Palette may have shifted in Frame {}.", index + 1),
                metric_value: Some(palette),
                metric_unit: Some("RGB distance"),
                repair_action: Some("lock_palette"),
            });
        }
        set_job_state(
            app,
            state,
            job_id,
            JobProgress {
                status: "analyzing",
                progress: 0.48 + 0.37 * (index as f64 / analyzed.len() as f64),
                stage: &format!("Comparing transition {} of {}", index, analyzed.len() - 1),
                error_message: None,
                result_path: None,
            },
        )?;
    }
    let limb_checks = limb_shading_checks(&analyzed);
    if !limb_checks.is_empty() {
        consistency_penalty += (limb_checks.len() as f64 * 6.0).min(18.0);
        checks.extend(limb_checks);
    }
    for check in leg_alternation_checks(&analyzed) {
        consistency_penalty += if check.severity == "error" {
            20.0
        } else {
            8.0
        };
        checks.push(check);
    }
    let loop_quality_score = if looping && analyzed.len() > 1 {
        let difference = pixel_difference(&analyzed[analyzed.len() - 1].image, &analyzed[0].image);
        let drift = centroid_distance(&analyzed[analyzed.len() - 1].metrics, &analyzed[0].metrics);
        let score = score_with_penalty(difference * 120.0 + drift * 2.0);
        if score < 75.0 {
            checks.push(PendingCheck {
                check_type: "loop_transition",
                frame_index: Some((analyzed.len() - 1) as u32),
                comparison_frame_index: Some(0),
                severity: if score < 45.0 { "error" } else { "warning" },
                score,
                message: "The final-to-first transition may produce a visible loop jump.".into(),
                metric_value: Some(difference * 100.0),
                metric_unit: Some("% pixel difference"),
                repair_action: Some("repair_loop"),
            });
        }
        score
    } else {
        100.0
    };
    let character_consistency_score = score_with_penalty(consistency_penalty);
    let motion_continuity_score = score_with_penalty(continuity_penalty);
    let frame_alignment_score = score_with_penalty(alignment_penalty);
    let weapon_consistency_score = score_with_penalty(weapon_penalty);
    let transparency_score = score_with_penalty(transparency_penalty);
    let overall_score = (character_consistency_score * 0.22
        + motion_continuity_score * 0.24
        + frame_alignment_score * 0.18
        + weapon_consistency_score * 0.10
        + loop_quality_score * 0.12
        + transparency_score * 0.14)
        .round();
    if checks.is_empty() {
        checks.push(PendingCheck {
            check_type: "summary",
            frame_index: None,
            comparison_frame_index: None,
            severity: "info",
            score: overall_score,
            message: "No deterministic quality warnings were detected. Visual review is still recommended.".into(),
            metric_value: None,
            metric_unit: None,
            repair_action: None,
        });
    }
    let completed_at = Utc::now().to_rfc3339();
    {
        let mut connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let transaction = connection.transaction()?;
        transaction.execute(
            r#"UPDATE quality_reports SET status='completed', overall_score=?2,
                character_consistency_score=?3, motion_continuity_score=?4,
                frame_alignment_score=?5, weapon_consistency_score=?6,
                loop_quality_score=?7, transparency_score=?8,
                completed_at=?9, updated_at=?9 WHERE id=?1"#,
            params![
                report_id,
                overall_score,
                character_consistency_score,
                motion_continuity_score,
                frame_alignment_score,
                weapon_consistency_score,
                loop_quality_score,
                transparency_score,
                completed_at
            ],
        )?;
        for (position, check) in checks.iter().enumerate() {
            let check_id = Uuid::new_v4().to_string();
            transaction.execute(
                r#"INSERT INTO quality_checks(
                    id, report_id, position, check_type, frame_index,
                    comparison_frame_index, severity, score, message,
                    metric_value, metric_unit, repair_action, created_at
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"#,
                params![
                    check_id,
                    report_id,
                    position as u32,
                    check.check_type,
                    check.frame_index,
                    check.comparison_frame_index,
                    check.severity,
                    check.score,
                    check.message,
                    check.metric_value,
                    check.metric_unit,
                    check.repair_action,
                    completed_at
                ],
            )?;
            if check.severity != "info" {
                transaction.execute(
                    "INSERT INTO quality_warnings(id,report_id,check_id,created_at,updated_at) VALUES (?1,?2,?3,?4,?4)",
                    params![Uuid::new_v4().to_string(), report_id, check_id, completed_at],
                )?;
            }
        }
        transaction.commit()?;
    }
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut report = connection.query_row(
        &format!("{} WHERE id=?1", select_report()),
        [report_id],
        report_row,
    )?;
    hydrate_report(&connection, &mut report)?;
    let _ = project_id;
    let _ = worktree_id;
    Ok(report)
}

#[tauri::command]
pub fn get_quality_report(
    animation_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Option<QualityReport>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut report = connection
        .query_row(
            &format!(
                "{} WHERE animation_id=?1 ORDER BY created_at DESC LIMIT 1",
                select_report()
            ),
            [animation_id],
            report_row,
        )
        .optional()?;
    if let Some(report) = &mut report {
        hydrate_report(&connection, report)?;
    }
    Ok(report)
}

#[tauri::command]
pub fn acknowledge_quality_check(
    check_id: String,
    ignored: bool,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let now = Utc::now().to_rfc3339();
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let changed = connection.execute(
        "UPDATE quality_warnings SET acknowledged=1, ignored=?2, updated_at=?3 WHERE check_id=?1",
        params![check_id, ignored, now],
    )?;
    if changed == 0 {
        return Err(CommandError::new(
            "quality_check_not_found",
            "The quality warning no longer exists",
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn queue_quality_analysis(
    animation_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<BackgroundJob> {
    let (project_id, worktree_id, frame_count): (String, Option<String>, u32) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                "SELECT workspace_id, worktree_id, json_array_length(frames_json) FROM animations WHERE id=?1",
                [&animation_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "The animation no longer exists"))?
    };
    if frame_count == 0 {
        return Err(CommandError::new(
            "empty_animation",
            "Add frames before running quality analysis",
        ));
    }
    let active_job_id: Option<String> = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                r#"SELECT bj.id FROM background_jobs bj
                   JOIN quality_reports qr ON qr.id=bj.target_id
                   WHERE qr.animation_id=?1 AND bj.kind='quality_analysis'
                     AND bj.status IN ('queued','running','analyzing')
                   ORDER BY bj.created_at DESC LIMIT 1"#,
                [&animation_id],
                |row| row.get(0),
            )
            .optional()?
    };
    if let Some(active_job_id) = active_job_id {
        return load_job(&state, &active_job_id);
    }
    let job_id = Uuid::new_v4().to_string();
    let report_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    {
        let mut connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let transaction = connection.transaction()?;
        transaction.execute(
            r#"INSERT INTO background_jobs(
                id,project_id,worktree_id,kind,target_type,target_id,status,
                progress,stage,created_at,updated_at
            ) VALUES (?1,?2,?3,'quality_analysis','quality_report',?4,'queued',0.0,'Queued',?5,?5)"#,
            params![job_id, project_id, worktree_id, report_id, now],
        )?;
        transaction.execute(
            r#"INSERT INTO quality_reports(
                id,project_id,worktree_id,animation_id,job_id,status,frame_count,
                analyzer_version,created_at,updated_at
            ) VALUES (?1,?2,?3,?4,?5,'running',?6,?7,?8,?8)"#,
            params![
                report_id,
                project_id,
                worktree_id,
                animation_id,
                job_id,
                frame_count,
                ANALYZER_VERSION,
                now
            ],
        )?;
        transaction.commit()?;
    }
    let queued = load_job(&state, &job_id)?;
    app.emit(
        "job-event",
        crate::models::JobEvent {
            job: queued.clone(),
        },
    )
    .map_err(|error| CommandError::new("event_error", error.to_string()))?;
    let task_app = app.clone();
    let task_job_id = job_id.clone();
    let task_report_id = report_id.clone();
    let task_animation_id = animation_id.clone();
    tauri::async_runtime::spawn(async move {
        let analysis_app = task_app.clone();
        let analysis_job_id = task_job_id.clone();
        let analysis_report_id = task_report_id.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            let analysis_state = analysis_app.state::<AppState>();
            run_analysis(
                &analysis_app,
                &analysis_state,
                &analysis_job_id,
                &analysis_report_id,
                &task_animation_id,
            )
        })
        .await;
        let task_state = task_app.state::<AppState>();
        match result {
            Ok(Ok(_)) => {
                let _ = set_job_state(
                    &task_app,
                    &task_state,
                    &task_job_id,
                    JobProgress {
                        status: "completed",
                        progress: 1.0,
                        stage: "Completed",
                        error_message: None,
                        result_path: None,
                    },
                );
            }
            Ok(Err(error)) if error.code == "job_cancelled" => {
                if let Ok(connection) = task_state.db.lock() {
                    let _ = connection.execute(
                        "UPDATE quality_reports SET status='cancelled',updated_at=?2 WHERE id=?1",
                        params![task_report_id, Utc::now().to_rfc3339()],
                    );
                }
                let _ = set_job_state(
                    &task_app,
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
                if let Ok(connection) = task_state.db.lock() {
                    let _ = connection.execute(
                        "UPDATE quality_reports SET status='failed',updated_at=?2 WHERE id=?1",
                        params![task_report_id, Utc::now().to_rfc3339()],
                    );
                }
                let _ = set_job_state(
                    &task_app,
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
                if let Ok(connection) = task_state.db.lock() {
                    let _ = connection.execute(
                        "UPDATE quality_reports SET status='failed',updated_at=?2 WHERE id=?1",
                        params![task_report_id, Utc::now().to_rfc3339()],
                    );
                }
                let _ = set_job_state(
                    &task_app,
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

fn rebalance_motion_plan(mut plan: MotionPlan, frame_count: u32) -> MotionPlan {
    while plan.phases.len() > frame_count as usize {
        if let Some((index, _)) = plan
            .phases
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| left.timing_weight.total_cmp(&right.timing_weight))
        {
            plan.phases.remove(index);
        }
    }
    let mut allocated = plan
        .phases
        .iter()
        .map(|phase| phase.frame_count)
        .sum::<u32>();
    while allocated > frame_count {
        let candidate = plan
            .phases
            .iter()
            .enumerate()
            .filter(|(_, phase)| phase.frame_count > 1)
            .min_by(|(_, left), (_, right)| left.timing_weight.total_cmp(&right.timing_weight))
            .map(|(index, _)| index);
        let Some(index) = candidate else { break };
        plan.phases[index].frame_count -= 1;
        allocated -= 1;
    }
    while allocated < frame_count && !plan.phases.is_empty() {
        let index = plan
            .phases
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| left.timing_weight.total_cmp(&right.timing_weight))
            .map(|(index, _)| index)
            .unwrap_or(0);
        plan.phases[index].frame_count += 1;
        allocated += 1;
    }
    plan.selected_frame_count = frame_count;
    plan.explanation = format!(
        "{} Quality-aware optimization produced {frame_count} frames inside the configured {}–{} budget.",
        plan.explanation, plan.minimum_frame_count, plan.maximum_frame_count
    );
    plan
}

fn interpolate_rgba(first: &RgbaImage, second: &RgbaImage) -> CommandResult<RgbaImage> {
    if first.dimensions() != second.dimensions() {
        return Err(CommandError::new(
            "interpolation_dimensions",
            "Align frame dimensions before inserting a transition",
        ));
    }
    let mut output = RgbaImage::new(first.width(), first.height());
    for (target, (left, right)) in output.pixels_mut().zip(first.pixels().zip(second.pixels())) {
        for channel in 0..4 {
            target[channel] = ((u16::from(left[channel]) + u16::from(right[channel])) / 2) as u8;
        }
    }
    Ok(output)
}

fn interpolation_neighbors(
    index: usize,
    frame_count: usize,
    looping: bool,
) -> Option<(usize, usize)> {
    if frame_count < 3 || index >= frame_count {
        return None;
    }
    if index == 0 {
        return looping.then_some((frame_count - 1, 1));
    }
    if index + 1 == frame_count {
        return looping.then_some((frame_count - 2, 0));
    }
    Some((index - 1, index + 1))
}

#[allow(clippy::too_many_arguments)]
fn create_interpolated_repair(
    app: &tauri::AppHandle,
    state: &AppState,
    source: &Animation,
    first_index: usize,
    second_index: usize,
    output_directory: &Path,
    file_name: &str,
    duration_ms: u32,
) -> CommandResult<AnimationFrame> {
    let first_frame = source.frames.get(first_index).ok_or_else(|| {
        CommandError::new(
            "repair_frame_missing",
            "The first repair frame no longer exists",
        )
    })?;
    let second_frame = source.frames.get(second_index).ok_or_else(|| {
        CommandError::new(
            "repair_frame_missing",
            "The second repair frame no longer exists",
        )
    })?;
    let first_asset = get_asset(state, &first_frame.asset_id)?;
    let second_asset = get_asset(state, &second_frame.asset_id)?;
    let first = image::open(&first_asset.path)?.to_rgba8();
    let second = image::open(&second_asset.path)?.to_rgba8();
    let transition = interpolate_rgba(&first, &second)?;
    let root = workspace_path(state, &source.workspace_id)?;
    std::fs::create_dir_all(output_directory)?;
    app.asset_protocol_scope()
        .allow_directory(output_directory, true)
        .map_err(|error| CommandError::new("asset_scope_error", error.to_string()))?;
    let path = output_directory.join(file_name);
    transition.save(&path)?;
    let asset = inspect(&source.workspace_id, &root, &path, None)?;
    upsert(state, &asset, "quality_frame_repair")?;
    if let Some(worktree_id) = &source.worktree_id {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.execute(
            "INSERT OR REPLACE INTO asset_worktrees(asset_id,worktree_id,relationship,created_at) VALUES (?1,?2,'owned',?3)",
            params![asset.id, worktree_id, Utc::now().to_rfc3339()],
        )?;
    }
    Ok(AnimationFrame {
        asset_id: asset.id,
        duration_ms: Some(duration_ms),
    })
}

fn optimize_animation_frames_inner(
    app: &tauri::AppHandle,
    state: &AppState,
    input: FrameOptimizationInput,
) -> CommandResult<FrameOptimizationResult> {
    let source = load_animation_by_id(state, &input.animation_id)?;
    let plan = source.motion_plan.clone().unwrap_or_else(|| MotionPlan {
        frame_mode: "fixed".into(),
        selected_frame_count: source.frames.len() as u32,
        minimum_frame_count: source.frames.len() as u32,
        maximum_frame_count: source.frames.len() as u32,
        fps: source.fps.round().max(1.0) as u32,
        looping: source.looping,
        allow_interpolation: true,
        allow_auto_adjust: false,
        explanation: "Preserve this imported animation's existing frame budget during repair."
            .into(),
        phases: Vec::new(),
    });
    let report_id: String = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                "SELECT id FROM quality_reports WHERE animation_id=?1 AND status='completed' ORDER BY created_at DESC LIMIT 1",
                [&source.id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| CommandError::new("quality_report_required", "Run quality analysis before optimizing frames"))?
    };
    let checks = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        load_checks(&connection, &report_id)?
    };
    let change_limit = input.max_changes.clamp(1, 8) as usize;
    let mut frames = source.frames.clone();
    let mut removed_frames = 0_u32;
    let mut inserted_frames = 0_u32;
    let mut replaced_frames = 0_u32;
    let fixed_budget = plan.frame_mode != "auto" || !plan.allow_auto_adjust;
    let optimization_id = Uuid::new_v4().to_string();
    let root = workspace_path(state, &source.workspace_id)?;
    let output_directory = root
        .join("assets")
        .join("repairs")
        .join(format!("loop-{}", &optimization_id[..8]));

    let mut duplicates: Vec<usize> = checks
        .iter()
        .filter(|check| {
            !check.ignored
                && check.repair_action.as_deref() == Some("remove_duplicate")
                && check.severity != "info"
        })
        .filter_map(|check| check.frame_index.map(|index| index as usize))
        .collect();
    duplicates.sort_unstable();
    duplicates.dedup();
    if fixed_budget {
        let mut repairs: Vec<usize> = checks
            .iter()
            .filter(|check| {
                !check.ignored
                    && check.severity != "info"
                    && matches!(
                        check.repair_action.as_deref(),
                        Some("remove_duplicate" | "regenerate_transition")
                    )
            })
            .filter_map(|check| check.frame_index.map(|index| index as usize))
            .collect();
        repairs.sort_unstable();
        repairs.dedup();
        for index in repairs.into_iter().take(change_limit) {
            let Some((first_index, second_index)) =
                interpolation_neighbors(index, source.frames.len(), source.looping)
            else {
                continue;
            };
            let duration_ms = source.frames[index]
                .duration_ms
                .unwrap_or_else(|| (1000.0 / source.fps).round() as u32);
            frames[index] = create_interpolated_repair(
                app,
                state,
                &source,
                first_index,
                second_index,
                &output_directory,
                &format!("repair_{:02}.png", index + 1),
                duration_ms,
            )?;
            replaced_frames += 1;
        }
    } else {
        duplicates.reverse();
        for index in duplicates.into_iter().take(change_limit) {
            if frames.len() <= plan.minimum_frame_count as usize || index >= frames.len() {
                continue;
            }
            frames.remove(index);
            removed_frames += 1;
        }
    }

    if !fixed_budget && removed_frames == 0 && plan.allow_interpolation {
        let mut transitions: Vec<usize> = checks
            .iter()
            .filter(|check| {
                !check.ignored
                    && check.repair_action.as_deref() == Some("regenerate_transition")
                    && check.severity != "info"
            })
            .filter_map(|check| check.frame_index.map(|index| index as usize))
            .collect();
        transitions.sort_unstable();
        transitions.dedup();
        transitions.reverse();
        for index in transitions.into_iter().take(change_limit) {
            if frames.len() >= plan.maximum_frame_count as usize
                || index == 0
                || index >= frames.len()
            {
                continue;
            }
            let duration_ms = frames[index]
                .duration_ms
                .unwrap_or_else(|| (1000.0 / source.fps).round() as u32);
            frames.insert(
                index,
                create_interpolated_repair(
                    app,
                    state,
                    &source,
                    index - 1,
                    index,
                    &output_directory,
                    &format!("transition_{:02}.png", index),
                    duration_ms,
                )?,
            );
            inserted_frames += 1;
        }
    }
    if removed_frames == 0 && inserted_frames == 0 && replaced_frames == 0 {
        return Err(CommandError::new(
            "no_frame_optimizations",
            "No eligible duplicate or transition repairs fit this animation's frame policy",
        ));
    }
    let summary = if replaced_frames > 0 {
        format!("Replaced {replaced_frames} weak frame(s) without changing the fixed frame budget")
    } else if removed_frames > 0 {
        format!("Removed {removed_frames} redundant frame(s) within the dynamic frame budget")
    } else {
        format!("Inserted {inserted_frames} interpolated transition frame(s) within the dynamic frame budget")
    };
    let optimized_plan = rebalance_motion_plan(plan, frames.len() as u32);
    let optimized = save_animation_inner(
        AnimationInput {
            id: None,
            workspace_id: source.workspace_id.clone(),
            worktree_id: source.worktree_id.clone(),
            name: format!("{} — repaired {}f", source.name, frames.len()),
            fps: source.fps,
            looping: source.looping,
            frames,
            motion_plan: Some(optimized_plan),
        },
        state,
    )?;
    {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.execute(
            r#"UPDATE animation_revisions
               SET parent_animation_id=?2,source_quality_report_id=?3,
                   change_kind=?4,summary=?5
               WHERE animation_id=?1"#,
            params![
                optimized.id,
                source.id,
                report_id,
                if replaced_frames > 0 {
                    "fixed_frame_repair"
                } else {
                    "frame_optimization"
                },
                summary
            ],
        )?;
    }
    Ok(FrameOptimizationResult {
        animation: optimized,
        removed_frames,
        inserted_frames,
        replaced_frames,
        summary,
    })
}

#[tauri::command]
pub async fn optimize_animation_frames(
    input: FrameOptimizationInput,
    app: tauri::AppHandle,
) -> CommandResult<FrameOptimizationResult> {
    let optimization_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = optimization_app.state::<AppState>();
        optimize_animation_frames_inner(&optimization_app, &state, input)
    })
    .await
    .map_err(|error| CommandError::new("optimization_task_failed", error.to_string()))?
}

fn repair_alignment_inner(
    app: &tauri::AppHandle,
    state: &AppState,
    animation_id: &str,
) -> CommandResult<Animation> {
    let (project_id, worktree_id, name, fps, looping, frames_json, created_at): (
        String,
        Option<String>,
        String,
        f64,
        bool,
        String,
        String,
    ) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                "SELECT workspace_id,worktree_id,name,fps,looping,frames_json,created_at FROM animations WHERE id=?1",
                [animation_id],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)),
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "The animation no longer exists"))?
    };
    let source_frames: Vec<AnimationFrame> = serde_json::from_str(&frames_json).unwrap_or_default();
    if source_frames.is_empty() {
        return Err(CommandError::new(
            "empty_animation",
            "There are no frames to align",
        ));
    }
    let mut decoded = Vec::with_capacity(source_frames.len());
    let mut canvas_width = 0;
    let mut canvas_height = 0;
    for frame in &source_frames {
        let asset = get_asset(state, &frame.asset_id)?;
        let image = image::open(&asset.path)?.to_rgba8();
        canvas_width = canvas_width.max(image.width());
        canvas_height = canvas_height.max(image.height());
        let metrics = compute_metrics(&asset.id, &asset.path)?;
        decoded.push((frame, image, metrics.metrics.bounds));
    }
    let root = workspace_path(state, &project_id)?;
    let repair_id = Uuid::new_v4().to_string();
    let slug = name
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() {
                value.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('-');
    let output_directory = root.join("assets").join("repairs").join(format!(
        "{}-{}",
        if slug.is_empty() { "aligned" } else { slug },
        &repair_id[..8]
    ));
    std::fs::create_dir_all(&output_directory)?;
    app.asset_protocol_scope()
        .allow_directory(&output_directory, true)
        .map_err(|error| CommandError::new("asset_scope_error", error.to_string()))?;
    let mut repaired_frames = Vec::with_capacity(decoded.len());
    for (index, (source_frame, image, bounds)) in decoded.into_iter().enumerate() {
        let aligned = align_frame_to_canvas(&image, bounds, canvas_width, canvas_height);
        let path = output_directory.join(format!("aligned_{:02}.png", index + 1));
        aligned.save(&path)?;
        let asset = inspect(&project_id, &root, &path, None)?;
        upsert(state, &asset, "quality_alignment_repair")?;
        if let Some(worktree_id) = &worktree_id {
            let connection = state
                .db
                .lock()
                .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
            connection.execute(
                "INSERT OR REPLACE INTO asset_worktrees(asset_id,worktree_id,relationship,created_at) VALUES (?1,?2,'owned',?3)",
                params![asset.id, worktree_id, Utc::now().to_rfc3339()],
            )?;
        }
        repaired_frames.push(AnimationFrame {
            asset_id: asset.id,
            duration_ms: source_frame.duration_ms,
        });
    }
    let now = Utc::now().to_rfc3339();
    let repaired = Animation {
        id: repair_id,
        workspace_id: project_id,
        worktree_id,
        name: format!("{name} — aligned"),
        fps,
        looping,
        frames: repaired_frames,
        motion_plan: None,
        created_at: now.clone(),
        updated_at: now,
    };
    let repaired_json = serde_json::to_string(&repaired.frames)
        .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
    {
        let mut connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let transaction = connection.transaction()?;
        transaction.execute(
            r#"INSERT INTO animations(id,workspace_id,worktree_id,name,fps,looping,frames_json,created_at,updated_at)
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8)"#,
            params![repaired.id,repaired.workspace_id,repaired.worktree_id,repaired.name,repaired.fps,repaired.looping,repaired_json,repaired.created_at],
        )?;
        transaction.execute(
            r#"INSERT INTO animation_revisions(
                id,animation_id,parent_animation_id,change_kind,summary,created_at
            ) VALUES (?1,?2,?3,'alignment_repair','Bottom-centered every opaque subject on a shared canvas without overwriting the source animation',?4)"#,
            params![Uuid::new_v4().to_string(),repaired.id,animation_id,repaired.created_at],
        )?;
        for (position, frame) in repaired.frames.iter().enumerate() {
            transaction.execute(
                r#"INSERT INTO animation_frames(id,animation_id,asset_id,position,duration_ms,pivot_x,pivot_y,created_at)
                   VALUES (?1,?2,?3,?4,?5,0.5,1.0,?6)"#,
                params![Uuid::new_v4().to_string(),repaired.id,frame.asset_id,position as u32,frame.duration_ms,repaired.created_at],
            )?;
        }
        transaction.commit()?;
    }
    std::fs::write(
        root.join("animations")
            .join(format!("{}.json", repaired.id)),
        serde_json::to_vec_pretty(&repaired)
            .map_err(|error| CommandError::new("serialization_error", error.to_string()))?,
    )?;
    let _ = created_at;
    Ok(repaired)
}

fn align_frame_to_canvas(
    image: &RgbaImage,
    bounds: Option<(u32, u32, u32, u32)>,
    canvas_width: u32,
    canvas_height: u32,
) -> RgbaImage {
    let mut aligned = RgbaImage::new(canvas_width, canvas_height);
    let Some((minimum_x, minimum_y, maximum_x, maximum_y)) = bounds else {
        return aligned;
    };
    let subject_width = maximum_x - minimum_x + 1;
    let subject_height = maximum_y - minimum_y + 1;
    let destination_x = canvas_width.saturating_sub(subject_width) / 2;
    let destination_y = canvas_height
        .saturating_sub(subject_height)
        .saturating_sub(1);
    for source_y in minimum_y..=maximum_y {
        for source_x in minimum_x..=maximum_x {
            let target_x = destination_x + source_x - minimum_x;
            let target_y = destination_y + source_y - minimum_y;
            if target_x < canvas_width && target_y < canvas_height {
                aligned.put_pixel(target_x, target_y, *image.get_pixel(source_x, source_y));
            }
        }
    }
    aligned
}

#[tauri::command]
pub async fn repair_animation_alignment(
    animation_id: String,
    app: tauri::AppHandle,
) -> CommandResult<Animation> {
    let repair_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = repair_app.state::<AppState>();
        repair_alignment_inner(&repair_app, &state, &animation_id)
    })
    .await
    .map_err(|error| CommandError::new("repair_task_failed", error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::{
        align_frame_to_canvas, compute_metrics, interpolate_rgba, interpolation_neighbors,
        leg_alternation_checks, limb_shading_checks, lower_body_view, pixel_difference,
        rebalance_motion_plan, AnalyzedFrame, FrameMetrics, LowerBodyView,
    };
    use crate::models::{MotionPhase, MotionPlan};
    use image::{Rgba, RgbaImage};
    use uuid::Uuid;

    #[test]
    fn pixel_difference_distinguishes_identical_and_changed_frames() {
        let first = RgbaImage::from_pixel(16, 16, Rgba([0, 0, 0, 0]));
        let mut second = first.clone();
        assert_eq!(pixel_difference(&first, &second), 0.0);
        second.put_pixel(8, 8, Rgba([255, 255, 255, 255]));
        assert!(pixel_difference(&first, &second) > 0.0);
    }

    #[test]
    fn metrics_find_alpha_bounds_centroid_and_edges() {
        let directory = std::env::temp_dir().join(format!("sprite-quality-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("temp directory should create");
        let path = directory.join("frame.png");
        let mut image = RgbaImage::new(16, 16);
        for y in 5..10 {
            for x in 4..8 {
                image.put_pixel(x, y, Rgba([255, 100, 20, 255]));
            }
        }
        image.save(&path).expect("frame should save");
        let analyzed = compute_metrics("asset", path.to_str().expect("path should be utf8"))
            .expect("metrics should compute");
        assert_eq!(analyzed.metrics.bounds, Some((4, 5, 7, 9)));
        assert_eq!(analyzed.metrics.opaque_edge_pixels, 0);
        assert!(analyzed.metrics.alpha_coverage > 0.0);
        std::fs::remove_dir_all(directory).expect("temp directory should remove");
    }

    #[test]
    fn alignment_repair_bottom_centers_subject_without_losing_pixels() {
        let mut source = RgbaImage::new(6, 6);
        source.put_pixel(0, 1, Rgba([255, 0, 0, 255]));
        source.put_pixel(1, 2, Rgba([0, 255, 0, 255]));

        let repaired = align_frame_to_canvas(&source, Some((0, 1, 1, 2)), 8, 8);

        assert_eq!(repaired.get_pixel(3, 5), &Rgba([255, 0, 0, 255]));
        assert_eq!(repaired.get_pixel(4, 6), &Rgba([0, 255, 0, 255]));
        assert_eq!(repaired.pixels().filter(|pixel| pixel[3] > 0).count(), 2);
    }

    #[test]
    fn interpolation_creates_a_true_midpoint_frame() {
        let first = RgbaImage::from_pixel(2, 2, Rgba([20, 40, 60, 0]));
        let second = RgbaImage::from_pixel(2, 2, Rgba([220, 140, 100, 200]));
        let transition = interpolate_rgba(&first, &second).expect("frames should interpolate");
        assert_eq!(transition.get_pixel(0, 0), &Rgba([120, 90, 80, 100]));
    }

    #[test]
    fn fixed_loop_repairs_use_neighbors_without_changing_the_budget() {
        assert_eq!(interpolation_neighbors(0, 8, true), Some((7, 1)));
        assert_eq!(interpolation_neighbors(3, 8, true), Some((2, 4)));
        assert_eq!(interpolation_neighbors(7, 8, true), Some((6, 0)));
        assert_eq!(interpolation_neighbors(0, 8, false), None);
        assert_eq!(interpolation_neighbors(7, 8, false), None);
        assert_eq!(interpolation_neighbors(1, 2, true), None);
    }

    #[test]
    fn optimization_rebalances_phase_counts_to_the_new_budget() {
        let plan = MotionPlan {
            frame_mode: "auto".into(),
            selected_frame_count: 6,
            minimum_frame_count: 4,
            maximum_frame_count: 10,
            fps: 12,
            looping: false,
            allow_interpolation: true,
            allow_auto_adjust: true,
            explanation: "Initial plan.".into(),
            phases: vec![
                MotionPhase {
                    name: "Start".into(),
                    description: "Start".into(),
                    frame_count: 2,
                    timing_weight: 0.8,
                },
                MotionPhase {
                    name: "Impact".into(),
                    description: "Impact".into(),
                    frame_count: 2,
                    timing_weight: 1.2,
                },
                MotionPhase {
                    name: "Recovery".into(),
                    description: "Recovery".into(),
                    frame_count: 2,
                    timing_weight: 1.0,
                },
            ],
        };
        let reduced = rebalance_motion_plan(plan.clone(), 5);
        let expanded = rebalance_motion_plan(plan, 8);
        assert_eq!(reduced.selected_frame_count, 5);
        assert_eq!(
            reduced
                .phases
                .iter()
                .map(|phase| phase.frame_count)
                .sum::<u32>(),
            5
        );
        assert_eq!(expanded.selected_frame_count, 8);
        assert_eq!(
            expanded
                .phases
                .iter()
                .map(|phase| phase.frame_count)
                .sum::<u32>(),
            8
        );
        assert!(expanded.phases[1].frame_count > 2);
    }

    fn walker_frame(left_leg: Rgba<u8>, right_leg: Rgba<u8>, merged: bool) -> AnalyzedFrame {
        let mut image = RgbaImage::new(16, 24);
        for y in 2..13usize {
            for x in 4..12usize {
                image.put_pixel(x as u32, y as u32, Rgba([180, 180, 180, 255]));
            }
        }
        let leg_columns: Vec<u32> = if merged {
            (5..12).collect()
        } else {
            (5..8).chain(9..12).collect()
        };
        for y in 13..23usize {
            for x in &leg_columns {
                let color = if *x < 8 { left_leg } else { right_leg };
                image.put_pixel(*x, y as u32, color);
            }
        }
        AnalyzedFrame {
            metrics: FrameMetrics {
                asset_id: Uuid::new_v4().to_string(),
                content_hash: String::new(),
                width: 16,
                height: 24,
                bounds: Some((4, 2, 11, 22)),
                centroid: None,
                alpha_coverage: 0.4,
                opaque_edge_pixels: 0,
                perceptual_hash: 0,
                palette: (0.0, 0.0, 0.0),
            },
            image,
        }
    }

    const NEAR_LEG: Rgba<u8> = Rgba([220, 220, 220, 255]);
    const FAR_LEG: Rgba<u8> = Rgba([120, 120, 120, 255]);

    #[test]
    fn lower_limb_blobs_separate_two_legs_and_measure_shading() {
        let frame = walker_frame(FAR_LEG, NEAR_LEG, false);
        let blobs = match lower_body_view(&frame.image, frame.metrics.bounds) {
            LowerBodyView::TwoBlobs(blobs) => blobs,
            other => panic!("two legs should split, got {other:?}"),
        };
        assert_eq!(blobs.len(), 2);
        assert!(blobs[0].max_x < blobs[1].min_x, "blobs must be left-to-right");
        let gap = (blobs[0].luminance - blobs[1].luminance).abs();
        assert!(
            (gap - 100.0).abs() < 1.0,
            "dark far leg vs light near leg should be ~100 apart, got {gap}"
        );
    }

    #[test]
    fn passing_pose_legs_merge_into_one_blob_and_are_skipped() {
        let frame = walker_frame(FAR_LEG, NEAR_LEG, true);
        assert_eq!(
            lower_body_view(&frame.image, frame.metrics.bounds),
            LowerBodyView::Indistinct,
            "a merged passing pose must classify as indistinct"
        );
    }

    #[test]
    fn shading_lock_break_flags_only_the_flat_frames() {
        let analyzed = vec![
            walker_frame(FAR_LEG, NEAR_LEG, false),
            walker_frame(NEAR_LEG, FAR_LEG, false),
            walker_frame(NEAR_LEG, NEAR_LEG, false),
            walker_frame(FAR_LEG, NEAR_LEG, false),
        ];
        let checks = limb_shading_checks(&analyzed);
        assert_eq!(checks.len(), 1, "only the shading-less frame flags");
        assert_eq!(checks[0].frame_index, Some(2));
        assert_eq!(checks[0].check_type, "limb_identity");
        assert!(checks[0].message.contains("far-limb shading lock"));
        assert_eq!(checks[0].repair_action, Some("regenerate"));
    }

    #[test]
    fn animations_without_a_shading_lock_never_flag() {
        let analyzed = vec![
            walker_frame(NEAR_LEG, NEAR_LEG, false),
            walker_frame(NEAR_LEG, NEAR_LEG, false),
            walker_frame(NEAR_LEG, NEAR_LEG, false),
            walker_frame(NEAR_LEG, NEAR_LEG, false),
        ];
        assert!(
            limb_shading_checks(&analyzed).is_empty(),
            "no established lock means nothing to break"
        );
    }

    #[test]
    fn hop_dominant_cycles_flag_as_lost_leg_alternation() {
        // One split contact pose, then the legs fuse for the rest of the
        // cycle — the exact shape of a failed AI run cycle.
        let analyzed = vec![
            walker_frame(FAR_LEG, NEAR_LEG, false),
            walker_frame(FAR_LEG, NEAR_LEG, true),
            walker_frame(FAR_LEG, NEAR_LEG, true),
            walker_frame(FAR_LEG, NEAR_LEG, true),
            walker_frame(FAR_LEG, NEAR_LEG, true),
            walker_frame(FAR_LEG, NEAR_LEG, true),
        ];
        let checks = leg_alternation_checks(&analyzed);
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].check_type, "leg_separation");
        assert_eq!(checks[0].severity, "error", "5/6 fused frames is severe");
        assert!(checks[0].message.contains("plays as a hop"));
        assert!(checks[0].message.contains("split stances"));
        assert_eq!(checks[0].frame_index, None, "the whole cycle is flagged");
        assert_eq!(checks[0].repair_action, Some("regenerate"));
    }

    #[test]
    fn alternating_run_cycles_with_gathered_flights_stay_clean() {
        let analyzed = vec![
            walker_frame(FAR_LEG, NEAR_LEG, false),
            walker_frame(FAR_LEG, NEAR_LEG, false),
            walker_frame(FAR_LEG, NEAR_LEG, true),
            walker_frame(FAR_LEG, NEAR_LEG, false),
            walker_frame(FAR_LEG, NEAR_LEG, false),
            walker_frame(FAR_LEG, NEAR_LEG, true),
            walker_frame(FAR_LEG, NEAR_LEG, false),
            walker_frame(FAR_LEG, NEAR_LEG, false),
        ];
        assert!(
            leg_alternation_checks(&analyzed).is_empty(),
            "two gathered flight frames in eight is a healthy run"
        );
    }

    #[test]
    fn idle_stances_with_always_together_legs_never_flag() {
        let analyzed = (0..6)
            .map(|_| walker_frame(NEAR_LEG, NEAR_LEG, true))
            .collect::<Vec<_>>();
        assert!(
            leg_alternation_checks(&analyzed).is_empty(),
            "no separated-leg frame means no proof of leg alternation to lose"
        );
    }
}
