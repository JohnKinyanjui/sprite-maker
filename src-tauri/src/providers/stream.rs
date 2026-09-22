use super::antigravity_stream::parse_antigravity_line;
use super::cursor_stream::parse_cursor_line;
use crate::conversations::append_provider_event_log;
use crate::models::ProviderEvent;
use crate::AppState;
use tauri::{AppHandle, Emitter};

pub(crate) fn emit(
    app: Option<&AppHandle>,
    state: &AppState,
    request_id: &str,
    conversation_id: &str,
    event_type: &str,
    content: impl Into<String>,
) {
    let event = ProviderEvent {
        request_id: request_id.to_string(),
        conversation_id: conversation_id.to_string(),
        event_type: event_type.to_string(),
        content: content.into(),
    };
    state.record_provider_event(&event);
    append_provider_event_log(
        state,
        request_id,
        conversation_id,
        event_type,
        &event.content,
    );
    if let Some(app) = app {
        let _ = app.emit("provider-event", event);
    }
}

pub(crate) fn parse_codex_line(line: &str) -> (Option<String>, Option<String>, Option<String>) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return (Some(line.to_string()), None, None);
    };
    let event_type = value
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("activity");
    if event_type == "thread.started" {
        return (
            None,
            Some(format!(
                "Session {} started",
                value
                    .get("thread_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
            )),
            value
                .get("thread_id")
                .and_then(|v| v.as_str())
                .map(str::to_string),
        );
    }
    if event_type == "item.completed" || event_type == "item.updated" {
        if let Some(item) = value.get("item") {
            let item_type = item
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or("activity");
            if item_type == "agent_message" {
                return (
                    item.get("text")
                        .and_then(|value| value.as_str())
                        .map(str::to_string),
                    None,
                    None,
                );
            }
            if item_type == "error" {
                let message = item
                    .get("message")
                    .or_else(|| item.get("error").and_then(|error| error.get("message")))
                    .and_then(|value| value.as_str())
                    .unwrap_or("Codex reported a recoverable issue");
                // Item-level errors can be advisory (for example, a skills
                // context-budget notice) and do not determine the process
                // result. Preserve the useful detail without showing a false
                // terminal generation failure; child.wait() remains the
                // authority for whether the request failed.
                return (None, Some(format!("Codex notice — {message}")), None);
            }
            let label = item
                .get("command")
                .and_then(|value| value.as_str())
                .or_else(|| item.get("name").and_then(|value| value.as_str()))
                .unwrap_or(item_type);
            return (
                None,
                Some(format!("{}: {}", item_type.replace('_', " "), label)),
                None,
            );
        }
    }
    if event_type == "turn.failed" || event_type == "error" {
        let message = value
            .get("message")
            .or_else(|| value.get("error").and_then(|error| error.get("message")))
            .and_then(|value| value.as_str())
            .unwrap_or(line);
        // The Codex CLI emits these JSON events for recoverable transport
        // reconnects before it has decided the overall exec result. Treat
        // them as activity here; `child.wait()` remains the single terminal
        // authority and will emit a real failed event if the run ultimately
        // exits unsuccessfully. Rendering these as assistant text made a
        // temporary websocket reconnect look like a failed sprite request.
        return (
            None,
            Some(format!("Connection interrupted; recovering — {message}")),
            None,
        );
    }
    (None, None, None)
}

pub(crate) fn provider_display_name(id: &str) -> &'static str {
    match id {
        "codex" => "Codex CLI",
        "claude" => "Claude Code",
        "gemini" => "Gemini CLI",
        "grok" => "Grok CLI",
        "cursor" => "Cursor CLI",
        "antigravity" => "Antigravity CLI",
        _ => "Provider CLI",
    }
}

pub(crate) fn with_optional_auth_hint(provider_id: &str, message: &str) -> String {
    let lower = message.to_lowercase();
    if lower.contains("auth")
        || lower.contains("login")
        || lower.contains("credential")
        || lower.contains("api key")
    {
        format!(
            "{message}\n\nIf this looks like a sign-in problem: {}",
            provider_auth_help(provider_id)
        )
    } else {
        message.to_string()
    }
}

pub(crate) fn provider_auth_help(id: &str) -> String {
    match id {
        "codex" => "Codex CLI is installed but not authenticated. Run `codex login`, then retry.".into(),
        "claude" => "Claude Code is installed but not authenticated. Run `claude auth login`, then retry.".into(),
        "gemini" => "Gemini CLI could not authenticate the headless request. Run `gemini` and complete its supported sign-in flow, then retry.".into(),
        "grok" => "Grok CLI is installed but not authenticated. Run `grok login`, then retry.".into(),
        "cursor" => "Cursor CLI is not signed in. Open Settings → Providers, click Sign in to Cursor, complete `agent login`, then retry. You can also set CURSOR_API_KEY in your user environment.".into(),
        "antigravity" => "Antigravity CLI is not signed in. Open Settings → Providers, click Sign in to Antigravity, complete the `agy` Google sign-in, then retry.".into(),
        _ => "The provider is not authenticated.".into(),
    }
}

pub(crate) fn nested_string(value: &serde_json::Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| {
        let mut current = value;
        for key in *path {
            current = current.get(*key)?;
        }
        current.as_str().map(str::to_string)
    })
}

pub(crate) fn message_text(value: &serde_json::Value) -> Option<String> {
    let content = value
        .get("message")
        .and_then(|message| message.get("content"))
        .or_else(|| value.get("content"))?;
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }
    let values = content.as_array()?;
    let text = values
        .iter()
        .filter(|block| block.get("type").and_then(|value| value.as_str()) == Some("text"))
        .filter_map(|block| block.get("text").and_then(|value| value.as_str()))
        .collect::<Vec<_>>()
        .join("");
    (!text.is_empty()).then_some(text)
}

pub(crate) fn append_stream_text(response: &mut String, provider_id: &str, text: &str) {
    // Codex emits completed assistant items, so separate distinct items. The
    // other CLIs emit token/content deltas whose leading spaces are meaningful;
    // inserting a newline here turns a streamed sentence into one word per line.
    if provider_id == "codex" && !response.is_empty() && !response.ends_with('\n') {
        response.push('\n');
    }
    response.push_str(text);
}
pub(crate) fn parse_stream_line(
    provider_id: &str,
    line: &str,
    already_has_content: bool,
) -> (Option<String>, Option<String>, Option<String>) {
    if provider_id == "codex" {
        return parse_codex_line(line);
    }
    if provider_id == "cursor" {
        return parse_cursor_line(line, already_has_content);
    }
    if provider_id == "antigravity" {
        return parse_antigravity_line(line, already_has_content);
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return (None, Some(line.to_string()), None);
    };
    let event_type = value
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("activity");
    let session_id = nested_string(
        &value,
        &[&["session_id"], &["sessionId"], &["session", "id"]],
    );
    let delta = nested_string(
        &value,
        &[
            &["event", "delta", "text"],
            &["delta", "text"],
            &["delta", "content"],
        ],
    );
    if matches!(event_type, "stream_event" | "content_block_delta") && delta.is_some() {
        return (delta, None, session_id);
    }
    if provider_id == "gemini" && event_type == "message" {
        let role = value.get("role").and_then(|value| value.as_str());
        if role == Some("assistant") {
            return (
                value
                    .get("content")
                    .and_then(|value| value.as_str())
                    .map(str::to_string),
                None,
                session_id,
            );
        }
    }
    if matches!(event_type, "tool_use" | "tool_result") {
        let name = nested_string(&value, &[&["tool_name"], &["name"], &["tool", "name"]])
            .unwrap_or_else(|| event_type.replace('_', " "));
        return (None, Some(name), session_id);
    }
    if event_type == "assistant" && !already_has_content {
        return (message_text(&value), None, session_id);
    }
    if matches!(event_type, "result" | "completed") && !already_has_content {
        return (
            nested_string(&value, &[&["result"], &["response"], &["text"]]),
            None,
            session_id,
        );
    }
    if matches!(event_type, "error" | "failed") {
        return (
            None,
            nested_string(&value, &[&["error", "message"], &["message"]])
                .or_else(|| Some("Provider reported an error".into())),
            session_id,
        );
    }
    let activity = match event_type {
        "system" | "init" | "message_start" => Some(format!(
            "{} session started",
            provider_display_name(provider_id)
        )),
        "content_block_start" | "content_block_stop" | "message_delta" | "message_stop" => None,
        _ => None,
    };
    (None, activity, session_id)
}

pub(crate) fn provider_failure_message(
    provider_id: &str,
    status: &str,
    stderr: &str,
    response: &str,
) -> String {
    if !response.trim().is_empty() && !stderr.trim().is_empty() {
        format!("{response}\n\nProvider stderr:\n{stderr}")
    } else if !response.trim().is_empty() {
        response.to_string()
    } else if !stderr.trim().is_empty() {
        stderr.to_string()
    } else {
        format!(
            "{} exited with status {status}",
            provider_display_name(provider_id)
        )
    }
}

pub(crate) fn response_reports_generation_failure(response: &str) -> bool {
    let lower = response.to_ascii_lowercase();
    lower.contains("generation_failed:")
        || lower.contains("antigravity cli reported")
        || lower.contains("unable to publish the")
        || lower.contains("withdrawing the candidate")
        || (lower.contains("did not pass the final visual acceptance gate")
            && lower.contains("restor"))
}

pub(crate) fn cursor_path_detail(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let segments: Vec<&str> = normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if segments.len() >= 2 {
        format!(
            "{}/{}",
            segments[segments.len() - 2],
            segments[segments.len() - 1]
        )
    } else {
        segments.last().copied().unwrap_or(path).to_string()
    }
}

pub(crate) fn cursor_trim_detail(value: &str, max_len: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_len {
        return trimmed.to_string();
    }
    let mut end = max_len;
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &trimmed[..end])
}
