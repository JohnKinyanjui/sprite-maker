use super::{
    collect_workspace_rig_specs, generation_fingerprint, inspect, read_generation_manifest,
    recover_manifest_from_imagegen_sources, safe_category, scan_generation_assets_inner, upsert,
};
use crate::workspace::create_workspace_inner;
use crate::{database, AppState};
use image::{Rgba, RgbaImage};
use uuid::Uuid;

#[test]
fn manifest_uses_its_real_file_time_when_provider_timestamp_is_stale() {
    let root = std::env::temp_dir().join(format!("sprite-studio-manifest-test-{}", Uuid::new_v4()));
    let output = root.join("assets/characters/hero.png");
    std::fs::create_dir_all(output.parent().expect("asset parent")).expect("asset directory");
    std::fs::write(&output, b"placeholder").expect("asset file");
    let manifest_path = root.join(".sprite-studio/last-generation.json");
    std::fs::create_dir_all(manifest_path.parent().expect("manifest parent"))
        .expect("manifest directory");
    std::fs::write(
            &manifest_path,
            r#"{"name":"hero","category":"characters","fps":1,"files":["assets/characters/hero.png"],"generatedAt":"2020-01-01T00:00:00Z"}"#,
        )
        .expect("manifest file");

    let manifest = read_generation_manifest(&root)
        .expect("manifest should load")
        .expect("manifest should exist");
    let effective =
        chrono::DateTime::parse_from_rfc3339(&manifest.generated_at).expect("effective timestamp");
    let stale =
        chrono::DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z").expect("stale timestamp");
    assert!(effective > stale);

    std::fs::remove_dir_all(root).expect("test directory should be removed");
}

#[test]
fn generation_fingerprint_tracks_output_content_not_manifest_rewrites() {
    let root =
        std::env::temp_dir().join(format!("sprite-studio-fingerprint-test-{}", Uuid::new_v4()));
    let output = root.join("assets/creatures/rabbit.png");
    std::fs::create_dir_all(output.parent().expect("asset parent")).expect("asset directory");
    std::fs::write(&output, b"first rabbit").expect("asset file");
    let manifest = crate::models::GenerationManifest {
        kind: Some("sprite".into()),
        name: "rabbit-hop".into(),
        category: "creatures".into(),
        fps: 12.0,
        files: vec!["assets/creatures/rabbit.png".into()],
        generated_at: "2026-08-19T00:00:00Z".into(),
        rig: None,
        rig_id: None,
        source: None,
        quality: None,
        direction_family: None,
        facing: None,
        mirrored_from: None,
        anchor_slug: None,
    };

    let original = generation_fingerprint(&root, &manifest).expect("first fingerprint");
    let mut rewritten = manifest.clone();
    rewritten.generated_at = "2026-08-19T01:00:00Z".into();
    assert_eq!(
        original,
        generation_fingerprint(&root, &rewritten).expect("rewritten fingerprint")
    );

    std::fs::write(&output, b"repaired rabbit").expect("updated asset");
    assert_ne!(
        original,
        generation_fingerprint(&root, &rewritten).expect("updated fingerprint")
    );
    std::fs::remove_dir_all(root).expect("test directory should be removed");
}

#[test]
fn accepts_only_workspace_asset_categories() {
    assert_eq!(
        safe_category("Characters").expect("valid category"),
        "characters"
    );
    assert!(safe_category("../../outside").is_err());
    assert!(safe_category("misc").is_err());
}

#[test]
fn invalid_image_fails_without_becoming_an_asset() {
    let root = std::env::temp_dir().join(format!("sprite-studio-invalid-image-{}", Uuid::new_v4()));
    let asset_dir = root.join("assets/characters");
    std::fs::create_dir_all(&asset_dir).expect("asset directory should exist");
    let path = asset_dir.join("broken.png");
    std::fs::write(&path, b"not an image").expect("invalid fixture should write");
    let error = inspect("workspace", &root, &path, None).expect_err("invalid image must fail");
    assert!(matches!(
        error.code.as_str(),
        "invalid_image" | "filesystem_error"
    ));
    std::fs::remove_dir_all(root).expect("temporary fixture should be removable");
}

#[test]
fn scanning_changed_pixels_creates_a_new_asset_version() {
    let root = std::env::temp_dir().join(format!("sprite-studio-version-test-{}", Uuid::new_v4()));
    let asset_dir = root.join("assets/characters");
    std::fs::create_dir_all(&asset_dir).expect("asset directory should exist");
    let connection = database::open(&root.join("studio.sqlite3")).expect("database opens");
    connection.execute(
            "INSERT INTO projects(id,name,path,created_at,last_opened_at) VALUES ('project','Test',?1,'now','now')",
            [root.to_string_lossy().as_ref()],
        ).expect("project inserts");
    let state = AppState::from_connection(connection);
    let path = asset_dir.join("hero.png");
    RgbaImage::from_pixel(8, 8, Rgba([255, 0, 0, 255]))
        .save(&path)
        .expect("first image saves");
    let first = inspect("project", &root, &path, None).expect("first image inspects");
    upsert(&state, &first, "scanned").expect("first image indexes");
    upsert(&state, &first, "scanned").expect("unchanged image reindexes");
    RgbaImage::from_pixel(8, 8, Rgba([0, 255, 0, 255]))
        .save(&path)
        .expect("changed image saves");
    let changed =
        inspect("project", &root, &path, Some(first.id.clone())).expect("changed image inspects");
    upsert(&state, &changed, "scanned").expect("changed image indexes");
    let versions: i64 = state
        .db
        .lock()
        .expect("database lock")
        .query_row(
            "SELECT COUNT(*) FROM asset_versions WHERE asset_id=?1",
            [&first.id],
            |row| row.get(0),
        )
        .expect("versions count");
    assert_eq!(versions, 2);
    drop(state);
    std::fs::remove_dir_all(root).expect("temporary fixture should be removable");
}

#[test]
fn lists_workspace_mask_rig_specs_from_json_files() {
    let root = std::env::temp_dir().join(format!("sprite-studio-rig-specs-{}", Uuid::new_v4()));
    let rig_path = root.join(".sprite-studio/rigs/walk-cycle.json");
    std::fs::create_dir_all(rig_path.parent().expect("rig parent")).expect("rig directory");
    std::fs::write(
        &rig_path,
        r#"{"name":"Walk cycle","source":"assets/characters/hero.png","fps":10,"frames":[{"phase":"contact"},{"phase":"pass"}]}"#,
    )
    .expect("rig json");

    let specs = collect_workspace_rig_specs(&root).expect("rig specs should load");
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].name, "Walk cycle");
    assert_eq!(specs[0].frame_count, 2);
    assert_eq!(specs[0].source.as_deref(), Some("assets/characters/hero.png"));
    assert!(specs[0].relative_path.ends_with(".sprite-studio/rigs/walk-cycle.json"));

    std::fs::remove_dir_all(root).expect("temporary fixture should be removable");
}

#[test]
fn recover_manifest_prefers_newest_imagegen_source() {
    let root = std::env::temp_dir().join(format!("sprite-manifest-recover-{}", Uuid::new_v4()));
    let imagegen = root.join(".sprite-studio/imagegen-sources/fox");
    std::fs::create_dir_all(&imagegen).expect("imagegen dir");
    std::fs::create_dir_all(root.join("assets/characters")).expect("assets dir");
    let older = imagegen.join("draft.png");
    let newer = imagegen.join("master.png");
    RgbaImage::from_pixel(4, 4, Rgba([10, 10, 10, 255]))
        .save(&older)
        .expect("older save");
    std::thread::sleep(std::time::Duration::from_millis(5));
    RgbaImage::from_pixel(8, 8, Rgba([20, 20, 20, 255]))
        .save(&newer)
        .expect("newer save");

    let manifest = recover_manifest_from_imagegen_sources(&root)
        .expect("recover")
        .expect("manifest");
    assert_eq!(manifest.name, "master");
    assert!(manifest.source.as_deref().unwrap().contains("imagegen-sources"));
    assert!(manifest.files[0].starts_with("assets/characters/"));
    assert!(root.join(&manifest.files[0]).is_file());
    let rescanned = read_generation_manifest(&root).expect("manifest reload");
    assert!(rescanned.is_some(), "recovered manifest must pass validation on reload");

    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn scan_generation_propagates_normalize_errors() {
    let root = std::env::temp_dir().join(format!("sprite-scan-generation-{}", Uuid::new_v4()));
    let project = root.join("game");
    std::fs::create_dir_all(project.join("assets/characters")).expect("asset directory");
    std::fs::write(project.join("assets/characters/bad.png"), b"not-a-png").expect("corrupt png");
    std::fs::create_dir_all(project.join(".sprite-studio")).expect("studio directory");
    std::fs::write(
        project.join(".sprite-studio/last-generation.json"),
        r#"{"name":"bad","category":"characters","fps":12,"files":["assets/characters/bad.png"],"generatedAt":"2026-01-01T00:00:00Z"}"#,
    )
    .expect("manifest");
    let connection = database::open(&root.join("app.sqlite3")).expect("database");
    let state = AppState::from_connection(connection);
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");
    let error = scan_generation_assets_inner(&workspace.id, None, None, &state)
        .expect_err("invalid png should fail scan");
    assert!(!error.message.is_empty());
    drop(state);
    std::fs::remove_dir_all(root).expect("temporary fixture should be removable");
}
