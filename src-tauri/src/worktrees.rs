use crate::{
    error::{CommandError, CommandResult},
    models::Worktree,
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use tauri::State;
use uuid::Uuid;

const KINDS: [&str; 9] = [
    "general",
    "character",
    "environment",
    "creature",
    "object",
    "tileset",
    "animation",
    "vfx",
    "ui",
];

fn row_to_worktree(row: &rusqlite::Row<'_>) -> rusqlite::Result<Worktree> {
    Ok(Worktree {
        id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        slug: row.get(3)?,
        kind: row.get(4)?,
        description: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn validate_kind(kind: &str) -> CommandResult<&str> {
    KINDS.contains(&kind).then_some(kind).ok_or_else(|| {
        CommandError::new("invalid_worktree_kind", "Choose a supported worktree type")
    })
}

fn slug(value: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            separator = false;
        } else if !separator && !output.is_empty() {
            output.push('-');
            separator = true;
        }
    }
    output.trim_matches('-').to_string()
}

fn available_slug(
    connection: &rusqlite::Connection,
    project_id: &str,
    name: &str,
) -> CommandResult<String> {
    let base = slug(name);
    if base.is_empty() {
        return Err(CommandError::new(
            "invalid_worktree_name",
            "Worktree name must include a letter or number",
        ));
    }
    for suffix in 1..=10_000 {
        let candidate = if suffix == 1 {
            base.clone()
        } else {
            format!("{base}-{suffix}")
        };
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM worktrees WHERE project_id = ?1 AND slug = ?2)",
            params![project_id, candidate],
            |row| row.get(0),
        )?;
        if !exists {
            return Ok(candidate);
        }
    }
    Err(CommandError::new(
        "worktree_slug_exhausted",
        "Could not create a unique worktree folder name",
    ))
}

#[tauri::command]
pub fn list_worktrees(
    project_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Worktree>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut statement = connection.prepare(
        "SELECT id, project_id, name, slug, kind, description, created_at, updated_at FROM worktrees WHERE project_id = ?1 ORDER BY CASE kind WHEN 'general' THEN 0 ELSE 1 END, updated_at DESC, name",
    )?;
    let rows = statement.query_map([project_id], row_to_worktree)?;
    Ok(rows.filter_map(Result::ok).collect())
}

#[tauri::command]
pub fn create_worktree(
    project_id: String,
    name: String,
    kind: String,
    description: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<Worktree> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CommandError::new(
            "invalid_worktree_name",
            "Worktree name cannot be empty",
        ));
    }
    let kind = validate_kind(kind.trim())?;
    let now = Utc::now().to_rfc3339();
    let worktree = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let project_exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?1)",
            [&project_id],
            |row| row.get(0),
        )?;
        if !project_exists {
            return Err(CommandError::new(
                "project_not_found",
                "Project is no longer registered",
            ));
        }
        let worktree = Worktree {
            id: Uuid::new_v4().to_string(),
            project_id: project_id.clone(),
            name: name.to_string(),
            slug: available_slug(&connection, &project_id, name)?,
            kind: kind.to_string(),
            description: description
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            created_at: now.clone(),
            updated_at: now,
        };
        connection.execute(
            "INSERT INTO worktrees(id, project_id, name, slug, kind, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![worktree.id, worktree.project_id, worktree.name, worktree.slug, worktree.kind, worktree.description, worktree.created_at, worktree.updated_at],
        )?;
        worktree
    };
    let root = workspace_path(&state, &project_id)?
        .join("worktrees")
        .join(&worktree.slug);
    for folder in ["references", "exports"] {
        std::fs::create_dir_all(root.join(folder))?;
    }
    Ok(worktree)
}

#[tauri::command]
pub fn update_worktree(
    id: String,
    name: String,
    description: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<Worktree> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CommandError::new(
            "invalid_worktree_name",
            "Worktree name cannot be empty",
        ));
    }
    let description = description
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let now = Utc::now().to_rfc3339();
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let changed = connection.execute(
        "UPDATE worktrees SET name = ?1, description = ?2, updated_at = ?3 WHERE id = ?4",
        params![name, description, now, id],
    )?;
    if changed == 0 {
        return Err(CommandError::new(
            "worktree_not_found",
            "Worktree no longer exists",
        ));
    }
    connection
        .query_row(
            "SELECT id, project_id, name, slug, kind, description, created_at, updated_at FROM worktrees WHERE id = ?1",
            [id],
            row_to_worktree,
        )
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn delete_worktree(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let worktree: Option<(String, String)> = connection
        .query_row(
            "SELECT project_id, kind FROM worktrees WHERE id = ?1",
            [&id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((_project_id, kind)) = worktree else {
        return Err(CommandError::new(
            "worktree_not_found",
            "Worktree no longer exists",
        ));
    };
    if kind == "general" {
        return Err(CommandError::new(
            "protected_worktree",
            "The General worktree preserves project-level and migrated content",
        ));
    }
    connection.execute("DELETE FROM worktrees WHERE id = ?1", [id])?;
    Ok(())
}

#[tauri::command]
pub fn list_worktree_asset_ids(
    worktree_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<String>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut statement = connection.prepare(
        "SELECT asset_id FROM asset_worktrees WHERE worktree_id = ?1 ORDER BY created_at",
    )?;
    let rows = statement.query_map([worktree_id], |row| row.get(0))?;
    Ok(rows.filter_map(Result::ok).collect())
}

#[tauri::command]
pub fn link_asset_to_worktree(
    worktree_id: String,
    asset_id: String,
    relationship: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let relationship = relationship.unwrap_or_else(|| "owned".to_string());
    if !matches!(relationship.as_str(), "owned" | "referenced") {
        return Err(CommandError::new(
            "invalid_asset_relationship",
            "Asset relationship must be owned or referenced",
        ));
    }
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let same_project: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM worktrees w JOIN assets a ON a.workspace_id = w.project_id WHERE w.id = ?1 AND a.id = ?2)",
        params![worktree_id, asset_id],
        |row| row.get(0),
    )?;
    if !same_project {
        return Err(CommandError::new(
            "asset_worktree_mismatch",
            "Asset and worktree must belong to the same project",
        ));
    }
    connection.execute(
        "INSERT INTO asset_worktrees(asset_id, worktree_id, relationship, created_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(asset_id, worktree_id) DO UPDATE SET relationship = excluded.relationship",
        params![asset_id, worktree_id, relationship, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{slug, validate_kind};

    #[test]
    fn validates_explicit_worktree_types() {
        for kind in [
            "general",
            "character",
            "environment",
            "creature",
            "object",
            "tileset",
            "animation",
            "vfx",
            "ui",
        ] {
            assert_eq!(validate_kind(kind).expect("kind should be valid"), kind);
        }
        assert!(validate_kind("unknown").is_err());
    }

    #[test]
    fn creates_portable_worktree_slugs() {
        assert_eq!(slug("Knight / One-Handed"), "knight-one-handed");
        assert_eq!(slug("  Fire   Magic  "), "fire-magic");
    }
}
