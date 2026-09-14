use crate::{
    assets::{read_generation_manifest, scan_generation_assets_inner},
    error::{CommandError, CommandResult},
    jobs::{
        cancellation_requested, load_job, request_job_cancellation, set_job_metadata, set_job_state,
        JobProgress,
    },
    models::{
        HardenAnimationOptions, MotionBatchProgressEntry, MotionBatchResult, MotionPreset,
        QueueDirectionSetInput, QueueMotionBatchInput,
    },
    providers::start_provider_run,
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use std::time::{Duration, Instant};
use tauri::AppHandle;
use uuid::Uuid;

use super::anchors::get_anchor_inner;
use super::contract_retry::load_animation_context;
use super::contract_retry_job::{find_horizontal_strip_path, wait_for_generation};
use super::direction_meta::{
    infer_direction_family, list_direction_meta_for_worktree, load_direction_meta,
};
use super::direction_set::{
    build_direction_facing_prompt, import_facing_animation_from_strip, queue_direction_set_inner,
};
use super::harden::harden_animation_inner;
use super::logging::log_pipeline_event;
use super::motion_presets::{load_motion_presets_inner, resolve_batch_presets};

const JOB_POLL_INTERVAL: Duration = Duration::from_millis(500);
const JOB_WAIT_TIMEOUT: Duration = Duration::from_secs(3600);

fn matches_anchor(meta: &crate::models::AnimationDirectionMeta, anchor_slug: &str) -> bool {
    meta.anchor_slug
        .as_deref()
        .map(|slug| slug == anchor_slug)
        .unwrap_or(true)
}

fn find_motion_source(
    state: &AppState,
    workspace_id: &str,
    worktree_id: &str,
    motion: &str,
    anchor_slug: &str,
) -> Option<String> {
    let metas = list_direction_meta_for_worktree(state, workspace_id, worktree_id).ok()?;
    metas
        .iter()
        .find(|meta| {
            meta.direction_family == motion
                && meta.facing == "w"
                && matches_anchor(meta, anchor_slug)
        })
        .map(|meta| meta.animation_id.clone())
        .or_else(|| {
            metas
                .iter()
                .find(|meta| meta.direction_family == motion && matches_anchor(meta, anchor_slug))
                .map(|meta| meta.animation_id.clone())
        })
}

fn animation_ids_for_motion(
    state: &AppState,
    workspace_id: &str,
    worktree_id: &str,
    motion: &str,
    anchor_slug: &str,
) -> Vec<String> {
    list_direction_meta_for_worktree(state, workspace_id, worktree_id)
        .unwrap_or_default()
        .into_iter()
        .filter(|meta| meta.direction_family == motion && matches_anchor(meta, anchor_slug))
        .map(|meta| meta.animation_id)
        .collect()
}

fn seed_timing(
    state: &AppState,
    workspace_id: &str,
    seed_animation_id: Option<&str>,
    preset: &MotionPreset,
) -> (u32, f64, bool) {
    if let Some(seed_id) = seed_animation_id {
        if let Ok((_, name, fps, looping, frame_count, _)) = load_animation_context(state, seed_id) {
            let family = load_direction_meta(state, workspace_id, seed_id)
                .ok()
                .flatten()
                .map(|meta| meta.direction_family)
                .unwrap_or_else(|| infer_direction_family(&name));
            if family == preset.motion {
                return (frame_count, fps, looping);
            }
        }
    }
    (preset.frame_count, preset.fps, preset.looping)
}

fn facings_for_set(set: &str) -> Vec<&'static str> {
    if set == "8" {
        vec!["n", "ne", "e", "se", "s", "sw", "w", "nw"]
    } else {
        vec!["n", "e", "s", "w"]
    }
}

fn facing_from_stage(stage: &str) -> Option<String> {
    for prefix in ["Generating ", "Mirroring "] {
        if let Some(rest) = stage.strip_prefix(prefix) {
            return Some(rest.trim().to_ascii_lowercase());
        }
    }
    None
}

fn build_facing_entries(
    motion: &str,
    set: &str,
    source_facing: &str,
    pending: &[String],
) -> Vec<MotionBatchProgressEntry> {
    facings_for_set(set)
        .into_iter()
        .map(|facing| {
            let status = if facing == source_facing {
                "completed"
            } else if pending.iter().any(|value| value == facing) {
                "queued"
            } else {
                "queued"
            };
            MotionBatchProgressEntry {
                motion: motion.to_string(),
                phase: "direction_set".into(),
                status: status.into(),
                facing: Some(facing.to_string()),
                message: None,
                child_job_id: None,
            }
        })
        .collect()
}

fn sync_facing_progress(
    motion: &str,
    stage: &str,
    facing_entries: &mut [MotionBatchProgressEntry],
) {
    if let Some(facing) = facing_from_stage(stage) {
        if let Some(entry) = facing_entries
            .iter_mut()
            .find(|entry| entry.motion == motion && entry.facing.as_deref() == Some(facing.as_str()))
        {
            entry.status = "running".into();
            entry.phase = stage.to_string();
        }
    }
}

fn mark_facing_entries(
    motion: &str,
    facing_entries: &mut [MotionBatchProgressEntry],
    status: &str,
    phase: &str,
) {
    for entry in facing_entries
        .iter_mut()
        .filter(|entry| entry.motion == motion)
    {
        entry.status = status.into();
        entry.phase = phase.into();
    }
}

async fn wait_for_child_job<F>(
    app: Option<&AppHandle>,
    state: &AppState,
    batch_job_id: &str,
    child_job_id: &str,
    mut on_progress: F,
) -> CommandResult<()>
where
    F: FnMut(&crate::models::BackgroundJob),
{
    let batch_job_id = batch_job_id.to_string();
    let child_job_id = child_job_id.to_string();
    let started = Instant::now();
    while started.elapsed() < JOB_WAIT_TIMEOUT {
        if matches!(
            tauri::async_runtime::spawn_blocking({
                let batch_job_id = batch_job_id.clone();
                let state = state.clone();
                move || cancellation_requested(&state, &batch_job_id)
            })
            .await,
            Ok(Ok(true))
        ) {
            let _ = tauri::async_runtime::spawn_blocking({
                let child_job_id = child_job_id.clone();
                let state = state.clone();
                move || request_job_cancellation(&state, &child_job_id)
            })
            .await;
            return Err(CommandError::new("job_cancelled", "Motion batch was cancelled"));
        }
        let child = tauri::async_runtime::spawn_blocking({
            let child_job_id = child_job_id.clone();
            let state = state.clone();
            move || load_job(&state, &child_job_id)
        })
        .await
        .map_err(|error| CommandError::new("job_wait_failed", error.to_string()))??;
        on_progress(&child);
        let _ = set_job_state(
            app,
            state,
            &batch_job_id,
            JobProgress {
                status: "running",
                progress: child.progress,
                stage: &child.stage,
                error_message: None,
                result_path: None,
            },
        );
        match child.status.as_str() {
            "completed" => return Ok(()),
            "failed" => {
                return Err(CommandError::new(
                    "child_job_failed",
                    child
                        .error_message
                        .unwrap_or_else(|| "Direction set failed".into()),
                ));
            }
            "cancelled" => {
                return Err(CommandError::new("child_job_cancelled", "Direction set cancelled"));
            }
            _ => tokio::time::sleep(JOB_POLL_INTERVAL).await,
        }
    }
    Err(CommandError::new("job_timeout", "Timed out waiting for direction set"))
}

async fn generate_canonical_motion_source(
    app: Option<&AppHandle>,
    state: &AppState,
    batch_job_id: &str,
    input: &QueueMotionBatchInput,
    preset: &MotionPreset,
    frame_count: u32,
    fps: f64,
    looping: bool,
) -> CommandResult<String> {
    let conversation_id = input
        .conversation_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CommandError::new(
                "conversation_required",
                format!(
                    "Motion \"{}\" has no west-facing source; open a chat conversation for AI generation",
                    preset.motion
                ),
            )
        })?;
    let anchor = get_anchor_inner(&input.workspace_id, &input.anchor_slug, state)?;
    let root = workspace_path(state, &input.workspace_id)?;
    let source_name = format!("{}-{}", input.anchor_slug, preset.motion);
    let prompt = build_direction_facing_prompt(
        &anchor,
        &preset.motion,
        "w",
        frame_count,
        &source_name,
    );
    let manifest_before = read_generation_manifest(&root)
        .ok()
        .flatten()
        .map(|value| value.generated_at);
    let request_id = tauri::async_runtime::spawn_blocking({
        let conversation_id = conversation_id.to_string();
        let prompt = prompt.clone();
        let source_name = source_name.clone();
        let state = state.clone();
        let app = app.cloned();
        move || {
            start_provider_run(
                conversation_id,
                prompt,
                Some(format!(
                    "Motion batch canonical strip for \"{source_name}\" facing west."
                )),
                None,
                app,
                &state,
            )
        }
    })
    .await
    .map_err(|error| CommandError::new("generation_spawn_failed", error.to_string()))??;
    wait_for_generation(state, &request_id, batch_job_id).await?;
    tauri::async_runtime::spawn_blocking({
        let workspace_id = input.workspace_id.clone();
        let worktree_id = input.worktree_id.clone();
        let anchor_slug = input.anchor_slug.clone();
        let motion = preset.motion.clone();
        let state = state.clone();
        let app = app.cloned();
        let manifest_before = manifest_before.clone();
        move || {
            let _ = scan_generation_assets_inner(
                &workspace_id,
                Some(&worktree_id),
                app.as_ref(),
                &state,
            )?;
            let manifest = read_generation_manifest(&workspace_path(&state, &workspace_id)?)?
                .ok_or_else(|| {
                    CommandError::new("generation_missing", "No generation manifest found")
                })?;
            if manifest_before.as_deref() == Some(manifest.generated_at.as_str()) {
                return Err(CommandError::new(
                    "generation_stale",
                    "Generation completed but manifest was not refreshed",
                ));
            }
            let strip_path = find_horizontal_strip_path(
                &workspace_path(&state, &workspace_id)?,
                &manifest,
            )
            .ok_or_else(|| {
                CommandError::new(
                    "strip_not_found",
                    "Could not find a horizontal strip in the latest generation manifest",
                )
            })?;
            import_facing_animation_from_strip(
                app.as_ref(),
                &state,
                &workspace_id,
                &worktree_id,
                &source_name,
                "w",
                "w",
                &motion,
                &anchor_slug,
                strip_path.to_string_lossy().as_ref(),
                frame_count,
                fps,
                looping,
            )
        }
    })
    .await
    .map_err(|error| CommandError::new("generation_import_failed", error.to_string()))?
}

fn update_batch_metadata(
    app: Option<&AppHandle>,
    state: &AppState,
    job_id: &str,
    entries: &[MotionBatchProgressEntry],
    facing_entries: &[MotionBatchProgressEntry],
    skipped: &[String],
) {
    let metadata = serde_json::json!({
        "entries": entries,
        "facingEntries": facing_entries,
        "skippedMotions": skipped,
    });
    let _ = set_job_metadata(app, state, job_id, &metadata.to_string());
}

pub(crate) fn queue_motion_batch_inner(
    input: QueueMotionBatchInput,
    app: Option<AppHandle>,
    state: &AppState,
) -> CommandResult<MotionBatchResult> {
    let set = input.set.trim();
    if set != "4" && set != "8" {
        return Err(CommandError::new(
            "invalid_direction_set",
            "set must be \"4\" or \"8\"",
        ));
    }
    get_anchor_inner(&input.workspace_id, &input.anchor_slug, state)?;

    let catalog = load_motion_presets_inner(state, &input.workspace_id)?;
    let mut presets = resolve_batch_presets(&catalog, &input.motions);
    if input.only_missing == Some(true) {
        let missing = super::motion_presets::list_missing_motions_inner(
            state,
            &input.workspace_id,
            &input.worktree_id,
            &input.anchor_slug,
        )?;
        presets.retain(|preset| missing.missing_preset_ids.iter().any(|id| id == &preset.id));
        if presets.is_empty() {
            return Err(CommandError::new(
                "no_missing_motions",
                "Every selected motion already has a passing canonical-facing animation",
            ));
        }
    }
    if presets.is_empty() {
        return Err(CommandError::new(
            "no_motion_presets",
            "No enabled motion presets selected for batch",
        ));
    }

    let active_job_id: Option<String> = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                r#"SELECT id FROM background_jobs
                   WHERE kind='motion_batch' AND worktree_id=?1
                     AND status IN ('queued','running')
                   ORDER BY created_at DESC LIMIT 1"#,
                [&input.worktree_id],
                |row| row.get(0),
            )
            .optional()?
    };
    if active_job_id.is_some() {
        return Err(CommandError::new(
            "motion_batch_active",
            "A motion batch job is already running for this worktree",
        ));
    }

    let job_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let motions = presets.iter().map(|preset| preset.motion.clone()).collect::<Vec<_>>();
    let initial_entries = motions
        .iter()
        .map(|motion| MotionBatchProgressEntry {
            motion: motion.clone(),
            phase: "queued".into(),
            status: "queued".into(),
            facing: None,
            message: None,
            child_job_id: None,
        })
        .collect::<Vec<_>>();
    {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.execute(
            r#"INSERT INTO background_jobs(
                id,project_id,worktree_id,kind,target_type,target_id,status,
                progress,stage,created_at,updated_at,started_at
            ) VALUES (?1,?2,?3,'motion_batch','worktree',?3,'queued',0.0,'Queued',?4,?4,?4)"#,
            params![job_id, input.workspace_id, input.worktree_id, now],
        )?;
    }
    update_batch_metadata(
        app.as_ref(),
        state,
        &job_id,
        &initial_entries,
        &[],
        &[],
    );

    let task_app = app.clone();
    let task_state = state.clone();
    let task_input = input.clone();
    let task_job_id = job_id.clone();
    let task_presets = presets;
    tauri::async_runtime::spawn(async move {
        run_motion_batch_async(task_app, task_state, task_job_id, task_input, task_presets).await;
    });

    Ok(MotionBatchResult {
        job_id,
        motions,
        entries: initial_entries,
    })
}

async fn run_motion_batch_async(
    app: Option<AppHandle>,
    state: AppState,
    job_id: String,
    input: QueueMotionBatchInput,
    presets: Vec<MotionPreset>,
) {
    let app_ref = app.as_ref();
    let harden_after = input.harden_after.unwrap_or(true);
    let total = presets.len().max(1) as f64;
    let mut entries = presets
        .iter()
        .map(|preset| MotionBatchProgressEntry {
            motion: preset.motion.clone(),
            phase: "queued".into(),
            status: "queued".into(),
            facing: None,
            message: None,
            child_job_id: None,
        })
        .collect::<Vec<_>>();
    let mut facing_entries = Vec::<MotionBatchProgressEntry>::new();
    let mut skipped = Vec::<String>::new();

    let _ = set_job_state(
        app_ref,
        &state,
        &job_id,
        JobProgress {
            status: "running",
            progress: 0.0,
            stage: "Motion batch started",
            error_message: None,
            result_path: None,
        },
    );

    for (index, preset) in presets.iter().enumerate() {
        if matches!(
            tauri::async_runtime::spawn_blocking({
                let job_id = job_id.clone();
                let state = state.clone();
                move || cancellation_requested(&state, &job_id)
            })
            .await,
            Ok(Ok(true))
        ) {
            entries[index].status = "cancelled".into();
            mark_facing_entries(&preset.motion, &mut facing_entries, "cancelled", "Cancelled");
            update_batch_metadata(app_ref, &state, &job_id, &entries, &facing_entries, &skipped);
            let _ = set_job_state(
                app_ref,
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

        let base_progress = index as f64 / total;
        let stage = format!("Motion batch: {}", preset.motion);
        entries[index].phase = "resolve_source".into();
        entries[index].status = "running".into();
        update_batch_metadata(app_ref, &state, &job_id, &entries, &facing_entries, &skipped);
        let _ = set_job_state(
            app_ref,
            &state,
            &job_id,
            JobProgress {
                status: "running",
                progress: base_progress,
                stage: &stage,
                error_message: None,
                result_path: None,
            },
        );

        let (frame_count, fps, looping) = seed_timing(
            &state,
            &input.workspace_id,
            input.seed_animation_id.as_deref(),
            preset,
        );
        let source_animation_id = match find_motion_source(
            &state,
            &input.workspace_id,
            &input.worktree_id,
            &preset.motion,
            &input.anchor_slug,
        ) {
            Some(animation_id) => animation_id,
            None => match generate_canonical_motion_source(
                app_ref,
                &state,
                &job_id,
                &input,
                preset,
                frame_count,
                fps,
                looping,
            )
            .await
            {
                Ok(animation_id) => animation_id,
                Err(error) => {
                    entries[index].phase = "skipped".into();
                    entries[index].status = "skipped".into();
                    entries[index].message = Some(error.message.clone());
                    skipped.push(preset.motion.clone());
                    update_batch_metadata(
                        app_ref,
                        &state,
                        &job_id,
                        &entries,
                        &facing_entries,
                        &skipped,
                    );
                    continue;
                }
            },
        };

        entries[index].phase = "direction_set".into();
        entries[index].status = "running".into();
        let source_facing = load_direction_meta(&state, &input.workspace_id, &source_animation_id)
            .ok()
            .flatten()
            .map(|meta| meta.facing)
            .unwrap_or_else(|| "w".to_string());
        let direction_result = queue_direction_set_inner(
            QueueDirectionSetInput {
                workspace_id: input.workspace_id.clone(),
                worktree_id: input.worktree_id.clone(),
                source_animation_id: source_animation_id.clone(),
                anchor_slug: input.anchor_slug.clone(),
                motion: preset.motion.clone(),
                set: input.set.clone(),
                conversation_id: input.conversation_id.clone(),
            },
            app.clone(),
            &state,
        );
        let (child_job_id, pending_facings) = match direction_result {
            Ok(result) => (result.job_id, result.pending_facings),
            Err(error) => {
                entries[index].phase = "failed".into();
                entries[index].status = "failed".into();
                entries[index].message = Some(error.message.clone());
                update_batch_metadata(
                    app_ref,
                    &state,
                    &job_id,
                    &entries,
                    &facing_entries,
                    &skipped,
                );
                let _ = set_job_state(
                    app_ref,
                    &state,
                    &job_id,
                    JobProgress {
                        status: "failed",
                        progress: 1.0,
                        stage: "Direction set queue failed",
                        error_message: Some(&error.message),
                        result_path: None,
                    },
                );
                return;
            }
        };
        entries[index].child_job_id = Some(child_job_id.clone());
        facing_entries.extend(build_facing_entries(
            &preset.motion,
            &input.set,
            &source_facing,
            &pending_facings,
        ));
        update_batch_metadata(app_ref, &state, &job_id, &entries, &facing_entries, &skipped);

        let motion_label = preset.motion.clone();
        let wait_result = wait_for_child_job(
            app_ref,
            &state,
            &job_id,
            &child_job_id,
            |child| {
                sync_facing_progress(&motion_label, &child.stage, &mut facing_entries);
                update_batch_metadata(
                    app_ref,
                    &state,
                    &job_id,
                    &entries,
                    &facing_entries,
                    &skipped,
                );
            },
        )
        .await;
        if let Err(error) = wait_result {
            entries[index].phase = if error.code == "job_cancelled" {
                "cancelled"
            } else {
                "failed"
            }
            .into();
            entries[index].status = entries[index].phase.clone();
            entries[index].message = Some(error.message.clone());
            mark_facing_entries(
                &preset.motion,
                &mut facing_entries,
                entries[index].status.as_str(),
                entries[index].phase.as_str(),
            );
            update_batch_metadata(
                app_ref,
                &state,
                &job_id,
                &entries,
                &facing_entries,
                &skipped,
            );
            let _ = set_job_state(
                app_ref,
                &state,
                &job_id,
                JobProgress {
                    status: if error.code == "job_cancelled" {
                        "cancelled"
                    } else {
                        "failed"
                    },
                    progress: 1.0,
                    stage: "Direction set failed",
                    error_message: Some(&error.message),
                    result_path: None,
                },
            );
            return;
        }
        mark_facing_entries(&preset.motion, &mut facing_entries, "completed", "Direction set");

        let mut motion_failed = false;
        if harden_after {
            entries[index].phase = "harden".into();
            update_batch_metadata(
                app_ref,
                &state,
                &job_id,
                &entries,
                &facing_entries,
                &skipped,
            );
            let animation_ids = animation_ids_for_motion(
                &state,
                &input.workspace_id,
                &input.worktree_id,
                &preset.motion,
                &input.anchor_slug,
            );
            let conversation_id = input.conversation_id.clone();
            for animation_id in animation_ids {
                let harden_result = tauri::async_runtime::spawn_blocking({
                    let animation_id = animation_id.clone();
                    let anchor_slug = input.anchor_slug.clone();
                    let conversation_id = conversation_id.clone();
                    let state = state.clone();
                    let app = app.clone();
                    move || {
                        harden_animation_inner(
                            app.as_ref(),
                            &state,
                            &animation_id,
                            Some(&anchor_slug),
                            None,
                            None,
                            conversation_id.as_deref(),
                            HardenAnimationOptions {
                                clean_alpha: Some(true),
                                normalize: Some(true),
                                snap_grid: Some(true),
                                grid_size: Some(1),
                                split_layout: None,
                                queue_contract_retry: Some(false),
                                derive_mirrored_facing: None,
                                ..Default::default()
                            },
                        )
                    }
                })
                .await;
                match harden_result {
                    Ok(Err(error)) => {
                        motion_failed = true;
                        entries[index].message = Some(error.message.clone());
                    }
                    Err(error) => {
                        motion_failed = true;
                        entries[index].message = Some(error.to_string());
                    }
                    Ok(Ok(_)) => {}
                }
            }
        }

        if motion_failed {
            entries[index].phase = "failed".into();
            entries[index].status = "failed".into();
        } else {
            entries[index].phase = "completed".into();
            entries[index].status = "completed".into();
        }
        update_batch_metadata(app_ref, &state, &job_id, &entries, &facing_entries, &skipped);
    }

    let passed = entries.iter().all(|entry| entry.status == "completed" || entry.status == "skipped");
    let metadata = serde_json::json!({
        "entries": entries,
        "facingEntries": facing_entries,
        "skippedMotions": skipped,
    });
    let _ = set_job_metadata(app_ref, &state, &job_id, &metadata.to_string());
    let _ = set_job_state(
        app_ref,
        &state,
        &job_id,
        JobProgress {
            status: if passed { "completed" } else { "failed" },
            progress: 1.0,
            stage: if passed {
                "Motion batch finished"
            } else {
                "Motion batch finished with failures"
            },
            error_message: None,
            result_path: None,
        },
    );
    if let Ok(root) = workspace_path(&state, &input.workspace_id) {
        log_pipeline_event(
            &root,
            &input.workspace_id,
            "motion_batch",
            Some(if passed { "completed" } else { "failed" }),
            None,
            Some(&input.worktree_id),
            Some(&job_id),
            Some(passed),
            None,
            Some(metadata),
        );
    }
    if passed {
        super::sessions::append_generation_session_for_workspace(
            &state,
            &input.workspace_id,
            &input.worktree_id,
            "motion_batch",
            None,
            ".sprite-studio/last-generation.json",
            None,
            None,
        );
    }
}

#[tauri::command]
pub fn queue_motion_batch(
    input: QueueMotionBatchInput,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> CommandResult<MotionBatchResult> {
    queue_motion_batch_inner(input, Some(app), &state)
}
