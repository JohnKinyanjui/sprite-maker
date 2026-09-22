use super::arguments::{provider_arguments, provider_stdin_bytes};
use super::discovery::{find_executable, provider_process_path};
use super::headless::apply_tokio_headless_flags;
use super::modes::provider_is_authenticated;
use super::stream::{
    append_stream_text, parse_stream_line, provider_auth_help, provider_display_name,
    provider_failure_message,
};
use crate::error::{CommandError, CommandResult};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::oneshot,
};
use uuid::Uuid;

pub(crate) fn create_provider_prompt_file(
    workspace: &Path,
    request_id: &str,
    prompt: &str,
) -> std::io::Result<PathBuf> {
    let directory = workspace.join(".sprite-studio/provider-prompts");
    fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{request_id}.txt"));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut file = options.open(&path)?;
    file.write_all(prompt.as_bytes())?;
    Ok(path)
}

/// Runs a one-shot headless agent request outside the chat pipeline and
/// returns the accumulated assistant text. Used for structured asks such as
/// AI rig-point suggestions, where the caller only needs the final answer.
pub(crate) async fn run_agent_text_request(
    provider_id: &str,
    model: Option<&str>,
    reasoning_effort: Option<&str>,
    cwd: &Path,
    prompt: &str,
    image_paths: &[String],
    cancel_rx: Option<oneshot::Receiver<()>>,
) -> CommandResult<String> {
    if !matches!(
        provider_id,
        "codex" | "claude" | "gemini" | "grok" | "cursor" | "antigravity"
    ) {
        return Err(CommandError::new(
            "provider_unsupported",
            "Choose an installed agent provider for this request",
        ));
    }
    let executable = find_executable(provider_id).ok_or_else(|| {
        CommandError::new(
            "provider_unavailable",
            format!(
                "{} was not found. Install its CLI, authenticate it, then retry detection.",
                provider_display_name(provider_id)
            ),
        )
    })?;
    if !provider_is_authenticated(provider_id, &executable) {
        return Err(CommandError::new(
            "provider_unauthenticated",
            provider_auth_help(provider_id),
        ));
    }
    let request_id = Uuid::new_v4().to_string();
    let prompt_file = if provider_id == "grok" {
        Some(create_provider_prompt_file(cwd, &request_id, prompt)?)
    } else {
        None
    };
    let arguments = provider_arguments(
        provider_id,
        None,
        model,
        reasoning_effort,
        image_paths,
        prompt_file.as_deref(),
    );
    let mut command = Command::new(&executable);
    command
        .args(&arguments)
        .env("PATH", provider_process_path(provider_id))
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    apply_tokio_headless_flags(&mut command);
    let mut child = command.spawn().map_err(|error| {
        CommandError::new(
            "process_error",
            format!(
                "Could not start {}: {error}",
                provider_display_name(provider_id)
            ),
        )
    })?;
    if provider_id != "grok" {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&provider_stdin_bytes(provider_id, prompt))
                .await
                .map_err(|error| {
                    CommandError::new(
                        "process_error",
                        format!(
                            "Could not send the prompt to {}: {error}",
                            provider_display_name(provider_id)
                        ),
                    )
                })?;
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
    let mut read_failed: Option<String> = None;
    let mut cancelled = false;
    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
        let mut cancel_rx = cancel_rx;
        loop {
            if tokio::time::Instant::now() >= deadline {
                let _ = child.kill().await;
                if let Some(path) = prompt_file.as_ref() {
                    let _ = fs::remove_file(path);
                }
                return Err(CommandError::new(
                    "provider_timeout",
                    format!(
                        "{} did not answer within five minutes. Try again or use Auto-suggest.",
                        provider_display_name(provider_id)
                    ),
                ));
            }
            let next_line = lines.next_line();
            match cancel_rx.as_mut() {
                Some(receiver) => {
                    tokio::select! {
                        _ = receiver => {
                            cancelled = true;
                            let _ = child.kill().await;
                            break;
                        }
                        result = next_line => match result {
                            Ok(Some(line)) => {
                                let (text, _, _) =
                                    parse_stream_line(provider_id, &line, !response.is_empty());
                                if let Some(text) = text {
                                    append_stream_text(&mut response, provider_id, &text);
                                }
                            }
                            Ok(None) => break,
                            Err(error) => {
                                read_failed = Some(error.to_string());
                                break;
                            }
                        },
                    }
                }
                None => match next_line.await {
                    Ok(Some(line)) => {
                        let (text, _, _) =
                            parse_stream_line(provider_id, &line, !response.is_empty());
                        if let Some(text) = text {
                            append_stream_text(&mut response, provider_id, &text);
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        read_failed = Some(error.to_string());
                        break;
                    }
                },
            }
        }
    }
    if cancelled {
        if let Some(path) = prompt_file.as_ref() {
            let _ = fs::remove_file(path);
        }
        return Err(CommandError::new(
            "request_cancelled",
            "Prompt refinement cancelled",
        ));
    }
    let status = child.wait().await;
    let stderr_output = stderr_task.await.unwrap_or_default();
    if let Some(path) = prompt_file.as_ref() {
        let _ = fs::remove_file(path);
    }
    if let Some(error) = read_failed {
        return Err(CommandError::new("process_error", error));
    }
    let status = status.map_err(|error| {
        CommandError::new(
            "process_error",
            format!(
                "Could not observe the {} process: {error}",
                provider_display_name(provider_id)
            ),
        )
    })?;
    if !status.success() {
        return Err(CommandError::new(
            "provider_failed",
            provider_failure_message(provider_id, &status.to_string(), &stderr_output, &response),
        ));
    }
    if response.trim().is_empty() {
        return Err(CommandError::new(
            "provider_empty_response",
            format!(
                "{} completed without returning a text response.",
                provider_display_name(provider_id)
            ),
        ));
    }
    Ok(response)
}
