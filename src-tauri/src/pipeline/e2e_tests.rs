use super::{
    contract::size_contract_check_inner,
    contract_retry_job::queue_contract_retry_inner,
    normalize::normalize_animation_inner,
    strip::split_sprite_strip_inner,
    test_fixtures::{
        build_walk_strip, copy_walk_strip_fixture, save_walk_animation_from_split,
        setup_hero_workspace, temp_pipeline_state, walk_strip_fixture_path,
    },
};
use crate::{
    jobs::load_job,
    models::{NormalizeAnimationInput, QueueContractRetryInput},
    AppState,
};
use std::path::Path;
use std::time::Duration;

fn parse_pipeline_log_entries(content: &str) -> Vec<serde_json::Value> {
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("pipeline log json line"))
        .collect()
}

fn setup_walk_animation(
    root: &Path,
    state: &AppState,
) -> (crate::models::Workspace, crate::models::Animation) {
    let (workspace, project) = setup_hero_workspace(root, state);
    let strip_path = project.join("assets/imports/walk-strip.png");
    copy_walk_strip_fixture(&strip_path);

    let split = split_sprite_strip_inner(
        &workspace.id,
        strip_path.to_string_lossy().as_ref(),
        "horizontal",
        4,
        None,
        true,
        "characters",
        state,
    )
    .expect("split");
    assert_eq!(split.asset_ids.len(), 4);

    let animation = save_walk_animation_from_split(&workspace.id, split.asset_ids, state);

    normalize_animation_inner(
        None,
        state,
        NormalizeAnimationInput {
            animation_id: animation.id.clone(),
            anchor_slug: Some("hero".into()),
            lock_first_frame: Some(true),
            shared_scale: Some(true),
            padding: None,
        },
    )
    .expect("normalize");

    (workspace, animation)
}

#[test]
#[ignore = "run once to refresh committed walk strip fixture"]
fn materialize_walk_strip_fixture() {
    let path = walk_strip_fixture_path();
    std::fs::create_dir_all(path.parent().expect("parent")).expect("fixture dir");
    build_walk_strip(&path, 4, 32);
    assert!(path.is_file());
}

#[test]
fn pipeline_e2e_promote_split_normalize_contract() {
    let (root, state) = temp_pipeline_state();
    let (_workspace, animation) = setup_walk_animation(&root, &state);

    let report =
        size_contract_check_inner(&state, &animation.id, Some("hero")).expect("contract");
    assert!(
        report.passed,
        "normalized walk strip should pass size contract: {:?}",
        report.violations
    );

    drop(state);
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn pipeline_e2e_contract_retry_logs_metrics() {
    let (root, state) = temp_pipeline_state();
    let project = root.join("game");
    let (_workspace, animation) = setup_walk_animation(&root, &state);

    let report =
        size_contract_check_inner(&state, &animation.id, Some("hero")).expect("contract");
    assert!(report.passed, "animation must pass before retry is queued");

    let queued = queue_contract_retry_inner(
        QueueContractRetryInput {
            animation_id: animation.id.clone(),
            conversation_id: "conv-e2e".into(),
            anchor_slug: Some("hero".into()),
            max_ai_attempts: Some(1),
            max_deterministic_passes: Some(1),
        },
        None,
        &state,
    )
    .expect("queue contract retry");

    let job_id = queued.job_id.expect("job id");
    let mut job = load_job(&state, &job_id).expect("load job");
    for _ in 0..80 {
        if matches!(job.status.as_str(), "completed" | "failed" | "cancelled") {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
        job = load_job(&state, &job_id).expect("reload job");
    }
    assert_eq!(job.status, "completed");

    let metadata_json = job
        .metadata_json
        .expect("contract retry metadata should be persisted");
    let metadata: serde_json::Value = serde_json::from_str(&metadata_json).expect("metadata json");
    assert_eq!(metadata["metrics"]["outcome"], "passed");
    assert!(metadata["metrics"]["stageDurationsMs"].is_object());

    let log_path = project.join(".sprite-studio/pipeline-log.jsonl");
    let log_content = std::fs::read_to_string(&log_path).expect("pipeline log");
    let contract_log = parse_pipeline_log_entries(&log_content)
        .into_iter()
        .find(|entry| entry["event"] == "contract_retry")
        .expect("contract retry log entry");
    assert_eq!(contract_log["jobId"], job_id);
    assert_eq!(contract_log["details"]["outcome"], "passed");
    assert!(contract_log["details"]["stageDurationsMs"].is_object());

    drop(state);
    std::fs::remove_dir_all(root).expect("cleanup");
}
