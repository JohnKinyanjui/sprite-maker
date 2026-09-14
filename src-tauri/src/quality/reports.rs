use crate::{
    error::{CommandError, CommandResult},
    jobs::{load_job, set_job_state, JobProgress},
    models::{BackgroundJob, QualityCheck, QualityReport},
    AppState,
};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use tauri::{Emitter, State};
use uuid::Uuid;

use crate::animations::resolve_animation_frames;

use super::analysis::run_analysis;
use super::metrics::ANALYZER_VERSION;

pub(super) fn report_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<QualityReport> {
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

pub(super) fn select_report() -> &'static str {
    r#"SELECT id, project_id, worktree_id, animation_id, job_id, status,
              overall_score, character_consistency_score, motion_continuity_score,
              frame_alignment_score, weapon_consistency_score, loop_quality_score,
              transparency_score, frame_count, analyzer_version, created_at,
              completed_at, updated_at
       FROM quality_reports"#
}

pub(super) fn load_checks(
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

pub(super) fn hydrate_report(
    connection: &rusqlite::Connection,
    report: &mut QualityReport,
) -> CommandResult<()> {
    report.checks = load_checks(connection, &report.id)?;
    Ok(())
}
#[tauri::command]
pub fn get_quality_report(
    animation_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Option<QualityReport>> {
    get_quality_report_inner(&animation_id, &state)
}

pub(crate) fn get_quality_report_inner(
    animation_id: &str,
    state: &AppState,
) -> CommandResult<Option<QualityReport>> {
    let mut report = {
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
        report
    };
    if let Some(report) = &mut report {
        if let Ok(contract_report) =
            crate::pipeline::size_contract_check_inner(state, animation_id, None)
        {
            super::contract_bridge::merge_contract_checks(
                &mut report.checks,
                &contract_report.violations,
            );
        }
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
    queue_quality_analysis_inner(animation_id, Some(app), &state)
}

pub(crate) fn queue_quality_analysis_inner(
    animation_id: String,
    app: Option<tauri::AppHandle>,
    state: &AppState,
) -> CommandResult<BackgroundJob> {
    let (project_id, worktree_id, frame_count): (String, Option<String>, u32) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let (project_id, worktree_id, frames_json): (String, Option<String>, String) = connection
            .query_row(
                "SELECT workspace_id, worktree_id, frames_json FROM animations WHERE id=?1",
                [&animation_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "The animation no longer exists"))?;
        let frames =
            resolve_animation_frames(&connection, &animation_id, &frames_json)?;
        (project_id, worktree_id, frames.len() as u32)
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
        return load_job(state, &active_job_id);
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
    let queued = load_job(state, &job_id)?;
    if let Some(app) = app.as_ref() {
        app.emit(
            "job-event",
            crate::models::JobEvent {
                job: queued.clone(),
            },
        )
        .map_err(|error| CommandError::new("event_error", error.to_string()))?;
    }
    let task_app = app.clone();
    let task_state = state.clone();
    let task_job_id = job_id.clone();
    let task_report_id = report_id.clone();
    let task_animation_id = animation_id.clone();
    tauri::async_runtime::spawn(async move {
        let analysis_app = task_app.clone();
        let analysis_state = task_state.clone();
        let analysis_job_id = task_job_id.clone();
        let analysis_report_id = task_report_id.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            run_analysis(
                analysis_app.as_ref(),
                &analysis_state,
                &analysis_job_id,
                &analysis_report_id,
                &task_animation_id,
            )
        })
        .await;
        match result {
            Ok(Ok(_)) => {
                let _ = set_job_state(
                    task_app.as_ref(),
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
                if let Ok(connection) = task_state.db.lock() {
                    let _ = connection.execute(
                        "UPDATE quality_reports SET status='failed',updated_at=?2 WHERE id=?1",
                        params![task_report_id, Utc::now().to_rfc3339()],
                    );
                }
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
                if let Ok(connection) = task_state.db.lock() {
                    let _ = connection.execute(
                        "UPDATE quality_reports SET status='failed',updated_at=?2 WHERE id=?1",
                        params![task_report_id, Utc::now().to_rfc3339()],
                    );
                }
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
