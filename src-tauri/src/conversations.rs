use crate::{
    error::{CommandError, CommandResult},
    models::{Conversation, Message},
    AppState,
};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use tauri::State;
use uuid::Uuid;

pub(crate) fn conversation_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Conversation> {
    Ok(Conversation {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        worktree_id: row.get(2)?,
        title: row.get(3)?,
        provider: row.get(4)?,
        provider_session_id: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        archived_at: row.get(8)?,
    })
}

fn message_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
    let metadata: String = row.get(6)?;
    Ok(Message {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        role: row.get(2)?,
        kind: row.get(3)?,
        content: row.get(4)?,
        status: row.get(5)?,
        metadata: serde_json::from_str(&metadata).unwrap_or_default(),
        created_at: row.get(7)?,
    })
}

#[tauri::command]
pub fn list_conversations(
    workspace_id: String,
    worktree_id: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Conversation>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let select = "SELECT id, workspace_id, worktree_id, title, provider, provider_session_id, created_at, updated_at, archived_at FROM conversations";
    let mut statement = if worktree_id.is_some() {
        connection.prepare(&format!(
            "{select} WHERE workspace_id=?1 AND worktree_id=?2 AND archived_at IS NULL ORDER BY updated_at DESC"
        ))?
    } else {
        connection.prepare(&format!(
            "{select} WHERE workspace_id=?1 AND archived_at IS NULL ORDER BY updated_at DESC"
        ))?
    };
    let rows = if let Some(worktree_id) = worktree_id {
        statement.query_map(params![workspace_id, worktree_id], conversation_row)?
    } else {
        statement.query_map([workspace_id], conversation_row)?
    };
    Ok(rows.filter_map(Result::ok).collect())
}

#[tauri::command]
pub fn list_archived_conversations(
    workspace_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Conversation>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut statement = connection.prepare(
        "SELECT id, workspace_id, worktree_id, title, provider, provider_session_id, created_at, updated_at, archived_at
         FROM conversations
         WHERE workspace_id=?1 AND archived_at IS NOT NULL
         ORDER BY archived_at DESC",
    )?;
    let rows = statement.query_map([workspace_id], conversation_row)?;
    Ok(rows.filter_map(Result::ok).collect())
}

#[tauri::command]
pub fn create_conversation(
    workspace_id: String,
    worktree_id: Option<String>,
    title: Option<String>,
    provider: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<Conversation> {
    let now = Utc::now().to_rfc3339();
    let conversation = Conversation {
        id: Uuid::new_v4().to_string(),
        workspace_id,
        worktree_id,
        title: title.unwrap_or_else(|| "New conversation".into()),
        provider: provider.unwrap_or_else(|| "codex".into()),
        provider_session_id: None,
        created_at: now.clone(),
        updated_at: now,
        archived_at: None,
    };
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    if let Some(worktree_id) = &conversation.worktree_id {
        let valid: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM worktrees WHERE id=?1 AND project_id=?2)",
            params![worktree_id, conversation.workspace_id],
            |row| row.get(0),
        )?;
        if !valid {
            return Err(CommandError::new(
                "invalid_worktree",
                "Chat worktree must belong to the same project",
            ));
        }
    }
    connection.execute(
        "INSERT INTO conversations(id, workspace_id, worktree_id, title, provider, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![conversation.id, conversation.workspace_id, conversation.worktree_id, conversation.title, conversation.provider, conversation.created_at, conversation.updated_at],
    )?;
    Ok(conversation)
}

#[tauri::command]
pub fn rename_conversation(
    id: String,
    title: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let title = title.trim();
    if title.is_empty() {
        return Err(CommandError::new(
            "invalid_title",
            "Conversation title cannot be empty",
        ));
    }
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let changed = connection.execute(
        "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
        params![title, Utc::now().to_rfc3339(), id],
    )?;
    if changed == 0 {
        return Err(CommandError::new(
            "conversation_not_found",
            "Chat was not found",
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn switch_conversation_provider(
    id: String,
    provider: String,
    state: State<'_, AppState>,
) -> CommandResult<Conversation> {
    if !matches!(provider.as_str(), "codex" | "claude" | "gemini" | "grok") {
        return Err(CommandError::new(
            "provider_unsupported",
            "Choose one of the installed chat providers",
        ));
    }
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let changed = connection.execute(
        "UPDATE conversations SET provider = ?1, provider_session_id = NULL, updated_at = ?2 WHERE id = ?3 AND archived_at IS NULL",
        params![provider, Utc::now().to_rfc3339(), id],
    )?;
    if changed == 0 {
        return Err(CommandError::new(
            "conversation_not_found",
            "Chat was not found",
        ));
    }
    connection.query_row(
        "SELECT id, workspace_id, worktree_id, title, provider, provider_session_id, created_at, updated_at, archived_at FROM conversations WHERE id = ?1",
        [id],
        conversation_row,
    ).map_err(Into::into)
}

#[tauri::command]
pub fn archive_conversation(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    archive_conversation_record(&connection, &id, &Utc::now().to_rfc3339())
}

fn archive_conversation_record(connection: &Connection, id: &str, now: &str) -> CommandResult<()> {
    let changed = connection.execute(
        "UPDATE conversations SET archived_at = ?1, updated_at = ?1 WHERE id = ?2 AND archived_at IS NULL",
        params![now, id],
    )?;
    if changed == 0 {
        return Err(CommandError::new(
            "conversation_not_found",
            "Chat was not found or is already archived",
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn restore_conversation(id: String, state: State<'_, AppState>) -> CommandResult<Conversation> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    restore_conversation_record(&connection, &id, &Utc::now().to_rfc3339())
}

fn restore_conversation_record(
    connection: &Connection,
    id: &str,
    now: &str,
) -> CommandResult<Conversation> {
    let changed = connection.execute(
        "UPDATE conversations SET archived_at = NULL, updated_at = ?1 WHERE id = ?2 AND archived_at IS NOT NULL",
        params![now, id],
    )?;
    if changed == 0 {
        return Err(CommandError::new(
            "conversation_not_found",
            "Chat was not found or is already active",
        ));
    }
    connection
        .query_row(
            "SELECT id, workspace_id, worktree_id, title, provider, provider_session_id, created_at, updated_at, archived_at FROM conversations WHERE id = ?1",
            [id],
            conversation_row,
        )
        .map_err(Into::into)
}

#[tauri::command]
pub fn delete_conversation(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute("DELETE FROM conversations WHERE id = ?1", [id])?;
    Ok(())
}

#[tauri::command]
pub fn list_messages(
    conversation_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Message>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut statement = connection.prepare(
        "SELECT id, conversation_id, role, kind, content, status, metadata_json, created_at FROM messages WHERE conversation_id = ?1 ORDER BY created_at"
    )?;
    let rows = statement.query_map([conversation_id], message_row)?;
    Ok(rows.filter_map(Result::ok).collect())
}

pub fn add_message(
    state: &AppState,
    conversation_id: &str,
    role: &str,
    kind: &str,
    content: &str,
    status: &str,
) -> CommandResult<Message> {
    let message = Message {
        id: Uuid::new_v4().to_string(),
        conversation_id: conversation_id.to_string(),
        role: role.to_string(),
        kind: kind.to_string(),
        content: content.to_string(),
        status: status.to_string(),
        metadata: serde_json::json!({}),
        created_at: Utc::now().to_rfc3339(),
    };
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        "INSERT INTO messages(id, conversation_id, role, kind, content, status, metadata_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![message.id, message.conversation_id, message.role, message.kind, message.content, message.status, message.metadata.to_string(), message.created_at],
    )?;
    connection.execute(
        "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
        params![Utc::now().to_rfc3339(), conversation_id],
    )?;
    Ok(message)
}

pub fn update_message(
    state: &AppState,
    id: &str,
    content: &str,
    status: &str,
) -> CommandResult<()> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        "UPDATE messages SET content = ?1, status = ?2 WHERE id = ?3",
        params![content, status, id],
    )?;
    Ok(())
}

#[tauri::command]
pub fn update_message_metadata(
    id: String,
    metadata: serde_json::Value,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    if !metadata.is_object() {
        return Err(CommandError::new(
            "invalid_metadata",
            "Message metadata must be a JSON object",
        ));
    }
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let changed = connection.execute(
        "UPDATE messages SET metadata_json = ?1 WHERE id = ?2",
        params![metadata.to_string(), id],
    )?;
    if changed == 0 {
        return Err(CommandError::new(
            "message_not_found",
            "Message no longer exists",
        ));
    }
    Ok(())
}

pub fn set_provider_session(
    state: &AppState,
    conversation_id: &str,
    session_id: &str,
) -> CommandResult<()> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        "UPDATE conversations SET provider_session_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![session_id, Utc::now().to_rfc3339(), conversation_id],
    )?;
    Ok(())
}

pub fn get_conversation(state: &AppState, id: &str) -> CommandResult<Conversation> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.query_row(
        "SELECT id, workspace_id, worktree_id, title, provider, provider_session_id, created_at, updated_at, archived_at FROM conversations WHERE id = ?1 AND archived_at IS NULL",
        [id], conversation_row
    ).optional()?.ok_or_else(|| CommandError::new("conversation_not_found", "Conversation no longer exists"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archiving_hides_a_chat_without_deleting_its_messages() {
        let connection = Connection::open_in_memory().expect("test database should open");
        connection.execute_batch(
            r#"
            CREATE TABLE conversations (
              id TEXT PRIMARY KEY,
              updated_at TEXT NOT NULL,
              archived_at TEXT
            );
            CREATE TABLE messages (
              id TEXT PRIMARY KEY,
              conversation_id TEXT NOT NULL,
              content TEXT NOT NULL
            );
            INSERT INTO conversations(id, updated_at) VALUES ('chat-1', 'before');
            INSERT INTO messages(id, conversation_id, content) VALUES ('message-1', 'chat-1', 'Keep me');
            "#,
        ).expect("test records should insert");

        archive_conversation_record(&connection, "chat-1", "2026-08-09T12:00:00Z")
            .expect("chat should archive");

        let active: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM conversations WHERE id='chat-1' AND archived_at IS NULL",
                [],
                |row| row.get(0),
            )
            .expect("active chat count should query");
        let messages: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE conversation_id='chat-1'",
                [],
                |row| row.get(0),
            )
            .expect("message count should query");

        assert_eq!(active, 0);
        assert_eq!(messages, 1);
        assert!(archive_conversation_record(&connection, "chat-1", "later").is_err());
    }

    #[test]
    fn restoring_an_archived_chat_makes_it_active_again() {
        let connection = Connection::open_in_memory().expect("test database should open");
        connection.execute_batch(
            r#"
            CREATE TABLE conversations (
              id TEXT PRIMARY KEY,
              workspace_id TEXT NOT NULL,
              worktree_id TEXT,
              title TEXT NOT NULL,
              provider TEXT NOT NULL,
              provider_session_id TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              archived_at TEXT
            );
            INSERT INTO conversations(
              id, workspace_id, worktree_id, title, provider, created_at, updated_at, archived_at
            ) VALUES (
              'chat-1', 'project-1', 'tree-1', 'Recovered chat', 'codex', 'created', 'archived', 'archived'
            );
            "#,
        ).expect("test record should insert");

        let restored = restore_conversation_record(&connection, "chat-1", "restored")
            .expect("chat should restore");

        assert_eq!(restored.title, "Recovered chat");
        assert_eq!(restored.worktree_id.as_deref(), Some("tree-1"));
        assert!(restored.archived_at.is_none());
        assert!(restore_conversation_record(&connection, "chat-1", "later").is_err());
    }
}
