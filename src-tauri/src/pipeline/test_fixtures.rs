use super::anchors::promote_anchor_inner;
use crate::{
    animations::save_animation_inner,
    assets::{inspect, upsert},
    models::{Animation, AnimationFrame, AnimationInput, Workspace},
    workspace::create_workspace_inner,
    AppState,
};
use image::{Rgba, RgbaImage};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(crate) fn strips_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/strips")
}

pub(crate) fn walk_strip_fixture_path() -> PathBuf {
    strips_fixture_dir().join("walk-4-horizontal.png")
}

pub(crate) fn list_strip_fixtures() -> Vec<PathBuf> {
    let dir = strips_fixture_dir();
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut fixtures = std::fs::read_dir(&dir)
        .expect("read strip fixtures dir")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("png") {
                Some(path)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    fixtures.sort();
    fixtures
}

pub(crate) fn build_walk_strip(path: &Path, frames: u32, cell_width: u32) {
    let height = 32_u32;
    let width = cell_width * frames;
    let mut image = RgbaImage::new(width, height);
    for frame in 0..frames {
        let x = frame * cell_width + cell_width / 2;
        image.put_pixel(x, 30, Rgba([255, 0, 0, 255]));
    }
    image.save(path).expect("strip save");
}

pub(crate) fn copy_walk_strip_fixture(destination: &Path) {
    let fixture = walk_strip_fixture_path();
    assert!(
        fixture.is_file(),
        "missing committed fixture at {}; run materialize_walk_strip_fixture",
        fixture.display()
    );
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).expect("imports dir");
    }
    std::fs::copy(&fixture, destination).expect("copy walk strip fixture");
}

pub(crate) fn temp_pipeline_state() -> (PathBuf, AppState) {
    let root = std::env::temp_dir().join(format!("sprite-pipeline-fixture-{}", Uuid::new_v4()));
    let connection = crate::database::open(&root.join("app.sqlite3")).expect("db");
    (root, AppState::from_connection(connection))
}

pub(crate) fn setup_hero_workspace(root: &Path, state: &AppState) -> (Workspace, PathBuf) {
    let project = root.join("game");
    std::fs::create_dir_all(&project.join("assets/characters")).expect("asset dir");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        state,
    )
    .expect("workspace");

    let anchor_path = project.join("assets/characters/hero.png");
    let mut anchor_image = RgbaImage::new(32, 32);
    anchor_image.put_pixel(16, 31, Rgba([255, 255, 255, 255]));
    anchor_image.save(&anchor_path).expect("anchor save");
    let anchor_asset = inspect(&workspace.id, &project, &anchor_path, None).expect("inspect");
    upsert(state, &anchor_asset, "test").expect("upsert");
    promote_anchor_inner(&workspace.id, &anchor_asset.id, Some("hero"), false, None, state)
        .expect("promote");

    (workspace, project)
}

pub(crate) fn save_walk_animation_from_split(
    workspace_id: &str,
    asset_ids: Vec<String>,
    state: &AppState,
) -> Animation {
    save_animation_inner(
        AnimationInput {
            id: None,
            workspace_id: workspace_id.to_string(),
            worktree_id: None,
            name: "Walk".into(),
            fps: 8.0,
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
        state,
    )
    .expect("save animation")
}
