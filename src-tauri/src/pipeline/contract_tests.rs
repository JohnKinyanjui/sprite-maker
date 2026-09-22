use super::contract::size_contract_check_inner;
use crate::{
    animations::save_animation_inner,
    assets::{inspect, upsert},
    database,
    models::{AnimationFrame, AnimationInput},
    pipeline::anchors::promote_anchor_inner,
    workspace::create_workspace_inner,
    AppState,
};
use image::{Rgba, RgbaImage};
use uuid::Uuid;

fn fixture() -> (std::path::PathBuf, AppState) {
    let root = std::env::temp_dir().join(format!("sprite-pipeline-contract-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("temp dir");
    let connection = database::open(&root.join("app.sqlite3")).expect("db");
    (root, AppState::from_connection(connection))
}

#[test]
fn size_contract_blocks_wrong_canvas_dimensions() {
    let (root, state) = fixture();
    let project = root.join("game");
    std::fs::create_dir_all(&project).expect("project");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");

    let anchor_dir = project.join("assets").join("characters");
    std::fs::create_dir_all(&anchor_dir).expect("anchor dir");
    let anchor_path = anchor_dir.join("hero.png");
    let mut anchor_image = RgbaImage::new(64, 64);
    anchor_image.put_pixel(32, 63, Rgba([255, 255, 255, 255]));
    anchor_image.save(&anchor_path).expect("anchor save");

    let anchor_asset = inspect(&workspace.id, &project, &anchor_path, None).expect("inspect");
    upsert(&state, &anchor_asset, "test").expect("upsert anchor");
    promote_anchor_inner(&workspace.id, &anchor_asset.id, Some("hero"), false, None, &state)
        .expect("promote anchor");

    let wrong_dir = project.join("assets").join("walk");
    std::fs::create_dir_all(&wrong_dir).expect("walk dir");
    let wrong_path = wrong_dir.join("frame-01.png");
    let mut wrong_image = RgbaImage::new(62, 64);
    wrong_image.put_pixel(31, 63, Rgba([255, 0, 0, 255]));
    wrong_image.save(&wrong_path).expect("wrong save");
    let wrong_asset = inspect(&workspace.id, &project, &wrong_path, None).expect("inspect wrong");
    upsert(&state, &wrong_asset, "test").expect("upsert wrong");

    let animation = save_animation_inner(
        AnimationInput {
            id: None,
            workspace_id: workspace.id.clone(),
            worktree_id: None,
            name: "Walk".into(),
            fps: 10.0,
            looping: true,
            frames: vec![AnimationFrame {
                asset_id: wrong_asset.id,
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

    let report = size_contract_check_inner(&state, &animation.id, Some("hero"))
        .expect("contract check");
    assert!(!report.passed);
    assert!(
        report
            .violations
            .iter()
            .any(|violation| violation.code == "canvas_size" && violation.blocking),
        "expected blocking canvas_size violation"
    );

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn missing_anchor_violation_is_non_blocking_for_character_worktrees() {
    let (root, state) = fixture();
    let project = root.join("game");
    std::fs::create_dir_all(&project).expect("project");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");

    let frame_dir = project.join("assets").join("frames");
    std::fs::create_dir_all(&frame_dir).expect("frame dir");
    let frame_path = frame_dir.join("frame-01.png");
    let mut frame_image = RgbaImage::new(32, 32);
    frame_image.put_pixel(16, 31, Rgba([255, 0, 0, 255]));
    frame_image.save(&frame_path).expect("frame save");
    let frame_asset = inspect(&workspace.id, &project, &frame_path, None).expect("inspect");
    upsert(&state, &frame_asset, "test").expect("upsert");

    let worktree_id = {
        let connection = state.db.lock().expect("db lock");
        let id = Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO worktrees (id, project_id, name, slug, kind, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL, datetime('now'), datetime('now'))",
                rusqlite::params![id, workspace.id, "Hero", "hero", "character"],
            )
            .expect("worktree insert");
        id
    };

    let animation = save_animation_inner(
        AnimationInput {
            id: None,
            workspace_id: workspace.id.clone(),
            worktree_id: Some(worktree_id),
            name: "Idle".into(),
            fps: 8.0,
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

    let report = size_contract_check_inner(&state, &animation.id, None).expect("contract check");
    assert!(report.passed);
    assert!(
        report
            .violations
            .iter()
            .any(|violation| violation.code == "missing_anchor" && !violation.blocking)
    );

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn vfx_animation_skips_size_contract_without_anchor() {
    let (root, state) = fixture();
    let project = root.join("game");
    std::fs::create_dir_all(&project).expect("project");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");

    let worktree_id = {
        let connection = state.db.lock().expect("db lock");
        let id = Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO worktrees (id, project_id, name, slug, kind, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL, datetime('now'), datetime('now'))",
                rusqlite::params![id, workspace.id, "VFX", "vfx", "vfx"],
            )
            .expect("worktree insert");
        id
    };

    let frame_dir = project.join("assets").join("vfx");
    std::fs::create_dir_all(&frame_dir).expect("frame dir");
    let frame_path = frame_dir.join("spark.png");
    let mut frame_image = RgbaImage::new(32, 32);
    frame_image.put_pixel(16, 16, Rgba([255, 200, 0, 255]));
    frame_image.save(&frame_path).expect("frame save");
    let frame_asset = inspect(&workspace.id, &project, &frame_path, None).expect("inspect");
    upsert(&state, &frame_asset, "test").expect("upsert");

    let animation = save_animation_inner(
        AnimationInput {
            id: None,
            workspace_id: workspace.id.clone(),
            worktree_id: Some(worktree_id),
            name: "Spark".into(),
            fps: 12.0,
            looping: false,
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

    let report = size_contract_check_inner(&state, &animation.id, None).expect("contract check");
    assert!(report.passed);
    assert!(report.violations.is_empty());

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}
