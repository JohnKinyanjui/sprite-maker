use super::snap_grid::{quantize_channel, quantize_rgba_palette, snap_animation_offsets_inner};
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

#[test]
fn quantize_channel_snaps_to_grid() {
    assert_eq!(quantize_channel(7, 4), 8);
    assert_eq!(quantize_channel(9, 4), 8);
    assert_eq!(quantize_channel(10, 4), 12);
}

#[test]
fn quantize_palette_reduces_color_steps() {
    let mut image = RgbaImage::new(2, 1);
    image.put_pixel(0, 0, Rgba([7, 17, 27, 255]));
    image.put_pixel(1, 0, Rgba([9, 19, 29, 255]));
    let quantized = quantize_rgba_palette(&image, 4);
    assert_eq!(quantized.get_pixel(0, 0)[0], 8);
    assert_eq!(quantized.get_pixel(1, 0)[0], 8);
}

#[test]
fn snap_offsets_round_to_grid() {
    let root = std::env::temp_dir().join(format!("sprite-snap-grid-{}", Uuid::new_v4()));
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
    let path = project.join("assets/characters/frame.png");
    RgbaImage::from_pixel(8, 8, Rgba([255, 0, 0, 255]))
        .save(&path)
        .expect("frame save");
    let asset = inspect(&workspace.id, &project, &path, None).expect("inspect");
    upsert(&state, &asset, "test").expect("upsert");
    let animation = save_animation_inner(
        AnimationInput {
            id: None,
            workspace_id: workspace.id.clone(),
            worktree_id: None,
            name: "Walk".into(),
            fps: 10.0,
            looping: true,
            frames: vec![AnimationFrame {
                asset_id: asset.id,
                duration_ms: None,
                offset_x: 3,
                offset_y: 5,
            }],
            motion_plan: None,
            review_status: Some("draft".into()),
        },
        &state,
    )
    .expect("save animation");

    let snapped = snap_animation_offsets_inner(&state, &animation.id, 2).expect("snap");
    assert_eq!(snapped.frames[0].offset_x, 4);
    assert_eq!(snapped.frames[0].offset_y, 6);

    drop(state);
    std::fs::remove_dir_all(root).expect("cleanup");
}
