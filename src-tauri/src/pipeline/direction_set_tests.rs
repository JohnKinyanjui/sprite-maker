use super::direction_meta::{list_direction_meta_for_worktree, write_direction_meta};
use super::direction_set::{apply_derived_facing_mirrors, queue_direction_set_inner};
use super::mirror::mirror_targets_for_set;
use crate::models::AnimationDirectionMeta;
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
use rusqlite::params;
use uuid::Uuid;

fn fixture() -> (std::path::PathBuf, AppState) {
    let root = std::env::temp_dir().join(format!("sprite-direction-set-{}", Uuid::new_v4()));
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
fn mirror_targets_for_eight_dir_include_west_from_east() {
    let pairs = mirror_targets_for_set("8", &["n", "e", "s", "ne", "nw"]);
    assert!(pairs.contains(&("e".to_string(), "w".to_string())));
}

#[test]
fn queue_direction_set_returns_job_id() {
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

    let queued = queue_direction_set_inner(
        crate::models::QueueDirectionSetInput {
            workspace_id: workspace.id.clone(),
            worktree_id,
            source_animation_id: animation_id,
            anchor_slug: "hero".into(),
            motion: "walk".into(),
            set: "4".into(),
            conversation_id: None,
        },
        None,
        &state,
    )
    .expect("queue");

    assert!(!queued.job_id.is_empty());
    assert!(queued.pending_facings.contains(&"n".to_string()));

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn apply_derived_facing_mirrors_creates_south_from_north() {
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

    let frame_path = project.join("assets").join("walk-n.png");
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

    let north_id = Uuid::new_v4().to_string();
    save_animation_inner(
        AnimationInput {
            id: Some(north_id.clone()),
            workspace_id: workspace.id.clone(),
            worktree_id: Some(worktree_id.clone()),
            name: "walk-n".into(),
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
            animation_id: north_id.clone(),
            direction_family: "walk".into(),
            facing: "n".into(),
            mirrored_from: None,
            anchor_slug: Some("hero".into()),
        },
    )
    .expect("north meta");

    let mirrored = apply_derived_facing_mirrors(
        None,
        &state,
        &crate::models::QueueDirectionSetInput {
            workspace_id: workspace.id.clone(),
            worktree_id: worktree_id.clone(),
            source_animation_id: north_id.clone(),
            anchor_slug: "hero".into(),
            motion: "walk".into(),
            set: "4".into(),
            conversation_id: None,
        },
        "4",
        "walk",
        &["n", "e"],
    )
    .expect("derived mirrors");

    assert_eq!(mirrored.len(), 1);
    let south_id = &mirrored[0];
    let south_meta = super::direction_meta::load_direction_meta(&state, &workspace.id, south_id)
        .expect("load south meta")
        .expect("south meta exists");
    assert_eq!(south_meta.facing, "s");
    assert_eq!(south_meta.mirrored_from.as_deref(), Some(north_id.as_str()));

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn list_direction_meta_returns_written_sidecars() {
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
    let animation_id = Uuid::new_v4().to_string();
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

    let empty = list_direction_meta_for_worktree(&state, &workspace.id, &worktree_id)
        .expect("list meta");
    assert!(empty.is_empty());

    {
        let connection = state.db.lock().expect("db lock");
        connection
            .execute(
                "INSERT INTO animations (id, workspace_id, worktree_id, name, fps, looping, frames_json, review_status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 10, 1, '[]', 'draft', datetime('now'), datetime('now'))",
                params![animation_id, workspace.id, worktree_id, "hero-walk-w"],
            )
            .expect("animation insert");
    }

    let listed = list_direction_meta_for_worktree(&state, &workspace.id, &worktree_id)
        .expect("list meta");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].facing, "w");

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}
