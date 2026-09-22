use super::{
    attach_references, export_item, resolved_provider, studio_status, style_context_line,
    AttachReferencesParams, ExportParams, SpriteStudioMcp, BUILTIN_STYLES,
};
use crate::{
    conversations::create_conversation_inner, database, models::ProviderEvent,
    settings::set_setting_value, workspace::create_workspace_inner, AppState,
};
use rmcp::ServerHandler;
use serde_json::Value;
use std::path::PathBuf;
use uuid::Uuid;

fn fixture() -> (PathBuf, AppState) {
    let root = std::env::temp_dir().join(format!("sprite-studio-mcp-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("temp dir");
    let connection = database::open(&root.join("app.sqlite3")).expect("db");
    (root, AppState::from_connection(connection))
}

#[test]
fn default_generation_provider_is_codex() {
    assert_eq!(resolved_provider(None), "codex");
    assert_eq!(resolved_provider(Some("cursor")), "cursor");
    assert_eq!(resolved_provider(Some("")), "codex");
}

#[test]
fn style_context_includes_art_direction_line() {
    let (root, state) = fixture();
    let project = root.join("game");
    std::fs::create_dir_all(&project).expect("project");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");
    let conversation = create_conversation_inner(
        workspace.id.clone(),
        None,
        Some("Test".into()),
        Some("codex".into()),
        &state,
    )
    .expect("conversation");
    set_setting_value(
        &state,
        &format!("conversation-style:{}", conversation.id),
        Value::String("nes-eight-bit".into()),
    )
    .expect("style");
    let line = style_context_line(&state, &conversation).expect("context");
    assert!(
        line.starts_with("Selected art direction: NES 8-bit."),
        "{line}"
    );
    assert!(line.contains("four-color"));
    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn status_json_omits_api_keys() {
    let (root, state) = fixture();
    let payload = studio_status(&state, &root.join("app.sqlite3")).expect("status");
    let json = serde_json::to_value(&payload).expect("json");
    let encoded = json.to_string();
    assert!(!encoded.contains("apiKey"));
    assert!(!encoded.contains("CURSOR_API_KEY"));
    assert!(payload.providers.iter().all(|provider| provider
        .id
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch == '-')));
    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn generation_snapshot_tracks_progress_without_spawning() {
    let (root, state) = fixture();
    state.track_generation("req-1", "chat-1", "msg-1", "ws-1");
    state.record_provider_event(&ProviderEvent {
        request_id: "req-1".into(),
        conversation_id: "chat-1".into(),
        event_type: "activity".into(),
        content: "Codex CLI is working".into(),
    });
    state.record_provider_event(&ProviderEvent {
        request_id: "req-1".into(),
        conversation_id: "chat-1".into(),
        event_type: "completed".into(),
        content: "Done".into(),
    });
    let snapshot = state.generation_snapshot("req-1").expect("snapshot");
    assert_eq!(snapshot.status, "completed");
    assert_eq!(
        snapshot.last_activity.as_deref(),
        Some("Codex CLI is working")
    );
    assert_eq!(snapshot.last_content.as_deref(), Some("Done"));
    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn builtin_styles_cover_named_presets() {
    assert!(BUILTIN_STYLES.iter().any(|(id, _, _)| *id == "pixel-rpg"));
    assert!(BUILTIN_STYLES
        .iter()
        .any(|(id, _, _)| *id == "top-down-adventure"));
}

#[test]
fn tool_router_exposes_pipeline_tools() {
    let mut names: Vec<String> = SpriteStudioMcp::listed_tool_names();
    names.sort();
    assert_eq!(
        names,
        [
            "align_frames",
            "analyze_rig_fit",
            "attach_references",
            "cancel_generation",
            "check_anchor_facing",
            "check_character_contract",
            "check_size_contract",
            "clean_alpha",
            "ensure_conversation",
            "export",
            "export_character_pack",
            "extract_video_frames",
            "finalize_contract_retry",
            "generate",
            "get_anchor",
            "get_character_profile",
            "get_generation",
            "get_job",
            "get_production_score",
            "harden_animation",
            "interpolate_rig_frames",
            "list_anchors",
            "list_artifacts",
            "list_assets",
            "list_facing_checks",
            "list_packs",
            "mirror_animation",
            "normalize_animation",
            "open_workspace",
            "orient_anchor",
            "promote_anchor",
            "quality_report",
            "queue_contract_retry",
            "queue_direction_set",
            "queue_motion_batch",
            "queue_procedural_vfx",
            "queue_region_regen",
            "queue_sprite_sheet",
            "render_rig_animation",
            "retry_size_contract",
            "save_rig",
            "score_animation_frames",
            "score_strip",
            "set_animation_review_status",
            "snap_to_pixel_grid",
            "split_strip",
            "studio_status",
            "suggest_rig_points",
        ]
    );
}

#[test]
fn server_info_distinguishes_this_mcp_from_the_rig_helper() {
    let (root, state) = fixture();
    let server = SpriteStudioMcp {
        state,
        db_path: root.join("app.sqlite3"),
    };
    let info = server.get_info();
    assert_eq!(info.server_info.name, "sprite-studio");
    let instructions = info.instructions.unwrap_or_default();
    assert!(instructions.contains("Codex"));
    assert!(instructions.contains("sprite_rig_mcp.py"));
    drop(server);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn attach_references_imports_a_local_png() {
    let (root, state) = fixture();
    let project = root.join("game");
    std::fs::create_dir_all(&project).expect("project");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");
    let worktrees =
        crate::worktrees::list_worktrees_inner(&workspace.id, &state).expect("worktrees");
    let general = worktrees
        .iter()
        .find(|worktree| worktree.kind == "general")
        .expect("general worktree");
    let conversation = create_conversation_inner(
        workspace.id,
        Some(general.id.clone()),
        Some("Test".into()),
        Some("codex".into()),
        &state,
    )
    .expect("conversation");
    let png = project.join("slime.png");
    image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 255, 0, 255]))
        .save(&png)
        .expect("png");
    let ids = attach_references(
        &state,
        AttachReferencesParams {
            conversation_id: conversation.id,
            paths: Some(vec![png.to_string_lossy().into_owned()]),
            reference_ids: None,
        },
    )
    .expect("attach");
    assert_eq!(ids.len(), 1);
    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn export_rejects_unknown_kind() {
    let (root, state) = fixture();
    let error = export_item(
        &state,
        ExportParams {
            kind: "sheet".into(),
            id: "missing".into(),
            destination: None,
            project_id: None,
            worktree_id: None,
            name: None,
            tile_width: None,
            tile_height: None,
            export_format: None,
        },
    )
    .expect_err("unknown kind");
    assert_eq!(error.code, "invalid_export");
    drop(state);
    let _ = std::fs::remove_dir_all(root);
}
