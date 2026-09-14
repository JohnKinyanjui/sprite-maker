use super::frame_score::score_animation_frames_inner;
use super::strip::split_sprite_strip_inner;
use crate::{
    animations::save_animation_inner,
    database,
    models::{AnimationFrame, AnimationInput},
    workspace::create_workspace_inner,
    AppState,
};
use std::path::PathBuf;
use uuid::Uuid;

fn fixture() -> (PathBuf, AppState) {
    let root = std::env::temp_dir().join(format!("sprite-frame-score-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("temp dir");
    let connection = database::open(&root.join("app.sqlite3")).expect("db");
    (root, AppState::from_connection(connection))
}

#[test]
fn score_animation_frames_on_walk_fixture_strip() {
    let (root, state) = fixture();
    let project = root.join("game");
    std::fs::create_dir_all(&project).expect("project");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");

    let strip_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test-fixtures/strips/walk-4-horizontal.png");
    assert!(strip_path.is_file(), "missing walk strip fixture");

    let split = split_sprite_strip_inner(
        &workspace.id,
        strip_path.to_string_lossy().as_ref(),
        "horizontal",
        4,
        None,
        true,
        "characters",
        &state,
    )
    .expect("split strip");
    assert_eq!(split.asset_ids.len(), 4);

    let frames = split
        .asset_ids
        .into_iter()
        .map(|asset_id| AnimationFrame {
            asset_id,
            duration_ms: None,
            offset_x: 0,
            offset_y: 0,
        })
        .collect();
    let animation = save_animation_inner(
        AnimationInput {
            id: None,
            workspace_id: workspace.id.clone(),
            worktree_id: None,
            name: "walk".into(),
            fps: 10.0,
            looping: true,
            frames,
            motion_plan: None,
            review_status: Some("draft".into()),
        },
        &state,
    )
    .expect("save animation");

    let report = score_animation_frames_inner(&animation.id, &state).expect("score frames");
    assert_eq!(report.frame_count, 4);
    assert_eq!(report.frames.len(), 4);
    assert!(report.mean_score > 0.0);
    assert!(report.frames[0].motion_delta == 0.0);
    assert!(report.frames[1].motion_delta >= 0.0);

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}
