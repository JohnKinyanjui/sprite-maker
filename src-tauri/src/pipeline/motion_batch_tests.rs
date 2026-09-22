use super::motion_batch::queue_motion_batch_inner;
use super::{contract::size_contract_check_inner, normalize::normalize_animation_inner};
use crate::models::NormalizeAnimationInput;
use super::direction_meta::write_direction_meta;
use crate::models::AnimationDirectionMeta;
use crate::{
    animations::save_animation_inner,
    assets::{inspect, upsert},
    database,
    models::{AnimationFrame, AnimationInput, QueueMotionBatchInput},
    pipeline::anchors::promote_anchor_inner,
    workspace::create_workspace_inner,
    AppState,
};
use image::{Rgba, RgbaImage};
use rusqlite::params;
use uuid::Uuid;

fn fixture() -> (std::path::PathBuf, AppState) {
    let root = std::env::temp_dir().join(format!("sprite-motion-batch-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("temp dir");
    let connection = database::open(&root.join("app.sqlite3")).expect("db");
    (root, AppState::from_connection(connection))
}

fn insert_character_worktree(state: &AppState, workspace_id: &str) -> String {
    let connection = state.db.lock().expect("db lock");
    let id = Uuid::new_v4().to_string();
    connection
        .execute(
            "INSERT INTO worktrees (id, project_id, name, slug, kind, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL, datetime('now'), datetime('now'))",
            params![id, workspace_id, "Hero", "hero", "character"],
        )
        .expect("worktree insert");
    id
}

#[test]
fn queue_motion_batch_returns_job_id() {
    let (root, state) = fixture();
    let project = root.join("game");
    std::fs::create_dir_all(&project).expect("project");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");
    let worktree_id = insert_character_worktree(&state, &workspace.id);

    let frame_path = project.join("assets").join("walk-w.png");
    std::fs::create_dir_all(frame_path.parent().unwrap()).expect("dir");
    let mut frame = RgbaImage::new(64, 64);
    frame.put_pixel(32, 63, Rgba([255, 255, 255, 255]));
    frame.save(&frame_path).expect("save");
    let frame_asset = inspect(&workspace.id, &project, &frame_path, None).expect("inspect");
    upsert(&state, &frame_asset, "test").expect("upsert");

    let anchor_path = project.join("assets").join("hero.png");
    frame.save(&anchor_path).expect("anchor");
    let anchor_asset = inspect(&workspace.id, &project, &anchor_path, None).expect("inspect anchor");
    upsert(&state, &anchor_asset, "test").expect("upsert anchor");
    promote_anchor_inner(&workspace.id, &anchor_asset.id, Some("hero"), false, None, &state)
        .expect("promote");

    let animation_id = Uuid::new_v4().to_string();
    save_animation_inner(
        AnimationInput {
            id: Some(animation_id.clone()),
            workspace_id: workspace.id.clone(),
            worktree_id: Some(worktree_id.clone()),
            name: "walk-w".into(),
            fps: 10.0,
            looping: true,
            frames: vec![AnimationFrame {
                asset_id: frame_asset.id,
                duration_ms: Some(100),
                offset_x: 0,
                offset_y: 0,
            }],
            motion_plan: None,
            review_status: Some("draft".into()),
        },
        &state,
    )
    .expect("save animation");

    write_direction_meta(
        &project,
        &AnimationDirectionMeta {
            animation_id: animation_id.clone(),
            direction_family: "walk".into(),
            facing: "w".into(),
            mirrored_from: None,
            anchor_slug: Some("hero".into()),
        },
    )
    .expect("write meta");

    let queued = queue_motion_batch_inner(
        QueueMotionBatchInput {
            workspace_id: workspace.id.clone(),
            worktree_id,
            anchor_slug: "hero".into(),
            motions: vec!["walk".into()],
            set: "4".into(),
            conversation_id: None,
            harden_after: Some(false),
            seed_animation_id: Some(animation_id),
            only_missing: None,
        },
        None,
        &state,
    )
    .expect("queue");

    assert!(!queued.job_id.is_empty());
    assert_eq!(queued.motions, vec!["walk".to_string()]);
    assert_eq!(queued.entries.len(), 1);

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn queue_motion_batch_only_missing_rejects_when_nothing_missing() {
    let (root, state) = fixture();
    let project = root.join("game");
    std::fs::create_dir_all(&project).expect("project");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");
    let worktree_id = insert_character_worktree(&state, &workspace.id);

    let frame_path = project.join("assets").join("walk-w.png");
    std::fs::create_dir_all(frame_path.parent().unwrap()).expect("dir");
    let mut frame = RgbaImage::new(64, 64);
    frame.put_pixel(32, 63, Rgba([255, 255, 255, 255]));
    frame.save(&frame_path).expect("save");
    let frame_asset = inspect(&workspace.id, &project, &frame_path, None).expect("inspect");
    upsert(&state, &frame_asset, "test").expect("upsert");

    let anchor_path = project.join("assets").join("hero.png");
    frame.save(&anchor_path).expect("anchor");
    let anchor_asset = inspect(&workspace.id, &project, &anchor_path, None).expect("inspect anchor");
    upsert(&state, &anchor_asset, "test").expect("upsert anchor");
    promote_anchor_inner(&workspace.id, &anchor_asset.id, Some("hero"), false, None, &state)
        .expect("promote");

    let animation_id = Uuid::new_v4().to_string();
    save_animation_inner(
        AnimationInput {
            id: Some(animation_id.clone()),
            workspace_id: workspace.id.clone(),
            worktree_id: Some(worktree_id.clone()),
            name: "walk-w".into(),
            fps: 10.0,
            looping: true,
            frames: vec![AnimationFrame {
                asset_id: frame_asset.id,
                duration_ms: Some(100),
                offset_x: 0,
                offset_y: 0,
            }],
            motion_plan: None,
            review_status: Some("draft".into()),
        },
        &state,
    )
    .expect("save animation");

    write_direction_meta(
        &project,
        &AnimationDirectionMeta {
            animation_id: animation_id.clone(),
            direction_family: "walk".into(),
            facing: "w".into(),
            mirrored_from: None,
            anchor_slug: Some("hero".into()),
        },
    )
    .expect("write meta");
    normalize_animation_inner(
        None,
        &state,
        NormalizeAnimationInput {
            animation_id: animation_id.clone(),
            anchor_slug: Some("hero".into()),
            lock_first_frame: Some(true),
            shared_scale: Some(true),
            padding: None,
        },
    )
    .expect("normalize");
    let contract =
        size_contract_check_inner(&state, &animation_id, Some("hero")).expect("contract");
    assert!(contract.passed);

    let error = queue_motion_batch_inner(
        QueueMotionBatchInput {
            workspace_id: workspace.id.clone(),
            worktree_id,
            anchor_slug: "hero".into(),
            motions: vec!["walk".into()],
            set: "4".into(),
            conversation_id: None,
            harden_after: Some(false),
            seed_animation_id: Some(animation_id),
            only_missing: Some(true),
        },
        None,
        &state,
    )
    .expect_err("only missing should reject when walk is already satisfied");

    assert_eq!(error.code, "no_missing_motions");

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}
