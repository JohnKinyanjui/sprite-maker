use crate::models::{QualityCheck, SizeContractViolation};
use chrono::Utc;

use super::metrics::PendingCheck;

fn violation_profile(code: &str, blocking: bool) -> (&'static str, &'static str, f64) {
    match code {
        "canvas_size" => ("dimensions", if blocking { "error" } else { "warning" }, 22.0),
        "baseline_drift" => ("alignment", if blocking { "error" } else { "warning" }, 18.0),
        "identity_drift" => ("palette_consistency", "warning", 14.0),
        "identity_warning" => ("palette_consistency", "warning", 6.0),
        "motion_still" => ("sudden_change", "warning", 12.0),
        "centroid_drift" => ("alignment", if blocking { "error" } else { "warning" }, 16.0),
        "facing_identity_drift" => ("palette_consistency", "error", 18.0),
        "facing_baseline_mismatch" => ("alignment", "error", 16.0),
        "missing_anchor" => ("contract_setup", "warning", 8.0),
        _ => ("contract", "warning", 8.0),
    }
}

fn repair_action_for(code: &str) -> Option<&'static str> {
    match code {
        "canvas_size" | "baseline_drift" | "centroid_drift" => Some("contract_auto_fix"),
        "motion_still" | "identity_drift" | "identity_warning" | "facing_identity_drift" => {
            Some("regenerate_transition")
        }
        "facing_baseline_mismatch" => Some("contract_auto_fix"),
        "missing_anchor" => Some("promote_anchor"),
        _ => Some("contract_auto_fix"),
    }
}

pub(crate) fn contract_violations_to_pending_checks(
    violations: &[SizeContractViolation],
) -> Vec<PendingCheck> {
    violations
        .iter()
        .map(|violation| {
            let (check_type, severity, score) =
                violation_profile(&violation.code, violation.blocking);
            PendingCheck {
                check_type,
                frame_index: violation.frame_index,
                comparison_frame_index: None,
                severity,
                score,
                message: violation.message.clone(),
                metric_value: None,
                metric_unit: None,
                repair_action: repair_action_for(&violation.code),
            }
        })
        .collect()
}

pub(crate) fn apply_contract_penalties(
    violations: &[SizeContractViolation],
    alignment_penalty: &mut f64,
    consistency_penalty: &mut f64,
    continuity_penalty: &mut f64,
) {
    for violation in violations {
        match violation.code.as_str() {
            "baseline_drift" | "centroid_drift" => {
                *alignment_penalty += if violation.blocking { 18.0 } else { 8.0 };
            }
            "canvas_size" => *alignment_penalty += 22.0,
            "identity_drift" => *consistency_penalty += 14.0,
            "identity_warning" => *consistency_penalty += 6.0,
            "motion_still" => *continuity_penalty += 12.0,
            "facing_identity_drift" => *consistency_penalty += 18.0,
            "facing_baseline_mismatch" => *alignment_penalty += 16.0,
            _ => {}
        }
    }
}

pub(crate) fn contract_violations_to_quality_checks(
    report_id: &str,
    violations: &[SizeContractViolation],
    position_offset: u32,
) -> Vec<QualityCheck> {
    let created_at = Utc::now().to_rfc3339();
    violations
        .iter()
        .enumerate()
        .map(|(index, violation)| {
            let (check_type, severity, score) =
                violation_profile(&violation.code, violation.blocking);
            let frame_suffix = violation
                .frame_index
                .map(|value| value.to_string())
                .unwrap_or_else(|| "all".into());
            QualityCheck {
                id: format!("contract:{}:{}:{}", violation.code, frame_suffix, index),
                report_id: report_id.to_string(),
                position: position_offset + index as u32,
                check_type: check_type.to_string(),
                frame_index: violation.frame_index,
                comparison_frame_index: None,
                severity: severity.to_string(),
                score,
                message: violation.message.clone(),
                metric_value: None,
                metric_unit: None,
                repair_action: repair_action_for(&violation.code).map(str::to_string),
                acknowledged: false,
                ignored: false,
                created_at: created_at.clone(),
            }
        })
        .collect()
}

pub(crate) fn merge_contract_checks(
    checks: &mut Vec<QualityCheck>,
    violations: &[SizeContractViolation],
) {
    let position_offset = checks.len() as u32;
    let synthetic =
        contract_violations_to_quality_checks("live-contract", violations, position_offset);
    for check in synthetic {
        let duplicate = checks.iter().any(|existing| {
            existing.check_type == check.check_type
                && existing.frame_index == check.frame_index
                && existing.message == check.message
        });
        if !duplicate {
            checks.push(check);
        }
    }
}
