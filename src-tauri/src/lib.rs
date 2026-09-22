mod animations;
mod assets;
mod pipeline;
mod backups;
mod conversations;
mod database;
mod error;
mod jobs;
mod mcp;
mod models;
mod motion_planner;
mod packs;
mod providers;
mod quality;
mod references;
mod rig;
mod settings;
mod sprite_harness;
mod templates;
mod terrain;
mod workspace;
mod worktrees;

use crate::models::ProviderEvent;
use serde::Serialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use providers::RefineCancelEntry;
use tauri::{AppHandle, Manager};
use tokio::sync::oneshot;

const APP_IDENTIFIER: &str = "com.jakes.sprite-maker";

#[derive(Clone)]
pub struct AppState {
    db: Arc<Mutex<rusqlite::Connection>>,
    cancellers: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
    refine_cancellers: Arc<Mutex<HashMap<String, RefineCancelEntry>>>,
    generations: Arc<Mutex<HashMap<String, GenerationSnapshot>>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GenerationSnapshot {
    pub request_id: String,
    pub conversation_id: String,
    pub assistant_id: String,
    pub workspace_id: String,
    pub status: String,
    pub last_activity: Option<String>,
    pub last_content: Option<String>,
}

impl AppState {
    pub(crate) fn from_connection(connection: rusqlite::Connection) -> Self {
        Self {
            db: Arc::new(Mutex::new(connection)),
            cancellers: Arc::new(Mutex::new(HashMap::new())),
            refine_cancellers: Arc::new(Mutex::new(HashMap::new())),
            generations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn open_headless() -> error::CommandResult<(Self, PathBuf)> {
        let db_path = sqlite_path();
        let connection = database::open_shared(&db_path)?;
        Ok((Self::from_connection(connection), db_path))
    }

    pub(crate) fn track_generation(
        &self,
        request_id: &str,
        conversation_id: &str,
        assistant_id: &str,
        workspace_id: &str,
    ) {
        if let Ok(mut map) = self.generations.lock() {
            map.insert(
                request_id.to_string(),
                GenerationSnapshot {
                    request_id: request_id.to_string(),
                    conversation_id: conversation_id.to_string(),
                    assistant_id: assistant_id.to_string(),
                    workspace_id: workspace_id.to_string(),
                    status: "started".into(),
                    last_activity: None,
                    last_content: None,
                },
            );
        }
    }

    pub(crate) fn record_provider_event(&self, event: &ProviderEvent) {
        let Ok(mut map) = self.generations.lock() else {
            return;
        };
        let entry = map
            .entry(event.request_id.clone())
            .or_insert_with(|| GenerationSnapshot {
                request_id: event.request_id.clone(),
                conversation_id: event.conversation_id.clone(),
                assistant_id: String::new(),
                workspace_id: String::new(),
                status: "started".into(),
                last_activity: None,
                last_content: None,
            });
        entry.conversation_id = event.conversation_id.clone();
        match event.event_type.as_str() {
            "activity" => entry.last_activity = Some(event.content.clone()),
            "content" => entry.last_content = Some(event.content.clone()),
            "started" => {
                entry.status = "started".into();
                entry.last_activity = Some(event.content.clone());
            }
            "completed" | "failed" | "cancelled" => {
                entry.status = event.event_type.clone();
                entry.last_content = Some(event.content.clone());
            }
            _ => {}
        }
    }

    pub(crate) fn generation_snapshot(&self, request_id: &str) -> Option<GenerationSnapshot> {
        self.generations
            .lock()
            .ok()
            .and_then(|map| map.get(request_id).cloned())
    }
}

pub(crate) fn sqlite_path() -> PathBuf {
    app_data_dir().join("sprite-studio.sqlite3")
}

pub(crate) fn app_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        let roaming = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        roaming.join(APP_IDENTIFIER)
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join("Library/Application Support")
            .join(APP_IDENTIFIER)
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .unwrap_or_else(|| PathBuf::from("."));
        base.join(APP_IDENTIFIER)
    }
}

pub(crate) fn allow_asset_file(app: Option<&AppHandle>, path: &Path) -> error::CommandResult<()> {
    if let Some(app) = app {
        app.asset_protocol_scope()
            .allow_file(path)
            .map_err(|error| error::CommandError::new("asset_scope_error", error.to_string()))?;
    }
    Ok(())
}

pub(crate) fn allow_asset_directory(
    app: Option<&AppHandle>,
    path: &Path,
    recursive: bool,
) -> error::CommandResult<()> {
    if let Some(app) = app {
        app.asset_protocol_scope()
            .allow_directory(path, recursive)
            .map_err(|error| error::CommandError::new("asset_scope_error", error.to_string()))?;
    }
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_directory = app.path().app_data_dir().map_err(|error| {
                format!("Could not resolve application data directory: {error}")
            })?;
            let connection = database::open(&data_directory.join("sprite-studio.sqlite3"))?;
            app.manage(AppState::from_connection(connection));
            workspace::initialize_ffmpeg_runtime();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            workspace::list_workspaces,
            workspace::load_sidebar_state,
            workspace::create_workspace,
            workspace::open_workspace,
            workspace::check_python_runtime,
            workspace::check_ffmpeg_runtime,
            workspace::run_sprite_polish,
            workspace::archive_sprite_paths,
            workspace::restore_sprite_paths,
            workspace::touch_workspace,
            workspace::rename_workspace,
            workspace::remove_workspace,
            workspace::delete_workspace,
            backups::create_project_backup,
            backups::restore_project_backup,
            backups::import_project_backup,
            worktrees::list_worktrees,
            worktrees::create_worktree,
            worktrees::update_worktree,
            worktrees::delete_worktree,
            worktrees::list_worktree_asset_ids,
            worktrees::link_asset_to_worktree,
            conversations::list_conversations,
            conversations::list_archived_conversations,
            conversations::create_conversation,
            conversations::rename_conversation,
            conversations::switch_conversation_provider,
            conversations::archive_conversation,
            conversations::restore_conversation,
            conversations::delete_conversation,
            conversations::list_messages,
            conversations::record_chat_assistant,
            conversations::record_chat_turn,
            conversations::update_message_metadata,
            conversations::append_conversation_log_entry,
            conversations::export_conversation_debug_log,
            providers::detect_providers,
            providers::install_agent_provider,
            providers::authenticate_agent_provider,
            providers::save_image_provider,
            providers::delete_image_provider,
            providers::test_image_provider,
            providers::start_provider_message,
            providers::cancel_provider_request,
            providers::refine_generation_prompt_command,
            providers::cancel_refine_generation_prompt_command,
            motion_planner::plan_motion,
            references::list_reference_images,
            references::import_reference_image,
            references::import_reference_bytes,
            references::update_reference_image,
            references::delete_reference_image,
            references::set_conversation_reference,
            references::list_conversation_reference_ids,
            templates::list_animation_templates,
            templates::create_animation_template,
            templates::apply_animation_template,
            templates::delete_animation_template,
            assets::scan_assets,
            assets::list_assets,
            assets::import_asset,
            assets::rename_asset,
            assets::delete_asset,
            assets::get_asset_usage,
            assets::export_asset,
            terrain::export_godot_tileset,
            assets::get_generation_manifest,
            assets::get_generation_fingerprint,
            assets::list_workspace_rig_specs,
            assets::scan_generation_assets,
            assets::list_asset_versions,
            packs::list_asset_packs,
            animations::list_animations,
            animations::save_animation,
            animations::delete_animation,
            animations::export_animation,
            animations::export_animation_preview,
            jobs::list_jobs,
            jobs::cancel_job,
            jobs::list_sprite_sheets,
            jobs::queue_sprite_sheet,
            jobs::delete_sprite_sheet,
            jobs::list_vfx_effects,
            jobs::queue_procedural_vfx,
            quality::get_quality_report,
            quality::queue_quality_analysis,
            quality::acknowledge_quality_check,
            quality::optimize_animation_frames,
            quality::repair_animation_alignment,
            quality::repair_animation_transparency,
            pipeline::promote_anchor,
            pipeline::get_anchor,
            pipeline::list_anchors,
            pipeline::detect_anchor_facing,
            pipeline::list_facing_checks,
            pipeline::orient_anchor,
            pipeline::score_animation_frames,
            pipeline::mirror_animation,
            pipeline::list_direction_meta,
            pipeline::queue_direction_set,
            pipeline::list_motion_presets,
            pipeline::list_missing_motions,
            pipeline::save_motion_presets,
            pipeline::list_generation_sessions,
            pipeline::queue_motion_batch,
            pipeline::queue_region_regen,
            pipeline::clean_alpha_animation,
            pipeline::normalize_animation,
            pipeline::snap_to_pixel_grid,
            pipeline::harden_animation,
            pipeline::split_sprite_strip,
            pipeline::score_sprite_strip,
            pipeline::extract_video_frames,
            pipeline::subsample_video_frames,
            pipeline::quantize_worktree_palette,
            pipeline::paint_frame_alpha,
            pipeline::restore_asset_version,
            pipeline::nudge_animation_frames,
            pipeline::reset_animation_alignment_offsets,
            pipeline::check_size_contract,
            pipeline::check_character_contract,
            pipeline::export_character_pack,
            pipeline::get_character_profile,
            pipeline::refresh_character_profile,
            pipeline::export_character_profile,
            pipeline::import_character_profile,
            pipeline::get_production_score,
            pipeline::retry_size_contract,
            pipeline::finalize_contract_retry,
            pipeline::queue_contract_retry,
            pipeline::set_animation_review_status,
            rig::list_rigs,
            rig::save_rig,
            rig::delete_rig,
            rig::validate_rig_spec,
            rig::suggest_rig_points,
            rig::ai_suggest_rig_points,
            rig::analyze_rig_fit,
            rig::render_rig_preview,
            rig::render_rig_animation,
            rig::interpolate_rig_frames,
            rig::interpolate_rig_animation_frames,
            settings::get_setting,
            settings::set_setting,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub fn run_mcp() {
    if let Err(error) = mcp::run() {
        eprintln!("Sprite Studio MCP failed: {error}");
        std::process::exit(1);
    }
}
