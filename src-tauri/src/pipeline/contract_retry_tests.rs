use super::contract_retry::build_corrective_prompt;
use super::contract_retry_job::queue_contract_retry_inner;

use crate::jobs::load_job;
use crate::models::{
    CharacterAnchor, Pivot, QueueContractRetryInput, SizeContractReport, SizeContractViolation,
};
use crate::{
    animations::save_animation_inner,
    assets::{inspect, upsert},
    database,
    models::{AnimationFrame, AnimationInput},
    workspace::create_workspace_inner,
    AppState,
};
use image::{Rgba, RgbaImage};
use std::time::Duration;
use uuid::Uuid;

#[test]
fn corrective_prompt_includes_violation_specific_fixes() {
    let anchor = CharacterAnchor {
        slug: "hero".into(),
        asset_id: "asset-1".into(),
        relative_path: "assets/characters/hero.png".into(),
        frame_width: 64,
        frame_height: 64,
        pivot: Pivot { x: 32.0, y: 63.0 },
        baseline_y: 63,
        content_hash: "hash".into(),
        promoted_at: "now".into(),
        view: None,
        facing: None,
        facing_confidence: None,
        facing_status: None,
    };
    let report = SizeContractReport {
        animation_id: "anim-1".into(),
        anchor_slug: Some("hero".into()),
        passed: false,
        review_status: "draft".into(),
        violations: vec![
            SizeContractViolation {
                code: "baseline_drift".into(),
                message: "drift".into(),
                blocking: true,
                frame_index: Some(2),
            },
            SizeContractViolation {
                code: "motion_still".into(),
                message: "still".into(),
                blocking: false,
                frame_index: Some(1),
            },
        ],
    };

    let prompt = build_corrective_prompt(&report, &anchor, "Walk", 6, 2);

    assert!(prompt.contains("baseline Y=63"));
    assert!(prompt.contains("motion_still"));
    assert!(prompt.contains("6 poses"));
}

fn fixture() -> (std::path::PathBuf, AppState) {
    let root = std::env::temp_dir().join(format!("sprite-contract-retry-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("temp dir");
    let connection = database::open(&root.join("app.sqlite3")).expect("db");
    (root, AppState::from_connection(connection))
}

#[test]
fn queue_contract_retry_rejects_empty_conversation() {
    let (_root, state) = fixture();
    let error = queue_contract_retry_inner(
        QueueContractRetryInput {
            animation_id: "anim-1".into(),
            conversation_id: "   ".into(),
            anchor_slug: None,
            max_ai_attempts: None,
            max_deterministic_passes: None,
        },
        None,
        &state,
    )
    .expect_err("empty conversation should fail");
    assert_eq!(error.code, "conversation_required");
}

#[test]
fn queue_contract_retry_returns_job_id() {
    let (root, state) = fixture();
    let project = root.join("game");
    std::fs::create_dir_all(&project).expect("project");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");

    let frame_dir = project.join("assets").join("walk");
    std::fs::create_dir_all(&frame_dir).expect("frame dir");
    let frame_path = frame_dir.join("frame-01.png");
    let mut frame_image = RgbaImage::new(64, 64);
    frame_image.put_pixel(32, 63, Rgba([255, 255, 255, 255]));
    frame_image.save(&frame_path).expect("frame save");
    let frame_asset = inspect(&workspace.id, &project, &frame_path, None).expect("inspect");
    upsert(&state, &frame_asset, "test").expect("upsert");

    let animation = save_animation_inner(
        AnimationInput {
            id: None,
            workspace_id: workspace.id.clone(),
            worktree_id: None,
            name: "Walk".into(),
            fps: 10.0,
            looping: true,
            frames: vec![AnimationFrame {
                asset_id: frame_asset.id,
                duration_ms: None,
                offset_x: 0,
                offset_y: 0,
            }],
            motion_plan: None,
            review_status: Some("draft".into()),
        },
        &state,
    )
    .expect("save animation");

    let result = queue_contract_retry_inner(
        QueueContractRetryInput {
            animation_id: animation.id.clone(),
            conversation_id: "conv-1".into(),
            anchor_slug: None,
            max_ai_attempts: Some(1),
            max_deterministic_passes: Some(1),
        },
        None,
        &state,
    )
    .expect("queue contract retry");

    assert!(result.job_id.is_some());
    assert!(!result.passed);

    let job_id = result.job_id.clone().expect("job id");
    let mut job = load_job(&state, &job_id).expect("load queued job");
    for _ in 0..40 {
        if job.status != "queued" {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
        job = load_job(&state, &job_id).expect("reload job");
    }
    assert_ne!(job.status, "queued");
    assert!(["running", "completed", "failed", "cancelled"].contains(&job.status.as_str()));
    if let Some(metadata_json) = job.metadata_json {
        assert!(metadata_json.contains("\"attempts\""));
    }
}

#[test]
fn contract_retry_cancel_requested_stops_running_job() {
    let (root, state) = fixture();
    let project = root.join("game-cancel");
    std::fs::create_dir_all(&project).expect("project");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");

    let frame_dir = project.join("assets").join("walk");
    std::fs::create_dir_all(&frame_dir).expect("frame dir");
    let frame_path = frame_dir.join("frame-01.png");
    let mut frame_image = RgbaImage::new(64, 64);
    frame_image.put_pixel(32, 63, Rgba([255, 255, 255, 255]));
    frame_image.save(&frame_path).expect("frame save");
    let frame_asset = inspect(&workspace.id, &project, &frame_path, None).expect("inspect");
    upsert(&state, &frame_asset, "test").expect("upsert");

    let animation = save_animation_inner(
        AnimationInput {
            id: None,
            workspace_id: workspace.id.clone(),
            worktree_id: None,
            name: "Walk".into(),
            fps: 10.0,
            looping: true,
            frames: vec![AnimationFrame {
                asset_id: frame_asset.id,
                duration_ms: None,
                offset_x: 0,
                offset_y: 0,
            }],
            motion_plan: None,
            review_status: Some("draft".into()),
        },
        &state,
    )
    .expect("save animation");

    let result = queue_contract_retry_inner(
        QueueContractRetryInput {
            animation_id: animation.id.clone(),
            conversation_id: "conv-cancel".into(),
            anchor_slug: None,
            max_ai_attempts: Some(1),
            max_deterministic_passes: Some(1),
        },
        None,
        &state,
    )
    .expect("queue contract retry");
    let job_id = result.job_id.expect("job id");

    {
        let connection = state.db.lock().expect("db lock");
        connection
            .execute(
                "UPDATE background_jobs SET cancel_requested=1 WHERE id=?1",
                [&job_id],
            )
            .expect("request cancel");
    }

    let mut job = load_job(&state, &job_id).expect("load job");
    for _ in 0..60 {
        if matches!(job.status.as_str(), "cancelled" | "failed" | "completed") {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
        job = load_job(&state, &job_id).expect("reload job");
    }
    assert_eq!(job.status, "cancelled");

    let metadata_json = job
        .metadata_json
        .expect("cancel should persist contract retry metadata");
    let metadata: serde_json::Value = serde_json::from_str(&metadata_json).expect("metadata json");
    assert_eq!(metadata["metrics"]["outcome"], "cancelled");
    assert!(metadata["metrics"]["stageDurationsMs"].is_object());

    let log_path = project.join(".sprite-studio/pipeline-log.jsonl");
    let log_content = std::fs::read_to_string(&log_path).expect("pipeline log");
    let log_line = log_content
        .lines()
        .find(|line| line.contains("\"event\":\"contract_retry\""))
        .expect("contract retry log line");
    let log_entry: serde_json::Value = serde_json::from_str(log_line).expect("log json");
    assert_eq!(log_entry["details"]["outcome"], "cancelled");
    assert_eq!(log_entry["jobId"], job_id);
}

#[test]
fn contract_retry_failure_logs_failed_metrics() {
    let (root, state) = fixture();
    let project = root.join("game-fail");
    std::fs::create_dir_all(&project).expect("project");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");

    let frame_dir = project.join("assets").join("walk");
    std::fs::create_dir_all(&frame_dir).expect("frame dir");
    let frame_path = frame_dir.join("frame-01.png");
    let mut frame_image = RgbaImage::new(64, 64);
    frame_image.put_pixel(32, 63, Rgba([255, 255, 255, 255]));
    frame_image.save(&frame_path).expect("frame save");
    let frame_asset = inspect(&workspace.id, &project, &frame_path, None).expect("inspect");
    upsert(&state, &frame_asset, "test").expect("upsert");

    let animation = save_animation_inner(
        AnimationInput {
            id: None,
            workspace_id: workspace.id.clone(),
            worktree_id: None,
            name: "Walk".into(),
            fps: 10.0,
            looping: true,
            frames: vec![AnimationFrame {
                asset_id: frame_asset.id,
                duration_ms: None,
                offset_x: 0,
                offset_y: 0,
            }],
            motion_plan: None,
            review_status: Some("draft".into()),
        },
        &state,
    )
    .expect("save animation");

    let result = queue_contract_retry_inner(
        QueueContractRetryInput {
            animation_id: animation.id.clone(),
            conversation_id: "conv-fail".into(),
            anchor_slug: None,
            max_ai_attempts: Some(1),
            max_deterministic_passes: Some(1),
        },
        None,
        &state,
    )
    .expect("queue contract retry");
    let job_id = result.job_id.expect("job id");

    {
        let connection = state.db.lock().expect("db lock");
        connection
            .execute("DELETE FROM animations WHERE id=?1", [&animation.id])
            .expect("delete animation before async context load");
    }

    let mut job = load_job(&state, &job_id).expect("load job");
    for _ in 0..80 {
        if matches!(job.status.as_str(), "failed" | "completed" | "cancelled") {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
        job = load_job(&state, &job_id).expect("reload job");
    }
    assert_eq!(job.status, "failed");

    let metadata_json = job.metadata_json.expect("failed job metadata");
    let metadata: serde_json::Value = serde_json::from_str(&metadata_json).expect("metadata json");
    assert_eq!(metadata["metrics"]["outcome"], "failed");
    assert_eq!(metadata["metrics"]["failureReason"], "animation_not_found");
    assert!(metadata["metrics"]["stageDurationsMs"].is_object());

    let log_content =
        std::fs::read_to_string(project.join(".sprite-studio/pipeline-log.jsonl")).expect("log");
    let log_line = log_content
        .lines()
        .find(|line| line.contains("\"event\":\"contract_retry\""))
        .expect("contract retry log line");
    let log_entry: serde_json::Value = serde_json::from_str(log_line).expect("log json");
    assert_eq!(log_entry["details"]["outcome"], "failed");
    assert_eq!(log_entry["errorCode"], "animation_not_found");
}
