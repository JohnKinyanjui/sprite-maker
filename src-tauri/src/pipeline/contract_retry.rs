use crate::{
    animations::save_animation_inner,
    error::{CommandError, CommandResult},
    models::{
        Animation, AnimationFrame, AnimationInput, ContractRetryAttempt, ContractRetryResult,
        FrameNudgeDelta, NormalizeAnimationInput, NudgeAnimationFramesInput, SizeContractReport,
        SizeContractViolation,
    },
    pipeline::anchors::get_anchor_inner,
    providers::start_provider_run,
    quality::compute_metrics,
    AppState,
};
use rusqlite::OptionalExtension;
use tauri::AppHandle;

use super::{
    contract::size_contract_check_inner,
    normalize::normalize_animation_inner,
    strip::split_sprite_strip_inner,
};
use super::align::nudge_animation_frames_inner;

const DEFAULT_MAX_DETERMINISTIC_PASSES: u32 = 2;

pub(crate) fn build_corrective_prompt(
    report: &SizeContractReport,
    anchor: &crate::models::CharacterAnchor,
    animation_name: &str,
    frame_count: u32,
    attempt: u32,
) -> String {
    let mut lines = vec![
        format!(
            "SIZE CONTRACT RETRY (attempt {attempt}): regenerate ONE horizontal sprite strip for \"{animation_name}\"."
        ),
        format!(
            "Exactly {frame_count} poses in a single row. Each cell must be {width}x{height}px with transparent background.",
            width = anchor.frame_width,
            height = anchor.frame_height
        ),
        format!(
            "Foot baseline Y={baseline}. Torso pivot X≈{pivot:.0}. No visible grid borders between cells.",
            baseline = anchor.baseline_y,
            pivot = anchor.pivot.x
        ),
        "After generation: the studio will split with profile layout, normalize, and re-check the contract.".into(),
    ];
    for violation in &report.violations {
        match violation.code.as_str() {
            "canvas_size" => lines.push(
                "FIX canvas_size: every cell identical canvas; do not vary export dimensions per pose.".into(),
            ),
            "baseline_drift" => lines.push(format!(
                "FIX baseline_drift: plant every foot on baseline Y={} (±1px). No floating or sinking contacts.",
                anchor.baseline_y
            )),
            "centroid_drift" => lines.push(
                "FIX centroid_drift: keep horizontal body mass centered — torso centroid X must stay within ±4px.".into(),
            ),
            "motion_still" => {
                let frame = violation
                    .frame_index
                    .map(|value| value + 1)
                    .unwrap_or(2);
                lines.push(format!(
                    "FIX motion_still: frame {frame} must differ clearly from the previous pose — increase limb/torso motion."
                ));
            }
            "identity_drift" | "identity_warning" => lines.push(
                "FIX identity: preserve the promoted anchor silhouette, palette, costume, and proportions.".into(),
            ),
            _ => {}
        }
    }
    lines.join("\n")
}

pub(crate) fn has_deterministic_fixable(violations: &[SizeContractViolation]) -> bool {
    violations.iter().any(|violation| {
        violation.blocking
            && matches!(
                violation.code.as_str(),
                "canvas_size" | "baseline_drift" | "centroid_drift"
            )
    })
}

fn compute_baseline_nudges(
    state: &AppState,
    frames: &[AnimationFrame],
    baseline_y: u32,
) -> CommandResult<Vec<FrameNudgeDelta>> {
    let mut deltas = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        let asset = crate::assets::get_asset(state, &frame.asset_id)?;
        let metrics = compute_metrics(&asset.id, &asset.path)?;
        if let Some((_min_x, _min_y, _max_x, max_y)) = metrics.metrics.bounds {
            let foot_delta = baseline_y as i64 - max_y as i64;
            if foot_delta.abs() > 1 {
                deltas.push(FrameNudgeDelta {
                    frame_index: index as u32,
                    offset_x: 0,
                    offset_y: foot_delta as i32,
                });
            }
        }
    }
    Ok(deltas)
}

pub(crate) fn apply_deterministic_contract_repairs(
    state: &AppState,
    animation_id: &str,
    anchor_slug: Option<&str>,
    app: Option<&AppHandle>,
) -> CommandResult<(Animation, Vec<String>)> {
    let mut actions = Vec::new();
    let mut animation = normalize_animation_inner(
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
    actions.push("normalize_animation".into());

    let anchor = if let Some(slug) = anchor_slug {
        Some(get_anchor_inner(
            &animation.workspace_id,
            slug,
            state,
        )?)
    } else {
        None
    };
    if let Some(anchor) = &anchor {
        let deltas = compute_baseline_nudges(state, &animation.frames, anchor.baseline_y)?;
        if !deltas.is_empty() {
            animation = nudge_animation_frames_inner(
                state,
                NudgeAnimationFramesInput {
                    animation_id: animation_id.to_string(),
                    deltas,
                    apply_to_all: None,
                },
            )?;
            actions.push("baseline_nudge".into());
        }
    }
    Ok((animation, actions))
}

pub(crate) fn load_animation_context(
    state: &AppState,
    animation_id: &str,
) -> CommandResult<(String, String, f64, bool, u32, Option<String>)> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let (workspace_id, name, fps, looping, frames_json): (String, String, f64, bool, String) =
        connection
            .query_row(
                "SELECT workspace_id, name, fps, looping, frames_json FROM animations WHERE id=?1",
                [animation_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "The animation no longer exists"))?;
    let frames = crate::animations::resolve_animation_frames(
        &connection,
        animation_id,
        &frames_json,
    )?;
    let worktree_id: Option<String> = connection
        .query_row(
            "SELECT worktree_id FROM animations WHERE id=?1",
            [animation_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok((
        workspace_id,
        name,
        fps,
        looping,
        frames.len() as u32,
        worktree_id,
    ))
}

pub(crate) fn retry_size_contract_inner(
    state: &AppState,
    animation_id: &str,
    anchor_slug: Option<&str>,
    conversation_id: Option<&str>,
    regenerate: bool,
    max_deterministic_passes: Option<u32>,
    app: Option<&AppHandle>,
) -> CommandResult<ContractRetryResult> {
    let max_passes = max_deterministic_passes
        .unwrap_or(DEFAULT_MAX_DETERMINISTIC_PASSES)
        .clamp(1, 5);
    let (workspace_id, animation_name, _fps, _looping, frame_count, _worktree_id) =
        load_animation_context(state, animation_id)?;
    if frame_count == 0 {
        return Err(CommandError::new(
            "empty_animation",
            "Add frames before retrying the size contract",
        ));
    }

    let mut attempts = Vec::new();
    let mut report = size_contract_check_inner(state, animation_id, anchor_slug)?;

    for pass in 1..=max_passes {
        if report.passed || !has_deterministic_fixable(&report.violations) {
            break;
        }
        let (animation, actions) = apply_deterministic_contract_repairs(
            state,
            animation_id,
            anchor_slug,
            app,
        )?;
        let _ = animation;
        report = size_contract_check_inner(state, animation_id, anchor_slug)?;
        attempts.push(ContractRetryAttempt {
            attempt: pass,
            phase: "deterministic".into(),
            report: report.clone(),
            actions,
        });
    }

    let mut corrective_prompt = None;
    let mut generation_request_id = None;
    let mut next_step = None;

    if !report.passed {
        let slug = report
            .anchor_slug
            .clone()
            .or_else(|| anchor_slug.map(str::to_string));
        let anchor = if let Some(slug) = slug {
            get_anchor_inner(&workspace_id, &slug, state)?
        } else {
            return Ok(ContractRetryResult {
                animation_id: animation_id.to_string(),
                passed: false,
                attempts,
                corrective_prompt: None,
                generation_request_id: None,
                job_id: None,
                next_step: Some(
                    "Promote a character anchor before AI regeneration can run.".into(),
                ),
                final_report: report,
            });
        };

        let ai_attempt = attempts.len() as u32 + 1;
        corrective_prompt = Some(build_corrective_prompt(
            &report,
            &anchor,
            &animation_name,
            frame_count,
            ai_attempt,
        ));

        if regenerate {
            let conversation_id = conversation_id.ok_or_else(|| {
                CommandError::new(
                    "conversation_required",
                    "Provide a conversationId to regenerate the strip with AI",
                )
            })?;
            let request_id = start_provider_run(
                conversation_id.to_string(),
                corrective_prompt.clone().unwrap_or_default(),
                Some(format!(
                    "Contract retry for animation \"{animation_name}\" ({frame_count} frames)."
                )),
                None,
                app.cloned(),
                state,
            )?;
            generation_request_id = Some(request_id);
            next_step = Some(
                "Poll get_generation until complete, then call finalize_contract_retry with the new strip path.".into(),
            );
            attempts.push(ContractRetryAttempt {
                attempt: ai_attempt,
                phase: "regenerate".into(),
                report: report.clone(),
                actions: vec!["start_provider_run".into()],
            });
        } else {
            next_step = Some(
                "Call retry_size_contract with regenerate:true and a conversationId, or paste the corrective prompt into chat.".into(),
            );
        }
    }

    Ok(ContractRetryResult {
        animation_id: animation_id.to_string(),
        passed: report.passed,
        attempts,
        corrective_prompt,
        generation_request_id,
        job_id: None,
        next_step,
        final_report: report,
    })
}

pub(crate) fn finalize_contract_retry_inner(
    state: &AppState,
    animation_id: &str,
    source_path: &str,
    frame_count: u32,
    anchor_slug: Option<&str>,
    layout: Option<&str>,
    app: Option<&AppHandle>,
) -> CommandResult<ContractRetryResult> {
    let (workspace_id, animation_name, fps, looping, _, worktree_id) =
        load_animation_context(state, animation_id)?;
    let split_layout = layout
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("profile");

    let split = split_sprite_strip_inner(
        &workspace_id,
        source_path,
        split_layout,
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

    let _animation = save_animation_inner(
        AnimationInput {
            id: Some(animation_id.to_string()),
            workspace_id,
            worktree_id,
            name: animation_name,
            fps,
            looping,
            frames,
            motion_plan: None,
            review_status: Some("draft".into()),
        },
        state,
    )?;

    let mut attempts = vec![ContractRetryAttempt {
        attempt: 1,
        phase: "import_strip".into(),
        report: size_contract_check_inner(state, animation_id, anchor_slug)?,
        actions: vec![
            "split_strip".into(),
            "save_animation".into(),
        ],
    }];

    let (animation, repair_actions) = apply_deterministic_contract_repairs(
        state,
        animation_id,
        anchor_slug,
        app,
    )?;
    let _ = animation;
    let mut actions = attempts[0].actions.clone();
    actions.extend(repair_actions);
    let report = size_contract_check_inner(state, animation_id, anchor_slug)?;
    attempts[0].report = report.clone();
    attempts[0].actions = actions;

    Ok(ContractRetryResult {
        animation_id: animation_id.to_string(),
        passed: report.passed,
        attempts,
        corrective_prompt: None,
        generation_request_id: None,
        job_id: None,
        next_step: if report.passed {
            Some("Contract passed — you can accept the animation for export.".into())
        } else {
            Some("Contract still failing — run retry_size_contract again or adjust manually.".into())
        },
        final_report: report,
    })
}

#[tauri::command]
pub fn retry_size_contract(
    input: crate::models::RetrySizeContractInput,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> CommandResult<ContractRetryResult> {
    retry_size_contract_inner(
        &state,
        &input.animation_id,
        input.anchor_slug.as_deref(),
        input.conversation_id.as_deref(),
        input.regenerate.unwrap_or(false),
        input.max_deterministic_passes,
        Some(&app),
    )
}

#[tauri::command]
pub fn finalize_contract_retry(
    input: crate::models::FinalizeContractRetryInput,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> CommandResult<ContractRetryResult> {
    finalize_contract_retry_inner(
        &state,
        &input.animation_id,
        &input.source_path,
        input.frame_count,
        input.anchor_slug.as_deref(),
        input.layout.as_deref(),
        Some(&app),
    )
}
