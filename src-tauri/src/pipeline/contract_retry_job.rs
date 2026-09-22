use crate::{
    assets::{read_generation_manifest, scan_generation_assets_inner},
    conversations::get_message,
    error::{CommandError, CommandResult},
    jobs::{
        cancellation_requested, emit_job, set_job_metadata, set_job_state, JobProgress,
    },
    models::{ContractRetryAttempt, ContractRetryResult, QueueContractRetryInput},
    providers::start_provider_run,
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use image::GenericImageView;
use rusqlite::{params, OptionalExtension};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tauri::AppHandle;
use uuid::Uuid;

use super::anchors::get_anchor_inner;
use super::contract::size_contract_check_inner;
use super::logging::{
    log_pipeline_event, ContractRetryMetrics, PipelineStageTimer, PipelineTimer,
};
use super::contract_retry::{
    apply_deterministic_contract_repairs, build_corrective_prompt, finalize_contract_retry_inner,
    has_deterministic_fixable, load_animation_context,
};

const DEFAULT_MAX_AI_ATTEMPTS: u32 = 2;
const GENERATION_POLL_INTERVAL: Duration = Duration::from_secs(2);
const GENERATION_TIMEOUT: Duration = Duration::from_secs(900);

fn generation_status(state: &AppState, request_id: &str) -> Option<String> {
    let snapshot = state.generation_snapshot(request_id)?;
    if snapshot.assistant_id.is_empty() {
        return Some(snapshot.status);
    }
    if let Ok(message) = get_message(state, &snapshot.assistant_id) {
        if matches!(
            snapshot.status.as_str(),
            "completed" | "failed" | "cancelled"
        ) {
            return Some(snapshot.status);
        }
        return Some(message.status);
    }
    Some(snapshot.status)
}

pub(crate) fn find_horizontal_strip_path(
    root: &Path,
    manifest: &crate::models::GenerationManifest,
) -> Option<PathBuf> {
    let mut best: Option<(PathBuf, u32)> = None;
    for relative in &manifest.files {
        let path = root.join(relative);
        if !path.is_file() {
            continue;
        }
        if let Ok(image) = image::open(&path) {
            let (width, height) = image.dimensions();
            if width <= height {
                continue;
            }
            if best.as_ref().map_or(true, |(_, best_width)| width > *best_width) {
                best = Some((path, width));
            }
        }
    }
    best.map(|(path, _)| path)
}

pub(crate) async fn wait_for_generation(
    state: &AppState,
    request_id: &str,
    job_id: &str,
) -> CommandResult<()> {
    let started = Instant::now();
    while started.elapsed() < GENERATION_TIMEOUT {
        if cancellation_requested(state, job_id)? {
            return Err(CommandError::new("job_cancelled", "Contract retry was cancelled"));
        }
        match generation_status(state, request_id) {
            Some(status) if status == "completed" => return Ok(()),
            Some(status) if status == "failed" || status == "cancelled" => {
                return Err(CommandError::new(
                    "generation_failed",
                    format!("Strip regeneration ended with status `{status}`"),
                ));
            }
            _ => {}
        }
        tokio::time::sleep(GENERATION_POLL_INTERVAL).await;
    }
    Err(CommandError::new(
        "generation_timeout",
        "Strip regeneration timed out after 15 minutes",
    ))
}

pub(crate) fn queue_contract_retry_inner(
    input: QueueContractRetryInput,
    app: Option<AppHandle>,
    state: &AppState,
) -> CommandResult<ContractRetryResult> {
    let conversation_id = input.conversation_id.trim();
    if conversation_id.is_empty() {
        return Err(CommandError::new(
            "conversation_required",
            "A conversationId is required for autonomous contract retry",
        ));
    }
    let max_deterministic = input.max_deterministic_passes.unwrap_or(2).clamp(1, 5);
    let max_ai_attempts = input.max_ai_attempts.unwrap_or(DEFAULT_MAX_AI_ATTEMPTS).clamp(1, 4);
    let (workspace_id, _name, _fps, _looping, frame_count, worktree_id) =
        load_animation_context(state, &input.animation_id)?;
    if frame_count == 0 {
        return Err(CommandError::new(
            "empty_animation",
            "Add frames before queueing contract retry",
        ));
    }

    let initial_report = size_contract_check_inner(
        state,
        &input.animation_id,
        input.anchor_slug.as_deref(),
    )?;

    let active_job_id: Option<String> = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                r#"SELECT id FROM background_jobs
                   WHERE kind='contract_retry' AND target_id=?1
                     AND status IN ('queued','running')
                   ORDER BY created_at DESC LIMIT 1"#,
                [&input.animation_id],
                |row| row.get(0),
            )
            .optional()?
    };
    if let Some(active_job_id) = active_job_id {
        return Err(CommandError::new(
            "contract_retry_active",
            format!(
                "A contract retry job is already running for this animation (job {active_job_id})"
            ),
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
            ) VALUES (?1,?2,?3,'contract_retry','animation',?4,'queued',0.0,'Queued',?5,?5,?5)"#,
            params![job_id, workspace_id, worktree_id, input.animation_id, now],
        )?;
    }

    persist_contract_retry_attempts(app.as_ref(), &state, &job_id, &[], None);

    let task_app = app.clone();
    let task_state = state.clone();
    let task_job_id = job_id.clone();
    let animation_id = input.animation_id.clone();
    let anchor_slug = input.anchor_slug.clone();
    let conversation_id = conversation_id.to_string();

    tauri::async_runtime::spawn(async move {
        run_autonomous_contract_retry_async(
            task_app,
            task_state,
            task_job_id,
            animation_id,
            conversation_id,
            anchor_slug,
            max_deterministic,
            max_ai_attempts,
        )
        .await;
    });

    if let Some(app) = app.as_ref() {
        let _ = emit_job(Some(app), state, &job_id);
    }

    let animation_id = input.animation_id.clone();
    Ok(ContractRetryResult {
        animation_id,
        passed: false,
        attempts: Vec::new(),
        corrective_prompt: None,
        generation_request_id: None,
        job_id: Some(job_id),
        next_step: Some(
            "Autonomous contract retry queued — poll get_job with jobId until completed or failed, then check_size_contract.".into(),
        ),
        final_report: initial_report,
    })
}

fn persist_contract_retry_attempts(
    app: Option<&AppHandle>,
    state: &AppState,
    job_id: &str,
    attempts: &[ContractRetryAttempt],
    metrics: Option<&ContractRetryMetrics>,
) {
    let metadata = match metrics {
        Some(metrics) => serde_json::json!({ "attempts": attempts, "metrics": metrics }),
        None => serde_json::json!({ "attempts": attempts }),
    };
    let _ = set_job_metadata(app, state, job_id, &metadata.to_string());
}

fn contract_retry_metrics(
    timer: &PipelineTimer,
    stage_timer: &PipelineStageTimer,
    outcome: &str,
    failure_reason: Option<&str>,
    deterministic_passes: u32,
    ai_attempts: u32,
) -> ContractRetryMetrics {
    let stage_durations = stage_timer.stage_durations_ms();
    ContractRetryMetrics {
        duration_ms: timer.elapsed_ms(),
        outcome: outcome.to_string(),
        deterministic_passes,
        ai_attempts,
        failure_reason: failure_reason.map(str::to_string),
        stage_durations_ms: if stage_durations.is_empty() {
            None
        } else {
            Some(stage_durations.clone())
        },
    }
}

fn log_contract_retry_outcome(
    root: &Path,
    workspace_id: &str,
    animation_id: &str,
    job_id: &str,
    timer: &PipelineTimer,
    stage_timer: &PipelineStageTimer,
    outcome: &str,
    passed: bool,
    failure_reason: Option<&str>,
    deterministic_passes: u32,
    ai_attempts: u32,
) {
    let log_stage = match outcome {
        "passed" => "completed",
        "cancelled" => "cancelled",
        _ => "failed",
    };
    let metrics = contract_retry_metrics(
        timer,
        stage_timer,
        outcome,
        failure_reason,
        deterministic_passes,
        ai_attempts,
    );
    log_pipeline_event(
        root,
        workspace_id,
        "contract_retry",
        Some(log_stage),
        Some(std::time::Duration::from_millis(timer.elapsed_ms())),
        Some(animation_id),
        Some(job_id),
        Some(passed),
        failure_reason,
        Some(serde_json::to_value(metrics).unwrap_or(serde_json::Value::Null)),
    );
}

fn record_contract_retry_terminal(
    app: Option<&AppHandle>,
    state: &AppState,
    root: &Path,
    workspace_id: &str,
    animation_id: &str,
    job_id: &str,
    timer: &PipelineTimer,
    stage_timer: &PipelineStageTimer,
    attempts: &[ContractRetryAttempt],
    outcome: &str,
    passed: bool,
    failure_reason: Option<&str>,
    deterministic_passes: u32,
    ai_attempts: u32,
) {
    let metrics = contract_retry_metrics(
        timer,
        stage_timer,
        outcome,
        failure_reason,
        deterministic_passes,
        ai_attempts,
    );
    persist_contract_retry_attempts(app, state, job_id, attempts, Some(&metrics));
    log_contract_retry_outcome(
        root,
        workspace_id,
        animation_id,
        job_id,
        timer,
        stage_timer,
        outcome,
        passed,
        failure_reason,
        deterministic_passes,
        ai_attempts,
    );
}

fn resolve_contract_retry_workspace(
    state: &AppState,
    animation_id: &str,
    job_id: &str,
) -> Option<(String, PathBuf)> {
    if let Ok((workspace_id, ..)) = load_animation_context(state, animation_id) {
        if let Ok(root) = workspace_path(state, &workspace_id) {
            return Some((workspace_id, root));
        }
    }
    let workspace_id: Option<String> = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))
            .ok()?;
        connection
            .query_row(
                "SELECT project_id FROM background_jobs WHERE id=?1",
                [job_id],
                |row| row.get(0),
            )
            .optional()
            .ok()?
    };
    let workspace_id = workspace_id?;
    workspace_path(state, &workspace_id)
        .ok()
        .map(|root| (workspace_id, root))
}

fn try_record_contract_retry_terminal(
    app: Option<&AppHandle>,
    state: &AppState,
    animation_id: &str,
    job_id: &str,
    timer: &PipelineTimer,
    stage_timer: &PipelineStageTimer,
    attempts: &[ContractRetryAttempt],
    outcome: &str,
    passed: bool,
    failure_reason: Option<&str>,
    deterministic_passes: u32,
    ai_attempts: u32,
) {
    let metrics = contract_retry_metrics(
        timer,
        stage_timer,
        outcome,
        failure_reason,
        deterministic_passes,
        ai_attempts,
    );
    persist_contract_retry_attempts(app, state, job_id, attempts, Some(&metrics));
    if let Some((workspace_id, root)) =
        resolve_contract_retry_workspace(state, animation_id, job_id)
    {
        log_contract_retry_outcome(
            &root,
            &workspace_id,
            animation_id,
            job_id,
            timer,
            stage_timer,
            outcome,
            passed,
            failure_reason,
            deterministic_passes,
            ai_attempts,
        );
    }
}

fn mark_contract_retry_failed(
    app: Option<&AppHandle>,
    state: &AppState,
    job_id: &str,
    message: &str,
) {
    let _ = set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "failed",
            progress: 0.0,
            stage: "Failed",
            error_message: Some(message),
            result_path: None,
        },
    );
}

async fn run_autonomous_contract_retry_async(
    app: Option<AppHandle>,
    state: AppState,
    job_id: String,
    animation_id: String,
    conversation_id: String,
    anchor_slug: Option<String>,
    max_deterministic_passes: u32,
    max_ai_attempts: u32,
) {
    let timer = PipelineTimer::start();
    let mut stage_timer = PipelineStageTimer::start();
    let mut deterministic_passes_run = 0_u32;
    let mut ai_attempts_run = 0_u32;
    if matches!(
        tauri::async_runtime::spawn_blocking({
            let job_id = job_id.clone();
            let state = state.clone();
            move || cancellation_requested(&state, &job_id)
        })
        .await,
        Ok(Ok(true))
    ) {
        stage_timer.mark("cancelled_before_start");
        try_record_contract_retry_terminal(
            app.as_ref(),
            &state,
            &animation_id,
            &job_id,
            &timer,
            &stage_timer,
            &[],
            "cancelled",
            false,
            Some("job_cancelled"),
            deterministic_passes_run,
            ai_attempts_run,
        );
        let _ = set_job_state(
            app.as_ref(),
            &state,
            &job_id,
            JobProgress {
                status: "cancelled",
                progress: 0.0,
                stage: "Cancelled",
                error_message: None,
                result_path: None,
            },
        );
        return;
    }

    let _ = set_job_state(
        app.as_ref(),
        &state,
        &job_id,
        JobProgress {
            status: "running",
            progress: 0.05,
            stage: "Deterministic repair",
            error_message: None,
            result_path: None,
        },
    );
    persist_contract_retry_attempts(app.as_ref(), &state, &job_id, &[], None);

    let context = tauri::async_runtime::spawn_blocking({
        let animation_id = animation_id.clone();
        let anchor_slug = anchor_slug.clone();
        let state = state.clone();
        move || -> CommandResult<(String, String, u32, PathBuf, crate::models::SizeContractReport)> {
            let (workspace_id, animation_name, _fps, _looping, frame_count, _worktree_id) =
                load_animation_context(&state, &animation_id)?;
            let root = workspace_path(&state, &workspace_id)?;
            let report =
                size_contract_check_inner(&state, &animation_id, anchor_slug.as_deref())?;
            Ok((workspace_id, animation_name, frame_count, root, report))
        }
    })
    .await;

    let (workspace_id, animation_name, frame_count, root, mut report) = match context {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            stage_timer.mark("context_load_failed");
            try_record_contract_retry_terminal(
                app.as_ref(),
                &state,
                &animation_id,
                &job_id,
                &timer,
                &stage_timer,
                &[],
                "failed",
                false,
                Some(&error.code),
                deterministic_passes_run,
                ai_attempts_run,
            );
            mark_contract_retry_failed(app.as_ref(), &state, &job_id, &error.message);
            return;
        }
        Err(error) => {
            stage_timer.mark("context_load_failed");
            try_record_contract_retry_terminal(
                app.as_ref(),
                &state,
                &animation_id,
                &job_id,
                &timer,
                &stage_timer,
                &[],
                "failed",
                false,
                Some("job_error"),
                deterministic_passes_run,
                ai_attempts_run,
            );
            mark_contract_retry_failed(app.as_ref(), &state, &job_id, &error.to_string());
            return;
        }
    };
    stage_timer.mark("context_load");

    let mut attempts = Vec::new();
    for pass in 1..=max_deterministic_passes {
        if matches!(
            tauri::async_runtime::spawn_blocking({
                let job_id = job_id.clone();
                let state = state.clone();
                move || cancellation_requested(&state, &job_id)
            })
            .await,
            Ok(Ok(true))
        ) {
            stage_timer.mark(format!("cancelled_deterministic_{pass}"));
            record_contract_retry_terminal(
                app.as_ref(),
                &state,
                &root,
                &workspace_id,
                &animation_id,
                &job_id,
                &timer,
                &stage_timer,
                &attempts,
                "cancelled",
                false,
                Some("job_cancelled"),
                deterministic_passes_run,
                ai_attempts_run,
            );
            let _ = set_job_state(
                app.as_ref(),
                &state,
                &job_id,
                JobProgress {
                    status: "cancelled",
                    progress: 0.0,
                    stage: "Cancelled",
                    error_message: None,
                    result_path: None,
                },
            );
            return;
        }

        if report.passed || !has_deterministic_fixable(&report.violations) {
            break;
        }

        let stage = format!("Deterministic repair ({pass}/{max_deterministic_passes})");
        let _ = set_job_state(
            app.as_ref(),
            &state,
            &job_id,
            JobProgress {
                status: "running",
                progress: 0.05 + (pass as f64 * 0.08),
                stage: &stage,
                error_message: None,
                result_path: None,
            },
        );

        let repair = tauri::async_runtime::spawn_blocking({
            let animation_id = animation_id.clone();
            let anchor_slug = anchor_slug.clone();
            let state = state.clone();
            let app = app.clone();
            move || -> CommandResult<(Vec<String>, crate::models::SizeContractReport)> {
                let (_, actions) = apply_deterministic_contract_repairs(
                    &state,
                    &animation_id,
                    anchor_slug.as_deref(),
                    app.as_ref(),
                )?;
                let report =
                    size_contract_check_inner(&state, &animation_id, anchor_slug.as_deref())?;
                Ok((actions, report))
            }
        })
        .await;

        match repair {
            Ok(Ok((actions, updated_report))) => {
                report = updated_report;
                attempts.push(ContractRetryAttempt {
                    attempt: pass,
                    phase: "deterministic".into(),
                    report: report.clone(),
                    actions,
                });
                deterministic_passes_run += 1;
                stage_timer.mark(format!("deterministic_pass_{pass}"));
                persist_contract_retry_attempts(app.as_ref(), &state, &job_id, &attempts, None);
            }
            Ok(Err(error)) => {
                stage_timer.mark(format!("deterministic_failed_{pass}"));
                record_contract_retry_terminal(
                    app.as_ref(),
                    &state,
                    &root,
                    &workspace_id,
                    &animation_id,
                    &job_id,
                    &timer,
                    &stage_timer,
                    &attempts,
                    "failed",
                    false,
                    Some(&error.code),
                    deterministic_passes_run,
                    ai_attempts_run,
                );
                mark_contract_retry_failed(app.as_ref(), &state, &job_id, &error.message);
                return;
            }
            Err(error) => {
                stage_timer.mark(format!("deterministic_failed_{pass}"));
                record_contract_retry_terminal(
                    app.as_ref(),
                    &state,
                    &root,
                    &workspace_id,
                    &animation_id,
                    &job_id,
                    &timer,
                    &stage_timer,
                    &attempts,
                    "failed",
                    false,
                    Some("job_error"),
                    deterministic_passes_run,
                    ai_attempts_run,
                );
                mark_contract_retry_failed(app.as_ref(), &state, &job_id, &error.to_string());
                return;
            }
        }
    }

    if report.passed {
        stage_timer.mark("deterministic_passed");
        record_contract_retry_terminal(
            app.as_ref(),
            &state,
            &root,
            &workspace_id,
            &animation_id,
            &job_id,
            &timer,
            &stage_timer,
            &attempts,
            "passed",
            true,
            None,
            deterministic_passes_run,
            ai_attempts_run,
        );
        let _ = set_job_state(
            app.as_ref(),
            &state,
            &job_id,
            JobProgress {
                status: "completed",
                progress: 1.0,
                stage: "Contract passed",
                error_message: None,
                result_path: None,
            },
        );
        super::sessions::append_animation_job_session(&state, &animation_id, "contract_retry", None, None);
        return;
    }

    let slug = report
        .anchor_slug
        .clone()
        .or(anchor_slug.clone())
        .unwrap_or_default();
    let anchor = match tauri::async_runtime::spawn_blocking({
        let workspace_id = workspace_id.clone();
        let slug = slug.clone();
        let state = state.clone();
        move || get_anchor_inner(&workspace_id, &slug, &state)
    })
    .await
    {
        Ok(Ok(anchor)) => anchor,
        _ => {
            stage_timer.mark("anchor_missing");
            record_contract_retry_terminal(
                app.as_ref(),
                &state,
                &root,
                &workspace_id,
                &animation_id,
                &job_id,
                &timer,
                &stage_timer,
                &attempts,
                "failed",
                false,
                Some("anchor_required"),
                deterministic_passes_run,
                ai_attempts_run,
            );
            mark_contract_retry_failed(
                app.as_ref(),
                &state,
                &job_id,
                "Promote a character anchor before autonomous contract retry",
            );
            return;
        }
    };

    for ai_attempt in 1..=max_ai_attempts {
        ai_attempts_run += 1;
        if matches!(
            tauri::async_runtime::spawn_blocking({
                let job_id = job_id.clone();
                let state = state.clone();
                move || cancellation_requested(&state, &job_id)
            })
            .await,
            Ok(Ok(true))
        ) {
            stage_timer.mark(format!("cancelled_ai_{ai_attempt}"));
            record_contract_retry_terminal(
                app.as_ref(),
                &state,
                &root,
                &workspace_id,
                &animation_id,
                &job_id,
                &timer,
                &stage_timer,
                &attempts,
                "cancelled",
                false,
                Some("job_cancelled"),
                deterministic_passes_run,
                ai_attempts_run,
            );
            let _ = set_job_state(
                app.as_ref(),
                &state,
                &job_id,
                JobProgress {
                    status: "cancelled",
                    progress: 0.0,
                    stage: "Cancelled",
                    error_message: None,
                    result_path: None,
                },
            );
            return;
        }

        if report.passed {
            break;
        }

        let corrective_prompt = build_corrective_prompt(
            &report,
            &anchor,
            &animation_name,
            frame_count,
            attempts.len() as u32 + 1,
        );
        let manifest_before = read_generation_manifest(&root)
            .ok()
            .flatten()
            .map(|value| value.generated_at);

        let regen_stage = format!("Regenerating strip ({ai_attempt}/{max_ai_attempts})");
        let _ = set_job_state(
            app.as_ref(),
            &state,
            &job_id,
            JobProgress {
                status: "running",
                progress: 0.2 + ((ai_attempt - 1) as f64 * 0.35),
                stage: &regen_stage,
                error_message: None,
                result_path: None,
            },
        );

        let request_id = match tauri::async_runtime::spawn_blocking({
            let conversation_id = conversation_id.clone();
            let corrective_prompt = corrective_prompt.clone();
            let animation_name = animation_name.clone();
            let state = state.clone();
            let app = app.clone();
            move || {
                start_provider_run(
                    conversation_id,
                    corrective_prompt,
                    Some(format!(
                        "Autonomous contract retry for \"{animation_name}\" ({frame_count} frames)."
                    )),
                    None,
                    app.clone(),
                    &state,
                )
            }
        })
        .await
        {
            Ok(Ok(request_id)) => {
                stage_timer.mark(format!("ai_regenerate_{ai_attempt}"));
                request_id
            }
            Ok(Err(error)) => {
                stage_timer.mark(format!("ai_regenerate_failed_{ai_attempt}"));
                record_contract_retry_terminal(
                    app.as_ref(),
                    &state,
                    &root,
                    &workspace_id,
                    &animation_id,
                    &job_id,
                    &timer,
                    &stage_timer,
                    &attempts,
                    "failed",
                    false,
                    Some(&error.code),
                    deterministic_passes_run,
                    ai_attempts_run,
                );
                mark_contract_retry_failed(app.as_ref(), &state, &job_id, &error.message);
                return;
            }
            Err(error) => {
                stage_timer.mark(format!("ai_regenerate_failed_{ai_attempt}"));
                record_contract_retry_terminal(
                    app.as_ref(),
                    &state,
                    &root,
                    &workspace_id,
                    &animation_id,
                    &job_id,
                    &timer,
                    &stage_timer,
                    &attempts,
                    "failed",
                    false,
                    Some("job_error"),
                    deterministic_passes_run,
                    ai_attempts_run,
                );
                mark_contract_retry_failed(app.as_ref(), &state, &job_id, &error.to_string());
                return;
            }
        };

        attempts.push(ContractRetryAttempt {
            attempt: ai_attempt,
            phase: "regenerate".into(),
            report: report.clone(),
            actions: vec!["start_provider_run".into()],
        });
        persist_contract_retry_attempts(app.as_ref(), &state, &job_id, &attempts, None);

        let wait_stage = format!("Waiting for generation ({ai_attempt}/{max_ai_attempts})");
        let _ = set_job_state(
            app.as_ref(),
            &state,
            &job_id,
            JobProgress {
                status: "running",
                progress: 0.35 + ((ai_attempt - 1) as f64 * 0.35),
                stage: &wait_stage,
                error_message: None,
                result_path: None,
            },
        );

        if let Err(error) = wait_for_generation(&state, &request_id, &job_id).await {
            if error.code == "job_cancelled" {
                stage_timer.mark(format!("cancelled_wait_{ai_attempt}"));
                record_contract_retry_terminal(
                    app.as_ref(),
                    &state,
                    &root,
                    &workspace_id,
                    &animation_id,
                    &job_id,
                    &timer,
                    &stage_timer,
                    &attempts,
                    "cancelled",
                    false,
                    Some("job_cancelled"),
                    deterministic_passes_run,
                    ai_attempts_run,
                );
                let _ = set_job_state(
                    app.as_ref(),
                    &state,
                    &job_id,
                    JobProgress {
                        status: "cancelled",
                        progress: 0.0,
                        stage: "Cancelled",
                        error_message: None,
                        result_path: None,
                    },
                );
                return;
            }
            stage_timer.mark(format!("wait_failed_{ai_attempt}"));
            record_contract_retry_terminal(
                app.as_ref(),
                &state,
                &root,
                &workspace_id,
                &animation_id,
                &job_id,
                &timer,
                &stage_timer,
                &attempts,
                "failed",
                false,
                Some(&error.code),
                deterministic_passes_run,
                ai_attempts_run,
            );
            mark_contract_retry_failed(app.as_ref(), &state, &job_id, &error.message);
            return;
        }
        stage_timer.mark(format!("wait_generation_{ai_attempt}"));

        let import_stage = format!("Importing regenerated strip ({ai_attempt}/{max_ai_attempts})");
        let _ = set_job_state(
            app.as_ref(),
            &state,
            &job_id,
            JobProgress {
                status: "running",
                progress: 0.55 + ((ai_attempt - 1) as f64 * 0.35),
                stage: &import_stage,
                error_message: None,
                result_path: None,
            },
        );

        let finalize = tauri::async_runtime::spawn_blocking({
            let workspace_id = workspace_id.clone();
            let animation_id = animation_id.clone();
            let anchor_slug = anchor_slug.clone();
            let manifest_before = manifest_before.clone();
            let state = state.clone();
            let app = app.clone();
            move || {
                let root = workspace_path(&state, &workspace_id)?;
                let _ = scan_generation_assets_inner(&workspace_id, None, app.as_ref(), &state)?;
                let manifest = read_generation_manifest(&root)?
                    .ok_or_else(|| CommandError::new("generation_missing", "No generation manifest found"))?;
                if manifest_before.as_deref() == Some(manifest.generated_at.as_str()) {
                    return Err(CommandError::new(
                        "generation_stale",
                        "Generation completed but manifest was not refreshed",
                    ));
                }
                let strip_path = find_horizontal_strip_path(&root, &manifest).ok_or_else(|| {
                    CommandError::new(
                        "strip_not_found",
                        "Could not find a horizontal strip in the latest generation manifest",
                    )
                })?;
                finalize_contract_retry_inner(
                    &state,
                    &animation_id,
                    strip_path.to_string_lossy().as_ref(),
                    frame_count,
                    anchor_slug.as_deref(),
                    Some("profile"),
                    app.as_ref(),
                )
            }
        })
        .await;

        match finalize {
            Ok(Ok(result)) => {
                report = result.final_report.clone();
                attempts.extend(result.attempts);
                persist_contract_retry_attempts(app.as_ref(), &state, &job_id, &attempts, None);
                stage_timer.mark(format!("import_finalize_{ai_attempt}"));
                if result.passed {
                    record_contract_retry_terminal(
                        app.as_ref(),
                        &state,
                        &root,
                        &workspace_id,
                        &animation_id,
                        &job_id,
                        &timer,
                        &stage_timer,
                        &attempts,
                        "passed",
                        true,
                        None,
                        deterministic_passes_run,
                        ai_attempts_run,
                    );
                    let _ = set_job_state(
                        app.as_ref(),
                        &state,
                        &job_id,
                        JobProgress {
                            status: "completed",
                            progress: 1.0,
                            stage: "Contract passed",
                            error_message: None,
                            result_path: None,
                        },
                    );
                    super::sessions::append_animation_job_session(
                        &state,
                        &animation_id,
                        "contract_retry",
                        None,
                        None,
                    );
                    return;
                }
            }
            Ok(Err(error)) => {
                stage_timer.mark(format!("import_failed_{ai_attempt}"));
                record_contract_retry_terminal(
                    app.as_ref(),
                    &state,
                    &root,
                    &workspace_id,
                    &animation_id,
                    &job_id,
                    &timer,
                    &stage_timer,
                    &attempts,
                    "failed",
                    false,
                    Some(&error.code),
                    deterministic_passes_run,
                    ai_attempts_run,
                );
                mark_contract_retry_failed(app.as_ref(), &state, &job_id, &error.message);
                return;
            }
            Err(error) => {
                stage_timer.mark(format!("import_failed_{ai_attempt}"));
                record_contract_retry_terminal(
                    app.as_ref(),
                    &state,
                    &root,
                    &workspace_id,
                    &animation_id,
                    &job_id,
                    &timer,
                    &stage_timer,
                    &attempts,
                    "failed",
                    false,
                    Some("job_error"),
                    deterministic_passes_run,
                    ai_attempts_run,
                );
                mark_contract_retry_failed(app.as_ref(), &state, &job_id, &error.to_string());
                return;
            }
        }
    }

    let outcome = if report.passed { "passed" } else { "failed" };
    let failure_reason = if report.passed {
        None
    } else {
        Some("contract_violations_remain")
    };
    stage_timer.mark("final_outcome");
    record_contract_retry_terminal(
        app.as_ref(),
        &state,
        &root,
        &workspace_id,
        &animation_id,
        &job_id,
        &timer,
        &stage_timer,
        &attempts,
        outcome,
        report.passed,
        failure_reason,
        deterministic_passes_run,
        ai_attempts_run,
    );
    let _ = set_job_state(
        app.as_ref(),
        &state,
        &job_id,
        JobProgress {
            status: if report.passed { "completed" } else { "failed" },
            progress: 1.0,
            stage: if report.passed {
                "Contract passed"
            } else {
                "Contract still failing"
            },
            error_message: if report.passed {
                None
            } else {
                Some("Size contract still has violations after autonomous retry")
            },
            result_path: None,
        },
    );
}

#[tauri::command]
pub fn queue_contract_retry(
    input: QueueContractRetryInput,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> CommandResult<ContractRetryResult> {
    queue_contract_retry_inner(input, Some(app), &state)
}
