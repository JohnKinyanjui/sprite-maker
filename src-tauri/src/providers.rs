use crate::{
    conversations::{add_message, get_conversation, set_provider_session, update_message},
    error::{CommandError, CommandResult},
    models::{
        GenerationOptions, ImageProviderInput, ProviderCapabilities, ProviderConnectionTest,
        ProviderEvent, ProviderMode, ProviderRequestOptions, ProviderStatus,
    },
    references,
    sprite_harness::studio_prompt,
    workspace::workspace_path,
    AppState,
};
use base64::Engine;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
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

pub(crate) fn find_executable(name: &str) -> Option<PathBuf> {
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

pub(crate) fn current_provider_environment_path() -> &'static OsString {
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

fn command_output(executable: &Path, arguments: &[&str]) -> Option<std::process::Output> {
    const PROBE_TIMEOUT: Duration = Duration::from_secs(4);
    let token = Uuid::new_v4();
    let stdout_path = env::temp_dir().join(format!("sprite-studio-provider-{token}.out"));
    let stderr_path = env::temp_dir().join(format!("sprite-studio-provider-{token}.err"));
    let result = (|| {
        let stdout = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&stdout_path)
            .ok()?;
        let stderr = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&stderr_path)
            .ok()?;
        let mut command = StdCommand::new(executable);
        command
            .args(arguments)
            .env("PATH", current_provider_environment_path())
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        // Provider CLIs can leave helper processes alive after the command that
        // launched them exits. Keep every read-only probe in its own group so a
        // timeout cleans up the whole probe without touching a real generation.
        isolate_login_shell(&mut command);
        let mut child = command.spawn().ok()?;
        let deadline = Instant::now() + PROBE_TIMEOUT;
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
            thread::sleep(Duration::from_millis(10));
        };
        Some(std::process::Output {
            status,
            stdout: fs::read(&stdout_path).unwrap_or_default(),
            stderr: fs::read(&stderr_path).unwrap_or_default(),
        })
    })();
    let _ = fs::remove_file(stdout_path);
    let _ = fs::remove_file(stderr_path);
    result
}

fn provider_is_authenticated(id: &str, executable: &Path) -> bool {
    match id {
        "codex" => command_output(executable, &["login", "status"])
            .is_some_and(|output| output.status.success()),
        "claude" => command_output(executable, &["auth", "status", "--json"])
            .filter(|output| output.status.success())
            .and_then(|output| serde_json::from_slice::<serde_json::Value>(&output.stdout).ok())
            .and_then(|value| value.get("loggedIn").and_then(|value| value.as_bool()))
            .unwrap_or(false),
        // `models` is a read-only command which requires an authenticated Grok
        // session. It provides a stronger signal than checking credential files.
        "grok" => {
            command_output(executable, &["models"]).is_some_and(|output| output.status.success())
        }
        // Gemini has no stable auth-status command. A present executable is
        // reported as detected until a real headless request confirms auth.
        "gemini" => true,
        _ => false,
    }
}

fn grok_modes_from_output(output: &std::process::Output) -> Vec<ProviderMode> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            let value = line
                .strip_prefix('*')
                .or_else(|| line.strip_prefix('-'))?
                .trim()
                .strip_suffix("(default)")
                .unwrap_or_else(|| line.trim_start_matches(['*', '-']).trim())
                .trim();
            (!value.is_empty()).then(|| ProviderMode {
                id: value.to_string(),
                label: value.to_string(),
                description: "Model reported by the installed Grok CLI".into(),
                default_reasoning_effort: "medium".into(),
                reasoning_efforts: vec!["low".into(), "medium".into(), "high".into()],
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredImageProvider {
    id: String,
    name: String,
    provider_type: String,
    base_url: String,
    api_key: String,
    model: String,
}

fn image_provider_status(provider: &StoredImageProvider) -> ProviderStatus {
    let configured =
        !provider.api_key.is_empty() && !provider.base_url.is_empty() && !provider.model.is_empty();
    ProviderStatus {
        id: provider.id.clone(),
        name: provider.name.clone(),
        kind: "image".into(),
        installed: configured,
        executable: None,
        status: if configured { "ready" } else { "unavailable" }.into(),
        detail: if configured {
            format!("{} image API configured", provider.provider_type)
        } else {
            "Add an API key to enable this image source".into()
        },
        modes: vec![ProviderMode {
            id: provider.model.clone(),
            label: provider.model.clone(),
            description: format!("Image model served by {}", provider.name),
            default_reasoning_effort: String::new(),
            reasoning_efforts: Vec::new(),
        }],
        capabilities: ProviderCapabilities {
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
        configurable: true,
        has_api_key: !provider.api_key.is_empty(),
        base_url: Some(provider.base_url.clone()),
        model: Some(provider.model.clone()),
    }
}

fn stored_image_providers(state: &AppState) -> CommandResult<Vec<StoredImageProvider>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut statement = connection.prepare("SELECT settings_json FROM provider_settings WHERE provider LIKE 'image:%' AND enabled=1 ORDER BY provider")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows
        .filter_map(Result::ok)
        .filter_map(|json| serde_json::from_str(&json).ok())
        .collect())
}

fn load_image_provider(state: &AppState, id: &str) -> CommandResult<Option<StoredImageProvider>> {
    if id == "imagegen" {
        return Ok(None);
    }
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let json: Option<String> = connection
        .query_row(
            "SELECT settings_json FROM provider_settings WHERE provider=?1 AND enabled=1",
            [format!("image:{id}")],
            |row| row.get(0),
        )
        .optional()?;
    json.map(|value| {
        serde_json::from_str(&value)
            .map_err(|error| CommandError::new("invalid_provider", error.to_string()))
    })
    .transpose()
}

fn validate_provider_base_url(value: &str) -> CommandResult<String> {
    let value = value.trim().trim_end_matches('/');
    let parsed = reqwest::Url::parse(value).map_err(|_| {
        CommandError::new(
            "invalid_provider",
            "Enter a valid HTTPS API base URL, such as https://api.example.com/v1",
        )
    })?;
    let local_debug_url = cfg!(debug_assertions)
        && parsed.scheme() == "http"
        && matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if (parsed.scheme() != "https" && !local_debug_url)
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(CommandError::new(
            "invalid_provider",
            "Use an HTTPS API base URL without credentials, query parameters, or fragments",
        ));
    }
    Ok(value.to_string())
}

fn provider_from_input(
    state: &AppState,
    input: ImageProviderInput,
) -> CommandResult<StoredImageProvider> {
    let id = input.id.trim().to_lowercase();
    let provider_type = input.provider_type.trim().to_lowercase();
    if id.is_empty()
        || id == "imagegen"
        || id == "midjourney"
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(CommandError::new(
            "invalid_provider",
            "Use a simple provider ID containing letters, numbers, dashes, or underscores; reserved built-in IDs cannot be reused",
        ));
    }
    if !matches!(provider_type.as_str(), "grok" | "openai-compatible") {
        return Err(CommandError::new(
            "invalid_provider",
            "Choose Grok or an OpenAI-compatible image API",
        ));
    }
    let name = input.name.trim();
    let model = input.model.trim();
    if name.is_empty() || model.is_empty() || name.len() > 80 || model.len() > 160 {
        return Err(CommandError::new(
            "invalid_provider",
            "Display name and model ID are required",
        ));
    }
    let base_url = validate_provider_base_url(&input.base_url)?;
    let existing_key = load_image_provider(state, &id)?
        .map(|value| value.api_key)
        .unwrap_or_default();
    let api_key = if input.api_key.trim().is_empty() {
        existing_key
    } else {
        input.api_key.trim().to_string()
    };
    if api_key.is_empty() {
        return Err(CommandError::new(
            "invalid_provider",
            "An API key is required. It is stored locally and never shown after saving.",
        ));
    }
    Ok(StoredImageProvider {
        id,
        name: name.into(),
        provider_type,
        base_url,
        api_key,
        model: model.into(),
    })
}

#[tauri::command]
pub fn save_image_provider(
    input: ImageProviderInput,
    state: State<'_, AppState>,
) -> CommandResult<ProviderStatus> {
    let provider = provider_from_input(&state, input)?;
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        "INSERT INTO provider_settings(provider,enabled,settings_json,updated_at) VALUES (?1,1,?2,?3) ON CONFLICT(provider) DO UPDATE SET enabled=1,settings_json=excluded.settings_json,updated_at=excluded.updated_at",
        params![format!("image:{}", provider.id), serde_json::to_string(&provider).map_err(|error| CommandError::new("invalid_provider", error.to_string()))?, chrono::Utc::now().to_rfc3339()]
    )?;
    Ok(image_provider_status(&provider))
}

#[tauri::command]
pub async fn test_image_provider(
    input: ImageProviderInput,
    state: State<'_, AppState>,
) -> CommandResult<ProviderConnectionTest> {
    let provider = provider_from_input(&state, input)?;
    let endpoint = if provider.base_url.ends_with("/models") {
        provider.base_url.clone()
    } else {
        format!("{}/models", provider.base_url)
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| CommandError::new("provider_error", error.to_string()))?;
    let response = client
        .get(endpoint)
        .bearer_auth(&provider.api_key)
        .send()
        .await
        .map_err(|error| {
            CommandError::new(
                "provider_error",
                format!("Could not reach {}: {error}", provider.name),
            )
        })?;
    let status = response.status();
    if status.is_success() {
        return Ok(ProviderConnectionTest {
            ok: true,
            detail: format!(
                "Connected to {} and authenticated successfully",
                provider.name
            ),
        });
    }
    let detail = match status.as_u16() {
        401 | 403 => "The endpoint was reached, but it rejected the API key",
        404 => "The endpoint does not expose the OpenAI-compatible /models route",
        429 => "The endpoint was reached, but it is currently rate-limiting requests",
        _ => "The endpoint was reached, but it returned an unexpected response",
    };
    Err(CommandError::new(
        "provider_test_failed",
        format!("{detail} ({status})"),
    ))
}

#[tauri::command]
pub fn delete_image_provider(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    if id == "grok-image" {
        return Err(CommandError::new(
            "provider_builtin",
            "The Grok entry can be cleared but not removed",
        ));
    }
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        "DELETE FROM provider_settings WHERE provider=?1",
        [format!("image:{id}")],
    )?;
    Ok(())
}

#[tauri::command]
pub fn detect_providers(state: State<'_, AppState>) -> Vec<ProviderStatus> {
    let mut providers: Vec<ProviderStatus> = [
        ("codex", "Codex CLI"),
        ("claude", "Claude Code"),
        ("gemini", "Gemini CLI"),
        ("grok", "Grok CLI"),
    ]
    .into_iter()
    .map(|(id, name)| {
        let executable = find_executable(id);
        let installed = executable.is_some();
        // Grok's authenticated `models` probe used to run twice here and can
        // take over a minute when its session or network is unhealthy. Probe it
        // once, with the same hard timeout as every other status command.
        let grok_probe = (id == "grok")
            .then(|| executable.as_deref().and_then(|path| command_output(path, &["models"])))
            .flatten();
        let authenticated = match (id, executable.as_deref()) {
            ("grok", Some(_)) => grok_probe
                .as_ref()
                .is_some_and(|output| output.status.success()),
            (_, Some(path)) => provider_is_authenticated(id, path),
            (_, None) => false,
        };
        let modes = match (id, executable.as_deref(), authenticated) {
            ("codex", Some(path), true) => codex_modes(path),
            ("grok", Some(_), true) => grok_probe
                .as_ref()
                .map(grok_modes_from_output)
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let (status, detail) = match (id, installed, authenticated) {
            (_, false, _) => (
                "not_installed",
                match id {
                    "codex" => "Install the Codex CLI, then run `codex login`",
                    "claude" => "Install Claude Code, then run `claude auth login`",
                    "gemini" => "Install @google/gemini-cli, run `gemini`, and complete authentication",
                    "grok" => "Install Grok Build, then run `grok login`",
                    _ => "Install and authenticate this provider's CLI",
                },
            ),
            ("claude", true, true) => (
                "detected",
                "CLI reports signed in. Claude validates the stored credentials on the first headless request; run `claude auth login` if that request returns 401.",
            ),
            ("gemini", true, _) => (
                "detected",
                "CLI detected. Authentication is verified by the first headless request.",
            ),
            (_, true, false) => (
                "needs_auth",
                match id {
                    "codex" => "CLI detected, but `codex login status` did not confirm a session. Run `codex login`.",
                    "claude" => "CLI detected, but `claude auth status` reports signed out. Run `claude auth login`.",
                    "grok" => "CLI detected, but `grok models` could not verify a session. Run `grok login`.",
                    _ => "CLI detected, but authentication could not be verified.",
                },
            ),
            (_, true, true) => (
                "ready",
                match id {
                    "codex" => "Installed, authenticated, and ready for workspace conversations.",
                    "claude" => "Installed and authenticated. Uses Claude Code's supported headless event stream.",
                    "grok" => "Installed and authenticated. Uses Grok Build's supported single-turn event stream.",
                    _ => "Installed and ready.",
                },
            ),
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
            configurable: false,
            has_api_key: false,
            base_url: None,
            model: None,
        }
    })
    .collect();
    providers.push(ProviderStatus {
        id: "imagegen".into(),
        name: "OpenAI ImageGen".into(),
        kind: "image".into(),
        installed: providers
            .iter()
            .any(|provider| provider.id == "codex" && provider.status == "ready"),
        executable: None,
        status: if providers
            .iter()
            .any(|provider| provider.id == "codex" && provider.status == "ready")
        {
            "ready"
        } else {
            "needs_codex"
        }
        .into(),
        detail: "Provided through the authenticated Codex workflow; no separate image API key is required.".into(),
        modes: Vec::new(),
        capabilities: provider_capabilities("codex"),
        configurable: false,
        has_api_key: false,
        base_url: None,
        model: None,
    });
    let mut configured = stored_image_providers(&state).unwrap_or_default();
    if !configured
        .iter()
        .any(|provider| provider.id == "grok-image")
    {
        configured.push(StoredImageProvider {
            id: "grok-image".into(),
            name: "Grok Imagine".into(),
            provider_type: "grok".into(),
            base_url: "https://api.x.ai/v1".into(),
            api_key: String::new(),
            model: "grok-imagine-image-2.0".into(),
        });
    }
    providers.extend(configured.iter().map(image_provider_status));
    providers.push(ProviderStatus {
        id: "midjourney".into(),
        name: "Midjourney".into(),
        kind: "image".into(),
        installed: false,
        executable: None,
        status: "unsupported".into(),
        detail: "Use an authorized gateway. Midjourney does not offer a general public API, and Sprite Studio never automates its website or Discord bot.".into(),
        modes: Vec::new(),
        capabilities: provider_capabilities("unknown"),
        configurable: true,
        has_api_key: false,
        base_url: None,
        model: None,
    });
    providers
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
        "claude" | "gemini" | "grok" => ProviderCapabilities {
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

fn provider_display_name(id: &str) -> &'static str {
    match id {
        "codex" => "Codex CLI",
        "claude" => "Claude Code",
        "gemini" => "Gemini CLI",
        "grok" => "Grok CLI",
        _ => "Provider CLI",
    }
}

fn provider_auth_help(id: &str) -> String {
    match id {
        "codex" => "Codex CLI is installed but not authenticated. Run `codex login`, then retry.".into(),
        "claude" => "Claude Code is installed but not authenticated. Run `claude auth login`, then retry.".into(),
        "gemini" => "Gemini CLI could not authenticate the headless request. Run `gemini` and complete its supported sign-in flow, then retry.".into(),
        "grok" => "Grok CLI is installed but not authenticated. Run `grok login`, then retry.".into(),
        _ => "The provider is not authenticated.".into(),
    }
}

fn nested_string(value: &serde_json::Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| {
        let mut current = value;
        for key in *path {
            current = current.get(*key)?;
        }
        current.as_str().map(str::to_string)
    })
}

fn message_text(value: &serde_json::Value) -> Option<String> {
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

fn append_stream_text(response: &mut String, provider_id: &str, text: &str) {
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

fn provider_failure_message(
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

fn response_reports_generation_failure(response: &str) -> bool {
    let lower = response.to_ascii_lowercase();
    lower.contains("generation_failed:")
        || lower.contains("unable to publish the")
        || lower.contains("withdrawing the candidate")
        || (lower.contains("did not pass the final visual acceptance gate")
            && lower.contains("restor"))
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
    if !matches!(
        conversation.provider.as_str(),
        "codex" | "claude" | "gemini" | "grok"
    ) {
        return Err(CommandError::new(
            "provider_unsupported",
            "This conversation does not use a supported CLI provider",
        ));
    }
    let provider_id = conversation.provider.clone();
    let executable = find_executable(&provider_id).ok_or_else(|| {
        CommandError::new(
            "provider_unavailable",
            format!(
                "{} was not found. Install its CLI, authenticate it, then retry detection.",
                provider_display_name(&provider_id)
            ),
        )
    })?;
    if provider_id != "gemini" && !provider_is_authenticated(&provider_id, &executable) {
        return Err(CommandError::new(
            "provider_unauthenticated",
            provider_auth_help(&provider_id),
        ));
    }
    let options = options.unwrap_or_default();
    validate_provider_options(&options)?;
    let capabilities = provider_capabilities(&provider_id);
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
    let image_provider_id = options.image_provider_id.as_deref().unwrap_or("imagegen");
    // Chats created before the provider-native option inherited Codex's
    // `imagegen` setting. Keep those chats usable: non-Codex CLIs can work
    // directly, while Codex continues to use its existing ImageGen path.
    let provider_native_image = image_provider_id == "provider-native"
        || (image_provider_id == "imagegen" && provider_id != "codex");
    if image_provider_id == "midjourney" {
        return Err(CommandError::new(
            "provider_unsupported",
            "Midjourney does not provide a public API for this integration",
        ));
    }
    let image_provider = if provider_native_image {
        None
    } else {
        load_image_provider(&state, image_provider_id)?
    };
    if !provider_native_image && image_provider_id != "imagegen" && image_provider.is_none() {
        return Err(CommandError::new(
            "provider_unavailable",
            "Configure the selected image provider in Settings before generating",
        ));
    }
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
    let image_prompt = format!("{prompt}\n\n{combined_context}\n\nCreate one clean, centered, motion-ready game-art source master. Use a plain removable background, clear silhouette, and no text, labels, contact sheet, or multiple poses.");
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
        image_provider,
        image_prompt,
        provider_id,
    };
    tauri::async_runtime::spawn(async move {
        run_provider(task_app, run, cancel_rx).await;
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
    image_provider: Option<StoredImageProvider>,
    image_prompt: String,
    provider_id: String,
}

#[derive(Debug, Deserialize)]
struct ImageApiResponse {
    data: Vec<ImageApiItem>,
}

#[derive(Debug, Deserialize)]
struct ImageApiItem {
    #[serde(default)]
    b64_json: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

async fn generate_external_source(
    workspace: &Path,
    provider: &StoredImageProvider,
    prompt: &str,
) -> CommandResult<PathBuf> {
    let endpoint = if provider.base_url.ends_with("/images/generations") {
        provider.base_url.clone()
    } else {
        format!(
            "{}/images/generations",
            provider.base_url.trim_end_matches('/')
        )
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|error| CommandError::new("provider_error", error.to_string()))?;
    let response = client.post(endpoint)
        .bearer_auth(&provider.api_key)
        .json(&serde_json::json!({"model": provider.model, "prompt": prompt, "n": 1, "response_format": "b64_json"}))
        .send().await.map_err(|error| CommandError::new("provider_error", format!("{} request failed: {error}", provider.name)))?;
    let status = response.status();
    if !status.is_success() {
        return Err(CommandError::new(
            "provider_error",
            format!(
                "{} returned {status}. Check the endpoint, model ID, API key, and account access.",
                provider.name
            ),
        ));
    }
    let result: ImageApiResponse = response.json().await.map_err(|error| {
        CommandError::new(
            "provider_error",
            format!(
                "{} returned an invalid image response: {error}",
                provider.name
            ),
        )
    })?;
    let item = result.data.into_iter().next().ok_or_else(|| {
        CommandError::new(
            "provider_error",
            format!("{} returned no image", provider.name),
        )
    })?;
    let bytes = if let Some(encoded) = item.b64_json {
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|error| {
                CommandError::new(
                    "provider_error",
                    format!("Could not decode {} image: {error}", provider.name),
                )
            })?
    } else if let Some(url) = item.url {
        client
            .get(url)
            .send()
            .await
            .map_err(|error| {
                CommandError::new(
                    "provider_error",
                    format!("Could not download {} image: {error}", provider.name),
                )
            })?
            .error_for_status()
            .map_err(|error| CommandError::new("provider_error", error.to_string()))?
            .bytes()
            .await
            .map_err(|error| CommandError::new("provider_error", error.to_string()))?
            .to_vec()
    } else {
        return Err(CommandError::new(
            "provider_error",
            format!(
                "{} response contained neither image data nor a URL",
                provider.name
            ),
        ));
    };
    let image = image::load_from_memory(&bytes)?;
    let directory = workspace.join(".sprite-studio/provider-sources");
    fs::create_dir_all(&directory)?;
    let output = directory.join(format!("{}-{}.png", provider.id, Uuid::new_v4()));
    image.save_with_format(&output, image::ImageFormat::Png)?;
    Ok(output)
}

async fn run_provider(app: AppHandle, run: ProviderRun, mut cancel_rx: oneshot::Receiver<()>) {
    let ProviderRun {
        request_id,
        conversation_id,
        workspace_id,
        session_id,
        assistant_id,
        mut prompt,
        model,
        reasoning_effort,
        mut reference_paths,
        executable,
        image_provider,
        image_prompt,
        provider_id,
    } = run;
    let state = app.state::<AppState>();
    emit(
        &app,
        &request_id,
        &conversation_id,
        "started",
        format!(
            "{} is working in this workspace",
            provider_display_name(&provider_id)
        ),
    );
    let workspace = match workspace_path(&state, &workspace_id) {
        Ok(path) => path,
        Err(error) => {
            let _ = update_message(&state, &assistant_id, &error.message, "failed");
            emit(&app, &request_id, &conversation_id, "failed", error.message);
            return;
        }
    };
    if let Some(provider) = image_provider.as_ref() {
        emit(
            &app,
            &request_id,
            &conversation_id,
            "activity",
            format!("Generating the source master with {}", provider.name),
        );
        let generation = generate_external_source(&workspace, provider, &image_prompt);
        tokio::pin!(generation);
        let generated = tokio::select! {
            result = &mut generation => result,
            _ = &mut cancel_rx => {
                let _ = update_message(&state, &assistant_id, "Request cancelled", "cancelled");
                emit(&app, &request_id, &conversation_id, "cancelled", "Request cancelled");
                state.cancellers.lock().ok().map(|mut values| values.remove(&request_id));
                return;
            }
        };
        match generated {
            Ok(path) => {
                let display = path
                    .strip_prefix(&workspace)
                    .unwrap_or(&path)
                    .to_string_lossy();
                prompt = format!("EXTERNAL SOURCE MASTER\nA source image was generated by {} and attached at `{display}`. Treat this exact attached file as the context source master. Do not call ImageGen for the initial source pass. Inspect it, normalize it with the bundled PNG tools, and continue through the routed harness.\n\n{prompt}", provider.name);
                reference_paths.push(path.to_string_lossy().into_owned());
                emit(
                    &app,
                    &request_id,
                    &conversation_id,
                    "activity",
                    format!(
                        "{} source master saved; starting rig planning",
                        provider.name
                    ),
                );
            }
            Err(error) => {
                let _ = update_message(&state, &assistant_id, &error.message, "failed");
                emit(&app, &request_id, &conversation_id, "failed", error.message);
                state
                    .cancellers
                    .lock()
                    .ok()
                    .map(|mut values| values.remove(&request_id));
                return;
            }
        }
    }
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
    if provider_id != "codex" && !reference_paths.is_empty() {
        let paths = reference_paths
            .iter()
            .map(|path| format!("- `{path}`"))
            .collect::<Vec<_>>()
            .join("\n");
        prompt.push_str(&format!(
            "\n\nATTACHED REFERENCE FILES\nInspect these local files before acting:\n{paths}"
        ));
    }
    let prompt_file = if provider_id == "grok" {
        match create_provider_prompt_file(&workspace, &request_id, &prompt) {
            Ok(path) => Some(path),
            Err(error) => {
                let message = format!("Could not prepare the Grok prompt: {error}");
                let _ = update_message(&state, &assistant_id, &message, "failed");
                emit(&app, &request_id, &conversation_id, "failed", message);
                return;
            }
        }
    } else {
        None
    };
    let arguments = provider_arguments(
        &provider_id,
        session_id.as_deref(),
        model.as_deref(),
        reasoning_effort.as_deref(),
        &reference_paths,
        prompt_file.as_deref(),
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
            if let Some(path) = prompt_file.as_ref() {
                let _ = fs::remove_file(path);
            }
            let message = format!(
                "Could not start {}: {error}",
                provider_display_name(&provider_id)
            );
            let _ = update_message(&state, &assistant_id, &message, "failed");
            emit(&app, &request_id, &conversation_id, "failed", message);
            return;
        }
    };
    if provider_id != "grok" {
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(error) = stdin.write_all(prompt.as_bytes()).await {
                if let Some(path) = prompt_file.as_ref() {
                    let _ = fs::remove_file(path);
                }
                let message = format!(
                    "Could not send the prompt to {}: {error}",
                    provider_display_name(&provider_id)
                );
                let _ = update_message(&state, &assistant_id, &message, "failed");
                emit(&app, &request_id, &conversation_id, "failed", message);
                return;
            }
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
        // Codex can spend several minutes planning or running a tool without
        // emitting a JSON line. Silence is not evidence that its process is
        // stuck, so keep the request alive until it exits or the user stops it.
        // A lightweight heartbeat gives the UI useful feedback in that quiet
        // period without pretending that work has completed.
        let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
        heartbeat.tick().await;
        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    cancelled = true;
                    let _ = child.kill().await;
                    break;
                }
                _ = heartbeat.tick() => {
                    emit(&app, &request_id, &conversation_id, "activity", format!("{} is still working — you can stop this request at any time", provider_display_name(&provider_id)));
                }
                line = lines.next_line() => match line {
                    Ok(Some(line)) => {
                        let (text, activity, session_id) = parse_stream_line(&provider_id, &line, !response.is_empty());
                        if let Some(text) = text {
                            append_stream_text(&mut response, &provider_id, &text);
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
    if let Some(path) = prompt_file.as_ref() {
        let _ = fs::remove_file(path);
    }
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
                response = format!(
                    "{} completed without returning a text response.",
                    provider_display_name(&provider_id)
                );
            }
            // A /rig turn (or any reply carrying the rig-suggestion contract)
            // is captured here so the Rig editor can pick it up immediately.
            if let Ok(conversation) = get_conversation(&state, &conversation_id) {
                if let Ok(Some(name)) = crate::rig::capture_chat_suggestion(
                    &state,
                    &workspace_id,
                    conversation.worktree_id.as_deref(),
                    &response,
                ) {
                    emit(
                        &app,
                        &request_id,
                        &conversation_id,
                        "activity",
                        format!("{name} captured — open the Rig tab to review and render it"),
                    );
                }
            }
            if response_reports_generation_failure(&response) {
                let _ = update_message(&state, &assistant_id, &response, "failed");
                emit(&app, &request_id, &conversation_id, "failed", response);
            } else {
                let _ = update_message(&state, &assistant_id, &response, "completed");
                emit(&app, &request_id, &conversation_id, "completed", response);
            }
        }
        Ok(exit) => {
            let mut message = provider_failure_message(
                &provider_id,
                &exit.to_string(),
                &stderr_output,
                &response,
            );
            let lower = message.to_lowercase();
            if lower.contains("auth")
                || lower.contains("login")
                || lower.contains("credential")
                || lower.contains("api key")
            {
                message = provider_auth_help(&provider_id);
            }
            let _ = update_message(&state, &assistant_id, &message, "failed");
            emit(&app, &request_id, &conversation_id, "failed", message);
        }
        Err(error) => {
            let message = format!(
                "Could not observe the {} process: {error}",
                provider_display_name(&provider_id)
            );
            let _ = update_message(&state, &assistant_id, &message, "failed");
            emit(&app, &request_id, &conversation_id, "failed", message);
        }
    }
}

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
) -> CommandResult<String> {
    if !matches!(provider_id, "codex" | "claude" | "gemini" | "grok") {
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
    if provider_id != "gemini" && !provider_is_authenticated(provider_id, &executable) {
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
    let mut child = Command::new(&executable)
        .args(&arguments)
        .env("PATH", current_provider_environment_path())
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
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
            stdin.write_all(prompt.as_bytes()).await.map_err(|error| {
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
    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        let collect = async {
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let (text, _, _) = parse_stream_line(provider_id, &line, !response.is_empty());
                        if let Some(text) = text {
                            if !response.is_empty() && !response.ends_with('\n') {
                                response.push('\n');
                            }
                            response.push_str(&text);
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        read_failed = Some(error.to_string());
                        break;
                    }
                }
            }
        };
        // A structured ask should answer in one pass; a silent or stuck CLI
        // must not hold the caller's UI forever.
        if tokio::time::timeout(Duration::from_secs(300), collect)
            .await
            .is_err()
        {
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

pub(crate) fn provider_arguments(
    provider_id: &str,
    session_id: Option<&str>,
    model: Option<&str>,
    reasoning_effort: Option<&str>,
    reference_paths: &[String],
    prompt_file: Option<&Path>,
) -> Vec<String> {
    match provider_id {
        "codex" => codex_arguments(session_id, model, reasoning_effort, reference_paths),
        "claude" => {
            let mut arguments = vec![
                "--print".into(),
                "--input-format".into(),
                "text".into(),
                "--output-format".into(),
                "stream-json".into(),
                "--include-partial-messages".into(),
                "--verbose".into(),
                "--permission-mode".into(),
                "acceptEdits".into(),
            ];
            if let Some(session_id) = session_id {
                arguments.extend(["--resume".into(), session_id.into()]);
            }
            if let Some(model) = model {
                arguments.extend(["--model".into(), model.into()]);
            }
            if let Some(effort) = reasoning_effort {
                arguments.extend(["--effort".into(), effort.into()]);
            }
            arguments
        }
        "gemini" => {
            let mut arguments = vec![
                "--output-format".into(),
                "stream-json".into(),
                "--approval-mode".into(),
                "auto_edit".into(),
            ];
            if let Some(session_id) = session_id {
                arguments.extend(["--resume".into(), session_id.into()]);
            }
            if let Some(model) = model {
                arguments.extend(["--model".into(), model.into()]);
            }
            arguments
        }
        "grok" => {
            let mut arguments = vec![
                "--output-format".into(),
                "streaming-messages-json".into(),
                "--include-partial-messages".into(),
                "--permission-mode".into(),
                "acceptEdits".into(),
            ];
            if let Some(path) = prompt_file {
                arguments.extend(["--prompt-file".into(), path.to_string_lossy().into_owned()]);
            }
            if let Some(session_id) = session_id {
                arguments.extend(["--resume".into(), session_id.into()]);
            }
            if let Some(model) = model {
                arguments.extend(["--model".into(), model.into()]);
            }
            if let Some(effort) = reasoning_effort {
                arguments.extend(["--reasoning-effort".into(), effort.into()]);
            }
            arguments
        }
        _ => Vec::new(),
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
            "animate" | "sprite" | "character" | "effect" | "pack" | "rig"
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
        || !(1..=64).contains(&generation.frames)
        || !(1..=60).contains(&generation.fps)
        || !matches!(generation.frame_mode.as_str(), "fixed" | "auto")
        || !(1..=64).contains(&generation.min_frames)
        || !(1..=64).contains(&generation.max_frames)
        || generation.min_frames > generation.max_frames
    {
        return Err(CommandError::new(
            "invalid_generation_profile",
            "Use an 8–512 px canvas, Fixed or Auto frames within 1–64, and 1–60 FPS",
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
        append_stream_text, codex_arguments, login_shell_path, login_shell_path_with_timeout,
        merge_provider_paths, parse_codex_line, parse_stream_line, provider_arguments,
        provider_environment_path, provider_failure_message, response_reports_generation_failure,
        validate_provider_options,
    };
    use crate::models::{GenerationOptions, ProviderRequestOptions};
    use std::{
        env,
        path::{Path, PathBuf},
    };

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
    fn represents_transient_provider_errors_as_activity() {
        let line = r#"{"type":"error","message":"resume payload is invalid"}"#;
        let (content, activity, _) = parse_codex_line(line);

        assert!(content.is_none());
        assert_eq!(
            activity.as_deref(),
            Some("Connection interrupted; recovering — resume payload is invalid")
        );
    }

    #[test]
    fn nonzero_exit_uses_provider_response_when_stderr_is_empty() {
        let message =
            provider_failure_message("codex", "exit status: 1", "", "resume payload is invalid");

        assert_eq!(message, "resume payload is invalid");
    }

    #[test]
    fn nonzero_exit_preserves_provider_response_and_stderr() {
        let message = provider_failure_message(
            "codex",
            "exit status: 1",
            "provider diagnostic context",
            "resume payload is invalid",
        );

        assert!(message.contains("resume payload is invalid"));
        assert!(message.contains("provider diagnostic context"));
    }

    #[test]
    fn accepted_process_with_rejected_generation_is_a_failure() {
        assert!(response_reports_generation_failure(
            "Unable to publish the rabbit hop: the repaired rig did not pass the final visual acceptance gate, so I am restoring the prior manifest."
        ));
        assert!(!response_reports_generation_failure(
            "Published rabbit_hop with nine validated frames."
        ));
    }

    #[test]
    fn parses_claude_partial_text_and_session() {
        let line = r#"{"type":"stream_event","session_id":"session-1","event":{"delta":{"text":"hello"}}}"#;
        let (content, _, session) = parse_stream_line("claude", line, false);
        assert_eq!(content.as_deref(), Some("hello"));
        assert_eq!(session.as_deref(), Some("session-1"));
    }

    #[test]
    fn streamed_cli_deltas_keep_their_original_spacing() {
        let mut grok = String::new();
        append_stream_text(&mut grok, "grok", "Start");
        append_stream_text(&mut grok, "grok", " by reading");
        append_stream_text(&mut grok, "grok", " the project.");
        assert_eq!(grok, "Start by reading the project.");

        let mut codex = String::new();
        append_stream_text(&mut codex, "codex", "First item.");
        append_stream_text(&mut codex, "codex", "Second item.");
        assert_eq!(codex, "First item.\nSecond item.");
    }

    #[test]
    fn parses_gemini_assistant_messages() {
        let line = r#"{"type":"message","role":"assistant","content":"done"}"#;
        let (content, _, _) = parse_stream_line("gemini", line, false);
        assert_eq!(content.as_deref(), Some("done"));
    }

    #[test]
    fn builds_supported_headless_provider_arguments() {
        let claude = provider_arguments("claude", None, Some("sonnet"), Some("high"), &[], None);
        assert!(claude
            .windows(2)
            .any(|pair| pair == ["--output-format", "stream-json"]));
        assert!(claude
            .windows(2)
            .any(|pair| pair == ["--permission-mode", "acceptEdits"]));

        let gemini = provider_arguments("gemini", Some("session-2"), None, None, &[], None);
        assert!(gemini
            .windows(2)
            .any(|pair| pair == ["--resume", "session-2"]));
        assert!(gemini
            .windows(2)
            .any(|pair| pair == ["--approval-mode", "auto_edit"]));

        let grok = provider_arguments(
            "grok",
            None,
            Some("grok-4.6"),
            None,
            &[],
            Some(Path::new("/tmp/prompt.txt")),
        );
        assert!(grok
            .windows(2)
            .any(|pair| pair == ["--prompt-file", "/tmp/prompt.txt"]));
        assert!(grok
            .windows(2)
            .any(|pair| pair == ["--output-format", "streaming-messages-json"]));
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
            image_provider_id: None,
        };
        assert!(validate_provider_options(&options).is_ok());
        let mut pack_options = options;
        pack_options.command = Some("pack".into());
        assert!(validate_provider_options(&pack_options).is_ok());
    }
}
