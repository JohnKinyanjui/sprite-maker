use super::{get_conversation, message_row};
use crate::models::Message;
use crate::{
    error::{CommandError, CommandResult},
    models::{
        AppendConversationLogInput, ConversationLogEntry, ExportConversationDebugLogInput,
    },
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use rusqlite::params;
use tauri::State;
use uuid::Uuid;

fn log_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConversationLogEntry> {
    let details: String = row.get(7)?;
    Ok(ConversationLogEntry {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        request_id: row.get(2)?,
        level: row.get(3)?,
        category: row.get(4)?,
        event_type: row.get(5)?,
        message: row.get(6)?,
        details: serde_json::from_str(&details).unwrap_or_default(),
        created_at: row.get(8)?,
    })
}

pub(crate) fn append_conversation_log(
    state: &AppState,
    conversation_id: &str,
    request_id: Option<&str>,
    level: &str,
    category: &str,
    event_type: &str,
    message: &str,
    details: serde_json::Value,
) -> CommandResult<()> {
    if message.trim().is_empty() {
        return Ok(());
    }
    let entry = ConversationLogEntry {
        id: Uuid::new_v4().to_string(),
        conversation_id: conversation_id.to_string(),
        request_id: request_id.map(str::to_string),
        level: level.to_string(),
        category: category.to_string(),
        event_type: event_type.to_string(),
        message: message.to_string(),
        details,
        created_at: Utc::now().to_rfc3339(),
    };
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        "INSERT INTO conversation_log_entries(id, conversation_id, request_id, level, category, event_type, message, details_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            entry.id,
            entry.conversation_id,
            entry.request_id,
            entry.level,
            entry.category,
            entry.event_type,
            entry.message,
            entry.details.to_string(),
            entry.created_at,
        ],
    )?;
    Ok(())
}

pub(crate) fn delete_conversation_log_entries(
    state: &AppState,
    conversation_id: &str,
) -> CommandResult<()> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        "DELETE FROM conversation_log_entries WHERE conversation_id = ?1",
        [conversation_id],
    )?;
    Ok(())
}

pub(crate) fn append_provider_event_log(
    state: &AppState,
    request_id: &str,
    conversation_id: &str,
    event_type: &str,
    content: &str,
) {
    if event_type == "content" {
        return;
    }
    let level = match event_type {
        "failed" => "error",
        "cancelled" => "warning",
        _ => "info",
    };
    let _ = append_conversation_log(
        state,
        conversation_id,
        Some(request_id),
        level,
        "provider",
        event_type,
        content,
        serde_json::json!({}),
    );
}

fn list_conversation_log_entries(
    state: &AppState,
    conversation_id: &str,
) -> CommandResult<Vec<ConversationLogEntry>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut statement = connection.prepare(
        "SELECT id, conversation_id, request_id, level, category, event_type, message, details_json, created_at
         FROM conversation_log_entries
         WHERE conversation_id = ?1
         ORDER BY created_at ASC, id ASC",
    )?;
    let rows = statement.query_map([conversation_id], log_row)?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn escape_markdown_fence(value: &str) -> String {
    value.replace("```", "'''")
}

fn format_details_block(details: &serde_json::Value) -> String {
    if details.is_null() || details.as_object().is_some_and(|value| value.is_empty()) {
        return String::new();
    }
    format!(
        "\n\n```json\n{}\n```",
        serde_json::to_string_pretty(details).unwrap_or_else(|_| details.to_string())
    )
}

pub(crate) fn export_conversation_debug_log_inner(
    conversation_id: &str,
    destination_path: &str,
    state: &AppState,
) -> CommandResult<String> {
    let conversation = get_conversation(state, conversation_id)?;
    let workspace = workspace_path(state, &conversation.workspace_id)?;
    let worktree_name = if let Some(worktree_id) = conversation.worktree_id.as_deref() {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                "SELECT name FROM worktrees WHERE id = ?1",
                [worktree_id],
                |row| row.get::<_, String>(0),
            )
            .ok()
    } else {
        None
    };
    let messages = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let mut statement = connection.prepare(
            "SELECT id, conversation_id, role, kind, content, status, metadata_json, created_at FROM messages WHERE conversation_id = ?1 ORDER BY created_at",
        )?;
        let rows = statement.query_map([conversation_id], message_row)?;
        rows.filter_map(Result::ok).collect::<Vec<Message>>()
    };
    let entries = list_conversation_log_entries(state, conversation_id)?;
    let exported_at = Utc::now().to_rfc3339();
    let mut markdown = String::new();
    markdown.push_str(&format!("# Chat debug log — {}\n\n", conversation.title));
    markdown.push_str(&format!("- **Provider:** `{}`\n", conversation.provider));
    markdown.push_str(&format!("- **Conversation ID:** `{}`\n", conversation.id));
    markdown.push_str(&format!("- **Workspace:** `{}`\n", workspace.display()));
    if let Some(name) = worktree_name {
        markdown.push_str(&format!("- **Worktree:** {}\n", name));
    }
    markdown.push_str(&format!("- **Created:** {}\n", conversation.created_at));
    markdown.push_str(&format!("- **Exported:** {}\n", exported_at));
    markdown.push_str("\n## Messages\n\n");
    if messages.is_empty() {
        markdown.push_str("_No messages recorded._\n\n");
    } else {
        for message in messages {
            markdown.push_str(&format!(
                "### {} · {} · {}\n\n",
                message.created_at,
                message.role,
                message.status
            ));
            if message.content.trim().is_empty() {
                markdown.push_str("_Empty content_\n\n");
            } else {
                markdown.push_str("```text\n");
                markdown.push_str(&escape_markdown_fence(&message.content));
                markdown.push_str("\n```\n\n");
            }
            if !message.metadata.is_null()
                && message.metadata.as_object().is_some_and(|value| !value.is_empty())
            {
                markdown.push_str("**Metadata**\n");
                markdown.push_str(&format_details_block(&message.metadata));
                markdown.push('\n');
            }
        }
    }
    markdown.push_str("## Activity log\n\n");
    if entries.is_empty() {
        markdown.push_str("_No structured activity entries recorded yet._\n");
    } else {
        for entry in entries {
            markdown.push_str(&format!(
                "### {} · {} · {} · {}\n\n",
                entry.created_at,
                entry.level.to_uppercase(),
                entry.category,
                entry.event_type
            ));
            if let Some(request_id) = entry.request_id.as_deref() {
                markdown.push_str(&format!("**Request:** `{}`\n\n", request_id));
            }
            markdown.push_str(&escape_markdown_fence(&entry.message));
            markdown.push_str(&format_details_block(&entry.details));
            markdown.push_str("\n\n");
        }
    }
    std::fs::write(destination_path, markdown)?;
    Ok(destination_path.to_string())
}

#[tauri::command]
pub fn append_conversation_log_entry(
    input: AppendConversationLogInput,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    append_conversation_log(
        &state,
        &input.conversation_id,
        input.request_id.as_deref(),
        &input.level,
        &input.category,
        &input.event_type,
        &input.message,
        input.details.unwrap_or_else(|| serde_json::json!({})),
    )
}

#[tauri::command]
pub fn export_conversation_debug_log(
    input: ExportConversationDebugLogInput,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    export_conversation_debug_log_inner(
        &input.conversation_id,
        &input.destination_path,
        &state,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database;
    use uuid::Uuid;

    #[test]
    fn conversation_debug_log_exports_markdown() {
        let workspace_root = std::env::temp_dir().join(format!("sprite-studio-log-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&workspace_root).expect("workspace dir");
        let path = workspace_root.join("metadata.sqlite3");
        let connection = database::open(&path).expect("db");
        let state = AppState::from_connection(connection);
        let now = Utc::now().to_rfc3339();
        {
            let connection = state.db.lock().expect("lock");
            connection
                .execute(
                    "INSERT INTO projects(id,name,path,created_at,last_opened_at) VALUES ('ws','Demo',?1,'now','now')",
                    [workspace_root.to_string_lossy().as_ref()],
                )
                .expect("workspace");
            connection
                .execute(
                    "INSERT INTO conversations(id,workspace_id,title,provider,created_at,updated_at) VALUES ('chat','ws','Knight idle','codex',?1,?1)",
                    [&now],
                )
                .expect("conversation");
            connection
                .execute(
                    "INSERT INTO messages(id,conversation_id,role,kind,content,status,metadata_json,created_at) VALUES ('m1','chat','user','text','make a knight','completed','{}',?1)",
                    [&now],
                )
                .expect("message");
        }
        append_conversation_log(
            &state,
            "chat",
            Some("req-1"),
            "info",
            "provider",
            "started",
            "Generation started",
            serde_json::json!({"provider": "codex"}),
        )
        .expect("log");
        let path = std::env::temp_dir().join("sprite-studio-chat-log-test.md");
        let exported =
            export_conversation_debug_log_inner("chat", path.to_str().expect("path"), &state)
                .expect("export");
        let content = std::fs::read_to_string(&exported).expect("read");
        assert!(content.contains("# Chat debug log — Knight idle"));
        assert!(content.contains("make a knight"));
        assert!(content.contains("Generation started"));
        let _ = std::fs::remove_file(&exported);
        let _ = std::fs::remove_dir_all(&workspace_root);
    }

    #[test]
    fn deleting_conversation_log_entries_removes_debug_log_rows() {
        let workspace_root = std::env::temp_dir().join(format!("sprite-studio-log-del-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&workspace_root).expect("workspace dir");
        let path = workspace_root.join("metadata.sqlite3");
        let connection = database::open(&path).expect("db");
        let state = AppState::from_connection(connection);
        let now = Utc::now().to_rfc3339();
        {
            let connection = state.db.lock().expect("lock");
            connection
                .execute(
                    "INSERT INTO projects(id,name,path,created_at,last_opened_at) VALUES ('ws','Demo',?1,'now','now')",
                    [workspace_root.to_string_lossy().as_ref()],
                )
                .expect("workspace");
            connection
                .execute(
                    "INSERT INTO conversations(id,workspace_id,title,provider,created_at,updated_at) VALUES ('chat','ws','Knight idle','codex',?1,?1)",
                    [&now],
                )
                .expect("conversation");
        }
        append_conversation_log(
            &state,
            "chat",
            Some("req-1"),
            "info",
            "provider",
            "started",
            "Generation started",
            serde_json::json!({}),
        )
        .expect("log");
        delete_conversation_log_entries(&state, "chat").expect("delete logs");
        {
            let connection = state.db.lock().expect("lock");
            let count: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM conversation_log_entries WHERE conversation_id = 'chat'",
                    [],
                    |row| row.get(0),
                )
                .expect("count");
            assert_eq!(count, 0);
        }
        let _ = std::fs::remove_dir_all(&workspace_root);
    }
}
