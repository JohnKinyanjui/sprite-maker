use crate::{
    conversations::{add_message, get_conversation, set_provider_session, update_message},
    error::{CommandError, CommandResult},
    models::{
        GenerationOptions, ProviderCapabilities, ProviderEvent, ProviderMode,
        ProviderRequestOptions, ProviderStatus,
    },
    references,
    sprite_harness::studio_prompt,
    workspace::workspace_path,
    AppState,
};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::oneshot,
};
use uuid::Uuid;

fn find_executable(name: &str) -> Option<PathBuf> {
    if let Ok(path) = which::which(name) {
        return Some(path);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut candidates = vec![
        PathBuf::from("/opt/homebrew/bin").join(name),
        PathBuf::from("/usr/local/bin").join(name),
    ];
    if let Some(home) = home {
        candidates.push(home.join(".local/bin").join(name));
        candidates.push(home.join(".codex/bin").join(name));
    }
    candidates.into_iter().find(|path| path.is_file())
}

#[derive(Debug, Deserialize)]
struct CodexModelCatalog {
    models: Vec<CodexModelEntry>,
}

#[derive(Debug, Deserialize)]
struct CodexModelEntry {
    slug: String,
    display_name: String,
    description: String,
    default_reasoning_level: String,
    supported_reasoning_levels: Vec<CodexReasoningLevel>,
    visibility: String,
    priority: i64,
}

#[derive(Debug, Deserialize)]
struct CodexReasoningLevel {
    effort: String,
}

fn codex_modes(executable: &Path) -> Vec<ProviderMode> {
    let Ok(output) = StdCommand::new(executable)
        .args(["debug", "models"])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let Ok(mut catalog) = serde_json::from_slice::<CodexModelCatalog>(&output.stdout) else {
        return Vec::new();
    };
    catalog.models.sort_by_key(|model| model.priority);
    catalog
        .models
        .into_iter()
        .filter(|model| model.visibility == "list")
        .map(|model| ProviderMode {
            id: model.slug,
            label: model.display_name,
            description: model.description,
            default_reasoning_effort: model.default_reasoning_level,
            reasoning_efforts: model
                .supported_reasoning_levels
                .into_iter()
                .map(|level| level.effort)
                .collect(),
        })
        .collect()
}

#[tauri::command]
pub fn detect_providers() -> Vec<ProviderStatus> {
    [
        ("codex", "Codex CLI"),
        ("claude", "Claude Code"),
        ("gemini", "Gemini CLI"),
    ]
    .into_iter()
    .map(|(id, name)| {
        let executable = find_executable(id);
        let installed = executable.is_some();
        let modes = if id == "codex" {
            executable.as_deref().map(codex_modes).unwrap_or_default()
        } else {
            Vec::new()
        };
        let (status, detail) = if id == "codex" && installed {
            (
                "ready",
                "Installed and available for real workspace conversations",
            )
        } else if id == "codex" {
            (
                "unavailable",
                "Install and authenticate Codex CLI to use agent conversations",
            )
        } else if installed {
            (
                "coming_later",
                "Detected locally; adapter support is planned",
            )
        } else {
            ("coming_later", "Adapter support is planned")
        };
        ProviderStatus {
            id: id.into(),
            name: name.into(),
            kind: "agent".into(),
            installed,
            executable: executable.map(|path| path.to_string_lossy().into_owned()),
            status: status.into(),
            detail: detail.into(),
            modes,
            capabilities: provider_capabilities(id),
        }
    })
    .collect()
}

fn provider_capabilities(id: &str) -> ProviderCapabilities {
    match id {
        "codex" => ProviderCapabilities {
            text_input: true,
            image_input: true,
            multiple_image_input: true,
            image_editing: true,
            masks: false,
            transparency: true,
            structured_output: true,
            video_animation: false,
            image_to_image: true,
            maximum_reference_images: 5,
        },
        "claude" | "gemini" => ProviderCapabilities {
            text_input: true,
            image_input: true,
            multiple_image_input: true,
            image_editing: false,
            masks: false,
            transparency: false,
            structured_output: true,
            video_animation: false,
            image_to_image: false,
            maximum_reference_images: 10,
        },
        _ => ProviderCapabilities {
            text_input: true,
            image_input: false,
            multiple_image_input: false,
            image_editing: false,
            masks: false,
            transparency: false,
            structured_output: false,
            video_animation: false,
            image_to_image: false,
            maximum_reference_images: 0,
        },
    }
}

fn emit(
    app: &AppHandle,
    request_id: &str,
    conversation_id: &str,
    event_type: &str,
    content: impl Into<String>,
) {
    let _ = app.emit(
        "provider-event",
        ProviderEvent {
            request_id: request_id.to_string(),
            conversation_id: conversation_id.to_string(),
            event_type: event_type.to_string(),
            content: content.into(),
        },
    );
}

fn parse_codex_line(line: &str) -> (Option<String>, Option<String>, Option<String>) {
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
            .and_then(|value| value.as_str())
            .unwrap_or(line);
        return (None, Some(message.to_string()), None);
    }
    (None, None, None)
}

#[tauri::command]
pub fn start_provider_message(
    conversation_id: String,
    prompt: String,
    context: Option<String>,
    options: Option<ProviderRequestOptions>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Err(CommandError::new(
            "empty_prompt",
            "Write a message before sending",
        ));
    }
    let conversation = get_conversation(&state, &conversation_id)?;
    if conversation.provider != "codex" {
        return Err(CommandError::new(
            "provider_unsupported",
            "This provider adapter is not available yet",
        ));
    }
    let executable = find_executable("codex").ok_or_else(|| {
        CommandError::new(
            "provider_unavailable",
            "Codex CLI was not found. Install and authenticate it, then retry detection.",
        )
    })?;
    let options = options.unwrap_or_default();
    validate_provider_options(&options)?;
    let capabilities = provider_capabilities("codex");
    if !options.reference_ids.is_empty() && !capabilities.image_input {
        return Err(CommandError::new(
            "provider_image_input_unsupported",
            "The selected provider cannot accept reference images",
        ));
    }
    let reference_context = references::prompt_context(
        &state,
        &conversation_id,
        &options.reference_ids,
        capabilities.maximum_reference_images as usize,
    )?;
    let combined_context = [context.as_deref().unwrap_or(""), reference_context.as_str()]
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    workspace_path(&state, &conversation.workspace_id)?;
    add_message(
        &state,
        &conversation_id,
        "user",
        "text",
        &prompt,
        "completed",
    )?;
    let assistant = add_message(&state, &conversation_id, "assistant", "text", "", "running")?;
    let request_id = Uuid::new_v4().to_string();
    let (cancel_tx, cancel_rx) = oneshot::channel();
    state
        .cancellers
        .lock()
        .map_err(|_| {
            CommandError::new("process_error", "Provider process registry is unavailable")
        })?
        .insert(request_id.clone(), cancel_tx);

    let task_app = app.clone();
    let task_request_id = request_id.clone();
    let run = ProviderRun {
        request_id: task_request_id,
        conversation_id,
        workspace_id: conversation.workspace_id,
        session_id: conversation.provider_session_id,
        assistant_id: assistant.id,
        prompt: studio_prompt(
            &prompt,
            (!combined_context.is_empty()).then_some(combined_context.as_str()),
            options.generation.as_ref(),
            options.command.as_deref(),
        ),
        model: options.model,
        reasoning_effort: options.reasoning_effort,
        executable,
    };
    tauri::async_runtime::spawn(async move {
        run_codex(task_app, run, cancel_rx).await;
    });
    Ok(request_id)
}

struct ProviderRun {
    request_id: String,
    conversation_id: String,
    workspace_id: String,
    session_id: Option<String>,
    assistant_id: String,
    prompt: String,
    model: Option<String>,
    reasoning_effort: Option<String>,
    executable: PathBuf,
}

async fn run_codex(app: AppHandle, run: ProviderRun, mut cancel_rx: oneshot::Receiver<()>) {
    let ProviderRun {
        request_id,
        conversation_id,
        workspace_id,
        session_id,
        assistant_id,
        prompt,
        model,
        reasoning_effort,
        executable,
    } = run;
    let state = app.state::<AppState>();
    emit(
        &app,
        &request_id,
        &conversation_id,
        "started",
        "Codex is working in this workspace",
    );
    let workspace = match workspace_path(&state, &workspace_id) {
        Ok(path) => path,
        Err(error) => {
            let _ = update_message(&state, &assistant_id, &error.message, "failed");
            emit(&app, &request_id, &conversation_id, "failed", error.message);
            return;
        }
    };
    let arguments = codex_arguments(
        session_id.as_deref(),
        model.as_deref(),
        reasoning_effort.as_deref(),
    );
    let mut child = match Command::new(executable)
        .args(&arguments)
        .current_dir(workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let message = format!("Could not start Codex CLI: {error}");
            let _ = update_message(&state, &assistant_id, &message, "failed");
            emit(&app, &request_id, &conversation_id, "failed", message);
            return;
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        if let Err(error) = stdin.write_all(prompt.as_bytes()).await {
            let message = format!("Could not send the prompt to Codex: {error}");
            let _ = update_message(&state, &assistant_id, &message, "failed");
            emit(&app, &request_id, &conversation_id, "failed", message);
            return;
        }
    }
    let stderr = child.stderr.take();
    let stderr_task = tauri::async_runtime::spawn(async move {
        let mut output = String::new();
        if let Some(stderr) = stderr {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str(&line);
            }
        }
        output
    });
    let mut response = String::new();
    let mut cancelled = false;
    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    cancelled = true;
                    let _ = child.kill().await;
                    break;
                }
                line = lines.next_line() => match line {
                    Ok(Some(line)) => {
                        let (text, activity, session_id) = parse_codex_line(&line);
                        if let Some(text) = text {
                            if !response.is_empty() && !response.ends_with('\n') { response.push('\n'); }
                            response.push_str(&text);
                            emit(&app, &request_id, &conversation_id, "content", text);
                        }
                        if let Some(activity) = activity { emit(&app, &request_id, &conversation_id, "activity", activity); }
                        if let Some(session_id) = session_id {
                            let _ = set_provider_session(&state, &conversation_id, &session_id);
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        emit(&app, &request_id, &conversation_id, "activity", format!("Output stream error: {error}"));
                        break;
                    }
                }
            }
        }
    }
    let status = child.wait().await;
    let stderr_output = stderr_task.await.unwrap_or_default();
    state
        .cancellers
        .lock()
        .ok()
        .map(|mut values| values.remove(&request_id));
    if cancelled {
        let message = if response.is_empty() {
            "Request cancelled"
        } else {
            &response
        };
        let _ = update_message(&state, &assistant_id, message, "cancelled");
        emit(
            &app,
            &request_id,
            &conversation_id,
            "cancelled",
            "Request cancelled",
        );
        return;
    }
    match status {
        Ok(exit) if exit.success() => {
            if response.trim().is_empty() {
                response = "Codex completed without returning a text response.".into();
            }
            let _ = update_message(&state, &assistant_id, &response, "completed");
            emit(&app, &request_id, &conversation_id, "completed", response);
        }
        Ok(exit) => {
            let message = if stderr_output.trim().is_empty() {
                format!("Codex exited with status {exit}")
            } else {
                stderr_output
            };
            let _ = update_message(&state, &assistant_id, &message, "failed");
            emit(&app, &request_id, &conversation_id, "failed", message);
        }
        Err(error) => {
            let message = format!("Could not observe the Codex process: {error}");
            let _ = update_message(&state, &assistant_id, &message, "failed");
            emit(&app, &request_id, &conversation_id, "failed", message);
        }
    }
}

fn codex_arguments(
    session_id: Option<&str>,
    model: Option<&str>,
    reasoning_effort: Option<&str>,
) -> Vec<String> {
    let mut arguments = if session_id.is_some() {
        vec![
            "exec".into(),
            "--sandbox".into(),
            "workspace-write".into(),
            "resume".into(),
            "--json".into(),
            "--skip-git-repo-check".into(),
        ]
    } else {
        vec![
            "exec".into(),
            "--sandbox".into(),
            "workspace-write".into(),
            "--json".into(),
            "--skip-git-repo-check".into(),
        ]
    };
    if let Some(model) = model {
        arguments.extend(["--model".into(), model.into()]);
    }
    if let Some(reasoning_effort) = reasoning_effort {
        arguments.extend([
            "--config".into(),
            format!("model_reasoning_effort=\"{reasoning_effort}\""),
        ]);
    }
    if let Some(session_id) = session_id {
        arguments.push(session_id.into());
    }
    arguments.push("-".into());
    arguments
}

fn validate_provider_options(options: &ProviderRequestOptions) -> CommandResult<()> {
    if let Some(model) = options.model.as_deref() {
        if model.is_empty()
            || model.len() > 96
            || !model
                .chars()
                .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.'))
        {
            return Err(CommandError::new(
                "invalid_provider_model",
                "Choose a model reported by the selected provider",
            ));
        }
    }
    if let Some(effort) = options.reasoning_effort.as_deref() {
        if !matches!(
            effort,
            "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra"
        ) {
            return Err(CommandError::new(
                "invalid_reasoning_effort",
                "Choose a reasoning level reported by the selected model",
            ));
        }
    }
    if let Some(command) = options.command.as_deref() {
        if !matches!(command, "animate" | "sprite" | "character" | "effect") {
            return Err(CommandError::new(
                "invalid_slash_command",
                "Choose a supported Sprite Studio slash command",
            ));
        }
    }
    if let Some(generation) = options.generation.as_ref() {
        validate_generation_options(generation)?;
    }
    Ok(())
}

fn validate_generation_options(generation: &GenerationOptions) -> CommandResult<()> {
    if !matches!(
        generation.quality.as_str(),
        "low" | "mid" | "high" | "custom"
    ) || !(8..=512).contains(&generation.width)
        || !(8..=512).contains(&generation.height)
        || !(1..=32).contains(&generation.frames)
        || !(1..=60).contains(&generation.fps)
        || !matches!(generation.frame_mode.as_str(), "fixed" | "auto")
        || !(1..=32).contains(&generation.min_frames)
        || !(1..=32).contains(&generation.max_frames)
        || generation.min_frames > generation.max_frames
    {
        return Err(CommandError::new(
            "invalid_generation_profile",
            "Use an 8–512 px canvas, Fixed or Auto frames within 1–32, and 1–60 FPS",
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_provider_request(
    request_id: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let sender = state
        .cancellers
        .lock()
        .map_err(|_| {
            CommandError::new("process_error", "Provider process registry is unavailable")
        })?
        .remove(&request_id);
    if let Some(sender) = sender {
        let _ = sender.send(());
    } else {
        return Err(CommandError::new(
            "request_not_found",
            "The provider request is no longer running",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{codex_arguments, parse_codex_line, validate_provider_options};
    use crate::models::{GenerationOptions, ProviderRequestOptions};

    #[test]
    fn parses_streamed_agent_messages() {
        let line = r#"{"type":"item.completed","item":{"type":"agent_message","text":"Created the sprite metadata."}}"#;
        let (content, activity, _) = parse_codex_line(line);
        assert_eq!(content.as_deref(), Some("Created the sprite metadata."));
        assert!(activity.is_none());
    }

    #[test]
    fn represents_tool_execution_as_activity() {
        let line = r#"{"type":"item.completed","item":{"type":"command_execution","command":"inspect assets"}}"#;
        let (_, activity, _) = parse_codex_line(line);
        assert_eq!(
            activity.as_deref(),
            Some("command execution: inspect assets")
        );
    }

    #[test]
    fn preserves_non_json_provider_output() {
        let (content, _, _) = parse_codex_line("plain provider output");
        assert_eq!(content.as_deref(), Some("plain provider output"));
    }

    #[test]
    fn resumes_a_persisted_codex_session() {
        assert_eq!(
            codex_arguments(Some("session-123"), Some("gpt-5.6-terra"), Some("high")),
            [
                "exec",
                "--sandbox",
                "workspace-write",
                "resume",
                "--json",
                "--skip-git-repo-check",
                "--model",
                "gpt-5.6-terra",
                "--config",
                "model_reasoning_effort=\"high\"",
                "session-123",
                "-"
            ]
        );
    }

    #[test]
    fn validates_chat_generation_and_provider_modes() {
        let options = ProviderRequestOptions {
            model: Some("gpt-5.6-sol".into()),
            reasoning_effort: Some("xhigh".into()),
            command: Some("animate".into()),
            generation: Some(GenerationOptions {
                quality: "high".into(),
                width: 128,
                height: 128,
                frames: 8,
                fps: 12,
                frame_mode: "auto".into(),
                min_frames: 4,
                max_frames: 12,
                allow_interpolation: false,
                allow_auto_adjust: true,
            }),
            reference_ids: Vec::new(),
        };
        assert!(validate_provider_options(&options).is_ok());
    }
}
