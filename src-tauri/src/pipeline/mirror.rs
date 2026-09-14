use crate::{
    animations::{load_animation_by_id, save_animation_inner},
    assets::{extract_palette, get_asset, inspect, normalize_sprite_alpha, upsert},
    error::{CommandError, CommandResult},
    models::{
        AnimationDirectionMeta, AnimationFrame, AnimationInput, MirrorAnimationInput,
        MirrorAnimationResult, NormalizeAnimationInput,
    },
    rig::{mirror_rig_frames, render_frames, resolve_rig_for_animation},
    workspace::workspace_path,
    AppState,
};
use image::RgbaImage;
use rusqlite::OptionalExtension;
use tauri::{AppHandle, State};
use uuid::Uuid;

use super::anchors::portable_slug;
use super::contract::size_contract_check_inner;
use super::direction_meta::{
    infer_direction_family, infer_facing_from_name, load_direction_meta, write_direction_meta,
};
use super::facing::{is_valid_facing, mirror_rgba_horizontal, mirror_rgba_vertical};
use super::logging::log_pipeline_event;
use super::normalize::normalize_animation_inner;

pub(crate) fn mirror_facing_pair(source: &str) -> Option<&'static str> {
    match source {
        "w" => Some("e"),
        "e" => Some("w"),
        "n" => Some("s"),
        "s" => Some("n"),
        "ne" => Some("se"),
        "se" => Some("ne"),
        "nw" => Some("sw"),
        "sw" => Some("nw"),
        _ => None,
    }
}

pub(crate) fn mirror_axis_for_pair(source: &str, target: &str) -> Option<&'static str> {
    match (source, target) {
        ("w", "e") | ("e", "w") => Some("horizontal"),
        ("n", "s") | ("s", "n") | ("ne", "se") | ("se", "ne") | ("nw", "sw") | ("sw", "nw") => {
            Some("vertical")
        }
        _ => None,
    }
}

pub(crate) fn mirror_targets_for_set(set: &str, _generated: &[&str]) -> Vec<(String, String)> {
    if set == "8" {
        return vec![
            ("ne".into(), "se".into()),
            ("nw".into(), "sw".into()),
            ("e".into(), "w".into()),
        ];
    }
    vec![("n".into(), "s".into()), ("e".into(), "w".into())]
}

pub(crate) fn mirror_animation_inner(
    app: Option<&AppHandle>,
    state: &AppState,
    input: MirrorAnimationInput,
) -> CommandResult<MirrorAnimationResult> {
    if !is_valid_facing(&input.target_facing) {
        return Err(CommandError::new(
            "invalid_facing",
            "targetFacing must be one of w,e,n,s,nw,ne,sw,se",
        ));
    }

    let (project_id, worktree_id, name, fps, looping, _review_status, frames): (
        String,
        Option<String>,
        String,
        f64,
        bool,
        String,
        Vec<AnimationFrame>,
    ) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let (
            project_id,
            worktree_id,
            name,
            fps,
            looping,
            review_status,
            frames_json,
        ): (
            String,
            Option<String>,
            String,
            f64,
            bool,
            String,
            String,
        ) = connection
            .query_row(
                "SELECT workspace_id, worktree_id, name, fps, looping, review_status, frames_json FROM animations WHERE id=?1",
                [&input.animation_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "Animation no longer exists"))?;
        let frames = crate::animations::resolve_animation_frames(
            &connection,
            &input.animation_id,
            &frames_json,
        )?;
        (
            project_id,
            worktree_id,
            name,
            fps,
            looping,
            review_status,
            frames,
        )
    };
    if frames.is_empty() {
        return Err(CommandError::new("empty_animation", "There are no frames to mirror"));
    }

    let motion_plan = load_animation_by_id(state, &input.animation_id)?.motion_plan;
    let source_meta = load_direction_meta(state, &project_id, &input.animation_id)?;
    let source_facing = input
        .source_facing
        .or_else(|| source_meta.as_ref().map(|meta| meta.facing.clone()))
        .or_else(|| infer_facing_from_name(&name))
        .unwrap_or_else(|| "w".to_string());
    if mirror_facing_pair(&source_facing) != Some(input.target_facing.as_str()) {
        return Err(CommandError::new(
            "invalid_mirror_pair",
            format!(
                "Cannot mirror facing `{source_facing}` to `{target}`",
                target = input.target_facing
            ),
        ));
    }
    let mirror_axis = mirror_axis_for_pair(&source_facing, &input.target_facing).ok_or_else(|| {
        CommandError::new(
            "invalid_mirror_axis",
            format!(
                "No mirror axis defined for `{source_facing}` → `{target}`",
                target = input.target_facing
            ),
        )
    })?;

    let root = workspace_path(state, &project_id)?;
    let repair_id = Uuid::new_v4().to_string();
    let slug = portable_slug(&name);
    let output_directory = root.join("assets").join("mirrored").join(format!(
        "{}-{}-{}",
        slug,
        input.target_facing,
        &repair_id[..8]
    ));
    std::fs::create_dir_all(&output_directory)?;
    if let Some(app) = app {
        crate::allow_asset_directory(Some(app), &output_directory, true)?;
    }

    let rig = resolve_rig_for_animation(
        state,
        &project_id,
        worktree_id.as_deref(),
        &name,
        input.rig_id.as_deref(),
    );
    let mut mirrored_frames = Vec::with_capacity(frames.len());
    if let Some(rig) = rig.filter(|rig| rig.frames.len() == frames.len()) {
        let asset_id = rig.asset_id.clone().ok_or_else(|| {
            CommandError::new("missing_master", "Rig-native mirror requires a master asset")
        })?;
        let master = get_asset(state, &asset_id)?;
        let master_image = image::open(&master.path)?.to_rgba8();
        let palette = extract_palette(&master_image, 64);
        let canvas_width = master_image.width() as f64;
        let canvas_height = master_image.height() as f64;
        let mut render_rig = rig.clone();
        render_rig.frames =
            mirror_rig_frames(&render_rig, mirror_axis, canvas_width, canvas_height);
        let rendered = render_frames(&master_image, &render_rig);
        for (index, (frame, image)) in frames.iter().zip(rendered.iter()).enumerate() {
            let file_name = format!("frame-{:02}.png", index + 1);
            let output_path = output_directory.join(&file_name);
            normalize_sprite_alpha(image, Some(&palette)).save(&output_path)?;
            let mirrored_asset = inspect(&project_id, &root, &output_path, None)?;
            upsert(state, &mirrored_asset, "rig-mirror")?;
            mirrored_frames.push(AnimationFrame {
                asset_id: mirrored_asset.id,
                duration_ms: frame.duration_ms,
                offset_x: -(frame.offset_x),
                offset_y: frame.offset_y,
            });
        }
    } else {
        for (index, frame) in frames.iter().enumerate() {
            let asset = get_asset(state, &frame.asset_id)?;
            let image = image::open(&asset.path)?.to_rgba8();
            let mirrored: RgbaImage = if mirror_axis == "vertical" {
                mirror_rgba_vertical(&image)
            } else {
                mirror_rgba_horizontal(&image)
            };
            let file_name = format!("frame-{:02}.png", index + 1);
            let output_path = output_directory.join(&file_name);
            mirrored.save(&output_path)?;
            let mirrored_asset = inspect(&project_id, &root, &output_path, None)?;
            upsert(state, &mirrored_asset, "mirrored")?;
            mirrored_frames.push(AnimationFrame {
                asset_id: mirrored_asset.id,
                duration_ms: frame.duration_ms,
                offset_x: -(frame.offset_x),
                offset_y: frame.offset_y,
            });
        }
    }

    let mirrored_name = if name.to_ascii_lowercase().ends_with(&format!("-{}", source_facing)) {
        format!(
            "{}-{}",
            name.trim_end_matches(&source_facing).trim_end_matches('-'),
            input.target_facing
        )
    } else {
        format!("{}-{}", name, input.target_facing)
    };
    let mirrored_animation_id = Uuid::new_v4().to_string();
    save_animation_inner(
        AnimationInput {
            id: Some(mirrored_animation_id.clone()),
            workspace_id: project_id.clone(),
            worktree_id: worktree_id.clone(),
            name: mirrored_name,
            fps,
            looping,
            frames: mirrored_frames,
            motion_plan,
            review_status: Some("draft".into()),
        },
        state,
    )?;

    let direction_family = source_meta
        .as_ref()
        .map(|meta| meta.direction_family.clone())
        .unwrap_or_else(|| infer_direction_family(&name));
    write_direction_meta(
        &root,
        &AnimationDirectionMeta {
            animation_id: mirrored_animation_id.clone(),
            direction_family,
            facing: input.target_facing.clone(),
            mirrored_from: Some(input.animation_id.clone()),
            anchor_slug: input.anchor_slug.clone(),
        },
    )?;

    normalize_animation_inner(
        app,
        state,
        NormalizeAnimationInput {
            animation_id: mirrored_animation_id.clone(),
            anchor_slug: input.anchor_slug.clone(),
            lock_first_frame: Some(true),
            shared_scale: Some(true),
            padding: None,
        },
    )?;

    let contract_report = size_contract_check_inner(
        state,
        &mirrored_animation_id,
        input.anchor_slug.as_deref(),
    )?;

    log_pipeline_event(
        &root,
        &project_id,
        "mirror_animation",
        Some("completed"),
        None,
        Some(&mirrored_animation_id),
        None,
        Some(contract_report.passed),
        None,
        Some(serde_json::json!({
            "sourceAnimationId": input.animation_id,
            "sourceFacing": source_facing,
            "targetFacing": input.target_facing,
        })),
    );

    Ok(MirrorAnimationResult {
        source_animation_id: input.animation_id,
        mirrored_animation_id,
        target_facing: input.target_facing,
        contract_report,
    })
}

#[tauri::command]
pub fn mirror_animation(
    input: MirrorAnimationInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<MirrorAnimationResult> {
    mirror_animation_inner(Some(&app), &state, input)
}
