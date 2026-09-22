use crate::{
    animations::save_animation_inner,
    assets::{read_generation_manifest, scan_generation_assets_inner},
    error::{CommandError, CommandResult},
    jobs::{cancellation_requested, set_job_metadata, set_job_state, JobProgress},
    models::{
        AnimationDirectionMeta, AnimationFrame, AnimationInput, CharacterAnchor,
        DirectionSetResult, MirrorAnimationInput, NormalizeAnimationInput, QueueDirectionSetInput,
    },
    providers::start_provider_run,
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use tauri::{AppHandle, State};
use uuid::Uuid;

use super::anchors::get_anchor_inner;
use super::character_contract::character_contract_check_inner;
use super::contract_retry::load_animation_context;
use super::contract_retry_job::{find_horizontal_strip_path, wait_for_generation};
use super::direction_meta::{
    infer_direction_family, infer_facing_from_name, list_direction_meta_for_worktree,
    load_direction_meta, write_direction_meta,
};
use super::logging::log_pipeline_event;
use super::mirror::mirror_facing_pair;
use super::mirror::{mirror_animation_inner, mirror_targets_for_set};
use super::normalize::normalize_animation_inner;
use super::strip::split_sprite_strip_inner;

fn facing_label(facing: &str) -> &'static str {
    match facing {
        "n" => "north / back view",
        "s" => "south / front view",
        "e" => "east / right profile",
        "w" => "west / left profile (canonical)",
        "ne" => "north-east diagonal",
        "nw" => "north-west diagonal",
        "se" => "south-east diagonal",
        "sw" => "south-west diagonal",
        _ => "requested facing",
    }
}

pub(crate) fn build_direction_facing_prompt(
    anchor: &CharacterAnchor,
    motion: &str,
    facing: &str,
    frame_count: u32,
    source_name: &str,
) -> String {
    format!(
        "DIRECTION SET: generate ONE horizontal sprite strip for \"{source_name}\" facing {facing_label}.\n\
Motion: {motion}. Exactly {frame_count} poses in a single row.\n\
Each cell must be {width}x{height}px with transparent background.\n\
Foot baseline Y={baseline}. Torso pivot X≈{pivot:.0}. Preserve anchor silhouette, palette, and costume.\n\
Camera: {facing_label}. No grid borders between cells.",
        source_name = source_name,
        facing_label = facing_label(facing),
        motion = motion,
        frame_count = frame_count,
        width = anchor.frame_width,
        height = anchor.frame_height,
        baseline = anchor.baseline_y,
        pivot = anchor.pivot.x,
    )
}

fn facing_animation_name(source_name: &str, source_facing: &str, target_facing: &str) -> String {
    if source_name.to_ascii_lowercase().ends_with(&format!("-{}", source_facing)) {
        format!(
            "{}-{}",
            source_name
                .trim_end_matches(source_facing)
                .trim_end_matches('-'),
            target_facing
        )
    } else {
        format!("{}-{}", source_name, target_facing)
    }
}

fn animation_id_for_facing(
    state: &AppState,
    workspace_id: &str,
    worktree_id: &str,
    direction_family: &str,
    facing: &str,
) -> Option<String> {
    list_direction_meta_for_worktree(state, workspace_id, worktree_id)
        .ok()?
        .into_iter()
        .find(|meta| meta.direction_family == direction_family && meta.facing == facing)
        .map(|meta| meta.animation_id)
}

pub(crate) fn apply_derived_facing_mirrors(
    app: Option<&AppHandle>,
    state: &AppState,
    input: &QueueDirectionSetInput,
    set: &str,
    direction_family: &str,
    generated: &[&str],
) -> CommandResult<Vec<String>> {
    let mut mirrored_ids = Vec::new();
    for (source_facing, target_facing) in mirror_targets_for_set(set, generated) {
        if mirror_facing_pair(&source_facing) != Some(target_facing.as_str()) {
            continue;
        }
        if animation_id_for_facing(
            state,
            &input.workspace_id,
            &input.worktree_id,
            direction_family,
            &target_facing,
        )
        .is_some()
        {
            continue;
        }
        let Some(source_animation_id) = animation_id_for_facing(
            state,
            &input.workspace_id,
            &input.worktree_id,
            direction_family,
            &source_facing,
        ) else {
            continue;
        };
        let result = mirror_animation_inner(
            app,
            state,
            MirrorAnimationInput {
                animation_id: source_animation_id,
                target_facing: target_facing.clone(),
                source_facing: Some(source_facing.clone()),
                anchor_slug: Some(input.anchor_slug.clone()),
                rig_id: None,
            },
        )?;
        mirrored_ids.push(result.mirrored_animation_id);
    }
    Ok(mirrored_ids)
}

pub(crate) fn import_facing_animation_from_strip(
    app: Option<&AppHandle>,
    state: &AppState,
    workspace_id: &str,
    worktree_id: &str,
    source_name: &str,
    source_facing: &str,
    target_facing: &str,
    direction_family: &str,
    anchor_slug: &str,
    strip_path: &str,
    frame_count: u32,
    fps: f64,
    looping: bool,
) -> CommandResult<String> {
    let split = split_sprite_strip_inner(
        workspace_id,
        strip_path,
        "profile",
        frame_count,
        None,
        true,
        "characters",
        state,
    )?;
    let frames = split
        .asset_ids
        .into_iter()
        .map(|asset_id| AnimationFrame {
            asset_id,
            duration_ms: None,
            offset_x: 0,
            offset_y: 0,
        })
        .collect::<Vec<_>>();
    let animation_id = Uuid::new_v4().to_string();
    save_animation_inner(
        AnimationInput {
            id: Some(animation_id.clone()),
            workspace_id: workspace_id.to_string(),
            worktree_id: Some(worktree_id.to_string()),
            name: facing_animation_name(source_name, source_facing, target_facing),
            fps,
            looping,
            frames,
            motion_plan: None,
            review_status: Some("draft".into()),
        },
        state,
    )?;
    let root = workspace_path(state, workspace_id)?;
    write_direction_meta(
        &root,
        &AnimationDirectionMeta {
            animation_id: animation_id.clone(),
            direction_family: direction_family.to_string(),
            facing: target_facing.to_string(),
            mirrored_from: None,
            anchor_slug: Some(anchor_slug.to_string()),
        },
    )?;
    normalize_animation_inner(
        app,
        state,
        NormalizeAnimationInput {
            animation_id: animation_id.clone(),
            anchor_slug: Some(anchor_slug.to_string()),
            lock_first_frame: Some(true),
            shared_scale: Some(true),
            padding: None,
        },
    )?;
    Ok(animation_id)
}

fn load_source_animation(
    state: &AppState,
    animation_id: &str,
) -> CommandResult<(String, Option<String>, String)> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection
        .query_row(
            "SELECT workspace_id, worktree_id, name FROM animations WHERE id=?1",
            [animation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| CommandError::new("animation_not_found", "Source animation no longer exists"))
}

pub(crate) fn queue_direction_set_inner(
    input: QueueDirectionSetInput,
    app: Option<AppHandle>,
    state: &AppState,
) -> CommandResult<DirectionSetResult> {
    let set = input.set.trim();
    if set != "4" && set != "8" {
        return Err(CommandError::new(
            "invalid_direction_set",
            "set must be \"4\" or \"8\"",
        ));
    }

    let (workspace_id, worktree_id, name) = load_source_animation(state, &input.source_animation_id)?;
    if workspace_id != input.workspace_id {
        return Err(CommandError::new(
            "workspace_mismatch",
            "sourceAnimationId does not belong to the workspace",
        ));
    }
    if worktree_id.as_deref() != Some(input.worktree_id.as_str()) {
        return Err(CommandError::new(
            "worktree_mismatch",
            "sourceAnimationId does not belong to the worktree",
        ));
    }

    let source_meta = load_direction_meta(state, &workspace_id, &input.source_animation_id)?;
    let source_facing = source_meta
        .as_ref()
        .map(|meta| meta.facing.clone())
        .or_else(|| infer_facing_from_name(&name))
        .unwrap_or_else(|| "w".to_string());
    let direction_family = source_meta
        .as_ref()
        .map(|meta| meta.direction_family.clone())
        .unwrap_or_else(|| infer_direction_family(&input.motion));

    let root = workspace_path(state, &workspace_id)?;
    if source_meta.is_none() {
        write_direction_meta(
            &root,
            &AnimationDirectionMeta {
                animation_id: input.source_animation_id.clone(),
                direction_family: direction_family.clone(),
                facing: source_facing.clone(),
                mirrored_from: None,
                anchor_slug: Some(input.anchor_slug.clone()),
            },
        )?;
    }

    let generated: Vec<&str> = if set == "8" {
        vec!["n", "e", "s", "ne", "nw"]
    } else {
        vec!["n", "e"]
    };
    let mirror_pairs = mirror_targets_for_set(set, &generated);
    let derivable = mirror_facing_pair(&source_facing)
        .map(|target| vec![target.to_string()])
        .unwrap_or_else(|| {
            mirror_pairs
                .iter()
                .filter(|(source, _)| source == &source_facing)
                .map(|(_, target)| target.clone())
                .collect()
        });
    let pending = generated
        .iter()
        .filter(|facing| **facing != source_facing.as_str())
        .map(|facing| facing.to_string())
        .filter(|facing| !derivable.contains(facing))
        .collect::<Vec<_>>();

    let active_job_id: Option<String> = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                r#"SELECT id FROM background_jobs
                   WHERE kind='direction_set' AND target_id=?1
                     AND status IN ('queued','running')
                   ORDER BY created_at DESC LIMIT 1"#,
                [&input.source_animation_id],
                |row| row.get(0),
            )
            .optional()?
    };
    if active_job_id.is_some() {
        return Err(CommandError::new(
            "direction_set_active",
            "A direction set job is already running for this animation",
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
            ) VALUES (?1,?2,?3,'direction_set','animation',?4,'queued',0.0,'Queued',?5,?5,?5)"#,
            params![
                job_id,
                workspace_id,
                worktree_id,
                input.source_animation_id,
                now
            ],
        )?;
    }

    let task_app = app.clone();
    let task_state = state.clone();
    let task_input = input.clone();
    let task_job_id = job_id.clone();
    let pending_for_result = pending.clone();
    let derivable_clone = derivable.clone();
    tauri::async_runtime::spawn(async move {
        run_direction_set_async(
            task_app,
            task_state,
            task_job_id,
            task_input,
            derivable_clone,
            pending,
        )
        .await;
    });

    Ok(DirectionSetResult {
        job_id,
        source_animation_id: input.source_animation_id,
        mirrored_animation_ids: Vec::new(),
        pending_facings: pending_for_result,
        character_contract: None,
    })
}

async fn run_direction_set_async(
    app: Option<AppHandle>,
    state: AppState,
    job_id: String,
    input: QueueDirectionSetInput,
    derivable: Vec<String>,
    pending: Vec<String>,
) {
    let app_ref = app.as_ref();
    let (source_name, fps, looping, frame_count) = match load_animation_context(
        &state,
        &input.source_animation_id,
    ) {
        Ok((_, name, fps, looping, frame_count, _)) => (name, fps, looping, frame_count),
        Err(error) => {
            let _ = set_job_state(
                app_ref,
                &state,
                &job_id,
                JobProgress {
                    status: "failed",
                    progress: 1.0,
                    stage: "Source animation missing",
                    error_message: Some(&error.message),
                    result_path: None,
                },
            );
            return;
        }
    };
    let direction_family = infer_direction_family(&input.motion);
    let _ = set_job_state(
        app_ref,
        &state,
        &job_id,
        JobProgress {
            status: "running",
            progress: 0.05,
            stage: "Mirroring facings",
            error_message: None,
            result_path: None,
        },
    );

    let source_meta = load_direction_meta(&state, &input.workspace_id, &input.source_animation_id)
        .ok()
        .flatten();
    let source_facing = source_meta
        .map(|meta| meta.facing)
        .unwrap_or_else(|| "w".to_string());

    let mut mirrored_ids = Vec::new();
    let total = derivable.len().max(1) as f64;
    for (index, target_facing) in derivable.iter().enumerate() {
        if mirror_facing_pair(&source_facing) != Some(target_facing.as_str()) {
            continue;
        }
        let progress = 0.1 + (index as f64 / total) * 0.7;
        let stage = format!("Mirroring {target_facing}");
        let _ = set_job_state(
            app_ref,
            &state,
            &job_id,
            JobProgress {
                status: "running",
                progress,
                stage: &stage,
                error_message: None,
                result_path: None,
            },
        );
        match mirror_animation_inner(
            app_ref,
            &state,
            MirrorAnimationInput {
                animation_id: input.source_animation_id.clone(),
                target_facing: target_facing.clone(),
                source_facing: Some(source_facing.clone()),
                anchor_slug: Some(input.anchor_slug.clone()),
                rig_id: None,
            },
        ) {
            Ok(result) => mirrored_ids.push(result.mirrored_animation_id),
            Err(error) => {
                let _ = set_job_state(
                    app_ref,
                    &state,
                    &job_id,
                    JobProgress {
                        status: "failed",
                        progress: 1.0,
                        stage: "Mirror failed",
                        error_message: Some(&error.message),
                        result_path: None,
                    },
                );
                let _ = set_job_metadata(
                    app_ref,
                    &state,
                    &job_id,
                    &serde_json::json!({ "error": error.message }).to_string(),
                );
                return;
            }
        }
    }

    let mut generated_animation_ids = Vec::new();
    let conversation_id = input
        .conversation_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(conversation_id) = conversation_id {
        if !pending.is_empty() {
            let anchor = match get_anchor_inner(&input.workspace_id, &input.anchor_slug, &state) {
                Ok(value) => value,
                Err(error) => {
                    let _ = set_job_state(
                        app_ref,
                        &state,
                        &job_id,
                        JobProgress {
                            status: "failed",
                            progress: 1.0,
                            stage: "Anchor missing",
                            error_message: Some(&error.message),
                            result_path: None,
                        },
                    );
                    return;
                }
            };
            let root = match workspace_path(&state, &input.workspace_id) {
                Ok(value) => value,
                Err(error) => {
                    let _ = set_job_state(
                        app_ref,
                        &state,
                        &job_id,
                        JobProgress {
                            status: "failed",
                            progress: 1.0,
                            stage: "Workspace missing",
                            error_message: Some(&error.message),
                            result_path: None,
                        },
                    );
                    return;
                }
            };
            let ai_pending = pending.clone();
            let pending_total = ai_pending.len().max(1) as f64;
            for (index, target_facing) in ai_pending.into_iter().enumerate() {
                if matches!(
                    tauri::async_runtime::spawn_blocking({
                        let job_id = job_id.clone();
                        let state = state.clone();
                        move || cancellation_requested(&state, &job_id)
                    })
                    .await,
                    Ok(Ok(true))
                ) {
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
                let progress = 0.75 + (index as f64 / pending_total) * 0.1;
                let stage = format!("Generating {target_facing}");
                let _ = set_job_state(
                    app_ref,
                    &state,
                    &job_id,
                    JobProgress {
                        status: "running",
                        progress,
                        stage: &stage,
                        error_message: None,
                        result_path: None,
                    },
                );
                let prompt = build_direction_facing_prompt(
                    &anchor,
                    &input.motion,
                    &target_facing,
                    frame_count,
                    &source_name,
                );
                let manifest_before = read_generation_manifest(&root)
                    .ok()
                    .flatten()
                    .map(|value| value.generated_at);
                let request_id = match tauri::async_runtime::spawn_blocking({
                    let conversation_id = conversation_id.to_string();
                    let prompt = prompt.clone();
                    let source_name = source_name.clone();
                    let facing_label = target_facing.clone();
                    let state = state.clone();
                    let app = app.clone();
                    move || {
                        start_provider_run(
                            conversation_id,
                            prompt,
                            Some(format!(
                                "Direction set for \"{source_name}\" facing {facing_label}."
                            )),
                            None,
                            app,
                            &state,
                        )
                    }
                })
                .await
                {
                    Ok(Ok(request_id)) => request_id,
                    Ok(Err(error)) => {
                        let _ = set_job_state(
                            app_ref,
                            &state,
                            &job_id,
                            JobProgress {
                                status: "failed",
                                progress: 1.0,
                                stage: "Generation failed",
                                error_message: Some(&error.message),
                                result_path: None,
                            },
                        );
                        return;
                    }
                    Err(error) => {
                        let message = error.to_string();
                        let _ = set_job_state(
                            app_ref,
                            &state,
                            &job_id,
                            JobProgress {
                                status: "failed",
                                progress: 1.0,
                                stage: "Generation failed",
                                error_message: Some(&message),
                                result_path: None,
                            },
                        );
                        return;
                    }
                };
                if let Err(error) = wait_for_generation(&state, &request_id, &job_id).await {
                    if error.code != "job_cancelled" {
                        let _ = set_job_state(
                            app_ref,
                            &state,
                            &job_id,
                            JobProgress {
                                status: "failed",
                                progress: 1.0,
                                stage: "Generation wait failed",
                                error_message: Some(&error.message),
                                result_path: None,
                            },
                        );
                    }
                    return;
                }
                let import_result = tauri::async_runtime::spawn_blocking({
                    let workspace_id = input.workspace_id.clone();
                    let worktree_id = input.worktree_id.clone();
                    let source_name = source_name.clone();
                    let source_facing = source_facing.clone();
                    let target_facing = target_facing.clone();
                    let direction_family = direction_family.clone();
                    let anchor_slug = input.anchor_slug.clone();
                    let state = state.clone();
                    let app = app.clone();
                    let manifest_before = manifest_before.clone();
                    move || {
                        let _ = scan_generation_assets_inner(&workspace_id, None, app.as_ref(), &state)?;
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
                            &source_facing,
                            &target_facing,
                            &direction_family,
                            &anchor_slug,
                            strip_path.to_string_lossy().as_ref(),
                            frame_count,
                            fps,
                            looping,
                        )
                    }
                })
                .await;
                match import_result {
                    Ok(Ok(animation_id)) => generated_animation_ids.push(animation_id),
                    Ok(Err(error)) => {
                        let _ = set_job_state(
                            app_ref,
                            &state,
                            &job_id,
                            JobProgress {
                                status: "failed",
                                progress: 1.0,
                                stage: "Import failed",
                                error_message: Some(&error.message),
                                result_path: None,
                            },
                        );
                        return;
                    }
                    Err(error) => {
                        let message = error.to_string();
                        let _ = set_job_state(
                            app_ref,
                            &state,
                            &job_id,
                            JobProgress {
                                status: "failed",
                                progress: 1.0,
                                stage: "Import failed",
                                error_message: Some(&message),
                                result_path: None,
                            },
                        );
                        return;
                    }
                }
            }
        }
    }

    let generated_facings: Vec<&str> = if input.set == "8" {
        vec!["n", "e", "s", "ne", "nw"]
    } else {
        vec!["n", "e"]
    };
    let _ = set_job_state(
        app_ref,
        &state,
        &job_id,
        JobProgress {
            status: "running",
            progress: 0.88,
            stage: "Mirroring derived facings",
            error_message: None,
            result_path: None,
        },
    );
    if let Ok(derived_ids) = apply_derived_facing_mirrors(
        app_ref,
        &state,
        &input,
        &input.set,
        &direction_family,
        &generated_facings,
    ) {
        mirrored_ids.extend(derived_ids);
    }

    let _ = set_job_state(
        app_ref,
        &state,
        &job_id,
        JobProgress {
            status: "running",
            progress: 0.9,
            stage: "Character contract",
            error_message: None,
            result_path: None,
        },
    );
    let contract = character_contract_check_inner(
        &state,
        &input.workspace_id,
        &input.worktree_id,
        Some(&input.anchor_slug),
    );

    let metadata = serde_json::json!({
        "mirroredAnimationIds": mirrored_ids,
        "generatedAnimationIds": generated_animation_ids,
        "pendingFacings": if conversation_id.is_some() {
            Vec::<String>::new()
        } else {
            pending.clone()
        },
        "characterContract": contract.as_ref().ok(),
        "characterContractError": contract.as_ref().err().map(|error| error.message.clone()),
    });
    let _ = set_job_metadata(app_ref, &state, &job_id, &metadata.to_string());
    let passed = contract.as_ref().map(|report| report.passed).unwrap_or(false);
    let _ = set_job_state(
        app_ref,
        &state,
        &job_id,
        JobProgress {
            status: if passed { "completed" } else { "failed" },
            progress: 1.0,
            stage: if passed {
                "Direction set finished"
            } else {
                "Character contract failed"
            },
            error_message: if passed {
                None
            } else {
                Some("Character contract check failed")
            },
            result_path: None,
        },
    );
    if let Ok(root) = workspace_path(&state, &input.workspace_id) {
        log_pipeline_event(
            &root,
            &input.workspace_id,
            "direction_set",
            Some(if passed { "completed" } else { "failed" }),
            None,
            Some(&input.source_animation_id),
            Some(&job_id),
            Some(passed),
            None,
            Some(metadata),
        );
    }
}

#[tauri::command]
pub fn queue_direction_set(
    input: QueueDirectionSetInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<DirectionSetResult> {
    queue_direction_set_inner(input, Some(app), &state)
}
