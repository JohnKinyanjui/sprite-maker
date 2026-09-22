use crate::{
    assets::get_asset,
    error::{CommandError, CommandResult},
    models::{CharacterAnchor, FacingCheckReport, Pivot},
    quality::compute_metrics,
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use image::RgbaImage;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tauri::State;

use super::anchors::{anchor_path, get_anchor_inner, write_anchor_file};
use super::logging::log_pipeline_event;

const AUTO_ORIENT_THRESHOLD: f64 = 0.55;
const MAX_FACING_HISTORY_PER_SLUG: usize = 32;

pub(crate) fn mirror_rgba_horizontal(image: &RgbaImage) -> RgbaImage {
    let (width, height) = image.dimensions();
    let mut mirrored = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            mirrored.put_pixel(width - 1 - x, y, *image.get_pixel(x, y));
        }
    }
    mirrored
}

pub(crate) fn mirror_rgba_vertical(image: &RgbaImage) -> RgbaImage {
    let (width, height) = image.dimensions();
    let mut mirrored = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            mirrored.put_pixel(x, height - 1 - y, *image.get_pixel(x, y));
        }
    }
    mirrored
}

pub(crate) fn is_valid_facing(facing: &str) -> bool {
    matches!(
        facing,
        "w" | "e" | "n" | "s" | "nw" | "ne" | "sw" | "se"
    )
}

pub(crate) fn normalize_view(view: &str) -> &'static str {
    match view.trim() {
        "side_platformer" | "side" => "side",
        "top-down" | "top_down" => "top_down",
        "isometric" => "isometric",
        "rts_oblique" => "rts_oblique",
        _ => "side",
    }
}

pub(crate) fn canonical_facing_for_view(view: &str) -> &'static str {
    match normalize_view(view) {
        "top_down" | "isometric" => "n",
        _ => "w",
    }
}

pub(crate) fn detect_facing_from_image(image: &RgbaImage, view: &str) -> (String, f64) {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return ("w".to_string(), 0.0);
    }

    let mut total_alpha = 0u64;
    let mut weighted_x = 0f64;
    let mut weighted_y = 0f64;
    for y in 0..height {
        for x in 0..width {
            let alpha = image.get_pixel(x, y)[3] as u64;
            if alpha == 0 {
                continue;
            }
            total_alpha += alpha;
            weighted_x += x as f64 * alpha as f64;
            weighted_y += y as f64 * alpha as f64;
        }
    }
    if total_alpha == 0 {
        return ("w".to_string(), 0.0);
    }
    let _centroid_x = weighted_x / total_alpha as f64;
    let _centroid_y = weighted_y / total_alpha as f64;

    let mid_x = width as f64 / 2.0;
    let mid_y = height as f64 / 2.0;
    let mut left_mass = 0u64;
    let mut right_mass = 0u64;
    let mut upper_mass = 0u64;
    let mut lower_mass = 0u64;
    for y in 0..height {
        for x in 0..width {
            let alpha = image.get_pixel(x, y)[3] as u64;
            if alpha == 0 {
                continue;
            }
            if (x as f64) < mid_x {
                left_mass += alpha;
            } else {
                right_mass += alpha;
            }
            if (y as f64) < mid_y {
                upper_mass += alpha;
            } else {
                lower_mass += alpha;
            }
        }
    }

    if view == "top_down" || view == "isometric" {
        let total = (upper_mass + lower_mass).max(1) as f64;
        let skew = (upper_mass as f64 - lower_mass as f64).abs() / total;
        let facing = if upper_mass > lower_mass { "n" } else { "s" };
        return (facing.to_string(), skew.clamp(0.0, 1.0));
    }

    let total = (left_mass + right_mass).max(1) as f64;
    let skew = (left_mass as f64 - right_mass as f64).abs() / total;
    let facing = if left_mass > right_mass { "w" } else { "e" };
    (facing.to_string(), skew.clamp(0.0, 1.0))
}

fn facing_check_path(root: &Path) -> PathBuf {
    root.join(".sprite-studio").join("facing-check.json")
}

fn read_facing_sidecar(root: &Path) -> (Vec<FacingCheckReport>, Vec<FacingCheckReport>) {
    let path = facing_check_path(root);
    if !path.is_file() {
        return (Vec::new(), Vec::new());
    }
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let value = serde_json::from_str::<serde_json::Value>(&existing).unwrap_or_default();
    let anchors = value
        .get("anchors")
        .and_then(|anchors| serde_json::from_value(anchors.clone()).ok())
        .unwrap_or_default();
    let history = value
        .get("history")
        .and_then(|history| serde_json::from_value(history.clone()).ok())
        .unwrap_or_default();
    (anchors, history)
}

fn write_facing_check_sidecar(
    root: &Path,
    workspace_id: &str,
    report: &FacingCheckReport,
) -> CommandResult<()> {
    let path = facing_check_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let (mut anchors, mut history) = read_facing_sidecar(root);
    let stamped = FacingCheckReport {
        checked_at: Some(Utc::now().to_rfc3339()),
        ..report.clone()
    };
    anchors.retain(|entry| entry.slug != stamped.slug);
    anchors.push(stamped.clone());
    history.push(stamped.clone());
    while history
        .iter()
        .filter(|entry| entry.slug == stamped.slug)
        .count() > MAX_FACING_HISTORY_PER_SLUG
    {
        if let Some(index) = history.iter().position(|entry| entry.slug == stamped.slug) {
            history.remove(index);
        } else {
            break;
        }
    }
    let payload = serde_json::json!({
        "workspaceId": workspace_id,
        "generatedAt": Utc::now().to_rfc3339(),
        "anchors": anchors,
        "history": history,
    });
    let bytes = serde_json::to_vec_pretty(&payload)
        .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
    std::fs::write(path, bytes)?;
    Ok(())
}

pub(crate) fn list_facing_checks_inner(
    workspace_id: &str,
    slug: Option<&str>,
    state: &AppState,
) -> CommandResult<Vec<FacingCheckReport>> {
    let root = workspace_path(state, workspace_id)?;
    let (_, history) = read_facing_sidecar(&root);
    let mut entries = if let Some(slug) = slug {
        history
            .into_iter()
            .filter(|entry| entry.slug == slug)
            .collect()
    } else {
        history
    };
    entries.sort_by(|left, right| {
        right
            .checked_at
            .as_deref()
            .unwrap_or("")
            .cmp(left.checked_at.as_deref().unwrap_or(""))
    });
    Ok(entries)
}

pub(crate) fn detect_anchor_facing_inner(
    workspace_id: &str,
    slug: &str,
    state: &AppState,
) -> CommandResult<FacingCheckReport> {
    let anchor = get_anchor_inner(workspace_id, slug, state)?;
    let asset = get_asset(state, &anchor.asset_id)?;
    let image = image::open(&asset.path)?.to_rgba8();
    let view = anchor.view.as_deref().unwrap_or("side");
    let canonical = canonical_facing_for_view(view);
    let (detected, confidence) = detect_facing_from_image(&image, view);
    let status = if confidence < 0.35 {
        "uncertain"
    } else if detected == canonical {
        "ok"
    } else {
        "mismatch"
    };
    let report = FacingCheckReport {
        slug: anchor.slug.clone(),
        asset_id: anchor.asset_id.clone(),
        view: view.to_string(),
        detected_facing: detected,
        canonical_facing: canonical.to_string(),
        confidence,
        status: status.to_string(),
        auto_oriented: anchor.facing_status.as_deref() == Some("auto_oriented"),
        checked_at: None,
    };
    let root = workspace_path(state, workspace_id)?;
    write_facing_check_sidecar(&root, workspace_id, &report)?;
    Ok(report)
}

pub(crate) fn orient_anchor_inner(
    workspace_id: &str,
    slug: &str,
    state: &AppState,
) -> CommandResult<CharacterAnchor> {
    let root = workspace_path(state, workspace_id)?;
    let anchor = get_anchor_inner(workspace_id, slug, state)?;
    let asset = get_asset(state, &anchor.asset_id)?;
    let asset_path = root.join(&asset.relative_path);
    let image = image::open(&asset_path)?.to_rgba8();
    let view = anchor.view.as_deref().unwrap_or("side");
    let canonical = canonical_facing_for_view(view);
    let (detected, confidence) = detect_facing_from_image(&image, view);

    if detected == canonical {
        let mut updated = anchor;
        updated.facing = Some(canonical.to_string());
        updated.facing_confidence = Some(confidence);
        updated.facing_status = Some("ok".to_string());
        write_anchor_file(&anchor_path(&root, &updated.slug), &updated)?;
        let _ = super::character_profile::sync_character_profile_for_anchor(
            workspace_id,
            &updated.slug,
            None,
            state,
        );
        return Ok(updated);
    }
    if confidence < AUTO_ORIENT_THRESHOLD {
        return Err(CommandError::new(
            "facing_uncertain",
            format!(
                "Facing confidence {:.0}% is below the auto-orient threshold",
                confidence * 100.0
            ),
        ));
    }

    let flipped = mirror_rgba_horizontal(&image);
    flipped.save(&asset_path)?;

    let analyzed = compute_metrics(&asset.id, &asset_path.to_string_lossy())?;
    let baseline_y = analyzed
        .metrics
        .bounds
        .map(|(_min_x, _min_y, _max_x, max_y)| max_y)
        .unwrap_or(anchor.baseline_y);
    let pivot_x = analyzed
        .metrics
        .centroid
        .map(|(cx, _)| cx)
        .unwrap_or(anchor.frame_width as f64 - anchor.pivot.x);
    let hash = blake3::hash(&std::fs::read(&asset_path)?).to_hex().to_string();

    let updated = CharacterAnchor {
        slug: anchor.slug,
        asset_id: anchor.asset_id,
        relative_path: anchor.relative_path,
        frame_width: anchor.frame_width,
        frame_height: anchor.frame_height,
        pivot: Pivot {
            x: pivot_x,
            y: baseline_y as f64,
        },
        baseline_y,
        content_hash: hash,
        promoted_at: anchor.promoted_at,
        view: anchor.view.clone(),
        facing: Some(canonical.to_string()),
        facing_confidence: Some(confidence),
        facing_status: Some("auto_oriented".to_string()),
    };
    write_anchor_file(&anchor_path(&root, &updated.slug), &updated)?;
    let _ = super::character_profile::sync_character_profile_for_anchor(
        workspace_id,
        &updated.slug,
        None,
        state,
    );

    let report = FacingCheckReport {
        slug: updated.slug.clone(),
        asset_id: updated.asset_id.clone(),
        view: view.to_string(),
        detected_facing: detected,
        canonical_facing: canonical.to_string(),
        confidence,
        status: "auto_oriented".to_string(),
        auto_oriented: true,
        checked_at: None,
    };
    write_facing_check_sidecar(&root, workspace_id, &report)?;
    log_pipeline_event(
        &root,
        workspace_id,
        "orient_anchor",
        Some("completed"),
        None,
        None,
        None,
        Some(true),
        None,
        Some(serde_json::json!({ "slug": slug, "facing": canonical })),
    );
    Ok(updated)
}

pub(crate) fn apply_facing_to_anchor(
    workspace_id: &str,
    anchor: &mut CharacterAnchor,
    auto_orient: bool,
    view: Option<&str>,
    state: &AppState,
) -> CommandResult<()> {
    let root = workspace_path(state, workspace_id)?;
    let asset = get_asset(state, &anchor.asset_id)?;
    let image = image::open(&asset.path)?.to_rgba8();
    let resolved_view = view
        .filter(|value| !value.trim().is_empty())
        .map(normalize_view)
        .map(str::to_string)
        .or_else(|| anchor.view.as_deref().map(normalize_view).map(str::to_string))
        .unwrap_or_else(|| "side".to_string());
    anchor.view = Some(resolved_view.clone());
    let canonical = canonical_facing_for_view(&resolved_view);
    let facing_view = if resolved_view == "rts_oblique" {
        "side"
    } else {
        resolved_view.as_str()
    };
    let (detected, confidence) = detect_facing_from_image(&image, facing_view);

    if auto_orient && detected != canonical && confidence >= AUTO_ORIENT_THRESHOLD {
        write_anchor_file(&anchor_path(&root, &anchor.slug), anchor)?;
        let oriented = orient_anchor_inner(workspace_id, &anchor.slug, state)?;
        *anchor = oriented;
        return Ok(());
    }

    let status = if confidence < 0.35 {
        "uncertain"
    } else if detected == canonical {
        "ok"
    } else {
        "mismatch"
    };
    anchor.facing = Some(detected.clone());
    anchor.facing_confidence = Some(confidence);
    anchor.facing_status = Some(status.to_string());
    write_anchor_file(&anchor_path(&root, &anchor.slug), anchor)?;
    let _ = super::character_profile::sync_character_profile_for_anchor(
        workspace_id,
        &anchor.slug,
        None,
        state,
    );

    let report = FacingCheckReport {
        slug: anchor.slug.clone(),
        asset_id: anchor.asset_id.clone(),
        view: resolved_view,
        detected_facing: detected.clone(),
        canonical_facing: canonical.to_string(),
        confidence,
        status: status.to_string(),
        auto_oriented: false,
        checked_at: None,
    };
    write_facing_check_sidecar(&root, workspace_id, &report)?;
    log_pipeline_event(
        &root,
        workspace_id,
        "detect_anchor_facing",
        Some("completed"),
        None,
        None,
        None,
        Some(status == "ok"),
        None,
        Some(serde_json::json!({
            "slug": anchor.slug,
            "detectedFacing": detected,
            "confidence": confidence,
        })),
    );
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrientAnchorInput {
    pub workspace_id: String,
    pub slug: String,
}

#[tauri::command]
pub fn detect_anchor_facing(
    workspace_id: String,
    slug: String,
    state: State<'_, AppState>,
) -> CommandResult<FacingCheckReport> {
    detect_anchor_facing_inner(&workspace_id, &slug, &state)
}

#[tauri::command]
pub fn orient_anchor(
    input: OrientAnchorInput,
    state: State<'_, AppState>,
) -> CommandResult<CharacterAnchor> {
    orient_anchor_inner(&input.workspace_id, &input.slug, &state)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListFacingChecksInput {
    pub workspace_id: String,
    pub slug: Option<String>,
}

#[tauri::command]
pub fn list_facing_checks(
    input: ListFacingChecksInput,
    state: State<'_, AppState>,
) -> CommandResult<Vec<FacingCheckReport>> {
    list_facing_checks_inner(&input.workspace_id, input.slug.as_deref(), &state)
}
