//! End-to-end checks for the runtime repair-budget finalizer against the real
//! bundled rig engine: once the provider is stopped, Sprite Studio itself
//! publishes a structurally valid result with a warning, or blocks it.

use super::test_support::*;
use crate::providers::{
    finalize_spent_budget, reports_generation_failure, BudgetExhaustion, BudgetFinalization,
};
use serde_json::{json, Value};
use std::time::{Duration, SystemTime};

fn finalize(
    fixture: &RigRendererFixture,
    started_at: SystemTime,
    category: &str,
    exhaustion: BudgetExhaustion,
) -> BudgetFinalization {
    tauri::async_runtime::block_on(finalize_spent_budget(
        &fixture.root,
        started_at,
        Some(category),
        exhaustion,
    ))
}

fn manifest(fixture: &RigRendererFixture) -> Value {
    serde_json::from_slice(
        &std::fs::read(fixture.root.join(".sprite-studio/last-generation.json"))
            .expect("manifest should exist"),
    )
    .expect("manifest should be JSON")
}

/// The live loop was repairing mask ownership and root dx when it was
/// stopped by hand. After the budget trips, those visual-quality failures
/// must no longer block a structurally valid animation.
#[test]
fn spent_budget_publishes_the_latest_rig_with_quality_failures_as_warnings() {
    let mut oversized_cap = good_biped_walk();
    oversized_cap["name"] = json!("running_man");
    oversized_cap["parts"][4]["mask"] = oversized_cap["parts"][3]["mask"].clone();
    oversized_cap["parts"][4]["overlapMode"] = json!("joint-cap");
    let mut root_drift = good_biped_walk();
    root_drift["name"] = json!("running_man");
    for index in 0..8 {
        root_drift["frames"][index]["root"]["dx"] = json!(index);
    }
    for (case, rig, diagnostic) in [
        ("mask ownership", oversized_cap, "bounded cap limit"),
        ("root drift", root_drift, "root"),
    ] {
        let fixture = RigRendererFixture::new();
        let strict = fixture.validate("running_man", &rig);
        assert_validation_failed(&strict, case, diagnostic);
        let started_at = SystemTime::now() - Duration::from_millis(200);
        fixture.write_rig("running_man", &rig);

        let outcome = finalize(&fixture, started_at, "characters", BudgetExhaustion::ValidationsBeforeRender);

        assert!(outcome.published, "{case}: {}", outcome.response);
        assert!(outcome.response.contains("GENERATION_WARNING:"), "{case}: {}", outcome.response);
        assert!(outcome.response.contains("running_man"), "{case}: attribution: {}", outcome.response);
        assert!(!reports_generation_failure(&outcome.response), "{case}: {}", outcome.response);
        let manifest = manifest(&fixture);
        assert_eq!(manifest["files"].as_array().map(Vec::len), Some(8), "{case}");
        let warnings = manifest["qualityWarnings"].as_array().expect("degraded checks recorded");
        assert!(
            warnings.iter().any(|warning| warning.as_str().is_some_and(|text| text.contains(diagnostic))),
            "{case}: {warnings:?}"
        );
        assert_eq!(manifest["quality"]["mechanics"], json!("degraded"), "{case}");
    }
}

#[test]
fn spent_budget_still_blocks_structural_failures() {
    let mut missing_source = good_biped_walk();
    missing_source["source"] = json!("assets/characters/missing.png");
    let mut frozen = good_biped_walk();
    for index in 0..8 {
        frozen["frames"][index] = json!({
            "phase": format!("pose_{index}"),
            "root": {"dx": 0, "dy": 0},
            "transforms": {},
        });
    }
    for (case, rig, reason) in [
        ("missing source", missing_source, "existing PNG"),
        ("identical frames", frozen, "two distinct frames"),
    ] {
        let fixture = RigRendererFixture::new();
        let started_at = SystemTime::now() - Duration::from_millis(200);
        fixture.write_rig("running_man", &rig);
        let outcome = finalize(&fixture, started_at, "characters", BudgetExhaustion::ValidationsBeforeRender);
        assert!(!outcome.published, "{case} must not publish: {}", outcome.response);
        assert!(outcome.response.starts_with("GENERATION_FAILED:"), "{case}: {}", outcome.response);
        assert!(reports_generation_failure(&outcome.response));
        assert!(outcome.response.contains(reason), "{case} failed for the wrong reason: {}", outcome.response);
        assert!(
            !fixture.root.join(".sprite-studio/last-generation.json").exists(),
            "{case}: nothing may be published"
        );
    }
}

#[test]
fn spent_budget_rejects_a_category_that_differs_from_the_route() {
    let fixture = RigRendererFixture::new();
    let started_at = SystemTime::now() - Duration::from_millis(200);
    fixture.write_rig("running_man", &good_biped_walk());
    let outcome = finalize(&fixture, started_at, "creatures", BudgetExhaustion::ValidationsBeforeRender);
    assert!(!outcome.published, "{}", outcome.response);
    assert!(outcome.response.contains("routed category"), "{}", outcome.response);
}

#[test]
fn spent_budget_after_render_publishes_the_requests_rendered_candidate() {
    let fixture = RigRendererFixture::new();
    let started_at = SystemTime::now() - Duration::from_millis(200);
    let rendered = fixture.render("running_man", &good_biped_walk());
    assert!(rendered.status.success(), "{}", String::from_utf8_lossy(&rendered.stderr));
    // The agent then broke the rig during its second repair attempt.
    let mut broken = good_biped_walk();
    broken["source"] = json!("assets/characters/missing.png");
    fixture.write_rig("running_man_v2", &broken);

    let outcome = finalize(&fixture, started_at, "characters", BudgetExhaustion::Rerenders);

    assert!(outcome.published, "{}", outcome.response);
    assert!(outcome.response.contains("regression_biped_walk"), "{}", outcome.response);
    assert!(outcome.response.contains("GENERATION_WARNING:"));
    assert!(manifest(&fixture).get("qualityWarnings").is_none());
}

#[test]
fn spent_budget_never_republishes_an_older_manifest() {
    let fixture = RigRendererFixture::new();
    let rendered = fixture.render("old_walk", &good_biped_walk());
    assert!(rendered.status.success(), "{}", String::from_utf8_lossy(&rendered.stderr));
    let path = fixture.root.join(".sprite-studio/last-generation.json");
    let mut stale = manifest(&fixture);
    stale["generatedAt"] = json!("2020-01-01T00:00:00+00:00");
    std::fs::write(&path, serde_json::to_vec(&stale).expect("manifest serializes")).expect("manifest writes");
    std::fs::remove_file(fixture.root.join(".sprite-studio/rigs/old_walk.json")).expect("rig removes");

    let outcome = finalize(&fixture, SystemTime::now(), "characters", BudgetExhaustion::ValidationsBeforeRender);

    assert!(!outcome.published, "{}", outcome.response);
    assert!(outcome.response.starts_with("GENERATION_FAILED:"));
}
