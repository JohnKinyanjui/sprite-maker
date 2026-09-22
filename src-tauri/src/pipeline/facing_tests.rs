use super::anchors::promote_anchor_inner;
use super::facing::{
    canonical_facing_for_view, detect_anchor_facing_inner, detect_facing_from_image,
    list_facing_checks_inner, mirror_rgba_horizontal, normalize_view, orient_anchor_inner,
};
use crate::{
    assets::{inspect, upsert},
    database,
    workspace::create_workspace_inner,
    AppState,
};
use image::{Rgba, RgbaImage};
use uuid::Uuid;

fn fixture() -> (std::path::PathBuf, AppState) {
    let root = std::env::temp_dir().join(format!("sprite-facing-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("temp dir");
    let connection = database::open(&root.join("app.sqlite3")).expect("db");
    (root, AppState::from_connection(connection))
}

fn write_side_sprite(path: &std::path::Path, facing_left: bool) {
    let mut image = RgbaImage::new(32, 32);
    let x_range = if facing_left { 4..12 } else { 20..28 };
    for y in 8..28 {
        for x in x_range.clone() {
            image.put_pixel(x, y, Rgba([200, 80, 40, 255]));
        }
    }
    image.save(path).expect("save sprite");
}

#[test]
fn detect_facing_side_view_left_and_right() {
    let left = RgbaImage::from_fn(32, 32, |x, y| {
        if x < 14 && y > 6 {
            Rgba([255, 255, 255, 255])
        } else {
            Rgba([0, 0, 0, 0])
        }
    });
    let right = mirror_rgba_horizontal(&left);
    let (left_facing, left_confidence) = detect_facing_from_image(&left, "side");
    let (right_facing, right_confidence) = detect_facing_from_image(&right, "side");
    assert_eq!(left_facing, "w");
    assert_eq!(right_facing, "e");
    assert!(left_confidence > 0.2);
    assert!(right_confidence > 0.2);
}

#[test]
fn promote_with_auto_orient_flips_e_facing_master() {
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
    write_side_sprite(&anchor_path, false);
    let anchor_asset = inspect(&workspace.id, &project, &anchor_path, None).expect("inspect");
    upsert(&state, &anchor_asset, "test").expect("upsert");

    let anchor = promote_anchor_inner(&workspace.id, &anchor_asset.id, Some("hero"), true, Some("side"), &state)
        .expect("promote with auto orient");
    assert_eq!(anchor.facing.as_deref(), Some("w"));
    assert_eq!(anchor.facing_status.as_deref(), Some("auto_oriented"));

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn orient_anchor_flips_e_to_canonical_w() {
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
    write_side_sprite(&anchor_path, false);
    let anchor_asset = inspect(&workspace.id, &project, &anchor_path, None).expect("inspect");
    upsert(&state, &anchor_asset, "test").expect("upsert");
    promote_anchor_inner(&workspace.id, &anchor_asset.id, Some("hero"), false, None, &state)
        .expect("promote");

    let oriented = orient_anchor_inner(&workspace.id, "hero", &state).expect("orient");
    assert_eq!(oriented.facing.as_deref(), Some("w"));
    assert_eq!(oriented.facing_status.as_deref(), Some("auto_oriented"));

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn list_facing_checks_returns_history_for_slug() {
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
    write_side_sprite(&anchor_path, true);
    let anchor_asset = inspect(&workspace.id, &project, &anchor_path, None).expect("inspect");
    upsert(&state, &anchor_asset, "test").expect("upsert");
    promote_anchor_inner(&workspace.id, &anchor_asset.id, Some("hero"), false, None, &state)
        .expect("promote");

    detect_anchor_facing_inner(&workspace.id, "hero", &state).expect("detect");
    let history = list_facing_checks_inner(&workspace.id, Some("hero"), &state)
        .expect("list facing checks");
    assert!(!history.is_empty());
    assert_eq!(history[0].slug, "hero");

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn view_aliases_and_canonical_facings_are_stable() {
    assert_eq!(normalize_view("side_platformer"), "side");
    assert_eq!(normalize_view("top-down"), "top_down");
    assert_eq!(canonical_facing_for_view("rts_oblique"), "w");
    assert_eq!(canonical_facing_for_view("top_down"), "n");
}
