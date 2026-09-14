use crate::{
    animations::{load_animation_by_id, save_animation_inner},
    assets::{get_asset, inspect, upsert},
    error::{CommandError, CommandResult},
    models::{
        Animation, AnimationFrame, AnimationInput, FrameOptimizationInput, FrameOptimizationResult,
        MotionPlan,
    },
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use image::RgbaImage;
use rusqlite::{params, OptionalExtension};
use std::path::Path;
use tauri::Manager;
use uuid::Uuid;

use crate::pipeline::blit_with_offset;

use super::reports::load_checks;
use super::rig_bridge::try_render_rig_transition;

pub(super) fn rebalance_motion_plan(mut plan: MotionPlan, frame_count: u32) -> MotionPlan {
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

pub(super) fn interpolate_rgba(first: &RgbaImage, second: &RgbaImage) -> CommandResult<RgbaImage> {
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

pub(super) fn interpolate_motion_aware_rgba(
    first: &RgbaImage,
    second: &RgbaImage,
    first_offset: (i32, i32),
    second_offset: (i32, i32),
) -> CommandResult<RgbaImage> {
    if first.dimensions() != second.dimensions() {
        return Err(CommandError::new(
            "interpolation_dimensions",
            "Align frame dimensions before inserting a transition",
        ));
    }
    let mid_offset = (
        (first_offset.0 + second_offset.0) / 2,
        (first_offset.1 + second_offset.1) / 2,
    );
    let width = first.width();
    let height = first.height();
    let mut aligned_first = RgbaImage::new(width, height);
    let mut aligned_second = RgbaImage::new(width, height);
    blit_with_offset(
        &mut aligned_first,
        first,
        mid_offset.0 - first_offset.0,
        mid_offset.1 - first_offset.1,
    );
    blit_with_offset(
        &mut aligned_second,
        second,
        mid_offset.0 - second_offset.0,
        mid_offset.1 - second_offset.1,
    );
    interpolate_rgba(&aligned_first, &aligned_second)
}

pub(super) fn interpolation_neighbors(
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
    let transition = if let Some(rig_frame) =
        try_render_rig_transition(state, source, first_index, second_index)?
    {
        rig_frame
    } else {
        let first = image::open(&first_asset.path)?.to_rgba8();
        let second = image::open(&second_asset.path)?.to_rgba8();
        interpolate_motion_aware_rgba(
            &first,
            &second,
            (first_frame.offset_x, first_frame.offset_y),
            (second_frame.offset_x, second_frame.offset_y),
        )?
    };
    let mid_offset = (
        (first_frame.offset_x + second_frame.offset_x) / 2,
        (first_frame.offset_y + second_frame.offset_y) / 2,
    );
    let root = workspace_path(state, &source.workspace_id)?;
    std::fs::create_dir_all(output_directory)?;
    crate::allow_asset_directory(Some(app), output_directory, true)?;
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
        offset_x: mid_offset.0,
        offset_y: mid_offset.1,
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
    let mut checks = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        load_checks(&connection, &report_id)?
    };
    if let Ok(contract_report) =
        crate::pipeline::size_contract_check_inner(state, &input.animation_id, None)
    {
        super::contract_bridge::merge_contract_checks(&mut checks, &contract_report.violations);
    }
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
            review_status: Some(source.review_status.clone()),
        },
        state,
    )?;
    {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let change_kind = if replaced_frames > 0 {
            "fixed_frame_repair"
        } else {
            "frame_optimization"
        };
        connection.execute(
            r#"INSERT INTO animation_revisions(
                id,animation_id,parent_animation_id,source_quality_report_id,
                change_kind,summary,created_at
            ) VALUES (?1,?2,?3,?4,?5,?6,?7)
            ON CONFLICT(animation_id) DO UPDATE SET
                parent_animation_id=excluded.parent_animation_id,
                source_quality_report_id=excluded.source_quality_report_id,
                change_kind=excluded.change_kind,
                summary=excluded.summary"#,
            params![
                Uuid::new_v4().to_string(),
                optimized.id,
                source.id,
                report_id,
                change_kind,
                summary,
                optimized.created_at
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
