use crate::{
    assets::{get_asset, inspect, upsert},
    error::{CommandError, CommandResult},
    models::{BrushStamp, PaintFrameAlphaInput, PaintFrameAlphaResult, RestoreAssetVersionInput},
    workspace::workspace_path,
    AppState,
};
use image::{Rgba, RgbaImage};
use rusqlite::{params, OptionalExtension};
use tauri::State;
use super::region_regen::build_region_mask;

fn count_opaque_pixels(image: &RgbaImage) -> u32 {
    image.pixels().filter(|pixel| pixel[3] > 0).count() as u32
}

fn erase_strokes(image: &mut RgbaImage, strokes: &[BrushStamp]) -> CommandResult<()> {
    let (width, height) = image.dimensions();
    let mask = build_region_mask(width, height, &[], strokes)?;
    for (x, y, mask_pixel) in mask.enumerate_pixels() {
        if mask_pixel[0] > 127 {
            image.put_pixel(x, y, Rgba([0, 0, 0, 0]));
        }
    }
    Ok(())
}

fn archive_live_asset_before_write(
    state: &AppState,
    asset: &crate::models::Asset,
) -> CommandResult<()> {
    let root = workspace_path(state, &asset.workspace_id)?;
    let snapshot_dir = root.join(".sprite-studio").join("asset-versions");
    std::fs::create_dir_all(&snapshot_dir)?;
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let selected: Option<(String, String)> = connection
        .query_row(
            "SELECT id, path FROM asset_versions WHERE asset_id = ?1 AND selected = 1 ORDER BY version_number DESC LIMIT 1",
            [&asset.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((version_id, path)) = selected {
        if path == asset.path {
            let snapshot = snapshot_dir.join(format!("{version_id}.png"));
            std::fs::copy(&asset.path, &snapshot)?;
            connection.execute(
                "UPDATE asset_versions SET path = ?1, available = 1 WHERE id = ?2",
                params![snapshot.to_string_lossy().into_owned(), version_id],
            )?;
        }
    }
    Ok(())
}

fn load_asset_version_path(
    state: &AppState,
    asset_id: &str,
    version_id: &str,
) -> CommandResult<String> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection
        .query_row(
            "SELECT path FROM asset_versions WHERE asset_id = ?1 AND id = ?2",
            [asset_id, version_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| {
            CommandError::new("version_not_found", "The requested asset version does not exist")
        })
}

pub(crate) fn restore_asset_version_inner(
    state: &AppState,
    asset_id: &str,
    version_id: &str,
) -> CommandResult<crate::models::Asset> {
    let asset = get_asset(state, asset_id)?;
    let version_path = load_asset_version_path(state, asset_id, version_id)?;
    if !std::path::Path::new(&version_path).is_file() {
        return Err(CommandError::new(
            "version_file_missing",
            "The asset version file is no longer available on disk",
        ));
    }
    std::fs::copy(&version_path, &asset.path)?;
    let root = workspace_path(state, &asset.workspace_id)?;
    let updated = inspect(
        &asset.workspace_id,
        &root,
        std::path::Path::new(&asset.path),
        Some(asset.id.clone()),
    )?;
    upsert(state, &updated, "paint_restore")?;
    Ok(updated)
}

pub(crate) fn paint_frame_alpha_inner(
    state: &AppState,
    input: PaintFrameAlphaInput,
) -> CommandResult<PaintFrameAlphaResult> {
    let asset = get_asset(state, &input.asset_id)?;
    match input.mode.as_str() {
        "erase" => {
            if input.strokes.is_empty() {
                return Err(CommandError::new(
                    "empty_strokes",
                    "Provide at least one brush stroke to erase",
                ));
            }
            let mut image = image::open(&asset.path)?.to_rgba8();
            archive_live_asset_before_write(state, &asset)?;
            erase_strokes(&mut image, &input.strokes)?;
            image.save(&asset.path)?;
            let root = workspace_path(state, &asset.workspace_id)?;
            let updated = inspect(
                &asset.workspace_id,
                &root,
                std::path::Path::new(&asset.path),
                Some(asset.id.clone()),
            )?;
            upsert(state, &updated, "paint_erase")?;
            let versions = {
                let connection = state
                    .db
                    .lock()
                    .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
                let mut statement = connection.prepare(
                    "SELECT id FROM asset_versions WHERE asset_id = ?1 ORDER BY version_number DESC LIMIT 1",
                )?;
                statement
                    .query_row([&asset.id], |row| row.get::<_, String>(0))
                    .map_err(|_| CommandError::new("version_missing", "Asset version row was not created"))
            }?;
            Ok(PaintFrameAlphaResult {
                asset_id: asset.id,
                version_id: versions,
                opaque_pixel_count: count_opaque_pixels(&image),
            })
        }
        "restore" => {
            let version_id = input.version_id.ok_or_else(|| {
                CommandError::new(
                    "version_required",
                    "Provide versionId when restoring a painted frame",
                )
            })?;
            let restored = restore_asset_version_inner(state, &input.asset_id, &version_id)?;
            let image = image::open(&restored.path)?.to_rgba8();
            Ok(PaintFrameAlphaResult {
                asset_id: restored.id,
                version_id,
                opaque_pixel_count: count_opaque_pixels(&image),
            })
        }
        _ => Err(CommandError::new(
            "invalid_paint_mode",
            "Paint mode must be erase or restore",
        )),
    }
}

#[tauri::command]
pub fn paint_frame_alpha(
    input: PaintFrameAlphaInput,
    state: State<'_, AppState>,
) -> CommandResult<PaintFrameAlphaResult> {
    paint_frame_alpha_inner(&state, input)
}

#[tauri::command]
pub fn restore_asset_version(
    input: RestoreAssetVersionInput,
    state: State<'_, AppState>,
) -> CommandResult<crate::models::Asset> {
    restore_asset_version_inner(&state, &input.asset_id, &input.version_id)
}
