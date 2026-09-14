use crate::{
    animations::save_animation_inner,
    assets::get_asset,
    error::{CommandError, CommandResult},
    models::{
        Animation, AnimationInput, SizeContractReport, SizeContractViolation,
    },
    quality::{compute_metrics, pixel_difference},
    AppState,
};
use image::RgbaImage;

use super::qc::perceptual_hash_distance;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use tauri::State;

use super::anchors::{get_anchor_inner, list_anchors_inner};

fn contract_applies_to_worktree(kind: Option<&str>) -> bool {
    matches!(kind, Some("character" | "creature" | "animation"))
}

fn resolve_worktree_kind(
    connection: &rusqlite::Connection,
    worktree_id: Option<&str>,
) -> CommandResult<Option<String>> {
    if let Some(id) = worktree_id {
        let kind: Option<String> = connection
            .query_row(
                "SELECT kind FROM worktrees WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()?;
        return Ok(kind);
    }
    Ok(None)
}

pub(crate) fn size_contract_check_inner(
    state: &AppState,
    animation_id: &str,
    anchor_slug: Option<&str>,
) -> CommandResult<SizeContractReport> {
    let (project_id, worktree_id, _name, _fps, _looping, review_status, frames): (
        String,
        Option<String>,
        String,
        f64,
        bool,
        String,
        Vec<crate::models::AnimationFrame>,
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
        ): (
            String,
            Option<String>,
            String,
            f64,
            bool,
            String,
            String,
        ) = connection
            .query_row(
                "SELECT workspace_id, worktree_id, name, fps, looping, review_status, frames_json FROM animations WHERE id=?1",
                [animation_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "The animation no longer exists"))?;
        let frames = crate::animations::resolve_animation_frames(
            &connection,
            animation_id,
            &frames_json,
        )?;
        (
            project_id,
            worktree_id,
            name,
            fps,
            looping,
            review_status,
            frames,
        )
    };
    if frames.is_empty() {
        return Err(CommandError::new(
            "empty_animation",
            "There are no frames to validate",
        ));
    }
    let worktree_kind = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        resolve_worktree_kind(&connection, worktree_id.as_deref())?
    };
    let contract_applies = anchor_slug.is_some()
        || contract_applies_to_worktree(worktree_kind.as_deref());
    if !contract_applies {
        return Ok(SizeContractReport {
            animation_id: animation_id.to_string(),
            anchor_slug: None,
            passed: true,
            review_status,
            violations: Vec::new(),
        });
    }
    let anchor = if let Some(slug) = anchor_slug {
        Some(get_anchor_inner(&project_id, slug, state)?)
    } else {
        list_anchors_inner(&project_id, state)?
            .first()
            .and_then(|summary| get_anchor_inner(&project_id, &summary.slug, state).ok())
    };
    let mut violations = Vec::new();
    if let Some(anchor) = &anchor {
        let anchor_asset = get_asset(state, &anchor.asset_id)?;
        let anchor_metrics = compute_metrics(&anchor.asset_id, &anchor_asset.path)?;
        let anchor_hash = anchor_metrics.metrics.perceptual_hash;
        let first_frame_metrics = compute_metrics(&frames[0].asset_id, &get_asset(state, &frames[0].asset_id)?.path)?;
        let cycle_identity_hash = first_frame_metrics.metrics.perceptual_hash;
        let mut prior_image: Option<RgbaImage> = None;
        let mut centroid_x_values = Vec::with_capacity(frames.len());
        for (index, frame) in frames.iter().enumerate() {
            let asset = get_asset(state, &frame.asset_id)?;
            if asset.width != anchor.frame_width || asset.height != anchor.frame_height {
                violations.push(SizeContractViolation {
                    code: "canvas_size".to_string(),
                    message: format!(
                        "Frame {} is {}x{} but anchor requires {}x{}",
                        index + 1,
                        asset.width,
                        asset.height,
                        anchor.frame_width,
                        anchor.frame_height
                    ),
                    blocking: true,
                    frame_index: Some(index as u32),
                });
            }
            let metrics = compute_metrics(&asset.id, &asset.path)?;
            if let Some((_min_x, _min_y, _max_x, max_y)) = metrics.metrics.bounds {
                let foot_delta = (max_y as i64 - anchor.baseline_y as i64).unsigned_abs();
                if foot_delta > 1 {
                    violations.push(SizeContractViolation {
                        code: "baseline_drift".to_string(),
                        message: format!(
                            "Frame {} foot baseline drifts by {}px from anchor",
                            index + 1,
                            foot_delta
                        ),
                        blocking: true,
                        frame_index: Some(index as u32),
                    });
                }
            }
            if let Some((centroid_x, _)) = metrics.metrics.centroid {
                centroid_x_values.push(centroid_x);
            }
            let reference_hash = if index == 0 {
                anchor_hash
            } else {
                cycle_identity_hash
            };
            let reference_label = if index == 0 { "anchor" } else { "cycle frame 1" };
            let drift_threshold = if index == 0 { 20 } else { 24 };
            let warning_threshold = if index == 0 { 14 } else { 18 };
            let hash_distance = perceptual_hash_distance(
                metrics.metrics.perceptual_hash,
                reference_hash,
            );
            if hash_distance > drift_threshold {
                violations.push(SizeContractViolation {
                    code: "identity_drift".to_string(),
                    message: format!(
                        "Frame {} silhouette diverges from {} (dHash distance {})",
                        index + 1,
                        reference_label,
                        hash_distance
                    ),
                    blocking: false,
                    frame_index: Some(index as u32),
                });
            } else if hash_distance > warning_threshold {
                violations.push(SizeContractViolation {
                    code: "identity_warning".to_string(),
                    message: format!(
                        "Frame {} may have drifted from {} identity (dHash distance {})",
                        index + 1,
                        reference_label,
                        hash_distance
                    ),
                    blocking: false,
                    frame_index: Some(index as u32),
                });
            }
            if let Some(previous) = &prior_image {
                let motion_delta = pixel_difference(previous, &metrics.image);
                if motion_delta < 0.004 {
                    violations.push(SizeContractViolation {
                        code: "motion_still".to_string(),
                        message: format!(
                            "Frame {} is nearly identical to frame {} — animation may be static",
                            index + 1,
                            index
                        ),
                        blocking: false,
                        frame_index: Some(index as u32),
                    });
                }
            }
            prior_image = Some(metrics.image);
        }
        if centroid_x_values.len() > 1 {
            let mean = centroid_x_values.iter().sum::<f64>() / centroid_x_values.len() as f64;
            let max_deviation = centroid_x_values
                .iter()
                .map(|value| (*value - mean).abs())
                .fold(0.0_f64, f64::max);
            if max_deviation > 4.0 {
                violations.push(SizeContractViolation {
                    code: "centroid_drift".to_string(),
                    message: format!(
                        "Horizontal body centroid drifts up to {:.1}px across frames",
                        max_deviation
                    ),
                    blocking: true,
                    frame_index: None,
                });
            }
        }
    } else {
        violations.push(SizeContractViolation {
            code: "missing_anchor".to_string(),
            message: "No promoted character anchor exists for this workspace".to_string(),
            blocking: false,
            frame_index: None,
        });
    }
    let passed = !violations.iter().any(|violation| violation.blocking);
    Ok(SizeContractReport {
        animation_id: animation_id.to_string(),
        anchor_slug: anchor.map(|value| value.slug),
        passed,
        review_status,
        violations,
    })
}

pub(crate) fn set_animation_review_status_inner(
    state: &AppState,
    animation_id: &str,
    status: &str,
) -> CommandResult<Animation> {
    let normalized = status.trim().to_ascii_lowercase();
    if !["draft", "accepted", "rejected"].contains(&normalized.as_str()) {
        return Err(CommandError::new(
            "invalid_review_status",
            "Review status must be draft, accepted, or rejected",
        ));
    }
    if normalized == "accepted" {
        let report = size_contract_check_inner(state, animation_id, None)?;
        if !report.passed {
            return Err(CommandError::new(
                "size_contract_failed",
                "Animation cannot be accepted while blocking size-contract violations remain",
            ));
        }
    }
    let now = Utc::now().to_rfc3339();
    {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let updated = connection.execute(
            "UPDATE animations SET review_status=?1, updated_at=?2 WHERE id=?3",
            params![normalized, now, animation_id],
        )?;
        if updated == 0 {
            return Err(CommandError::new(
                "animation_not_found",
                "The animation no longer exists",
            ));
        }
    }
    let (project_id, worktree_id, name, fps, looping, frames): (
        String,
        Option<String>,
        String,
        f64,
        bool,
        Vec<crate::models::AnimationFrame>,
    ) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let (project_id, worktree_id, name, fps, looping, frames_json): (
            String,
            Option<String>,
            String,
            f64,
            bool,
            String,
        ) = connection
            .query_row(
                "SELECT workspace_id, worktree_id, name, fps, looping, frames_json FROM animations WHERE id=?1",
                [animation_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "The animation no longer exists"))?;
        let frames = crate::animations::resolve_animation_frames(
            &connection,
            animation_id,
            &frames_json,
        )?;
        (
            project_id,
            worktree_id,
            name,
            fps,
            looping,
            frames,
        )
    };
    save_animation_inner(
        AnimationInput {
            id: Some(animation_id.to_string()),
            workspace_id: project_id,
            worktree_id,
            name,
            fps,
            looping,
            frames,
            motion_plan: None,
            review_status: Some(normalized),
        },
        state,
    )
}

pub(crate) fn ensure_export_allowed(
    state: &AppState,
    animation_id: &str,
) -> CommandResult<()> {
    let (review_status, workspace_id, worktree_id): (String, String, Option<String>) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                "SELECT review_status, workspace_id, worktree_id FROM animations WHERE id=?1",
                [animation_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "The animation no longer exists"))?
    };
    if let Some(worktree_id) = worktree_id {
        let worktree_has_direction_set = super::direction_meta::list_direction_meta_for_worktree(
            state,
            &workspace_id,
            &worktree_id,
        )?
        .len()
            >= 2;
        if worktree_has_direction_set
            || super::direction_meta::load_direction_meta(state, &workspace_id, animation_id)?
                .is_some()
        {
            super::character_contract::ensure_worktree_export_allowed(
                state,
                &workspace_id,
                &worktree_id,
                None,
            )?;
        }
    }
    if review_status == "accepted" {
        return Ok(());
    }
    let report = size_contract_check_inner(state, animation_id, None)?;
    if report.violations.iter().any(|violation| violation.blocking) {
        return Err(CommandError::new(
            "export_blocked",
            "Export is blocked until the animation passes size-contract review or is accepted",
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn check_size_contract(
    animation_id: String,
    anchor_slug: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<SizeContractReport> {
    size_contract_check_inner(&state, &animation_id, anchor_slug.as_deref())
}

#[tauri::command]
pub fn set_animation_review_status(
    animation_id: String,
    status: String,
    state: State<'_, AppState>,
) -> CommandResult<Animation> {
    set_animation_review_status_inner(&state, &animation_id, &status)
}
