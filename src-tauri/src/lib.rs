mod animations;
mod assets;
mod conversations;
mod database;
mod error;
mod jobs;
mod models;
mod motion_planner;
mod providers;
mod quality;
mod references;
mod settings;
mod sprite_harness;
mod templates;
mod workspace;
mod worktrees;

use std::{collections::HashMap, sync::Mutex};
use tauri::Manager;
use tokio::sync::oneshot;

pub struct AppState {
    db: Mutex<rusqlite::Connection>,
    cancellers: Mutex<HashMap<String, oneshot::Sender<()>>>,
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
            app.manage(AppState {
                db: Mutex::new(connection),
                cancellers: Mutex::new(HashMap::new()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            workspace::list_workspaces,
            workspace::create_workspace,
            workspace::open_workspace,
            workspace::touch_workspace,
            workspace::rename_workspace,
            workspace::remove_workspace,
            workspace::delete_workspace,
            worktrees::list_worktrees,
            worktrees::create_worktree,
            worktrees::update_worktree,
            worktrees::delete_worktree,
            worktrees::list_worktree_asset_ids,
            worktrees::link_asset_to_worktree,
            conversations::list_conversations,
            conversations::create_conversation,
            conversations::rename_conversation,
            conversations::archive_conversation,
            conversations::delete_conversation,
            conversations::list_messages,
            conversations::update_message_metadata,
            providers::detect_providers,
            providers::start_provider_message,
            providers::cancel_provider_request,
            motion_planner::plan_motion,
            references::list_reference_images,
            references::import_reference_image,
            references::update_reference_image,
            references::delete_reference_image,
            references::set_conversation_reference,
            references::list_conversation_reference_ids,
            templates::list_animation_templates,
            templates::create_animation_template,
            templates::apply_animation_template,
            templates::delete_animation_template,
            assets::scan_assets,
            assets::import_asset,
            assets::rename_asset,
            assets::delete_asset,
            assets::export_asset,
            assets::get_generation_manifest,
            assets::list_asset_versions,
            animations::list_animations,
            animations::save_animation,
            animations::delete_animation,
            animations::export_animation,
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
            settings::get_setting,
            settings::set_setting,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
