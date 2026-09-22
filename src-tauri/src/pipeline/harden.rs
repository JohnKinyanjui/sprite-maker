use crate::{
    animations::save_animation_inner,
    assets::write_generation_manifest,
    error::{CommandError, CommandResult},
    models::{
        AnimationFrame, AnimationInput, GenerationManifest, HardenAnimationOptions,
        HardenAnimationReport, NormalizeAnimationInput, QueueContractRetryInput,
    },
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use rusqlite::OptionalExtension;
use tauri::{AppHandle, State};

use super::clean_alpha::clean_alpha_animation_inner;
use super::contract::size_contract_check_inner;
use super::contract_retry_job::queue_contract_retry_inner;
use super::normalize::normalize_animation_inner;
use super::snap_grid::snap_animation_offsets_inner;
use super::logging::{log_pipeline_event, PipelineStageTimer, PipelineTimer};
use super::mirror::{mirror_animation_inner, mirror_facing_pair};
use super::strip::split_sprite_strip_inner;
use crate::models::{AnimationDirectionMeta, MirrorAnimationInput};
use super::direction_meta::{
    infer_direction_family, infer_facing_from_name, load_direction_meta, write_direction_meta,
};

fn load_animation_context(
    state: &AppState,
    animation_id: &str,
) -> CommandResult<(String, String, f64, bool, Option<String>)> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection
        .query_row(
            "SELECT workspace_id, name, fps, looping, worktree_id FROM animations WHERE id=?1",
            [animation_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| CommandError::new("animation_not_found", "Animation no longer exists"))
}

fn persist_animation_manifest(
    state: &AppState,
    workspace_id: &str,
    animation_id: &str,
    name: &str,
    fps: f64,
) -> CommandResult<()> {
    let root = workspace_path(state, workspace_id)?;
    let relative_paths = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let frames_json: String = connection.query_row(
            "SELECT frames_json FROM animations WHERE id=?1",
            [animation_id],
            |row| row.get(0),
        )?;
        let frames = crate::animations::resolve_animation_frames(&connection, animation_id, &frames_json)?;
        let mut paths = Vec::new();
        for frame in frames {
            let relative_path: String = connection.query_row(
                "SELECT relative_path FROM assets WHERE id=?1",
                [&frame.asset_id],
                |row| row.get(0),
            )?;
            paths.push(relative_path);
        }
        paths
    };
    if relative_paths.is_empty() {
        return Ok(());
    }
    let category = relative_paths[0]
        .split('/')
        .nth(1)
        .unwrap_or("characters")
        .to_string();
    let source = relative_paths.first().cloned();
    let direction_meta = load_direction_meta(state, workspace_id, animation_id)?.or_else(|| {
        infer_facing_from_name(name).map(|facing| AnimationDirectionMeta {
            animation_id: animation_id.to_string(),
            direction_family: infer_direction_family(name),
            facing,
            mirrored_from: None,
            anchor_slug: None,
        })
    });
    let worktree_id: Option<String> = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                "SELECT worktree_id FROM animations WHERE id=?1",
                [animation_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten()
    };
    let manifest = GenerationManifest {
        kind: Some(if relative_paths.len() == 1 {
            "sprite".into()
        } else {
            "animation".into()
        }),
        name: name.to_string(),
        category,
        fps,
        files: relative_paths,
        generated_at: Utc::now().to_rfc3339(),
        rig: None,
        rig_id: None,
        source,
        quality: None,
        direction_family: direction_meta.as_ref().map(|meta| meta.direction_family.clone()),
        facing: direction_meta.as_ref().map(|meta| meta.facing.clone()),
        mirrored_from: direction_meta.as_ref().and_then(|meta| meta.mirrored_from.clone()),
        anchor_slug: direction_meta.as_ref().and_then(|meta| meta.anchor_slug.clone()),
    };
    let session_hook = worktree_id.map(|worktree_id| {
        super::sessions::ManifestSessionHook {
            worktree_id,
            kind: "harden".into(),
            animation_id: Some(animation_id.to_string()),
            score: None,
        }
    });
    write_generation_manifest(&root, &manifest, session_hook.as_ref())?;
    Ok(())
}

struct HardenRunContext {
    workspace_id: String,
    animation_name: String,
    fps: f64,
}

fn log_harden_pipeline_event(
    state: &AppState,
    workspace_id: &str,
    animation_id: &str,
    timer: &PipelineTimer,
    stage_timer: &PipelineStageTimer,
    steps: &[String],
    passed: Option<bool>,
    error_code: Option<&str>,
    job_id: Option<&str>,
) {
    if let Ok(root) = workspace_path(state, workspace_id) {
        let stage = if error_code.is_some() {
            "failed"
        } else {
            "completed"
        };
        log_pipeline_event(
            &root,
            workspace_id,
            "harden_animation",
            Some(stage),
            Some(std::time::Duration::from_millis(timer.elapsed_ms())),
            Some(animation_id),
            job_id,
            passed,
            error_code,
            Some(serde_json::json!({
                "steps": steps,
                "stageDurationsMs": stage_timer.stage_durations_ms(),
            })),
        );
    }
}

pub(crate) fn harden_animation_inner(
    app: Option<&AppHandle>,
    state: &AppState,
    animation_id: &str,
    anchor_slug: Option<&str>,
    source_path: Option<&str>,
    frame_count: Option<u32>,
    conversation_id: Option<&str>,
    options: HardenAnimationOptions,
) -> CommandResult<HardenAnimationReport> {
    let timer = PipelineTimer::start();
    let mut stage_timer = PipelineStageTimer::start();
    let clean_alpha = options.clean_alpha.unwrap_or(true);
    let normalize = options.normalize.unwrap_or(true);
    let snap_grid = options.snap_grid.unwrap_or(false);
    let grid_size = options.grid_size.unwrap_or(1);
    let queue_retry = options.queue_contract_retry.unwrap_or(false);
    let derive_mirrored = options.derive_mirrored_facing.clone();

    let mut steps = Vec::new();
    let mut run_context: Option<HardenRunContext> = None;
    let mut contract_passed: Option<bool> = None;
    let mut job_id: Option<String> = None;

    let result: CommandResult<HardenAnimationReport> = (|| {
        let (workspace_id, animation_name, fps, looping, worktree_id) =
            load_animation_context(state, animation_id)?;
        run_context = Some(HardenRunContext {
            workspace_id: workspace_id.clone(),
            animation_name: animation_name.clone(),
            fps,
        });
        stage_timer.mark("load_context");

        if let Some(source_path) = source_path {
        let count = frame_count.ok_or_else(|| {
            CommandError::new(
                "frame_count_required",
                "Provide frameCount when sourcePath is set for harden_animation",
            )
        })?;
        let layout = options.split_layout.as_deref().unwrap_or("profile");
        let split = split_sprite_strip_inner(
            &workspace_id,
            source_path,
            layout,
            count,
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
            .collect();
        save_animation_inner(
            AnimationInput {
                id: Some(animation_id.to_string()),
                workspace_id: workspace_id.clone(),
                worktree_id: worktree_id.clone(),
                name: animation_name.clone(),
                fps,
                looping,
                frames,
                motion_plan: None,
                review_status: Some("draft".into()),
            },
            state,
        )?;
            steps.push(format!("split_strip:{layout}"));
            stage_timer.mark("split_strip");
        }

        if clean_alpha {
            clean_alpha_animation_inner(state, animation_id)?;
            steps.push("clean_alpha".into());
            stage_timer.mark("clean_alpha");
        }

        if normalize {
            normalize_animation_inner(
                app,
                state,
                NormalizeAnimationInput {
                    animation_id: animation_id.to_string(),
                    anchor_slug: anchor_slug.map(str::to_string),
                    lock_first_frame: Some(true),
                    shared_scale: Some(true),
                    padding: None,
                },
            )?;
            steps.push("normalize_animation".into());
            stage_timer.mark("normalize_animation");
        }

        if snap_grid {
            snap_animation_offsets_inner(state, animation_id, grid_size)?;
            steps.push(format!("snap_to_pixel_grid:{grid_size}"));
            stage_timer.mark("snap_to_pixel_grid");
        }

        if options.quantize_palette.unwrap_or(false) {
            let palette_count = options.palette_color_count.unwrap_or(32);
            super::shared_palette::quantize_animation_palette_inner(
                state,
                animation_id,
                palette_count,
            )?;
            steps.push(format!("quantize_shared_palette:{palette_count}"));
            stage_timer.mark("quantize_shared_palette");
        }

        let ctx = run_context.as_ref().expect("harden context");
        persist_animation_manifest(
            state,
            &ctx.workspace_id,
            animation_id,
            &ctx.animation_name,
            ctx.fps,
        )?;
        steps.push("write_generation_manifest".into());
        stage_timer.mark("write_generation_manifest");

        let contract_report = size_contract_check_inner(state, animation_id, anchor_slug)?;
        steps.push("check_size_contract".into());
        stage_timer.mark("check_size_contract");
        contract_passed = Some(contract_report.passed);

        if let Some(target_facing) = derive_mirrored.as_deref() {
            if contract_report.passed {
                let source_facing = "w";
                if mirror_facing_pair(source_facing) == Some(target_facing) {
                    let _mirror = mirror_animation_inner(
                        app,
                        state,
                        MirrorAnimationInput {
                            animation_id: animation_id.to_string(),
                            target_facing: target_facing.to_string(),
                            source_facing: Some(source_facing.to_string()),
                            anchor_slug: anchor_slug.map(str::to_string),
                            rig_id: None,
                        },
                    )?;
                    steps.push(format!("mirror_animation:{target_facing}"));
                    stage_timer.mark("mirror_animation");
                    let root = workspace_path(state, &ctx.workspace_id)?;
                    write_direction_meta(
                        &root,
                        &AnimationDirectionMeta {
                            animation_id: animation_id.to_string(),
                            direction_family: infer_direction_family(&ctx.animation_name),
                            facing: source_facing.to_string(),
                            mirrored_from: None,
                            anchor_slug: anchor_slug.map(str::to_string),
                        },
                    )?;
                    persist_animation_manifest(
                        state,
                        &ctx.workspace_id,
                        animation_id,
                        &ctx.animation_name,
                        ctx.fps,
                    )?;
                    steps.push("write_generation_manifest:after_mirror".into());
                }
            }
        }

        if queue_retry && !contract_report.passed {
            let conversation_id = conversation_id.ok_or_else(|| {
                CommandError::new(
                    "conversation_required",
                    "Provide conversationId when queueContractRetry is true",
                )
            })?;
            let retry = queue_contract_retry_inner(
                QueueContractRetryInput {
                    animation_id: animation_id.to_string(),
                    conversation_id: conversation_id.to_string(),
                    anchor_slug: anchor_slug.map(str::to_string),
                    max_ai_attempts: None,
                    max_deterministic_passes: None,
                },
                app.cloned(),
                state,
            )?;
            job_id = retry.job_id;
            steps.push("queue_contract_retry".into());
            stage_timer.mark("queue_contract_retry");
        }

        Ok(HardenAnimationReport {
            animation_id: animation_id.to_string(),
            steps: steps.clone(),
            contract_report,
            job_id: job_id.clone(),
        })
    })();

    if let Some(ctx) = &run_context {
        log_harden_pipeline_event(
            state,
            &ctx.workspace_id,
            animation_id,
            &timer,
            &stage_timer,
            &steps,
            contract_passed,
            result.as_ref().err().map(|error| error.code.as_str()),
            job_id.as_deref(),
        );
    }

    result
}

#[tauri::command]
pub fn harden_animation(
    animation_id: String,
    anchor_slug: Option<String>,
    source_path: Option<String>,
    frame_count: Option<u32>,
    conversation_id: Option<String>,
    options: Option<HardenAnimationOptions>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<HardenAnimationReport> {
    harden_animation_inner(
        Some(&app),
        &state,
        &animation_id,
        anchor_slug.as_deref(),
        source_path.as_deref(),
        frame_count,
        conversation_id.as_deref(),
        options.unwrap_or_default(),
    )
}
