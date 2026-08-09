use crate::{
    error::{CommandError, CommandResult},
    AppState,
};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use tauri::State;

#[tauri::command]
pub fn get_setting(key: String, state: State<'_, AppState>) -> CommandResult<serde_json::Value> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let value: Option<String> = connection
        .query_row(
            "SELECT value_json FROM settings WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .optional()?;
    value
        .map(|value| {
            serde_json::from_str(&value)
                .map_err(|error| CommandError::new("invalid_setting", error.to_string()))
        })
        .transpose()
        .map(|value| value.unwrap_or(serde_json::Value::Null))
}

#[tauri::command]
pub fn set_setting(
    key: String,
    value: serde_json::Value,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    if key.trim().is_empty() {
        return Err(CommandError::new(
            "invalid_setting",
            "Setting key cannot be empty",
        ));
    }
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        "INSERT INTO settings(key, value_json, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json, updated_at=excluded.updated_at",
        params![key, value.to_string(), Utc::now().to_rfc3339()],
    )?;
    Ok(())
}
