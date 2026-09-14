use crate::{
    error::{CommandError, CommandResult},
    models::{GenerationManifest, GenerationSession, GenerationSessionIndex},
    workspace::workspace_path,
    AppState,
};

#[derive(Debug, Clone)]
pub(crate) struct ManifestSessionHook {
    pub worktree_id: String,
    pub kind: String,
    pub animation_id: Option<String>,
    pub score: Option<f64>,
}

pub(crate) fn append_session_from_manifest(
    workspace_root: &Path,
    manifest: &GenerationManifest,
    hook: &ManifestSessionHook,
) -> CommandResult<()> {
    append_generation_session(
        workspace_root,
        &hook.worktree_id,
        &hook.kind,
        hook.animation_id.as_deref(),
        ".sprite-studio/last-generation.json",
        manifest.files.first().map(String::as_str),
        hook.score,
    )
}
use chrono::Utc;
use std::path::{Path, PathBuf};
use tauri::State;
use uuid::Uuid;

const SESSION_INDEX_VERSION: u32 = 1;
const SESSION_INDEX_FILE: &str = "generation-sessions.json";
const SESSION_CAP: usize = 200;

pub(crate) fn sessions_index_path(root: &Path) -> PathBuf {
    root.join(".sprite-studio").join(SESSION_INDEX_FILE)
}

fn load_session_index(path: &Path) -> GenerationSessionIndex {
    if !path.is_file() {
        return GenerationSessionIndex {
            version: SESSION_INDEX_VERSION,
            sessions: Vec::new(),
        };
    }
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<GenerationSessionIndex>(&bytes).ok())
        .unwrap_or(GenerationSessionIndex {
            version: SESSION_INDEX_VERSION,
            sessions: Vec::new(),
        })
}

fn save_session_index(path: &Path, index: &GenerationSessionIndex) -> CommandResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_vec_pretty(index)
        .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
    std::fs::write(path, payload)?;
    Ok(())
}

pub(crate) fn append_generation_session(
    workspace_root: &Path,
    worktree_id: &str,
    kind: &str,
    animation_id: Option<&str>,
    manifest_path: &str,
    thumbnail_path: Option<&str>,
    score: Option<f64>,
) -> CommandResult<()> {
    let path = sessions_index_path(workspace_root);
    let mut index = load_session_index(&path);
    index.version = SESSION_INDEX_VERSION;
    index.sessions.insert(
        0,
        GenerationSession {
            session_id: Uuid::new_v4().to_string(),
            worktree_id: worktree_id.to_string(),
            kind: kind.to_string(),
            animation_id: animation_id.map(str::to_string),
            manifest_path: manifest_path.to_string(),
            thumbnail_path: thumbnail_path.map(str::to_string),
            score,
            created_at: Utc::now().to_rfc3339(),
        },
    );
    if index.sessions.len() > SESSION_CAP {
        index.sessions.truncate(SESSION_CAP);
    }
    save_session_index(&path, &index)
}

pub(crate) fn append_animation_job_session(
    state: &AppState,
    animation_id: &str,
    kind: &str,
    thumbnail_path: Option<&str>,
    score: Option<f64>,
) {
    let row = {
        let connection = state.db.lock();
        if let Ok(connection) = connection {
            connection
                .query_row(
                    "SELECT workspace_id, worktree_id FROM animations WHERE id=?1",
                    [animation_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
                )
                .ok()
        } else {
            None
        }
    };
    if let Some((workspace_id, Some(worktree_id))) = row {
        append_generation_session_for_workspace(
            state,
            &workspace_id,
            &worktree_id,
            kind,
            Some(animation_id),
            ".sprite-studio/last-generation.json",
            thumbnail_path,
            score,
        );
    }
}

pub(crate) fn append_generation_session_for_workspace(
    state: &AppState,
    workspace_id: &str,
    worktree_id: &str,
    kind: &str,
    animation_id: Option<&str>,
    manifest_path: &str,
    thumbnail_path: Option<&str>,
    score: Option<f64>,
) {
    if let Ok(root) = workspace_path(state, workspace_id) {
        let _ = append_generation_session(
            &root,
            worktree_id,
            kind,
            animation_id,
            manifest_path,
            thumbnail_path,
            score,
        );
    }
}

pub(crate) fn list_generation_sessions_inner(
    state: &AppState,
    workspace_id: &str,
    worktree_id: Option<&str>,
) -> CommandResult<Vec<GenerationSession>> {
    let root = workspace_path(state, workspace_id)?;
    let index = load_session_index(&sessions_index_path(&root));
    Ok(index
        .sessions
        .into_iter()
        .filter(|session| {
            worktree_id
                .map(|id| session.worktree_id == id)
                .unwrap_or(true)
        })
        .collect())
}

#[tauri::command]
pub fn list_generation_sessions(
    workspace_id: String,
    worktree_id: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<GenerationSession>> {
    list_generation_sessions_inner(
        &state,
        &workspace_id,
        worktree_id.as_deref(),
    )
}
