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
    env,
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
    sync::OnceLock,
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::oneshot,
};
use uuid::Uuid;

fn find_executable(name: &str) -> Option<PathBuf> {
    let search_path = current_provider_environment_path();
    let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if let Ok(path) = which::which_in(name, Some(search_path), current_dir) {
        return Some(path);
    }
    let home = env::var_os("HOME").map(PathBuf::from);
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

fn merge_provider_paths(
    inherited: Option<&OsStr>,
    login_shell: Option<&OsStr>,
    home: Option<&Path>,
) -> OsString {
    let mut entries = Vec::<PathBuf>::new();
    let mut append = |value: &OsStr| {
        for path in env::split_paths(value) {
            if !entries.contains(&path) {
                entries.push(path);
            }
        }
    };
    if let Some(path) = login_shell {
        append(path);
    }
    if let Some(path) = inherited {
        append(path);
    }
    if let Some(home) = home {
        let local_bin = home.join(".local/bin");
        if !entries.contains(&local_bin) {
            entries.push(local_bin);
        }
    }
    env::join_paths(entries).unwrap_or_else(|_| inherited.unwrap_or_default().to_os_string())
}

const PROVIDER_PATH_MARKER: &str = "__SPRITE_STUDIO_PATH__=";

fn login_shell_path(shell: &Path) -> Option<OsString> {
    login_shell_path_with_timeout(shell, Duration::from_secs(3))
}

#[cfg(unix)]
fn isolate_login_shell(command: &mut StdCommand) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn isolate_login_shell(_command: &mut StdCommand) {}

#[cfg(unix)]
fn kill_login_shell_group(pid: u32, observed_exit: bool) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: the probe is placed in a dedicated process group whose id is the
    // direct child's pid. A negative pid asks kill(2) to signal that group.
    let result = unsafe { libc::kill(-pid, libc::SIGKILL) };
    let error = std::io::Error::last_os_error();
    result == 0
        || error.raw_os_error() == Some(libc::ESRCH)
        // macOS reports EPERM when WNOWAIT has confirmed that the group
        // contains only the unreaped zombie leader and no signalable members.
        || (observed_exit && error.raw_os_error() == Some(libc::EPERM))
}

#[cfg(not(unix))]
fn kill_login_shell_group(_pid: u32, _observed_exit: bool) -> bool {
    true
}

enum LoginShellState {
    Running,
    Exited,
    Error,
}

#[cfg(unix)]
fn login_shell_state(child: &mut std::process::Child) -> LoginShellState {
    let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    // SAFETY: info points to writable siginfo_t storage, and WNOWAIT observes
    // the child without reaping it so its PID/process-group id cannot be reused.
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            child.id() as libc::id_t,
            info.as_mut_ptr(),
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result != 0 {
        return LoginShellState::Error;
    }
    // SAFETY: waitid initialized info on success. A zero pid with WNOHANG means
    // the child has not changed state yet.
    let pid = unsafe { info.assume_init().si_pid() };
    if pid == 0 {
        LoginShellState::Running
    } else {
        LoginShellState::Exited
    }
}

#[cfg(not(unix))]
fn login_shell_state(child: &mut std::process::Child) -> LoginShellState {
    match child.try_wait() {
        Ok(Some(_)) => LoginShellState::Exited,
        Ok(None) => LoginShellState::Running,
        Err(_) => LoginShellState::Error,
    }
}

fn stop_login_shell(
    child: &mut std::process::Child,
    observed_exit: bool,
) -> Option<std::process::ExitStatus> {
    if !kill_login_shell_group(child.id(), observed_exit) {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }
    let _ = child.kill();
    child.wait().ok()
}

fn login_shell_path_with_timeout(shell: &Path, timeout: Duration) -> Option<OsString> {
    let output_path = env::temp_dir().join(format!("sprite-studio-path-{}.txt", Uuid::new_v4()));
    let result = (|| {
        let output_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&output_path)
            .ok()?;
        let mut command = StdCommand::new(shell);
        command
            .args(["-ilc", "printf '\\n__SPRITE_STUDIO_PATH__=%s\\n' \"$PATH\""])
            .stdout(Stdio::from(output_file))
            .stderr(Stdio::null());
        isolate_login_shell(&mut command);
        let mut child = command.spawn().ok()?;
        let deadline = Instant::now() + timeout;
        let status = loop {
            match login_shell_state(&mut child) {
                LoginShellState::Exited => break stop_login_shell(&mut child, true)?,
                LoginShellState::Running => {}
                LoginShellState::Error => {
                    let _ = stop_login_shell(&mut child, false);
                    return None;
                }
            }
            if Instant::now() >= deadline {
                let _ = stop_login_shell(&mut child, false);
                return None;
            }
            thread::sleep(
                Duration::from_millis(10).min(deadline.saturating_duration_since(Instant::now())),
            );
        };
        if !status.success() {
            return None;
        }
        let stdout = fs::read(&output_path).ok()?;

        let stdout = String::from_utf8(stdout).ok()?;
        stdout.lines().rev().find_map(|line| {
            line.strip_prefix(PROVIDER_PATH_MARKER)
                .filter(|path| !path.is_empty())
                .map(OsString::from)
        })
    })();
    let _ = fs::remove_file(output_path);
    result
}

fn provider_environment_path(
    inherited: Option<&OsStr>,
    shell: Option<&Path>,
    home: Option<&Path>,
) -> OsString {
    let login_shell = shell.and_then(login_shell_path);
    merge_provider_paths(inherited, login_shell.as_deref(), home)
}

fn current_provider_environment_path() -> &'static OsString {
    static PATH: OnceLock<OsString> = OnceLock::new();
    PATH.get_or_init(|| {
        let inherited = env::var_os("PATH");
        let home = env::var_os("HOME").map(PathBuf::from);
        #[cfg(not(windows))]
        let shell = env::var_os("SHELL")
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .or_else(|| {
                ["/bin/zsh", "/bin/bash", "/bin/sh"]
                    .into_iter()
                    .map(PathBuf::from)
                    .find(|path| path.is_file())
            });
        #[cfg(windows)]
        let shell: Option<PathBuf> = None;
        provider_environment_path(inherited.as_deref(), shell.as_deref(), home.as_deref())
    })
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
        .env("PATH", current_provider_environment_path())
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
            .or_else(|| value.get("error").and_then(|error| error.get("message")))
            .and_then(|value| value.as_str())
            .unwrap_or(line);
        return (Some(message.to_string()), None, None);
    }
    (None, None, None)
}

fn provider_failure_message(status: &str, stderr: &str, response: &str) -> String {
    if !response.trim().is_empty() && !stderr.trim().is_empty() {
        format!("{response}\n\nProvider stderr:\n{stderr}")
    } else if !response.trim().is_empty() {
        response.to_string()
    } else if !stderr.trim().is_empty() {
        stderr.to_string()
    } else {
        format!("Codex exited with status {status}")
    }
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
    let (reference_context, reference_paths) = references::prompt_context(
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
        reference_paths,
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
    reference_paths: Vec<String>,
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
        reference_paths,
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
    if !reference_paths.is_empty() {
        emit(
            &app,
            &request_id,
            &conversation_id,
            "activity",
            format!(
                "Visually inspecting {} attached image{} before rig planning",
                reference_paths.len(),
                if reference_paths.len() == 1 { "" } else { "s" }
            ),
        );
    }
    let arguments = codex_arguments(
        session_id.as_deref(),
        model.as_deref(),
        reasoning_effort.as_deref(),
        &reference_paths,
    );
    let mut child = match Command::new(executable)
        .args(&arguments)
        .env("PATH", current_provider_environment_path())
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
                            let _ = update_message(&state, &assistant_id, &response, "running");
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
            let message = provider_failure_message(&exit.to_string(), &stderr_output, &response);
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
    reference_paths: &[String],
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
    for path in reference_paths {
        arguments.extend(["--image".into(), path.clone()]);
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
        if !matches!(
            command,
            "animate" | "sprite" | "character" | "effect" | "pack"
        ) {
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
    use super::{
        codex_arguments, login_shell_path, login_shell_path_with_timeout, merge_provider_paths,
        parse_codex_line, provider_environment_path, provider_failure_message,
        validate_provider_options,
    };
    use crate::models::{GenerationOptions, ProviderRequestOptions};
    use std::{env, path::PathBuf};

    #[cfg(unix)]
    #[test]
    fn reads_provider_path_from_login_shell_output() {
        use std::{fs, os::unix::fs::PermissionsExt};
        use uuid::Uuid;

        let root = env::temp_dir().join(format!("sprite-studio-shell-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("test directory should exist");
        let shell = root.join("shell");
        fs::write(
            &shell,
            "#!/bin/sh\nprintf 'shell startup noise\\n__SPRITE_STUDIO_PATH__=/runtime/bin:/system/bin\\n'\n",
        )
        .expect("fake shell should be written");
        let mut permissions = fs::metadata(&shell)
            .expect("fake shell metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&shell, permissions).expect("fake shell should be executable");

        let discovered = login_shell_path(&shell);

        assert_eq!(
            discovered.as_deref(),
            Some(std::ffi::OsStr::new("/runtime/bin:/system/bin"))
        );
        fs::remove_dir_all(root).expect("test directory should be removed");
    }

    #[cfg(unix)]
    #[test]
    fn login_shell_path_times_out_and_reaps_stuck_shell() {
        use std::{
            fs,
            os::unix::fs::PermissionsExt,
            process::Command,
            time::{Duration, Instant},
        };
        use uuid::Uuid;

        let root = env::temp_dir().join(format!("sprite-studio-shell-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("test directory should exist");
        let shell = root.join("shell");
        fs::write(&shell, "#!/bin/sh\nwhile :; do :; done\n")
            .expect("fake shell should be written");
        let mut permissions = fs::metadata(&shell)
            .expect("fake shell metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&shell, permissions).expect("fake shell should be executable");

        let started = Instant::now();
        let discovered = login_shell_path_with_timeout(&shell, Duration::from_millis(250));
        let elapsed = started.elapsed();

        assert!(discovered.is_none());
        assert!(elapsed < Duration::from_secs(1), "elapsed: {elapsed:?}");
        assert!(!Command::new("/usr/bin/pgrep")
            .args(["-f", shell.to_str().expect("UTF-8 test path")])
            .output()
            .expect("process probe should run")
            .status
            .success());
        fs::remove_dir_all(root).expect("test directory should be removed");
    }

    #[cfg(unix)]
    #[test]
    fn login_shell_timeout_kills_descendants() {
        use std::{fs, os::unix::fs::PermissionsExt, process::Command, time::Duration};
        use uuid::Uuid;

        let root = env::temp_dir().join(format!("sprite-studio-shell-tree-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("test directory should exist");
        let descendant_pid = root.join("descendant.pid");
        let shell = root.join("shell");
        fs::write(
            &shell,
            format!(
                "#!/bin/sh\n(sleep 30) >/dev/null 2>&1 &\nprintf '%s' \"$!\" > '{}'\nwhile :; do :; done\n",
                descendant_pid.display()
            ),
        )
        .expect("fake shell should be written");
        let mut permissions = fs::metadata(&shell)
            .expect("fake shell metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&shell, permissions).expect("fake shell should be executable");

        let discovered = login_shell_path_with_timeout(&shell, Duration::from_secs(2));
        let pid = fs::read_to_string(&descendant_pid).expect("descendant should record its pid");
        let descendant_survived = Command::new("/bin/kill")
            .args(["-0", pid.trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .expect("descendant probe should run")
            .success();
        if descendant_survived {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", pid.trim()])
                .status();
        }
        fs::remove_dir_all(root).expect("test directory should be removed");

        assert!(discovered.is_none());
        assert!(!descendant_survived, "timed-out shell left a descendant");
    }

    #[cfg(unix)]
    #[test]
    fn login_shell_output_is_bounded_when_descendants_inherit_handles() {
        use std::{
            fs,
            os::unix::fs::PermissionsExt,
            time::{Duration, Instant},
        };
        use uuid::Uuid;

        let root = env::temp_dir().join(format!("sprite-studio-shell-output-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("test directory should exist");
        let descendant_pid = root.join("descendant.pid");
        let shell = root.join("shell");
        fs::write(
            &shell,
            format!(
                "#!/bin/sh\n(sleep 5) &\nprintf '%s' \"$!\" > '{}'\nprintf '\\n__SPRITE_STUDIO_PATH__=/runtime/bin:/system/bin\\n'\n",
                descendant_pid.display()
            ),
        )
        .expect("fake shell should be written");
        let mut permissions = fs::metadata(&shell)
            .expect("fake shell metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&shell, permissions).expect("fake shell should be executable");

        let started = Instant::now();
        let discovered = login_shell_path_with_timeout(&shell, Duration::from_secs(3));
        let elapsed = started.elapsed();
        let pid = fs::read_to_string(&descendant_pid).expect("descendant should record its pid");
        let descendant_survived = std::process::Command::new("/bin/kill")
            .args(["-0", pid.trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .expect("descendant probe should run")
            .success();
        if descendant_survived {
            let _ = std::process::Command::new("/bin/kill")
                .args(["-KILL", pid.trim()])
                .status();
        }
        fs::remove_dir_all(root).expect("test directory should be removed");

        assert_eq!(
            discovered.as_deref(),
            Some(std::ffi::OsStr::new("/runtime/bin:/system/bin"))
        );
        assert!(elapsed < Duration::from_secs(4), "elapsed: {elapsed:?}");
        assert!(!descendant_survived, "successful shell left a descendant");
    }

    #[cfg(unix)]
    #[test]
    fn provider_environment_runs_env_shebang_with_shell_runtime() {
        use std::{fs, os::unix::fs::PermissionsExt, process::Command};
        use uuid::Uuid;

        let root = env::temp_dir().join(format!("sprite-studio-provider-test-{}", Uuid::new_v4()));
        let runtime_bin = root.join("runtime/bin");
        fs::create_dir_all(&runtime_bin).expect("runtime directory should exist");

        let node = runtime_bin.join("node");
        fs::write(&node, "#!/bin/sh\nexit 0\n").expect("fake node should be written");
        let provider = root.join("codex");
        fs::write(&provider, "#!/usr/bin/env node\n").expect("fake provider should be written");
        let shell = root.join("shell");
        fs::write(
            &shell,
            format!(
                "#!/bin/sh\nprintf '__SPRITE_STUDIO_PATH__={}:{}\\n'\n",
                runtime_bin.display(),
                "/usr/bin:/bin"
            ),
        )
        .expect("fake shell should be written");

        for executable in [&node, &provider, &shell] {
            let mut permissions = fs::metadata(executable)
                .expect("executable metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(executable, permissions).expect("file should be executable");
        }

        let inherited = env::join_paths([PathBuf::from("/usr/bin"), PathBuf::from("/bin")])
            .expect("valid restricted PATH");
        let path =
            provider_environment_path(Some(inherited.as_os_str()), Some(&shell), Some(&root));
        let status = Command::new(&provider)
            .env("PATH", path)
            .status()
            .expect("provider should start");

        assert!(status.success());
        fs::remove_dir_all(root).expect("test directory should be removed");
    }

    #[test]
    fn merges_login_shell_path_for_gui_provider_processes() {
        let runtime_bin = PathBuf::from("/runtime/bin");
        let system_bin = PathBuf::from("/system/bin");
        let inherited = env::join_paths([system_bin.clone()]).expect("valid inherited PATH");
        let login_shell =
            env::join_paths([runtime_bin.clone(), system_bin.clone()]).expect("valid shell PATH");
        let home = PathBuf::from("/Users/example");

        let merged = merge_provider_paths(
            Some(inherited.as_os_str()),
            Some(login_shell.as_os_str()),
            Some(&home),
        );
        let entries = env::split_paths(&merged).collect::<Vec<_>>();

        assert_eq!(entries.first(), Some(&runtime_bin));
        assert_eq!(
            entries.iter().filter(|path| **path == system_bin).count(),
            1
        );
        assert!(entries.contains(&home.join(".local/bin")));
    }

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
    fn preserves_structured_provider_errors_as_response_content() {
        let line = r#"{"type":"error","message":"resume payload is invalid"}"#;
        let (content, activity, _) = parse_codex_line(line);

        assert_eq!(content.as_deref(), Some("resume payload is invalid"));
        assert!(activity.is_none());
    }

    #[test]
    fn nonzero_exit_uses_provider_response_when_stderr_is_empty() {
        let message = provider_failure_message("exit status: 1", "", "resume payload is invalid");

        assert_eq!(message, "resume payload is invalid");
    }

    #[test]
    fn nonzero_exit_preserves_provider_response_and_stderr() {
        let message = provider_failure_message(
            "exit status: 1",
            "provider diagnostic context",
            "resume payload is invalid",
        );

        assert!(message.contains("resume payload is invalid"));
        assert!(message.contains("provider diagnostic context"));
    }

    #[test]
    fn resumes_a_persisted_codex_session() {
        assert_eq!(
            codex_arguments(
                Some("session-123"),
                Some("gpt-5.6-terra"),
                Some("high"),
                &[]
            ),
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
    fn attaches_reference_images_to_new_and_resumed_codex_turns() {
        let images = vec![
            "/tmp/master.png".to_string(),
            "/tmp/palette.webp".to_string(),
        ];
        for session in [None, Some("session-123")] {
            let arguments = codex_arguments(session, None, None, &images);
            assert!(arguments
                .windows(2)
                .any(|pair| pair == ["--image", "/tmp/master.png"]));
            assert!(arguments
                .windows(2)
                .any(|pair| pair == ["--image", "/tmp/palette.webp"]));
        }
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
        let mut pack_options = options;
        pack_options.command = Some("pack".into());
        assert!(validate_provider_options(&pack_options).is_ok());
    }
}
