use crate::{
    assets::{defringe, get_asset, inspect, remove_orphan_pixels, upsert},
    animations::{
        load_animation_frames_from_table, resolve_animation_frames,
    },
    error::{CommandError, CommandResult},
    workspace::workspace_path,
    AppState,
};
use image::RgbaImage;
use rusqlite::OptionalExtension;
use tauri::State;

const SEMI_OPAQUE_THRESHOLD: u8 = 200;

pub(crate) fn clean_rgba_image(image: &RgbaImage) -> RgbaImage {
    let mut output = image.clone();
    for pixel in output.pixels_mut() {
        if pixel[3] == 0 {
            continue;
        }
        if pixel[3] < SEMI_OPAQUE_THRESHOLD {
            pixel[3] = 0;
            pixel[0] = 0;
            pixel[1] = 0;
            pixel[2] = 0;
        } else if pixel[3] < 255 {
            pixel[3] = 255;
        }
    }
    defringe(&mut output);
    remove_orphan_pixels(&mut output);
    output
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CleanAlphaReport {
    pub animation_id: String,
    pub frames_processed: u32,
    pub frames_changed: u32,
}

pub(crate) fn clean_alpha_animation_inner(
    state: &AppState,
    animation_id: &str,
) -> CommandResult<CleanAlphaReport> {
    let (workspace_id, frames_json) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let (workspace_id, frames_json): (String, String) = connection
            .query_row(
                "SELECT workspace_id, frames_json FROM animations WHERE id=?1",
                [animation_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "Animation no longer exists"))?;
        (workspace_id, frames_json)
    };
    let root = workspace_path(state, &workspace_id)?;
    let frames = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let table_frames = load_animation_frames_from_table(&connection, animation_id)?;
        if table_frames.is_empty() {
            resolve_animation_frames(&connection, animation_id, &frames_json)?
        } else {
            table_frames
        }
    };
    if frames.is_empty() {
        return Err(CommandError::new(
            "empty_animation",
            "Add frames before running alpha cleanup",
        ));
    }

    let mut frames_changed = 0u32;
    for frame in &frames {
        let asset = get_asset(state, &frame.asset_id)?;
        let image = image::open(&asset.path)?.to_rgba8();
        let cleaned = clean_rgba_image(&image);
        if cleaned.as_raw() != image.as_raw() {
            cleaned
                .save(&asset.path)
                .map_err(|error| CommandError::new("clean_alpha_write_failed", error.to_string()))?;
            let updated = inspect(&workspace_id, &root, std::path::Path::new(&asset.path), Some(asset.id.clone()))?;
            upsert(state, &updated, "clean-alpha")?;
            frames_changed += 1;
        }
    }

    Ok(CleanAlphaReport {
        animation_id: animation_id.to_string(),
        frames_processed: frames.len() as u32,
        frames_changed,
    })
}

#[tauri::command]
pub fn clean_alpha_animation(
    animation_id: String,
    state: State<'_, AppState>,
) -> CommandResult<CleanAlphaReport> {
    clean_alpha_animation_inner(&state, &animation_id)
}
