use super::harden::harden_animation_inner;
use crate::{
    animations::save_animation_inner,
    assets::{inspect, upsert},
    database,
    models::{AnimationFrame, AnimationInput, HardenAnimationOptions},
    pipeline::anchors::promote_anchor_inner,
    workspace::create_workspace_inner,
    AppState,
};
use image::{Rgba, RgbaImage};
use uuid::Uuid;

#[test]
fn harden_animation_writes_manifest_and_checks_contract() {
    let root = std::env::temp_dir().join(format!("sprite-harden-{}", Uuid::new_v4()));
    let project = root.join("game");
    std::fs::create_dir_all(&project.join("assets/characters")).expect("asset dir");
    let connection = database::open(&root.join("app.sqlite3")).expect("db");
    let state = AppState::from_connection(connection);
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");

    let anchor_path = project.join("assets/characters/hero.png");
    let mut anchor_image = RgbaImage::new(32, 32);
    anchor_image.put_pixel(16, 31, Rgba([255, 255, 255, 255]));
    anchor_image.save(&anchor_path).expect("anchor save");
    let anchor_asset = inspect(&workspace.id, &project, &anchor_path, None).expect("inspect");
    upsert(&state, &anchor_asset, "test").expect("upsert");
    promote_anchor_inner(&workspace.id, &anchor_asset.id, Some("hero"), false, None, &state)
        .expect("promote");

    let frame_path = project.join("assets/characters/walk-01.png");
    let mut frame_image = RgbaImage::new(32, 32);
    frame_image.put_pixel(16, 31, Rgba([255, 0, 0, 255]));
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

    let report = harden_animation_inner(
        None,
        &state,
        &animation.id,
        Some("hero"),
        None,
        None,
        None,
        HardenAnimationOptions {
            clean_alpha: Some(true),
            normalize: Some(true),
            snap_grid: Some(false),
            grid_size: None,
            split_layout: None,
            queue_contract_retry: Some(false),
            derive_mirrored_facing: None,
            ..Default::default()
        },
    )
    .expect("harden");

    assert!(report.steps.iter().any(|step| step == "clean_alpha"));
    assert!(report.steps.iter().any(|step| step == "normalize_animation"));
    assert!(report.steps.iter().any(|step| step == "write_generation_manifest"));
    assert!(report.steps.iter().any(|step| step == "check_size_contract"));
    assert!(project.join(".sprite-studio/last-generation.json").is_file());

    let log_content =
        std::fs::read_to_string(project.join(".sprite-studio/pipeline-log.jsonl")).expect("log");
    let log_line = log_content
        .lines()
        .find(|line| line.contains("\"event\":\"harden_animation\""))
        .expect("harden log line");
    let log_entry: serde_json::Value = serde_json::from_str(log_line).expect("log json");
    assert_eq!(log_entry["stage"], "completed");
    assert!(log_entry["details"]["stageDurationsMs"].is_object());
    assert!(log_entry["details"]["stageDurationsMs"]["load_context"].is_number());

    drop(state);
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn harden_animation_logs_failure_on_mid_chain_error() {
    let root = std::env::temp_dir().join(format!("sprite-harden-fail-{}", Uuid::new_v4()));
    let project = root.join("game");
    std::fs::create_dir_all(&project.join("assets/characters")).expect("asset dir");
    let connection = database::open(&root.join("app.sqlite3")).expect("db");
    let state = AppState::from_connection(connection);
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");

    let frame_path = project.join("assets/characters/walk-01.png");
    let mut frame_image = RgbaImage::new(32, 32);
    frame_image.put_pixel(16, 31, Rgba([255, 0, 0, 255]));
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

    let error = harden_animation_inner(
        None,
        &state,
        &animation.id,
        Some("hero"),
        Some("assets/imports/missing-strip.png"),
        Some(4),
        None,
        HardenAnimationOptions {
            clean_alpha: Some(true),
            normalize: Some(true),
            snap_grid: Some(false),
            grid_size: None,
            split_layout: None,
            queue_contract_retry: Some(false),
            derive_mirrored_facing: None,
            ..Default::default()
        },
    )
    .expect_err("missing strip should fail harden");

    assert_ne!(error.code, "animation_not_found");

    let log_content =
        std::fs::read_to_string(project.join(".sprite-studio/pipeline-log.jsonl")).expect("log");
    let log_line = log_content
        .lines()
        .find(|line| line.contains("\"event\":\"harden_animation\""))
        .expect("harden failure log line");
    let log_entry: serde_json::Value = serde_json::from_str(log_line).expect("log json");
    assert_eq!(log_entry["stage"], "failed");
    assert_eq!(log_entry["errorCode"], error.code);
    assert!(log_entry["details"]["stageDurationsMs"]["load_context"].is_number());

    drop(state);
    std::fs::remove_dir_all(root).expect("cleanup");
}
