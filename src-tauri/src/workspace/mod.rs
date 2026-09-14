use crate::{
    error::{CommandError, CommandResult},
    models::Workspace,
    AppState,
};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use std::path::{Component, Path, PathBuf};
use tauri::State;
use uuid::Uuid;

mod bundled;
mod ffmpeg;
mod polish;
mod python;
use bundled::initialize_workspace;

pub use ffmpeg::{
    __cmd__check_ffmpeg_runtime, __tauri_command_name_check_ffmpeg_runtime, check_ffmpeg_runtime,
};
pub(crate) use ffmpeg::{initialize_ffmpeg_runtime, resolve_ffmpeg_executable, FFMPEG_MISSING_DETAIL};
pub use python::{
    __cmd__check_python_runtime, __tauri_command_name_check_python_runtime, check_python_runtime,
};
pub(crate) use python::{inject_python_commands, launcher_command_line, resolve_python_launcher};

pub use polish::{
    __cmd__archive_sprite_paths, __cmd__restore_sprite_paths, __cmd__run_sprite_polish,
    __tauri_command_name_archive_sprite_paths, __tauri_command_name_restore_sprite_paths,
    __tauri_command_name_run_sprite_polish, archive_sprite_paths, restore_sprite_paths,
    run_sprite_polish,
};

fn row_to_workspace(row: &rusqlite::Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
        created_at: row.get(3)?,
        last_opened_at: row.get(4)?,
    })
}

fn normalized_path(path: &str, create: bool) -> CommandResult<PathBuf> {
    let candidate = PathBuf::from(path);
    if create {
        std::fs::create_dir_all(&candidate)?;
    }
    if !candidate.is_dir() {
        return Err(CommandError::new(
            "workspace_missing",
            format!("Workspace directory does not exist: {path}"),
        ));
    }
    candidate.canonicalize().map_err(CommandError::from)
}

pub(crate) fn resolve_export_directory(
    workspace_root: &Path,
    destination: Option<&str>,
) -> CommandResult<PathBuf> {
    let output_directory = if let Some(path) = destination {
        let candidate = PathBuf::from(path);
        let resolved = if candidate.is_absolute() {
            reject_parent_traversal(&candidate)?;
            candidate
        } else {
            sanitize_workspace_join(workspace_root, path)?
        };
        ensure_within_workspace(workspace_root, &resolved)?;
        resolved
    } else {
        workspace_root.join("exports")
    };
    Ok(output_directory)
}

fn reject_parent_traversal(path: &Path) -> CommandResult<()> {
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err(CommandError::new(
                "path_outside_workspace",
                "Parent directory traversal is not allowed",
            ));
        }
    }
    Ok(())
}

fn sanitize_workspace_join(root: &Path, value: &str) -> CommandResult<PathBuf> {
    let mut joined = root.to_path_buf();
    for component in Path::new(value).components() {
        match component {
            Component::Normal(part) => joined.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => {
                return Err(CommandError::new(
                    "path_outside_workspace",
                    "Absolute or parent-relative export paths are not allowed",
                ));
            }
        }
    }
    Ok(joined)
}

fn ensure_within_workspace(workspace_root: &Path, candidate: &Path) -> CommandResult<()> {
    reject_parent_traversal(candidate)?;
    let canonical_root = workspace_root.canonicalize().map_err(CommandError::from)?;
    let check_path = if let Ok(relative) = candidate.strip_prefix(workspace_root) {
        canonical_root.join(relative)
    } else if candidate.exists() {
        candidate.canonicalize().map_err(CommandError::from)?
    } else if let Some(parent) = candidate.parent().filter(|path| path.exists()) {
        parent
            .canonicalize()
            .map_err(CommandError::from)?
            .join(
                candidate
                    .file_name()
                    .ok_or_else(|| CommandError::new("path_outside_workspace", "Invalid export path"))?,
            )
    } else {
        return Err(CommandError::new(
            "path_outside_workspace",
            "Export destination must stay inside the workspace",
        ));
    };
    if !check_path.starts_with(&canonical_root) {
        return Err(CommandError::new(
            "path_outside_workspace",
            "Export destination must stay inside the workspace",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod export_path_tests {
    use super::{resolve_export_directory, sanitize_workspace_join};
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    #[test]
    fn export_join_rejects_parent_traversal() {
        let root = PathBuf::from("C:\\workspace");
        let error = sanitize_workspace_join(&root, "exports/../outside")
            .expect_err("traversal should fail");
        assert_eq!(error.code, "path_outside_workspace");
    }

    #[test]
    fn export_directory_stays_inside_workspace() {
        let root = std::env::temp_dir().join(format!("sprite-export-root-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("temp root");
        let exports = root.join("exports");
        let resolved =
            resolve_export_directory(&root, Some("exports/sheets")).expect("resolve export");
        assert!(resolved.starts_with(&root));
        assert_eq!(resolved, exports.join("sheets"));
        let error = resolve_export_directory(&root, Some("exports/../../outside"))
            .expect_err("outside export");
        assert_eq!(error.code, "path_outside_workspace");
        fs::remove_dir_all(root).ok();
    }
}

pub fn workspace_path(state: &AppState, workspace_id: &str) -> CommandResult<PathBuf> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let path: Option<String> = connection
        .query_row(
            "SELECT path FROM projects WHERE id = ?1",
            [workspace_id],
            |row| row.get(0),
        )
        .optional()?;
    let path = path.ok_or_else(|| {
        CommandError::new("workspace_not_found", "Workspace is no longer registered")
    })?;
    let path = normalized_path(&path, false)?;
    initialize_workspace(&path)?;
    Ok(path)
}

#[tauri::command]
pub fn list_workspaces(state: State<'_, AppState>) -> CommandResult<Vec<Workspace>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut statement = connection.prepare("SELECT id, name, path, created_at, last_opened_at FROM projects ORDER BY last_opened_at DESC")?;
    let rows = statement.query_map([], row_to_workspace)?;
    Ok(rows.filter_map(Result::ok).collect())
}

/// Sidebar payload in one command: all projects plus the active project's
/// worktrees and chats, queried under a single database lock so the shell
/// renders in one IPC round trip.
#[tauri::command]
pub fn load_sidebar_state(
    workspace_id: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<crate::models::SidebarSnapshot> {
    load_sidebar_state_inner(&state, workspace_id.as_deref())
}

pub(crate) fn load_sidebar_state_inner(
    state: &AppState,
    workspace_id: Option<&str>,
) -> CommandResult<crate::models::SidebarSnapshot> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut statement = connection.prepare("SELECT id, name, path, created_at, last_opened_at FROM projects ORDER BY last_opened_at DESC")?;
    let workspaces: Vec<Workspace> = statement
        .query_map([], row_to_workspace)?
        .filter_map(Result::ok)
        .collect();
    let mut worktrees = Vec::new();
    let mut conversations = Vec::new();
    if let Some(workspace_id) = workspace_id {
        let mut statement = connection.prepare(
            "SELECT id, project_id, name, slug, kind, description, created_at, updated_at FROM worktrees WHERE project_id = ?1 ORDER BY CASE kind WHEN 'general' THEN 0 ELSE 1 END, updated_at DESC, name",
        )?;
        worktrees = statement
            .query_map([workspace_id], crate::worktrees::row_to_worktree)?
            .filter_map(Result::ok)
            .collect();
        let mut statement = connection.prepare(
            "SELECT id, workspace_id, worktree_id, title, provider, provider_session_id, created_at, updated_at, archived_at FROM conversations WHERE workspace_id = ?1 AND archived_at IS NULL ORDER BY updated_at DESC",
        )?;
        conversations = statement
            .query_map([workspace_id], crate::conversations::conversation_row)?
            .filter_map(Result::ok)
            .collect();
    }
    Ok(crate::models::SidebarSnapshot {
        workspaces,
        worktrees,
        conversations,
    })
}

#[tauri::command]
pub fn create_workspace(
    name: String,
    path: String,
    state: State<'_, AppState>,
) -> CommandResult<Workspace> {
    create_workspace_inner(name, path, &state)
}

pub(crate) fn create_workspace_inner(
    name: String,
    path: String,
    state: &AppState,
) -> CommandResult<Workspace> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CommandError::new(
            "invalid_name",
            "Workspace name cannot be empty",
        ));
    }
    let path = normalized_path(&path, true)?;
    initialize_workspace(&path)?;
    register_workspace(name, &path, state)
}

#[tauri::command]
pub fn open_workspace(path: String, state: State<'_, AppState>) -> CommandResult<Workspace> {
    open_workspace_inner(path, &state)
}

pub(crate) fn open_workspace_inner(path: String, state: &AppState) -> CommandResult<Workspace> {
    let path = normalized_path(&path, false)?;
    initialize_workspace(&path)?;
    let fallback_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Untitled workspace");
    register_workspace(fallback_name, &path, state)
}

fn register_workspace(name: &str, path: &Path, state: &AppState) -> CommandResult<Workspace> {
    let now = Utc::now().to_rfc3339();
    let path_string = path.to_string_lossy().into_owned();
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let existing: Option<Workspace> = connection
        .query_row(
            "SELECT id, name, path, created_at, last_opened_at FROM projects WHERE path = ?1",
            [&path_string],
            row_to_workspace,
        )
        .optional()?;
    if let Some(mut workspace) = existing {
        connection.execute(
            "UPDATE projects SET last_opened_at = ?1 WHERE id = ?2",
            params![now, workspace.id],
        )?;
        ensure_default_worktree(&connection, &workspace.id, &now)?;
        workspace.last_opened_at = now;
        return Ok(workspace);
    }
    let workspace = Workspace {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        path: path_string,
        created_at: now.clone(),
        last_opened_at: now,
    };
    connection.execute(
        "INSERT INTO projects(id, name, path, created_at, last_opened_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![workspace.id, workspace.name, workspace.path, workspace.created_at, workspace.last_opened_at],
    )?;
    ensure_default_worktree(&connection, &workspace.id, &workspace.created_at)?;
    Ok(workspace)
}

fn ensure_default_worktree(
    connection: &rusqlite::Connection,
    project_id: &str,
    timestamp: &str,
) -> CommandResult<()> {
    connection.execute(
        "INSERT OR IGNORE INTO worktrees(id, project_id, name, slug, kind, description, created_at, updated_at) VALUES (?1, ?2, 'General', 'general', 'general', 'Project-level assets and conversations', ?3, ?3)",
        params![Uuid::new_v4().to_string(), project_id, timestamp],
    )?;
    Ok(())
}

#[tauri::command]
pub fn touch_workspace(id: String, state: State<'_, AppState>) -> CommandResult<Workspace> {
    let now = Utc::now().to_rfc3339();
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        "UPDATE projects SET last_opened_at = ?1 WHERE id = ?2",
        params![now, id],
    )?;
    let workspace = connection
        .query_row(
            "SELECT id, name, path, created_at, last_opened_at FROM projects WHERE id = ?1",
            [&id],
            row_to_workspace,
        )
        .optional()?
        .ok_or_else(|| {
            CommandError::new("workspace_not_found", "Workspace is no longer registered")
        })?;
    if !Path::new(&workspace.path).is_dir() {
        return Err(CommandError::new(
            "workspace_missing",
            "The workspace directory was moved or deleted",
        ));
    }
    Ok(workspace)
}

#[tauri::command]
pub fn rename_workspace(id: String, name: String, state: State<'_, AppState>) -> CommandResult<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CommandError::new(
            "invalid_name",
            "Workspace name cannot be empty",
        ));
    }
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let changed = connection.execute(
        "UPDATE projects SET name = ?1 WHERE id = ?2",
        params![name, id],
    )?;
    if changed == 0 {
        return Err(CommandError::new(
            "workspace_not_found",
            "Workspace is no longer registered",
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn remove_workspace(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute("DELETE FROM projects WHERE id = ?1", [id])?;
    Ok(())
}

#[tauri::command]
pub fn delete_workspace(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let path = workspace_path(&state, &id)?;
    if path.parent().is_none() || path.components().count() < 3 {
        return Err(CommandError::new(
            "unsafe_delete",
            "Refusing to delete a broad filesystem path",
        ));
    }
    std::fs::remove_dir_all(&path)?;
    remove_workspace(id, state)
}

#[cfg(test)]
mod rig_mcp_guard_tests;
#[cfg(test)]
mod rig_mcp_tests;
#[cfg(test)]
mod rig_renderer_interrupt_tests;
#[cfg(test)]
mod rig_renderer_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
