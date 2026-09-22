use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub path: String,
    pub created_at: String,
    pub last_opened_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBackup {
    pub format_version: u32,
    pub project_id: String,
    pub project_name: String,
    pub source_path: String,
    pub backup_path: String,
    pub created_at: String,
    pub file_count: u64,
    pub total_bytes: u64,
}

/// One-round-trip sidebar payload: every project plus the active project's
/// worktrees and chats, read under a single database lock.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarSnapshot {
    pub workspaces: Vec<Workspace>,
    pub worktrees: Vec<Worktree>,
    pub conversations: Vec<Conversation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub slug: String,
    pub kind: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub workspace_id: String,
    pub worktree_id: Option<String>,
    pub title: String,
    pub provider: String,
    pub provider_session_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub kind: String,
    pub content: String,
    pub status: String,
    pub metadata: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationLogEntry {
    pub id: String,
    pub conversation_id: String,
    pub request_id: Option<String>,
    pub level: String,
    pub category: String,
    pub event_type: String,
    pub message: String,
    pub details: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppendConversationLogInput {
    pub conversation_id: String,
    pub request_id: Option<String>,
    pub level: String,
    pub category: String,
    pub event_type: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportConversationDebugLogInput {
    pub conversation_id: String,
    pub destination_path: String,
}
