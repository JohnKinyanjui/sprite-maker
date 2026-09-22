use crate::{
    assets::get_asset,
    error::{CommandError, CommandResult},
    models::{CharacterAnchor, CharacterAnchorSummary, Pivot},
    quality::compute_metrics,
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tauri::State;

pub(crate) fn anchors_directory(root: &Path) -> PathBuf {
    root.join(".sprite-studio").join("anchors")
}

pub(crate) fn anchor_path(root: &Path, slug: &str) -> PathBuf {
    anchors_directory(root).join(format!("{slug}.json"))
}

pub(crate) fn portable_slug(name: &str) -> String {
    let slug = name
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() {
                value.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "anchor".to_string()
    } else {
        trimmed.to_string()
    }
}

fn read_anchor_file(path: &Path) -> CommandResult<CharacterAnchor> {
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(|error| {
        CommandError::new("invalid_anchor", format!("Anchor sidecar is invalid: {error}"))
    })
}

pub(crate) fn write_anchor_file(path: &Path, anchor: &CharacterAnchor) -> CommandResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_vec_pretty(anchor)
        .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
    std::fs::write(path, payload)?;
    Ok(())
}

pub(crate) fn list_anchors_inner(
    workspace_id: &str,
    state: &AppState,
) -> CommandResult<Vec<CharacterAnchorSummary>> {
    let root = workspace_path(state, workspace_id)?;
    let directory = anchors_directory(&root);
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut anchors = Vec::new();
    for entry in std::fs::read_dir(&directory)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let anchor = read_anchor_file(&path)?;
        anchors.push(CharacterAnchorSummary {
            slug: anchor.slug,
            asset_id: anchor.asset_id,
            relative_path: anchor.relative_path,
            frame_width: anchor.frame_width,
            frame_height: anchor.frame_height,
            pivot: anchor.pivot,
            baseline_y: anchor.baseline_y,
            promoted_at: anchor.promoted_at,
            view: anchor.view,
            facing: anchor.facing,
            facing_confidence: anchor.facing_confidence,
            facing_status: anchor.facing_status,
        });
    }
    anchors.sort_by(|left, right| left.slug.cmp(&right.slug));
    Ok(anchors)
}

pub(crate) fn remove_anchors_for_asset_inner(
    workspace_id: &str,
    asset_id: &str,
    state: &AppState,
) -> CommandResult<()> {
    let root = workspace_path(state, workspace_id)?;
    for anchor in list_anchors_inner(workspace_id, state)? {
        if anchor.asset_id != asset_id {
            continue;
        }
        let path = anchor_path(&root, &anchor.slug);
        if path.is_file() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

pub(crate) fn get_anchor_inner(
    workspace_id: &str,
    slug: &str,
    state: &AppState,
) -> CommandResult<CharacterAnchor> {
    let root = workspace_path(state, workspace_id)?;
    let path = anchor_path(&root, slug);
    if !path.is_file() {
        return Err(CommandError::new(
            "anchor_not_found",
            format!("No promoted anchor exists for slug '{slug}'"),
        ));
    }
    read_anchor_file(&path)
}

pub(crate) fn promote_anchor_inner(
    workspace_id: &str,
    asset_id: &str,
    slug: Option<&str>,
    auto_orient: bool,
    view: Option<&str>,
    state: &AppState,
) -> CommandResult<CharacterAnchor> {
    let asset = get_asset(state, asset_id)?;
    if asset.workspace_id != workspace_id {
        return Err(CommandError::new(
            "invalid_anchor_asset",
            "The anchor asset must belong to the same workspace",
        ));
    }
    let slug = slug
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| portable_slug(value))
        .unwrap_or_else(|| portable_slug(&asset.name));
    let hash = blake3::hash(&std::fs::read(&asset.path)?).to_hex().to_string();
    let analyzed = compute_metrics(&asset.id, &asset.path)?;
    let baseline_y = analyzed
        .metrics
        .bounds
        .map(|(_min_x, _min_y, _max_x, max_y)| max_y)
        .unwrap_or(asset.height.saturating_sub(1));
    let pivot_x = analyzed
        .metrics
        .centroid
        .map(|(cx, _)| cx)
        .unwrap_or(asset.width as f64 / 2.0);
    let mut anchor = CharacterAnchor {
        slug,
        asset_id: asset.id.clone(),
        relative_path: asset.relative_path.clone(),
        frame_width: asset.width,
        frame_height: asset.height,
        pivot: Pivot {
            x: pivot_x,
            y: baseline_y as f64,
        },
        baseline_y,
        content_hash: hash,
        promoted_at: Utc::now().to_rfc3339(),
        view: None,
        facing: None,
        facing_confidence: None,
        facing_status: None,
    };
    super::facing::apply_facing_to_anchor(workspace_id, &mut anchor, auto_orient, view, state)?;
    Ok(anchor)
}

pub(crate) fn anchor_contract_text(workspace_id: &str, state: &AppState) -> String {
    match list_anchors_inner(workspace_id, state) {
        Ok(anchors) if anchors.is_empty() => String::new(),
        Ok(anchors) => {
            let lines = anchors
                .iter()
                .map(|anchor| {
                    format!(
                        "- slug `{slug}`: {width}x{height}px foot-anchored master at `{path}` (asset {asset_id})",
                        slug = anchor.slug,
                        width = anchor.frame_width,
                        height = anchor.frame_height,
                        path = anchor.relative_path,
                        asset_id = anchor.asset_id
                    )
                })
                .collect::<Vec<_>>();
            format!(
                "CHARACTER ANCHOR CONTRACT\nPromoted anchors in `.sprite-studio/anchors/` are authoritative for canvas size, pivot, and foot baseline. Match these exactly for every follow-up animation unless the user explicitly replaces the anchor.\n{}\n",
                lines.join("\n")
            )
        }
        Err(_) => String::new(),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromoteAnchorInput {
    pub workspace_id: String,
    pub asset_id: String,
    pub slug: Option<String>,
    pub auto_orient: Option<bool>,
    pub view: Option<String>,
}

#[tauri::command]
pub fn promote_anchor(input: PromoteAnchorInput, state: State<'_, AppState>) -> CommandResult<CharacterAnchor> {
    promote_anchor_inner(
        &input.workspace_id,
        &input.asset_id,
        input.slug.as_deref(),
        input.auto_orient.unwrap_or(false),
        input.view.as_deref(),
        &state,
    )
}

#[tauri::command]
pub fn get_anchor(
    workspace_id: String,
    slug: String,
    state: State<'_, AppState>,
) -> CommandResult<CharacterAnchor> {
    get_anchor_inner(&workspace_id, &slug, &state)
}

#[tauri::command]
pub fn list_anchors(
    workspace_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<CharacterAnchorSummary>> {
    list_anchors_inner(&workspace_id, &state)
}

#[cfg(test)]
mod tests {
    use super::portable_slug;

    #[test]
    fn slugifies_anchor_names() {
        assert_eq!(portable_slug("Hero Knight"), "hero-knight");
        assert_eq!(portable_slug("---"), "anchor");
    }
}
