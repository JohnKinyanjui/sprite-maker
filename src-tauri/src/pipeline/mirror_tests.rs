use super::facing::mirror_rgba_horizontal;
use super::mirror::{mirror_animation_inner, mirror_facing_pair};
use super::anchors::promote_anchor_inner;
use crate::{
    animations::save_animation_inner,
    assets::{inspect, upsert},
    database,
    models::{AnimationFrame, AnimationInput},
    workspace::create_workspace_inner,
    AppState,
};
use image::{Rgba, RgbaImage};
use uuid::Uuid;

fn fixture() -> (std::path::PathBuf, AppState) {
    let root = std::env::temp_dir().join(format!("sprite-mirror-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("temp dir");
    let connection = database::open(&root.join("app.sqlite3")).expect("db");
    (root, AppState::from_connection(connection))
}

#[test]
fn mirror_facing_pairs_are_reciprocal() {
    assert_eq!(mirror_facing_pair("w"), Some("e"));
    assert_eq!(mirror_facing_pair("e"), Some("w"));
    assert_eq!(mirror_facing_pair("ne"), Some("se"));
}

#[test]
fn mirror_rgba_preserves_alpha() {
    let mut image = RgbaImage::new(8, 8);
    image.put_pixel(2, 4, Rgba([10, 20, 30, 200]));
    let mirrored = mirror_rgba_horizontal(&image);
    assert_eq!(mirrored.get_pixel(5, 4), &Rgba([10, 20, 30, 200]));
}

#[test]
fn mirror_animation_creates_target_facing_copy() {
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
    let mut frame = RgbaImage::new(64, 64);
    frame.put_pixel(20, 63, Rgba([255, 0, 0, 255]));
    frame.save(&frame_path).expect("frame save");
    let frame_asset = inspect(&workspace.id, &project, &frame_path, None).expect("inspect");
    upsert(&state, &frame_asset, "test").expect("upsert");

    let anchor_path = project.join("assets").join("characters").join("hero.png");
    std::fs::create_dir_all(anchor_path.parent().unwrap()).expect("anchor dir");
    frame.save(&anchor_path).expect("anchor save");
    let anchor_asset = inspect(&workspace.id, &project, &anchor_path, None).expect("inspect anchor");
    upsert(&state, &anchor_asset, "test").expect("upsert anchor");
    promote_anchor_inner(&workspace.id, &anchor_asset.id, Some("hero"), false, None, &state)
        .expect("promote");

    let animation_id = Uuid::new_v4().to_string();
    save_animation_inner(
        AnimationInput {
            id: Some(animation_id.clone()),
            workspace_id: workspace.id.clone(),
            worktree_id: None,
            name: "walk-w".into(),
            fps: 10.0,
            looping: true,
            frames: vec![AnimationFrame {
                asset_id: frame_asset.id,
                duration_ms: Some(100),
                offset_x: 2,
                offset_y: 0,
            }],
            motion_plan: None,
            review_status: Some("draft".into()),
        },
        &state,
    )
    .expect("save animation");

    let mirrored = mirror_animation_inner(
        None,
        &state,
        crate::models::MirrorAnimationInput {
            animation_id,
            target_facing: "e".into(),
            source_facing: Some("w".into()),
            anchor_slug: Some("hero".into()),
            rig_id: None,
        },
    )
    .expect("mirror");

    assert_eq!(mirrored.target_facing, "e");
    assert_ne!(mirrored.mirrored_animation_id, mirrored.source_animation_id);

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}
