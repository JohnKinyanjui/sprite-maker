use super::character_profile::{
    export_character_profile_inner, get_character_profile_inner, import_character_profile_inner,
    profile_path, sync_character_profile_for_anchor,
};
use crate::{
    assets::{inspect, upsert},
    database,
    pipeline::anchors::promote_anchor_inner,
    workspace::create_workspace_inner,
    AppState,
};
use image::{Rgba, RgbaImage};
use uuid::Uuid;

fn fixture() -> (std::path::PathBuf, AppState) {
    let root = std::env::temp_dir().join(format!("sprite-character-profile-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("temp dir");
    let connection = database::open(&root.join("app.sqlite3")).expect("db");
    (root, AppState::from_connection(connection))
}

#[test]
fn character_profile_round_trip() {
    let (root, state) = fixture();
    let project = root.join("game");
    std::fs::create_dir_all(&project).expect("project");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");

    let anchor_path = project.join("assets").join("hero.png");
    std::fs::create_dir_all(anchor_path.parent().unwrap()).expect("dir");
    let mut frame = RgbaImage::new(64, 64);
    frame.put_pixel(32, 63, Rgba([255, 120, 80, 255]));
    frame.save(&anchor_path).expect("save");
    let anchor_asset = inspect(&workspace.id, &project, &anchor_path, None).expect("inspect");
    upsert(&state, &anchor_asset, "test").expect("upsert");
    promote_anchor_inner(&workspace.id, &anchor_asset.id, Some("hero"), false, None, &state)
        .expect("promote");

    let profile = sync_character_profile_for_anchor(&workspace.id, "hero", None, &state)
        .expect("sync profile");
    assert_eq!(profile.anchor_slug, "hero");
    assert!(!profile.palette.is_empty());
    assert!(!profile.d_hash.is_empty());
    assert!(profile_path(&project, "hero").is_file());

    let export_path = project.join("hero-profile-export.json");
    export_character_profile_inner(
        &workspace.id,
        "hero",
        export_path.to_string_lossy().as_ref(),
        &state,
    )
    .expect("export");
    assert!(export_path.is_file());

    let imported = import_character_profile_inner(
        &workspace.id,
        export_path.to_string_lossy().as_ref(),
        &state,
    )
    .expect("import");
    let reloaded = get_character_profile_inner(&workspace.id, "hero", &state).expect("reload");
    assert_eq!(imported.d_hash, reloaded.d_hash);
    assert_eq!(imported.palette, reloaded.palette);

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}
