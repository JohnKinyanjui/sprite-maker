use crate::{
    error::{CommandError, CommandResult},
    models::AnimationDirectionMeta,
    workspace::workspace_path,
    AppState,
};
use std::path::{Path, PathBuf};
use tauri::State;

pub(crate) fn direction_meta_directory(root: &Path) -> PathBuf {
    root.join(".sprite-studio").join("direction-meta")
}

fn direction_meta_path(root: &Path, animation_id: &str) -> PathBuf {
    direction_meta_directory(root).join(format!("{animation_id}.json"))
}

pub(crate) fn read_direction_meta(path: &Path) -> CommandResult<AnimationDirectionMeta> {
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(|error| {
        CommandError::new("invalid_direction_meta", format!("Direction meta is invalid: {error}"))
    })
}

pub(crate) fn write_direction_meta(root: &Path, meta: &AnimationDirectionMeta) -> CommandResult<()> {
    let path = direction_meta_path(root, &meta.animation_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_vec_pretty(meta)
        .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
    std::fs::write(path, payload)?;
    Ok(())
}

pub(crate) fn load_direction_meta(
    state: &AppState,
    workspace_id: &str,
    animation_id: &str,
) -> CommandResult<Option<AnimationDirectionMeta>> {
    let root = workspace_path(state, workspace_id)?;
    let path = direction_meta_path(&root, animation_id);
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(read_direction_meta(&path)?))
}

pub(crate) fn list_direction_meta_for_worktree(
    state: &AppState,
    workspace_id: &str,
    worktree_id: &str,
) -> CommandResult<Vec<AnimationDirectionMeta>> {
    let animation_ids = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let mut statement = connection.prepare(
            "SELECT id FROM animations WHERE workspace_id=?1 AND worktree_id=?2",
        )?;
        let rows = statement
            .query_map(rusqlite::params![workspace_id, worktree_id], |row| row.get::<_, String>(0))?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        rows
    };
    let root = workspace_path(state, workspace_id)?;
    let mut metas = Vec::new();
    for animation_id in animation_ids {
        let path = direction_meta_path(&root, &animation_id);
        if path.is_file() {
            metas.push(read_direction_meta(&path)?);
        }
    }
    Ok(metas)
}

pub(crate) fn infer_direction_family(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    for token in ["walk", "run", "idle", "attack", "hurt", "jump", "cast"] {
        if lower.contains(token) {
            return token.to_string();
        }
    }
    portable_family_slug(name)
}

fn portable_family_slug(name: &str) -> String {
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
        "motion".to_string()
    } else {
        trimmed.to_string()
    }
}

#[tauri::command]
pub fn list_direction_meta(
    workspace_id: String,
    worktree_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<AnimationDirectionMeta>> {
    list_direction_meta_for_worktree(&state, &workspace_id, &worktree_id)
}

pub(crate) fn infer_facing_from_name(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    for facing in ["nw", "ne", "sw", "se", "n", "s", "e", "w"] {
        if lower.ends_with(&format!("-{facing}"))
            || lower.contains(&format!("_{facing}"))
            || lower.contains(&format!(" {facing}"))
        {
            return Some(facing.to_string());
        }
    }
    None
}
