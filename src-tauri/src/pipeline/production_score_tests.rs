use super::production_score::production_score_inner;
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
    let root = std::env::temp_dir().join(format!("sprite-production-score-{}", Uuid::new_v4()));
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
fn production_score_aggregates_passing_contracts() {
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

    let report = production_score_inner(&state, &workspace.id, &worktree_id, None)
        .expect("production score");
    assert_eq!(report.animation_count, 1);
    assert!(report.overall_score > 0.0);
    assert!(report.size_contract_score >= 0.0);

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}
