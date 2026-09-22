use crate::{
    error::{CommandError, CommandResult},
    models::ProductionScoreReport,
    quality::get_quality_report_inner,
    AppState,
};
use tauri::State;

use super::character_contract::character_contract_check_inner;

const SIZE_WEIGHT: f64 = 0.35;
const CHARACTER_WEIGHT: f64 = 0.35;
const QUALITY_WEIGHT: f64 = 0.30;

fn clamp_score(value: f64) -> f64 {
    value.clamp(0.0, 100.0)
}

pub(crate) fn production_score_inner(
    state: &AppState,
    workspace_id: &str,
    worktree_id: &str,
    anchor_slug: Option<&str>,
) -> CommandResult<ProductionScoreReport> {
    let contract = character_contract_check_inner(state, workspace_id, worktree_id, anchor_slug)?;

    let animation_count = contract.animation_count;
    let passed_animations = contract
        .animation_reports
        .iter()
        .filter(|report| report.passed)
        .count();
    let size_contract_pass_rate = if animation_count == 0 {
        0.0
    } else {
        passed_animations as f64 / animation_count as f64
    };
    let size_contract_score = clamp_score(size_contract_pass_rate * 100.0);

    let cross_facing_violation_count = contract.cross_facing_violations.len() as u32;
    let blocking_cross = contract
        .cross_facing_violations
        .iter()
        .filter(|violation| violation.blocking)
        .count();
    let character_contract_score = if contract.passed {
        100.0
    } else {
        let penalty = (contract.animation_reports.len()
            - contract.animation_reports.iter().filter(|report| report.passed).count()
            + blocking_cross * 2
            + cross_facing_violation_count as usize) as f64
            * 12.0;
        clamp_score(100.0 - penalty)
    };

    let mut quality_total = 0.0;
    let mut quality_count = 0u32;
    for report in &contract.animation_reports {
        if let Some(quality) = get_quality_report_inner(&report.animation_id, state)? {
            if quality.status == "completed" {
                quality_total += quality.overall_score;
                quality_count += 1;
            }
        }
    }
    let quality_score = if quality_count == 0 {
        size_contract_score
    } else {
        clamp_score(quality_total / quality_count as f64)
    };

    let overall_score = clamp_score(
        size_contract_score * SIZE_WEIGHT
            + character_contract_score * CHARACTER_WEIGHT
            + quality_score * QUALITY_WEIGHT,
    );

    Ok(ProductionScoreReport {
        workspace_id: workspace_id.to_string(),
        worktree_id: worktree_id.to_string(),
        anchor_slug: contract.anchor_slug,
        overall_score,
        size_contract_score,
        character_contract_score,
        quality_score,
        animation_count,
        animations_with_quality: quality_count,
        size_contract_pass_rate,
        character_contract_passed: contract.passed,
        cross_facing_violation_count,
    })
}

#[tauri::command]
pub fn get_production_score(
    workspace_id: String,
    worktree_id: String,
    anchor_slug: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<ProductionScoreReport> {
    if worktree_id.trim().is_empty() {
        return Err(CommandError::new(
            "missing_worktree",
            "Select a character worktree before computing production score",
        ));
    }
    production_score_inner(&state, &workspace_id, &worktree_id, anchor_slug.as_deref())
}
