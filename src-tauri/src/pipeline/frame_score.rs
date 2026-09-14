use crate::{
    animations::load_animation_by_id,
    assets::get_asset,
    error::CommandResult,
    models::{AnimationFrameScore, AnimationFrameScoreReport},
    quality::{bounds_area, centroid_distance, compute_metrics, pixel_difference},
    AppState,
};
use serde::Deserialize;
use tauri::State;

pub(crate) fn score_animation_frames_inner(
    animation_id: &str,
    state: &AppState,
) -> CommandResult<AnimationFrameScoreReport> {
    let animation = load_animation_by_id(state, animation_id)?;
    if animation.frames.is_empty() {
        return Ok(AnimationFrameScoreReport {
            animation_id: animation.id.clone(),
            frame_count: 0,
            mean_motion_delta: 0.0,
            mean_score: 100.0,
            frames: Vec::new(),
        });
    }

    let mut analyzed = Vec::with_capacity(animation.frames.len());
    for frame in &animation.frames {
        let asset = get_asset(state, &frame.asset_id)?;
        analyzed.push(compute_metrics(&frame.asset_id, &asset.path)?);
    }

    let baseline_area = bounds_area(&analyzed[0].metrics).max(1.0);
    let mut frames = Vec::with_capacity(analyzed.len());
    let mut motion_total = 0.0;
    let mut score_total = 0.0;

    for (index, current) in analyzed.iter().enumerate() {
        let previous = index
            .checked_sub(1)
            .map(|value| &analyzed[value])
            .unwrap_or(current);
        let motion_delta = if index == 0 {
            0.0
        } else {
            centroid_distance(&previous.metrics, &current.metrics)
        };
        let pixel_delta = if index == 0 {
            0.0
        } else {
            pixel_difference(&previous.image, &current.image)
        };
        let area = bounds_area(&current.metrics);
        let size_drift = ((area - baseline_area).abs() / baseline_area) * 100.0;
        let alpha_penalty = if current.metrics.alpha_coverage > 0.96 {
            25.0
        } else {
            0.0
        };
        let motion_penalty = motion_delta * 2.5;
        let drift_penalty = size_drift * 0.8;
        let pixel_penalty = pixel_delta * 120.0;
        let score = (100.0_f64 - alpha_penalty - motion_penalty - drift_penalty - pixel_penalty)
            .clamp(0.0, 100.0);

        motion_total += motion_delta;
        score_total += score;
        frames.push(AnimationFrameScore {
            frame_index: index as u32,
            asset_id: current.metrics.asset_id.clone(),
            motion_delta,
            alpha_coverage: current.metrics.alpha_coverage,
            size_drift,
            pixel_delta,
            score,
        });
    }

    let frame_count = frames.len() as u32;
    Ok(AnimationFrameScoreReport {
        animation_id: animation.id,
        frame_count,
        mean_motion_delta: if frame_count > 0 {
            motion_total / frame_count as f64
        } else {
            0.0
        },
        mean_score: if frame_count > 0 {
            score_total / frame_count as f64
        } else {
            100.0
        },
        frames,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreAnimationFramesInput {
    pub animation_id: String,
}

#[tauri::command]
pub fn score_animation_frames(
    input: ScoreAnimationFramesInput,
    state: State<'_, AppState>,
) -> CommandResult<AnimationFrameScoreReport> {
    score_animation_frames_inner(&input.animation_id, &state)
}
