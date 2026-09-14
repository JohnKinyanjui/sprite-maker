use super::sessions::{
    append_generation_session, list_generation_sessions_inner, sessions_index_path,
};
use super::ManifestSessionHook;
use crate::assets::write_generation_manifest;
use crate::models::GenerationManifest;
use super::test_fixtures::{setup_hero_workspace, temp_pipeline_state};
use crate::workspace::workspace_path;

#[test]
fn append_generation_session_writes_index() {
    let (root, state) = temp_pipeline_state();
    let (workspace, project) = setup_hero_workspace(&root, &state);
    let worktree_id = "wt-sessions";

    append_generation_session(
        &project,
        worktree_id,
        "harden",
        Some("anim-1"),
        ".sprite-studio/last-generation.json",
        Some("assets/characters/hero.png"),
        Some(72.0),
    )
    .expect("append session");

    let index_path = sessions_index_path(&project);
    assert!(index_path.is_file());

    let sessions =
        list_generation_sessions_inner(&state, &workspace.id, Some(worktree_id)).expect("list");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].kind, "harden");
    assert_eq!(sessions[0].animation_id.as_deref(), Some("anim-1"));
    assert_eq!(sessions[0].score, Some(72.0));

    let root_path = workspace_path(&state, &workspace.id).expect("workspace path");
    assert_eq!(
        root_path.canonicalize().expect("canonical workspace"),
        project.canonicalize().expect("canonical project"),
    );

    drop(state);
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn write_generation_manifest_appends_session_index() {
    let (root, state) = temp_pipeline_state();
    let (workspace, project) = setup_hero_workspace(&root, &state);
    let worktree_id = "wt-manifest-session";
    std::fs::create_dir_all(project.join("assets/characters")).expect("asset dir");
    std::fs::write(project.join("assets/characters/hero.png"), b"png").expect("asset");

    write_generation_manifest(
        &project,
        &GenerationManifest {
            kind: Some("sprite".into()),
            name: "hero".into(),
            category: "characters".into(),
            fps: 8.0,
            files: vec!["assets/characters/hero.png".into()],
            generated_at: chrono::Utc::now().to_rfc3339(),
            rig: None,
            rig_id: None,
            source: None,
            quality: None,
            direction_family: None,
            facing: None,
            mirrored_from: None,
            anchor_slug: None,
        },
        Some(&ManifestSessionHook {
            worktree_id: worktree_id.into(),
            kind: "generation".into(),
            animation_id: None,
            score: None,
        }),
    )
    .expect("write manifest");

    let sessions =
        list_generation_sessions_inner(&state, &workspace.id, Some(worktree_id)).expect("list");
    assert!(!sessions.is_empty());
    assert_eq!(sessions[0].kind, "generation");
    assert_eq!(sessions[0].thumbnail_path.as_deref(), Some("assets/characters/hero.png"));

    drop(state);
    std::fs::remove_dir_all(root).expect("cleanup");
}
