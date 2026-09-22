use super::paint::{paint_frame_alpha_inner, restore_asset_version_inner};
use crate::{
    assets::{inspect, upsert},
    database,
    models::{BrushStamp, PaintFrameAlphaInput},
    workspace::create_workspace_inner,
    AppState,
};
use image::{Rgba, RgbaImage};
use uuid::Uuid;

fn fixture() -> (std::path::PathBuf, AppState) {
    let root = std::env::temp_dir().join(format!("sprite-paint-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("temp dir");
    let connection = database::open(&root.join("app.sqlite3")).expect("db");
    (root, AppState::from_connection(connection))
}

#[test]
fn paint_erase_reduces_opaque_pixels_and_restore_recovers_hash() {
    let (root, state) = fixture();
    let project = root.join("game");
    std::fs::create_dir_all(&project.join("assets/characters")).expect("assets");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");
    let root = crate::workspace::workspace_path(&state, &workspace.id).expect("root");
    let path = root.join("assets/characters/frame.png");
    let image = RgbaImage::from_pixel(16, 16, Rgba([200, 40, 40, 255]));
    image.save(&path).expect("save");
    let asset = inspect(&workspace.id, &root, &path, None).expect("inspect");
    upsert(&state, &asset, "imported").expect("upsert");
    let before_hash = std::fs::read(&asset.path).expect("read");

    let erased = paint_frame_alpha_inner(
        &state,
        PaintFrameAlphaInput {
            asset_id: asset.id.clone(),
            strokes: vec![BrushStamp {
                x: 8,
                y: 8,
                radius: 6,
            }],
            mode: "erase".to_string(),
            version_id: None,
        },
    )
    .expect("erase");
    assert!(erased.opaque_pixel_count < 16 * 16);

    let previous_version: String = {
        let connection = state.db.lock().expect("db");
        connection
            .query_row(
                "SELECT id FROM asset_versions WHERE asset_id = ?1 AND selected = 0 ORDER BY version_number DESC LIMIT 1",
                [&asset.id],
                |row| row.get(0),
            )
            .expect("previous version")
    };
    let restored = restore_asset_version_inner(&state, &asset.id, &previous_version)
        .expect("restore");
    let after_hash = std::fs::read(&restored.path).expect("read restored");
    assert_eq!(before_hash, after_hash);

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn paint_restore_requires_version_id() {
    let (root, state) = fixture();
    let project = root.join("game");
    std::fs::create_dir_all(&project.join("assets/characters")).expect("assets");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");
    let root = crate::workspace::workspace_path(&state, &workspace.id).expect("root");
    let path = root.join("assets/characters/frame.png");
    RgbaImage::from_pixel(8, 8, Rgba([10, 10, 10, 255]))
        .save(&path)
        .expect("save");
    let asset = inspect(&workspace.id, &root, &path, None).expect("inspect");
    upsert(&state, &asset, "imported").expect("upsert");
    let error = paint_frame_alpha_inner(
        &state,
        PaintFrameAlphaInput {
            asset_id: asset.id,
            strokes: vec![],
            mode: "restore".to_string(),
            version_id: None,
        },
    )
    .expect_err("restore without version");
    assert_eq!(error.code, "version_required");

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}
