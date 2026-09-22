use super::strip::subsample_video_frames_inner;
use crate::{
    assets::{inspect, upsert},
    database,
    workspace::create_workspace_inner,
    AppState,
};
use image::{Rgba, RgbaImage};
use uuid::Uuid;

fn fixture() -> (std::path::PathBuf, AppState) {
    let root = std::env::temp_dir().join(format!("sprite-subsample-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("temp dir");
    let connection = database::open(&root.join("app.sqlite3")).expect("db");
    (root, AppState::from_connection(connection))
}

fn write_frame(path: &std::path::Path, fill: u8) {
    let image = RgbaImage::from_pixel(8, 8, Rgba([fill, fill, fill, 255]));
    image.save(path).expect("save frame");
}

#[test]
fn subsample_keeps_first_last_and_changed_frame() {
    let (root, state) = fixture();
    let project = root.join("game");
    std::fs::create_dir_all(&project).expect("project");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");
    let frames_dir = project.join("assets").join("imports").join("test-frames");
    std::fs::create_dir_all(&frames_dir).expect("frames dir");
    let mut asset_ids = Vec::new();
    for index in 0..8 {
        let fill = if index == 4 { 200 } else { 40 };
        let path = frames_dir.join(format!("frame-{:02}.png", index + 1));
        write_frame(&path, fill);
        let asset = inspect(&workspace.id, &project, &path, None).expect("inspect");
        upsert(&state, &asset, "imported").expect("upsert");
        asset_ids.push(asset.id);
    }
    let result = subsample_video_frames_inner(&state, &workspace.id, &asset_ids, 0.02, 1)
        .expect("subsample");
    assert_eq!(result.kept_indices.first().copied(), Some(0));
    assert_eq!(result.kept_indices.last().copied(), Some(7));
    assert!(result.kept_indices.contains(&4));
    assert!(result.kept_indices.len() < asset_ids.len());
}
