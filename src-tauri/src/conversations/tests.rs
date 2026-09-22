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

#[test]
fn deleting_a_chat_removes_it_and_its_messages() {
    let connection = Connection::open_in_memory().expect("test database should open");
    connection.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            CREATE TABLE conversations (
              id TEXT PRIMARY KEY
            );
            CREATE TABLE messages (
              id TEXT PRIMARY KEY,
              conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
              content TEXT NOT NULL
            );
            CREATE TABLE conversation_log_entries (
              id TEXT PRIMARY KEY,
              conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
              request_id TEXT,
              level TEXT NOT NULL DEFAULT 'info',
              category TEXT NOT NULL,
              event_type TEXT NOT NULL,
              message TEXT NOT NULL,
              details_json TEXT NOT NULL DEFAULT '{}',
              created_at TEXT NOT NULL
            );
            INSERT INTO conversations(id) VALUES ('chat-1');
            INSERT INTO messages(id, conversation_id, content) VALUES ('message-1', 'chat-1', 'Gone');
            INSERT INTO conversation_log_entries(id, conversation_id, category, event_type, message, created_at)
              VALUES ('log-1', 'chat-1', 'chat', 'user_message', 'hello', 'now');
            "#,
        ).expect("test records should insert");

    connection
        .execute(
            "DELETE FROM conversation_log_entries WHERE conversation_id = ?1",
            ["chat-1"],
        )
        .expect("log entries should delete");
    connection
        .execute("DELETE FROM conversations WHERE id = ?1", ["chat-1"])
        .expect("chat should delete");

    let conversations: i64 = connection
        .query_row("SELECT COUNT(*) FROM conversations", [], |row| row.get(0))
        .expect("conversation count should query");
    let messages: i64 = connection
        .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
        .expect("message count should query");
    let log_entries: i64 = connection
        .query_row("SELECT COUNT(*) FROM conversation_log_entries", [], |row| row.get(0))
        .expect("log count should query");

    assert_eq!(conversations, 0);
    assert_eq!(messages, 0);
    assert_eq!(log_entries, 0);
}
