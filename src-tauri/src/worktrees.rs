use crate::{
    error::{CommandError, CommandResult},
    models::Worktree,
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
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

pub(crate) fn row_to_worktree(row: &rusqlite::Row<'_>) -> rusqlite::Result<Worktree> {
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

fn delete_worktree_record(connection: &mut Connection, id: &str) -> CommandResult<()> {
    let worktree: Option<(String, String)> = connection
        .query_row(
            "SELECT project_id, kind FROM worktrees WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((project_id, kind)) = worktree else {
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
    let general_id: String = connection
        .query_row(
            "SELECT id FROM worktrees WHERE project_id = ?1 AND kind = 'general' ORDER BY created_at LIMIT 1",
            [&project_id],
            |row| row.get(0),
        )
        .map_err(|_| CommandError::new(
            "general_worktree_missing",
            "The project General worktree is missing; reopen the project and try again",
        ))?;
    let running: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM conversations c JOIN messages m ON m.conversation_id = c.id WHERE c.worktree_id = ?1 AND m.status = 'running')",
        [id],
        |row| row.get(0),
    )?;
    if running {
        return Err(CommandError::new(
            "worktree_busy",
            "Stop active chat generation before deleting this worktree",
        ));
    }

    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT OR IGNORE INTO asset_worktrees(asset_id, worktree_id, relationship, created_at) SELECT asset_id, ?1, relationship, created_at FROM asset_worktrees WHERE worktree_id = ?2",
        params![general_id, id],
    )?;
    for table in [
        "conversations",
        "generations",
        "animations",
        "reference_images",
        "background_jobs",
        "sprite_sheets",
        "vfx_effects",
        "quality_reports",
        "rigs",
    ] {
        transaction.execute(
            &format!("UPDATE {table} SET worktree_id = ?1 WHERE worktree_id = ?2"),
            params![general_id, id],
        )?;
    }
    transaction.execute("DELETE FROM asset_worktrees WHERE worktree_id = ?1", [id])?;
    transaction.execute("DELETE FROM worktrees WHERE id = ?1", [id])?;
    transaction.commit()?;
    Ok(())
}

#[tauri::command]
pub fn delete_worktree(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let mut connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    delete_worktree_record(&mut connection, &id)
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
    use super::{delete_worktree_record, slug, validate_kind};
    use crate::database;
    use rusqlite::params;

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

    #[test]
    fn deleting_a_worktree_moves_content_and_rigs_to_general() {
        let path = std::env::temp_dir().join(format!(
            "sprite-studio-worktree-delete-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let mut connection = database::open(&path).expect("database should open");
        connection.execute(
            "INSERT INTO projects(id, name, path, created_at, last_opened_at) VALUES ('p1','Test','test','now','now')",
            [],
        ).expect("project should insert");
        for (id, name, slug, kind) in [
            ("general", "General", "general", "general"),
            ("terrain", "Terrain", "terrain", "tileset"),
        ] {
            connection.execute(
                "INSERT INTO worktrees(id, project_id, name, slug, kind, created_at, updated_at) VALUES (?1,'p1',?2,?3,?4,'now','now')",
                params![id, name, slug, kind],
            ).expect("worktree should insert");
        }
        connection.execute(
            "INSERT INTO conversations(id, workspace_id, worktree_id, title, provider, created_at, updated_at) VALUES ('c1','p1','terrain','Chat','codex','now','now')",
            [],
        ).expect("conversation should insert");
        connection.execute(
            "INSERT INTO messages(id, conversation_id, role, content, status, created_at) VALUES ('m1','c1','user','hello','completed','now')",
            [],
        ).expect("message should insert");
        connection.execute(
            "INSERT INTO assets(id, workspace_id, name, path, relative_path, category, format, width, height, file_size, created_at) VALUES ('a1','p1','Tile','tile.png','assets/terrain/tile.png','terrain','png',1,1,1,'now')",
            [],
        ).expect("asset should insert");
        connection.execute(
            "INSERT INTO asset_worktrees(asset_id, worktree_id, relationship, created_at) VALUES ('a1','terrain','owned','now')",
            [],
        ).expect("asset link should insert");
        connection.execute(
            "INSERT INTO rigs(id, workspace_id, worktree_id, asset_id, name, spec_json, created_at, updated_at) VALUES ('r1','p1','terrain','a1','Tile rig','{}','now','now')",
            [],
        ).expect("rig should insert");

        delete_worktree_record(&mut connection, "terrain").expect("delete should succeed");

        let chat_worktree: String = connection
            .query_row(
                "SELECT worktree_id FROM conversations WHERE id='c1'",
                [],
                |row| row.get(0),
            )
            .expect("conversation should remain");
        let rig_worktree: String = connection
            .query_row("SELECT worktree_id FROM rigs WHERE id='r1'", [], |row| {
                row.get(0)
            })
            .expect("rig should remain");
        assert_eq!(chat_worktree, "general");
        assert_eq!(rig_worktree, "general");
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM messages WHERE conversation_id='c1'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("message count should work"),
            1
        );
        assert_eq!(connection.query_row(
            "SELECT COUNT(*) FROM asset_worktrees WHERE asset_id='a1' AND worktree_id='general'", [], |row| row.get::<_, i64>(0),
        ).expect("asset link count should work"), 1);
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM worktrees WHERE id='terrain'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("worktree count should work"),
            0
        );
        drop(connection);
        let _ = std::fs::remove_file(path);
    }
}
