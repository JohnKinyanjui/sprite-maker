use super::motion_presets::{
    default_motion_presets, list_missing_motions_inner, load_motion_presets_inner,
    motion_presets_path, resolve_batch_presets, save_motion_presets_inner,
};
use super::test_fixtures::{
    copy_walk_strip_fixture, save_walk_animation_from_split, setup_hero_workspace,
    temp_pipeline_state,
};
use super::{
    contract::size_contract_check_inner, direction_meta::write_direction_meta,
    normalize::normalize_animation_inner, strip::split_sprite_strip_inner,
};
use crate::{
    database,
    models::{AnimationDirectionMeta, NormalizeAnimationInput},
    workspace::create_workspace_inner,
    AppState,
};
use uuid::Uuid;

fn fixture() -> (std::path::PathBuf, AppState) {
    let root = std::env::temp_dir().join(format!("sprite-motion-presets-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("temp dir");
    let connection = database::open(&root.join("app.sqlite3")).expect("db");
    (root, AppState::from_connection(connection))
}

#[test]
fn default_motion_presets_include_core_motions() {
    let catalog = default_motion_presets();
    assert_eq!(catalog.version, 2);
    assert!(catalog.presets.len() >= 13);
    let motions = catalog
        .presets
        .iter()
        .map(|preset| preset.motion.as_str())
        .collect::<Vec<_>>();
    assert!(motions.contains(&"idle"));
    assert!(motions.contains(&"walk"));
    assert!(motions.contains(&"run"));
    assert!(motions.contains(&"attack"));
    assert!(motions.contains(&"hurt"));
    assert!(motions.contains(&"jump"));
    assert!(motions.contains(&"crouch"));
    assert!(motions.contains(&"cast"));
    let categories = catalog
        .presets
        .iter()
        .map(|preset| preset.category.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert!(categories.contains("locomotion"));
    assert!(categories.contains("combat"));
    assert!(categories.contains("reaction"));
    assert!(categories.contains("interaction"));
}

#[test]
fn motion_presets_round_trip() {
    let (root, state) = fixture();
    let project = root.join("game");
    std::fs::create_dir_all(&project).expect("project");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");

    let loaded = load_motion_presets_inner(&state, &workspace.id).expect("load default");
    assert_eq!(loaded.presets.len(), 13);

    let mut custom = loaded.clone();
    custom.presets[0].frame_count = 5;
    save_motion_presets_inner(&state, &workspace.id, &custom).expect("save");
    let path = motion_presets_path(&project);
    assert!(path.is_file());

    let reloaded = load_motion_presets_inner(&state, &workspace.id).expect("reload");
    assert_eq!(reloaded.presets[0].frame_count, 5);

    let selected = resolve_batch_presets(&reloaded, &["walk".into()]);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].motion, "walk");

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn motion_presets_v1_catalog_migrates_to_v2() {
    let (root, state) = fixture();
    let project = root.join("game");
    std::fs::create_dir_all(project.join(".sprite-studio")).expect("studio dir");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");

    let v1 = serde_json::json!({
        "version": 1,
        "presets": [
            {"id":"idle","label":"Idle","motion":"idle","frameCount":4,"fps":8.0,"looping":true,"enabled":false},
            {"id":"walk","label":"Walk","motion":"walk","frameCount":6,"fps":10.0,"looping":true,"enabled":true},
            {"id":"attack","label":"Attack","motion":"attack","frameCount":6,"fps":12.0,"looping":false,"enabled":true}
        ]
    });
    std::fs::write(
        motion_presets_path(&project),
        serde_json::to_vec_pretty(&v1).expect("serialize"),
    )
    .expect("write v1");

    let loaded = load_motion_presets_inner(&state, &workspace.id).expect("load migrated");
    assert_eq!(loaded.version, 2);
    assert!(loaded.presets.len() >= 13);
    assert!(
        loaded.presets.iter().any(|preset| preset.id == "cast"),
        "migration should append missing default presets"
    );
    assert!(
        loaded.presets.iter().all(|preset| !preset.category.is_empty()),
        "every preset should have a category after migration"
    );
    let attack = loaded
        .presets
        .iter()
        .find(|preset| preset.id == "attack")
        .expect("attack preset");
    assert_eq!(attack.category, "combat");
    let idle = loaded
        .presets
        .iter()
        .find(|preset| preset.id == "idle")
        .expect("idle preset");
    assert!(!idle.enabled, "migration should preserve user enabled flags");

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn list_missing_motions_excludes_passing_walk() {
    let (root, state) = temp_pipeline_state();
    let (workspace, project) = setup_hero_workspace(&root, &state);
    let worktree_id = "wt-missing";
    {
        let connection = state.db.lock().expect("db");
        connection
            .execute(
                "INSERT INTO worktrees (id, project_id, name, slug, kind, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL, datetime('now'), datetime('now'))",
                rusqlite::params![worktree_id, workspace.id, "Hero", "hero", "character"],
            )
            .expect("worktree");
    }

    let strip_path = project.join("assets/imports/walk-strip.png");
    copy_walk_strip_fixture(&strip_path);
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
    .expect("split");
    let animation = save_walk_animation_from_split(&workspace.id, split.asset_ids, &state);
    {
        let connection = state.db.lock().expect("db");
        connection
            .execute(
                "UPDATE animations SET worktree_id=?1 WHERE id=?2",
                rusqlite::params![worktree_id, animation.id],
            )
            .expect("attach worktree");
    }
    write_direction_meta(
        &project,
        &AnimationDirectionMeta {
            animation_id: animation.id.clone(),
            direction_family: "walk".into(),
            facing: "w".into(),
            mirrored_from: None,
            anchor_slug: Some("hero".into()),
        },
    )
    .expect("direction meta");
    normalize_animation_inner(
        None,
        &state,
        NormalizeAnimationInput {
            animation_id: animation.id.clone(),
            anchor_slug: Some("hero".into()),
            lock_first_frame: Some(true),
            shared_scale: Some(true),
            padding: None,
        },
    )
    .expect("normalize");
    let contract =
        size_contract_check_inner(&state, &animation.id, Some("hero")).expect("contract");
    assert!(contract.passed, "walk should pass contract for missing-motion test");

    let missing =
        list_missing_motions_inner(&state, &workspace.id, worktree_id, "hero").expect("missing");
    assert!(
        !missing.missing_motions.iter().any(|motion| motion == "walk"),
        "walk with passing canonical facing should not be missing"
    );
    assert!(
        missing.missing_motions.iter().any(|motion| motion == "idle"),
        "idle should still be missing"
    );

    drop(state);
    std::fs::remove_dir_all(root).expect("cleanup");
}
