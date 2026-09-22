use super::character_pack::export_character_pack_inner;
use crate::{
    animations::save_animation_inner,
    assets::{inspect, upsert},
    database,
    models::{AnimationFrame, AnimationInput, ExportCharacterPackInput},
    pipeline::anchors::promote_anchor_inner,
    workspace::create_workspace_inner,
    AppState,
};
use image::{Rgba, RgbaImage};
use uuid::Uuid;

fn fixture() -> (std::path::PathBuf, AppState) {
    let root = std::env::temp_dir().join(format!("sprite-character-pack-{}", Uuid::new_v4()));
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

#[test]
fn export_character_pack_writes_manifest_and_sheets() {
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

    let frame_dir = project.join("assets/characters");
    std::fs::create_dir_all(&frame_dir).expect("frame dir");
    let frame_path = frame_dir.join("hero.png");
    let mut frame_image = RgbaImage::new(64, 64);
    frame_image.put_pixel(32, 63, Rgba([255, 0, 0, 255]));
    frame_image.save(&frame_path).expect("frame save");
    let frame_asset = inspect(&workspace.id, &project, &frame_path, None).expect("inspect");
    upsert(&state, &frame_asset, "test").expect("upsert");
    promote_anchor_inner(&workspace.id, &frame_asset.id, Some("hero"), false, None, &state)
        .expect("promote");

    save_animation_inner(
        AnimationInput {
            id: None,
            workspace_id: workspace.id.clone(),
            worktree_id: Some(worktree_id.clone()),
            name: "hero-walk-w".into(),
            fps: 10.0,
            looping: true,
            frames: vec![AnimationFrame {
                asset_id: frame_asset.id.clone(),
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

    let result = export_character_pack_inner(
        &state,
        &ExportCharacterPackInput {
            workspace_id: workspace.id.clone(),
            worktree_id: worktree_id.clone(),
            destination: "exports/character-pack".into(),
            anchor_slug: None,
            metadata_format: Some("sprite-studio".into()),
            include_animated_previews: None,
        },
    )
    .expect("export pack");

    assert_eq!(result.animation_count, 1);
    assert!(std::path::Path::new(&result.manifest_path).is_file());
    assert!(std::path::Path::new(&result.anchor_path).is_file());
    assert!(!result.animations.is_empty());

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn export_character_pack_writes_gif_preview_when_requested() {
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

    let frame_dir = project.join("assets/characters");
    std::fs::create_dir_all(&frame_dir).expect("frame dir");
    let frame_paths = ["hero-a.png", "hero-b.png"];
    let mut asset_ids = Vec::new();
    for file_name in frame_paths {
        let frame_path = frame_dir.join(file_name);
        let mut frame_image = RgbaImage::new(64, 64);
        frame_image.put_pixel(32, 63, Rgba([255, 0, 0, 255]));
        frame_image.save(&frame_path).expect("frame save");
        let frame_asset = inspect(&workspace.id, &project, &frame_path, None).expect("inspect");
        upsert(&state, &frame_asset, "test").expect("upsert");
        asset_ids.push(frame_asset.id);
    }
    promote_anchor_inner(&workspace.id, &asset_ids[0], Some("hero"), false, None, &state)
        .expect("promote");

    save_animation_inner(
        AnimationInput {
            id: None,
            workspace_id: workspace.id.clone(),
            worktree_id: Some(worktree_id.clone()),
            name: "hero-walk-w".into(),
            fps: 10.0,
            looping: true,
            frames: asset_ids
                .into_iter()
                .map(|asset_id| AnimationFrame {
                    asset_id,
                    duration_ms: None,
                    offset_x: 0,
                    offset_y: 0,
                })
                .collect(),
            motion_plan: None,
            review_status: Some("draft".into()),
        },
        &state,
    )
    .expect("save animation");

    let result = export_character_pack_inner(
        &state,
        &ExportCharacterPackInput {
            workspace_id: workspace.id.clone(),
            worktree_id: worktree_id.clone(),
            destination: "exports/character-pack-gif".into(),
            anchor_slug: None,
            metadata_format: Some("sprite-studio".into()),
            include_animated_previews: Some(true),
        },
    )
    .expect("export pack");

    let gif_path = result
        .animations
        .first()
        .and_then(|entry| entry.preview_animated.clone())
        .expect("preview animated path");
    assert!(
        std::path::Path::new(&result.directory_path).join(gif_path).is_file(),
        "gif preview should exist"
    );

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}
