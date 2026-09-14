use super::character_contract::character_contract_check_inner;
use super::contract::ensure_export_allowed;
use super::direction_meta::write_direction_meta;
use crate::{
    animations::save_animation_inner,
    assets::{inspect, upsert},
    database,
    models::{AnimationDirectionMeta, AnimationFrame, AnimationInput},
    pipeline::anchors::promote_anchor_inner,
    workspace::create_workspace_inner,
    AppState,
};
use image::{Rgba, RgbaImage};
use uuid::Uuid;

fn fixture() -> (std::path::PathBuf, AppState) {
    let root = std::env::temp_dir().join(format!("sprite-character-contract-{}", Uuid::new_v4()));
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
            rusqlite::params![id, workspace_id, "Hero", "hero", "character"],
        )
        .expect("worktree insert");
    id
}

fn save_matching_frame(
    state: &AppState,
    workspace_id: &str,
    project: &std::path::Path,
    relative_dir: &str,
    file_name: &str,
) -> crate::models::Asset {
    let frame_dir = project.join(relative_dir);
    std::fs::create_dir_all(&frame_dir).expect("frame dir");
    let frame_path = frame_dir.join(file_name);
    let mut frame_image = RgbaImage::new(64, 64);
    frame_image.put_pixel(32, 63, Rgba([255, 0, 0, 255]));
    frame_image.save(&frame_path).expect("frame save");
    let frame_asset = inspect(workspace_id, project, &frame_path, None).expect("inspect");
    upsert(state, &frame_asset, "test").expect("upsert");
    frame_asset
}

#[test]
fn character_contract_requires_character_worktree() {
    let (root, state) = fixture();
    let project = root.join("game");
    std::fs::create_dir_all(&project).expect("project");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");

    let error = character_contract_check_inner(&state, &workspace.id, "missing-worktree", None)
        .expect_err("missing worktree should fail");
    assert_eq!(error.code, "worktree_not_found");

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn character_contract_aggregates_all_animations_in_worktree() {
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

    let worktree_id = insert_character_worktree(&state, &workspace.id);
    let good_frame = save_matching_frame(
        &state,
        &workspace.id,
        &project,
        "assets/walk",
        "frame-01.png",
    );
    let bad_dir = project.join("assets/run");
    std::fs::create_dir_all(&bad_dir).expect("run dir");
    let bad_path = bad_dir.join("frame-01.png");
    let mut bad_image = RgbaImage::new(62, 64);
    bad_image.put_pixel(31, 63, Rgba([255, 0, 0, 255]));
    bad_image.save(&bad_path).expect("bad save");
    let bad_frame = inspect(&workspace.id, &project, &bad_path, None).expect("inspect bad");
    upsert(&state, &bad_frame, "test").expect("upsert bad");

    save_animation_inner(
        AnimationInput {
            id: None,
            workspace_id: workspace.id.clone(),
            worktree_id: Some(worktree_id.clone()),
            name: "Walk".into(),
            fps: 10.0,
            looping: true,
            frames: vec![AnimationFrame {
                asset_id: good_frame.id,
                duration_ms: None,
                offset_x: 0,
                offset_y: 0,
            }],
            motion_plan: None,
            review_status: Some("draft".into()),
        },
        &state,
    )
    .expect("save walk");

    save_animation_inner(
        AnimationInput {
            id: None,
            workspace_id: workspace.id.clone(),
            worktree_id: Some(worktree_id.clone()),
            name: "Run".into(),
            fps: 12.0,
            looping: true,
            frames: vec![AnimationFrame {
                asset_id: bad_frame.id,
                duration_ms: None,
                offset_x: 0,
                offset_y: 0,
            }],
            motion_plan: None,
            review_status: Some("draft".into()),
        },
        &state,
    )
    .expect("save run");

    let report =
        character_contract_check_inner(&state, &workspace.id, &worktree_id, Some("hero"))
            .expect("character contract");
    assert_eq!(report.animation_count, 2);
    assert!(!report.passed);
    assert_eq!(report.anchor_slug, "hero");
    assert!(
        report
            .animation_reports
            .iter()
            .any(|animation| !animation.passed),
        "expected at least one failing animation report"
    );

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn accepted_animation_still_blocks_export_on_cross_facing_drift() {
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

    let anchor_path = project.join("assets").join("hero.png");
    std::fs::create_dir_all(anchor_path.parent().unwrap()).expect("anchor dir");
    let mut anchor_image = RgbaImage::new(64, 64);
    anchor_image.put_pixel(32, 63, Rgba([255, 255, 255, 255]));
    anchor_image.save(&anchor_path).expect("anchor save");
    let anchor_asset = inspect(&workspace.id, &project, &anchor_path, None).expect("inspect");
    upsert(&state, &anchor_asset, "test").expect("upsert anchor");
    promote_anchor_inner(&workspace.id, &anchor_asset.id, Some("hero"), false, None, &state)
        .expect("promote anchor");

    let west_frame = save_matching_frame(
        &state,
        &workspace.id,
        &project,
        "assets/walk-w",
        "frame-01.png",
    );
    let east_dir = project.join("assets").join("walk-e");
    std::fs::create_dir_all(&east_dir).expect("east dir");
    let east_path = east_dir.join("frame-01.png");
    let mut east_image = RgbaImage::new(64, 64);
    for x in 0..64 {
        for y in 0..64 {
            east_image.put_pixel(x, y, Rgba([(x * 3) as u8, (y * 5) as u8, 128, 255]));
        }
    }
    east_image.put_pixel(32, 63, Rgba([255, 0, 0, 255]));
    east_image.save(&east_path).expect("east save");
    let east_frame = inspect(&workspace.id, &project, &east_path, None).expect("inspect east");
    upsert(&state, &east_frame, "test").expect("upsert east");

    let west_id = Uuid::new_v4().to_string();
    save_animation_inner(
        AnimationInput {
            id: Some(west_id.clone()),
            workspace_id: workspace.id.clone(),
            worktree_id: Some(worktree_id.clone()),
            name: "walk-w".into(),
            fps: 10.0,
            looping: true,
            frames: vec![AnimationFrame {
                asset_id: west_frame.id,
                duration_ms: None,
                offset_x: 0,
                offset_y: 0,
            }],
            motion_plan: None,
            review_status: Some("accepted".into()),
        },
        &state,
    )
    .expect("save west");
    let east_id = Uuid::new_v4().to_string();
    save_animation_inner(
        AnimationInput {
            id: Some(east_id.clone()),
            workspace_id: workspace.id.clone(),
            worktree_id: Some(worktree_id.clone()),
            name: "walk-e".into(),
            fps: 10.0,
            looping: true,
            frames: vec![AnimationFrame {
                asset_id: east_frame.id,
                duration_ms: None,
                offset_x: 0,
                offset_y: 0,
            }],
            motion_plan: None,
            review_status: Some("draft".into()),
        },
        &state,
    )
    .expect("save east");

    write_direction_meta(
        &project,
        &AnimationDirectionMeta {
            animation_id: west_id.clone(),
            direction_family: "walk".into(),
            facing: "w".into(),
            mirrored_from: None,
            anchor_slug: Some("hero".into()),
        },
    )
    .expect("west meta");
    write_direction_meta(
        &project,
        &AnimationDirectionMeta {
            animation_id: east_id.clone(),
            direction_family: "walk".into(),
            facing: "e".into(),
            mirrored_from: None,
            anchor_slug: Some("hero".into()),
        },
    )
    .expect("east meta");

    let contract =
        character_contract_check_inner(&state, &workspace.id, &worktree_id, Some("hero"))
            .expect("character contract");
    assert!(!contract.passed);
    assert!(!contract.cross_facing_violations.is_empty());

    let error = ensure_export_allowed(&state, &west_id).expect_err("export should be blocked");
    assert_eq!(error.code, "export_blocked");

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}
