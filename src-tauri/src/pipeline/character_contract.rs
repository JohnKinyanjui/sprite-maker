use crate::{
    assets::get_asset,
    error::{CommandError, CommandResult},
    models::{CharacterContractReport, SizeContractViolation},
    quality::compute_metrics,
    workspace::workspace_path,
    AppState,
};
use rusqlite::OptionalExtension;
use std::collections::BTreeMap;
use tauri::State;

use super::anchors::{get_anchor_inner, list_anchors_inner};
use super::contract::size_contract_check_inner;
use super::direction_meta::list_direction_meta_for_worktree;
use super::qc::perceptual_hash_distance;

fn worktree_kind(connection: &rusqlite::Connection, worktree_id: &str) -> CommandResult<String> {
    connection
        .query_row(
            "SELECT kind FROM worktrees WHERE id=?1",
            [worktree_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| CommandError::new("worktree_not_found", "Worktree no longer exists"))
}

fn cross_facing_violations(
    state: &AppState,
    workspace_id: &str,
    worktree_id: &str,
    anchor_slug: &str,
) -> CommandResult<Vec<SizeContractViolation>> {
    let anchor = get_anchor_inner(workspace_id, anchor_slug, state)?;
    let anchor_asset = get_asset(state, &anchor.asset_id)?;
    let anchor_metrics = compute_metrics(&anchor.asset_id, &anchor_asset.path)?;
    let anchor_hash = anchor_metrics.metrics.perceptual_hash;
    let anchor_baseline = anchor.baseline_y;

    let metas = list_direction_meta_for_worktree(state, workspace_id, worktree_id)?;
    let mut families: BTreeMap<String, Vec<_>> = BTreeMap::new();
    for meta in metas {
        families
            .entry(meta.direction_family.clone())
            .or_default()
            .push(meta);
    }

    let mut violations = Vec::new();
    for (family, entries) in families {
        if entries.len() < 2 {
            continue;
        }
        let mut baselines = Vec::new();
        for entry in &entries {
            let frames = {
                let connection = state
                    .db
                    .lock()
                    .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
                let frames_json: String = connection.query_row(
                    "SELECT frames_json FROM animations WHERE id=?1",
                    [&entry.animation_id],
                    |row| row.get(0),
                )?;
                crate::animations::resolve_animation_frames(
                    &connection,
                    &entry.animation_id,
                    &frames_json,
                )?
            };
            if frames.is_empty() {
                continue;
            }
            let asset = get_asset(state, &frames[0].asset_id)?;
            let metrics = compute_metrics(&asset.id, &asset.path)?;
            let mirrored = entry.mirrored_from.is_some();
            let drift_threshold = if mirrored { 28 } else { 22 };
            let hash_distance =
                perceptual_hash_distance(metrics.metrics.perceptual_hash, anchor_hash);
            if hash_distance > drift_threshold {
                violations.push(SizeContractViolation {
                    code: "facing_identity_drift".to_string(),
                    message: format!(
                        "{family} facing {facing} frame 1 diverges from anchor (dHash distance {hash_distance})",
                        family = family,
                        facing = entry.facing,
                        hash_distance = hash_distance
                    ),
                    blocking: true,
                    frame_index: Some(0),
                });
            }
            if let Some((_min_x, _min_y, _max_x, max_y)) = metrics.metrics.bounds {
                baselines.push((entry.facing.clone(), max_y));
            }
        }
        if baselines.len() >= 2 {
            let reference = anchor_baseline;
            for (facing, baseline) in baselines {
                let delta = (baseline as i64 - reference as i64).unsigned_abs();
                if delta > 2 {
                    violations.push(SizeContractViolation {
                        code: "facing_baseline_mismatch".to_string(),
                        message: format!(
                            "{family} facing {facing} baseline drifts {delta}px from anchor across facings",
                            family = family,
                            facing = facing,
                            delta = delta
                        ),
                        blocking: true,
                        frame_index: None,
                    });
                }
            }
        }
    }
    Ok(violations)
}

pub(crate) fn character_contract_check_inner(
    state: &AppState,
    workspace_id: &str,
    worktree_id: &str,
    anchor_slug: Option<&str>,
) -> CommandResult<CharacterContractReport> {
    let root = workspace_path(state, workspace_id)?;
    let _ = root;

    let kind = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        worktree_kind(&connection, worktree_id)?
    };
    if !matches!(kind.as_str(), "character" | "creature" | "animation") {
        return Err(CommandError::new(
            "invalid_worktree_kind",
            "Character contract checks apply to character, creature, or animation worktrees",
        ));
    }

    let animation_ids = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let mut statement = connection.prepare(
            "SELECT id FROM animations WHERE workspace_id=?1 AND worktree_id=?2 ORDER BY name",
        )?;
        let rows = statement
            .query_map(rusqlite::params![workspace_id, worktree_id], |row| row.get::<_, String>(0))?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        rows
    };

    let resolved_anchor = if let Some(slug) = anchor_slug.filter(|value| !value.trim().is_empty()) {
        slug.to_string()
    } else {
        let anchors = list_anchors_inner(workspace_id, state)?;
        anchors
            .first()
            .map(|anchor| anchor.slug.clone())
            .ok_or_else(|| {
                CommandError::new(
                    "missing_anchor",
                    "Promote a character anchor before running character contract checks",
                )
            })?
    };

    let mut animation_reports = Vec::new();
    for animation_id in animation_ids {
        animation_reports.push(
            size_contract_check_inner(state, &animation_id, Some(&resolved_anchor))?,
        );
    }

    let cross_facing_violations =
        cross_facing_violations(state, workspace_id, worktree_id, &resolved_anchor)?;
    let passed = animation_reports.iter().all(|report| report.passed)
        && cross_facing_violations.is_empty();
    Ok(CharacterContractReport {
        workspace_id: workspace_id.to_string(),
        worktree_id: worktree_id.to_string(),
        anchor_slug: resolved_anchor,
        passed,
        animation_count: animation_reports.len() as u32,
        animation_reports,
        cross_facing_violations,
    })
}

pub(crate) fn ensure_worktree_export_allowed(
    state: &AppState,
    workspace_id: &str,
    worktree_id: &str,
    anchor_slug: Option<&str>,
) -> CommandResult<()> {
    let report = character_contract_check_inner(state, workspace_id, worktree_id, anchor_slug)?;
    if !report.passed {
        return Err(CommandError::new(
            "export_blocked",
            "Export is blocked until every facing in the direction family passes cross-facing contract checks",
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn check_character_contract(
    workspace_id: String,
    worktree_id: String,
    anchor_slug: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<CharacterContractReport> {
    character_contract_check_inner(
        &state,
        &workspace_id,
        &worktree_id,
        anchor_slug.as_deref(),
    )
}
