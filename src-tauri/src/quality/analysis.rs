use crate::{
    animations::resolve_animation_frames,
    assets::get_asset,
    error::{CommandError, CommandResult},
    jobs::{cancellation_requested, set_job_state, JobProgress},
    models::{AnimationFrame, QualityReport},
    AppState,
};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use super::contract_bridge::{
    apply_contract_penalties, contract_violations_to_pending_checks,
};
use super::frame_checks::collect_visual_checks;
use super::metrics::{
    cache_metrics, centroid_distance, compute_metrics, content_hash, load_cached_metrics,
    pixel_difference, score_with_penalty, AnalyzedFrame, PendingCheck,
};
use super::motion_checks::{leg_alternation_checks, limb_shading_checks};
use super::reports::{hydrate_report, report_row, select_report};

pub(super) fn run_analysis(
    app: Option<&tauri::AppHandle>,
    state: &AppState,
    job_id: &str,
    report_id: &str,
    animation_id: &str,
) -> CommandResult<QualityReport> {
    let (project_id, worktree_id, looping, frames): (String, Option<String>, bool, Vec<AnimationFrame>) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let (project_id, worktree_id, looping, frames_json): (
            String,
            Option<String>,
            bool,
            String,
        ) = connection
            .query_row(
                "SELECT workspace_id, worktree_id, looping, frames_json FROM animations WHERE id=?1",
                [animation_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "The animation no longer exists"))?;
        let frames = resolve_animation_frames(&connection, animation_id, &frames_json)?;
        (project_id, worktree_id, looping, frames)
    };
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
    let (mut checks, visual) = collect_visual_checks(app, state, job_id, &analyzed)?;
    let mut alignment_penalty = visual.alignment;
    let mut continuity_penalty = visual.continuity;
    let mut consistency_penalty = visual.consistency;
    let weapon_penalty = visual.weapon;
    let transparency_penalty = visual.transparency;
    let limb_checks = limb_shading_checks(&analyzed);
    if !limb_checks.is_empty() {
        consistency_penalty += (limb_checks.len() as f64 * 6.0).min(18.0);
        checks.extend(limb_checks);
    }
    for check in leg_alternation_checks(&analyzed) {
        consistency_penalty += if check.severity == "error" { 20.0 } else { 8.0 };
        checks.push(check);
    }
    if let Ok(contract_report) =
        crate::pipeline::size_contract_check_inner(state, animation_id, None)
    {
        if !contract_report.violations.is_empty() {
            apply_contract_penalties(
                &contract_report.violations,
                &mut alignment_penalty,
                &mut consistency_penalty,
                &mut continuity_penalty,
            );
            checks.extend(contract_violations_to_pending_checks(&contract_report.violations));
        }
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
