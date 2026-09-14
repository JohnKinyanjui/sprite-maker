use crate::{
    animations::export_animation_inner,
    assets::get_asset,
    error::{CommandError, CommandResult},
    models::{
        CharacterPackAnimationEntry, CharacterPackExportResult, ExportCharacterPackInput,
    },
    workspace::{resolve_export_directory, workspace_path},
    AppState,
};
use chrono::Utc;
use rusqlite::OptionalExtension;
use serde_json::json;
use std::path::{Path, PathBuf};
use tauri::State;

use crate::animations::write_animation_gif;

use super::anchors::get_anchor_inner;
use super::character_contract::character_contract_check_inner;
use super::direction_meta::{infer_facing_from_name, list_direction_meta_for_worktree, load_direction_meta};

fn slugify(value: &str) -> String {
    let slug: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "pack".to_string()
    } else {
        trimmed.to_string()
    }
}

fn copy_file(source: &Path, destination: &Path) -> CommandResult<()> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(source, destination).map_err(|error| {
        CommandError::new(
            "pack_copy_failed",
            format!("Failed to copy {}: {error}", source.display()),
        )
    })?;
    Ok(())
}

fn relative_pack_path(root: &Path, absolute: &Path) -> String {
    absolute
        .strip_prefix(root)
        .map(|value| value.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| absolute.to_string_lossy().replace('\\', "/"))
}

pub(crate) fn export_character_pack_inner(
    state: &AppState,
    input: &ExportCharacterPackInput,
) -> CommandResult<CharacterPackExportResult> {
    let contract = character_contract_check_inner(
        state,
        &input.workspace_id,
        &input.worktree_id,
        input.anchor_slug.as_deref(),
    )?;
    if !contract.passed {
        return Err(CommandError::new(
            "export_blocked",
            "Character pack export is blocked until every facing passes cross-facing contract checks",
        ));
    }

    let workspace_root = workspace_path(state, &input.workspace_id)?;
    let output_root =
        resolve_export_directory(&workspace_root, Some(input.destination.as_str()))?;
    std::fs::create_dir_all(&output_root)?;

    let anchor = get_anchor_inner(&input.workspace_id, &contract.anchor_slug, state)?;
    let anchor_asset = get_asset(state, &anchor.asset_id)?;
    let anchor_dest = output_root
        .join("anchor")
        .join(PathBuf::from(anchor_asset.relative_path.replace('/', "_")));
    copy_file(Path::new(&anchor_asset.path), &anchor_dest)?;

    let animation_rows = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let mut statement = connection.prepare(
            "SELECT id, name FROM animations WHERE workspace_id=?1 AND worktree_id=?2 ORDER BY name",
        )?;
        let rows = statement
            .query_map(
                rusqlite::params![input.workspace_id, input.worktree_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        rows
    };

    if animation_rows.is_empty() {
        return Err(CommandError::new(
            "empty_worktree",
            "Add at least one animation before exporting a character pack",
        ));
    }

    let metadata_format = input.metadata_format.as_deref();
    let direction_metas =
        list_direction_meta_for_worktree(state, &input.workspace_id, &input.worktree_id)?;
    let mut animations = Vec::new();
    let mut manifest_entries = Vec::new();

    for (animation_id, animation_name) in animation_rows {
        let meta = load_direction_meta(state, &input.workspace_id, &animation_id)?;
        let direction_family = meta
            .as_ref()
            .map(|value| value.direction_family.clone())
            .unwrap_or_else(|| super::direction_meta::infer_direction_family(&animation_name));
        let facing = meta
            .as_ref()
            .map(|value| value.facing.clone())
            .or_else(|| infer_facing_from_name(&animation_name))
            .unwrap_or_else(|| "w".to_string());
        let mirrored_from = meta.and_then(|value| value.mirrored_from);

        let folder_name = slugify(&format!("{direction_family}-{facing}"));
        let pack_root = input.destination.trim_matches(|c: char| c == '/' || c == '\\');
        let relative_export = format!("{pack_root}/animations/{folder_name}");

        let export = export_animation_inner(
            &animation_id,
            Some(relative_export),
            metadata_format,
            state,
        )?;

        let frame_count = {
            let connection = state
                .db
                .lock()
                .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
            let frames_json: String = connection
                .query_row(
                    "SELECT frames_json FROM animations WHERE id=?1",
                    [&animation_id],
                    |row| row.get(0),
                )
                .optional()?
                .unwrap_or_else(|| "[]".to_string());
            serde_json::from_str::<Vec<serde_json::Value>>(&frames_json)
                .map(|frames| frames.len() as u32)
                .unwrap_or(0)
        };

        let preview_dest = output_root
            .join("previews")
            .join(format!("{folder_name}.png"));
        let first_frame_asset_id = {
            let connection = state
                .db
                .lock()
                .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
            let frames_json: String = connection.query_row(
                "SELECT frames_json FROM animations WHERE id=?1",
                [&animation_id],
                |row| row.get(0),
            )?;
            let frames = crate::animations::resolve_animation_frames(
                &connection,
                &animation_id,
                &frames_json,
            )?;
            frames.first().map(|frame| frame.asset_id.clone())
        };
        let first_frame_path = if let Some(asset_id) = first_frame_asset_id {
            Some(get_asset(state, &asset_id)?.path)
        } else {
            None
        };
        if let Some(frame_path) = first_frame_path {
            copy_file(Path::new(&frame_path), &preview_dest)?;
        } else {
            copy_file(Path::new(&export.png_path), &preview_dest)?;
        }

        let preview_animated_path = if input.include_animated_previews == Some(true) {
            let gif_dest = output_root
                .join("previews")
                .join(format!("{folder_name}.gif"));
            write_animation_gif(&animation_id, &gif_dest, state)?;
            Some(relative_pack_path(&output_root, &gif_dest))
        } else {
            None
        };

        let png_path = relative_pack_path(&output_root, Path::new(&export.png_path));
        let metadata_path = relative_pack_path(&output_root, Path::new(&export.metadata_path));
        let preview_path = relative_pack_path(&output_root, &preview_dest);
        let entry = CharacterPackAnimationEntry {
            animation_id: animation_id.clone(),
            name: animation_name.clone(),
            direction_family: direction_family.clone(),
            facing: facing.clone(),
            png_path: png_path.clone(),
            metadata_path: metadata_path.clone(),
            preview_path: preview_path.clone(),
            preview_animated: preview_animated_path.clone(),
            frame_count,
            mirrored_from: mirrored_from.clone(),
        };
        animations.push(entry);
        manifest_entries.push(json!({
            "animationId": animation_id,
            "name": animation_name,
            "directionFamily": direction_family,
            "facing": facing,
            "mirroredFrom": mirrored_from,
            "frameCount": frame_count,
            "png": png_path,
            "metadata": metadata_path,
            "preview": preview_path,
            "previewAnimated": preview_animated_path,
        }));
    }

    let manifest_path = output_root.join("pack-manifest.json");
    let manifest_body = json!({
        "formatVersion": 1,
        "exportedAt": Utc::now().to_rfc3339(),
        "workspaceId": input.workspace_id,
        "worktreeId": input.worktree_id,
        "anchorSlug": contract.anchor_slug,
        "anchorPath": relative_pack_path(&output_root, &anchor_dest),
        "characterContractPassed": contract.passed,
        "directionMetaCount": direction_metas.len(),
        "animations": manifest_entries,
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest_body)
        .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
    std::fs::write(&manifest_path, manifest_bytes)?;

    Ok(CharacterPackExportResult {
        directory_path: output_root.to_string_lossy().into_owned(),
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        anchor_path: anchor_dest.to_string_lossy().into_owned(),
        animation_count: animations.len() as u32,
        animations,
        character_contract_passed: contract.passed,
    })
}

#[tauri::command]
pub fn export_character_pack(
    input: ExportCharacterPackInput,
    state: State<'_, AppState>,
) -> CommandResult<CharacterPackExportResult> {
    export_character_pack_inner(&state, &input)
}
