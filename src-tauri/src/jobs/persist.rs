use crate::{
    error::{CommandError, CommandResult},
    models::{BackgroundJob, JobEvent},
    AppState,
};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use tauri::{Emitter, State};

fn job_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BackgroundJob> {
    Ok(BackgroundJob {
        id: row.get(0)?,
        project_id: row.get(1)?,
        worktree_id: row.get(2)?,
        kind: row.get(3)?,
        target_type: row.get(4)?,
        target_id: row.get(5)?,
        status: row.get(6)?,
        progress: row.get(7)?,
        stage: row.get(8)?,
        error_message: row.get(9)?,
        cancel_requested: row.get(10)?,
        result_path: row.get(11)?,
        metadata_json: row.get(12)?,
        created_at: row.get(13)?,
        started_at: row.get(14)?,
        completed_at: row.get(15)?,
        updated_at: row.get(16)?,
    })
}

fn select_job() -> &'static str {
    r#"SELECT id, project_id, worktree_id, kind, target_type, target_id, status,
              progress, stage, error_message, cancel_requested, result_path,
              metadata_json, created_at, started_at, completed_at, updated_at
       FROM background_jobs"#
}

pub(crate) fn load_job(state: &AppState, id: &str) -> CommandResult<BackgroundJob> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection
        .query_row(&format!("{} WHERE id=?1", select_job()), [id], job_row)
        .optional()?
        .ok_or_else(|| CommandError::new("job_not_found", "The background job no longer exists"))
}

pub(crate) fn emit_job(
    app: Option<&tauri::AppHandle>,
    state: &AppState,
    id: &str,
) -> CommandResult<BackgroundJob> {
    let job = load_job(state, id)?;
    if let Some(app) = app {
        app.emit("job-event", JobEvent { job: job.clone() })
            .map_err(|error| CommandError::new("event_error", error.to_string()))?;
    }
    Ok(job)
}

pub(crate) struct JobProgress<'a> {
    pub(crate) status: &'a str,
    pub(crate) progress: f64,
    pub(crate) stage: &'a str,
    pub(crate) error_message: Option<&'a str>,
    pub(crate) result_path: Option<&'a str>,
}

pub(crate) fn set_job_state(
    app: Option<&tauri::AppHandle>,
    state: &AppState,
    id: &str,
    update: JobProgress<'_>,
) -> CommandResult<BackgroundJob> {
    let now = Utc::now().to_rfc3339();
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        r#"UPDATE background_jobs
           SET status=?2, progress=?3, stage=?4, error_message=?5,
               result_path=COALESCE(?6, result_path),
               started_at=CASE WHEN ?2='running' AND started_at IS NULL THEN ?7 ELSE started_at END,
               completed_at=CASE WHEN ?2 IN ('completed','failed','cancelled') THEN ?7 ELSE completed_at END,
               updated_at=?7
           WHERE id=?1"#,
        params![
            id,
            update.status,
            update.progress.clamp(0.0, 1.0),
            update.stage,
            update.error_message,
            update.result_path,
            now
        ],
    )?;
    drop(connection);
    emit_job(app, state, id)
}

pub(crate) fn set_job_metadata(
    app: Option<&tauri::AppHandle>,
    state: &AppState,
    id: &str,
    metadata_json: &str,
) -> CommandResult<BackgroundJob> {
    let now = Utc::now().to_rfc3339();
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        "UPDATE background_jobs SET metadata_json=?2, updated_at=?3 WHERE id=?1",
        params![id, metadata_json, now],
    )?;
    drop(connection);
    emit_job(app, state, id)
}

pub(crate) fn cancellation_requested(state: &AppState, id: &str) -> CommandResult<bool> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    Ok(connection.query_row(
        "SELECT cancel_requested FROM background_jobs WHERE id=?1",
        [id],
        |row| row.get(0),
    )?)
}
#[tauri::command]
pub fn list_jobs(
    project_id: String,
    worktree_id: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<BackgroundJob>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut jobs = Vec::new();
    if let Some(worktree_id) = worktree_id {
        let mut statement = connection.prepare(&format!(
            "{} WHERE project_id=?1 AND worktree_id=?2 ORDER BY created_at DESC LIMIT 100",
            select_job()
        ))?;
        let rows = statement.query_map(params![project_id, worktree_id], job_row)?;
        jobs.extend(rows.filter_map(Result::ok));
    } else {
        let mut statement = connection.prepare(&format!(
            "{} WHERE project_id=?1 ORDER BY created_at DESC LIMIT 100",
            select_job()
        ))?;
        let rows = statement.query_map([project_id], job_row)?;
        jobs.extend(rows.filter_map(Result::ok));
    }
    Ok(jobs)
}

pub(crate) fn request_job_cancellation(state: &AppState, id: &str) -> CommandResult<()> {
    let now = Utc::now().to_rfc3339();
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        r#"UPDATE background_jobs
           SET cancel_requested=1,
               status=CASE WHEN status='queued' THEN 'cancelled' ELSE status END,
               stage=CASE WHEN status='queued' THEN 'Cancelled' ELSE 'Cancelling' END,
               completed_at=CASE WHEN status='queued' THEN ?2 ELSE completed_at END,
               updated_at=?2
           WHERE id=?1 AND status IN ('queued','running','analyzing')"#,
        params![id, now],
    )?;
    Ok(())
}

#[tauri::command]
pub fn cancel_job(
    id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<BackgroundJob> {
    let now = Utc::now().to_rfc3339();
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        r#"UPDATE background_jobs
           SET cancel_requested=1,
               status=CASE WHEN status='queued' THEN 'cancelled' ELSE status END,
               stage=CASE WHEN status='queued' THEN 'Cancelled' ELSE 'Cancelling' END,
               completed_at=CASE WHEN status='queued' THEN ?2 ELSE completed_at END,
               updated_at=?2
           WHERE id=?1 AND status IN ('queued','running','analyzing')"#,
        params![id, now],
    )?;
    drop(connection);
    emit_job(Some(&app), &state, &id)
}
