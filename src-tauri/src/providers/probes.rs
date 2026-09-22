use super::discovery::{
    isolate_login_shell, login_shell_state, provider_process_path, stop_login_shell,
    LoginShellState,
};
use super::headless::apply_std_headless_flags;
use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbeFailureKind {
    Spawn,
    Timeout,
    Io,
}

#[derive(Debug, Clone)]
pub(crate) struct ProbeFailure {
    pub kind: ProbeFailureKind,
    pub message: String,
}

#[derive(Debug)]
pub(crate) enum ProbeOutcome {
    Ran(std::process::Output),
    Failed(ProbeFailure),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AuthCheck {
    Authenticated,
    NotLoggedIn,
    ProbeFailed { detail: String },
}

const PROBE_TIMEOUT: Duration = Duration::from_secs(4);
const PROBE_RETRY_DELAY: Duration = Duration::from_millis(200);

pub(crate) fn command_output(
    provider_id: &str,
    executable: &Path,
    arguments: &[&str],
) -> ProbeOutcome {
    let first = run_probe_once(provider_id, executable, arguments);
    if matches!(first, ProbeOutcome::Failed(_)) {
        thread::sleep(PROBE_RETRY_DELAY);
        let second = run_probe_once(provider_id, executable, arguments);
        log_probe(provider_id, executable, arguments, &second);
        return second;
    }
    log_probe(provider_id, executable, arguments, &first);
    first
}

pub(crate) fn probe_failure_detail(
    provider_id: &str,
    executable: &Path,
    arguments: &[&str],
    failure: &ProbeFailure,
) -> String {
    let command = format_probe_command(provider_id, arguments);
    let reason = match failure.kind {
        ProbeFailureKind::Spawn => format!("spawn error: {}", failure.message),
        ProbeFailureKind::Timeout => format!("timed out after {}s", PROBE_TIMEOUT.as_secs()),
        ProbeFailureKind::Io => format!("I/O error: {}", failure.message),
    };
    format!(
        "Could not run {command} ({reason}). Path: {path}. Press Detect again. This is not a sign-in problem.",
        path = executable.display()
    )
}

pub(crate) fn format_probe_command(provider_id: &str, arguments: &[&str]) -> String {
    let label = match provider_id {
        "codex" => "`codex login status`",
        "claude" => "`claude auth status`",
        "grok" | "antigravity" => "`models`",
        "cursor" => "`cursor-agent status`",
        _ => "the provider status probe",
    };
    if arguments.is_empty() {
        label.to_string()
    } else {
        format!("`{} {}`", provider_id, arguments.join(" "))
    }
}

fn run_probe_once(
    provider_id: &str,
    executable: &Path,
    arguments: &[&str],
) -> ProbeOutcome {
    let token = Uuid::new_v4();
    let stdout_path = env::temp_dir().join(format!("sprite-studio-provider-{token}.out"));
    let stderr_path = env::temp_dir().join(format!("sprite-studio-provider-{token}.err"));
    let outcome = (|| {
        let stdout = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&stdout_path)
            .map_err(|error| ProbeFailure {
                kind: ProbeFailureKind::Io,
                message: error.to_string(),
            })?;
        let stderr = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&stderr_path)
            .map_err(|error| ProbeFailure {
                kind: ProbeFailureKind::Io,
                message: error.to_string(),
            })?;
        let mut command = StdCommand::new(executable);
        command
            .args(arguments)
            .env("PATH", provider_process_path(provider_id))
            .current_dir(probe_working_directory())
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        apply_std_headless_flags(&mut command);
        isolate_login_shell(&mut command);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                return Err(ProbeFailure {
                    kind: ProbeFailureKind::Spawn,
                    message: error.to_string(),
                });
            }
        };
        let deadline = Instant::now() + PROBE_TIMEOUT;
        let status = loop {
            match login_shell_state(&mut child) {
                LoginShellState::Exited => match stop_login_shell(&mut child, true) {
                    Some(status) => break status,
                    None => {
                        return Err(ProbeFailure {
                            kind: ProbeFailureKind::Io,
                            message: "probe process exited without a status".into(),
                        });
                    }
                },
                LoginShellState::Running => {}
                LoginShellState::Error => {
                    let _ = stop_login_shell(&mut child, false);
                    return Err(ProbeFailure {
                        kind: ProbeFailureKind::Io,
                        message: "probe process state error".into(),
                    });
                }
            }
            if Instant::now() >= deadline {
                let _ = stop_login_shell(&mut child, false);
                return Err(ProbeFailure {
                    kind: ProbeFailureKind::Timeout,
                    message: format!("exceeded {}s", PROBE_TIMEOUT.as_secs()),
                });
            }
            thread::sleep(Duration::from_millis(10));
        };
        Ok(std::process::Output {
            status,
            stdout: fs::read(&stdout_path).unwrap_or_default(),
            stderr: fs::read(&stderr_path).unwrap_or_default(),
        })
    })();
    let _ = fs::remove_file(stdout_path);
    let _ = fs::remove_file(stderr_path);
    match outcome {
        Ok(output) => ProbeOutcome::Ran(output),
        Err(failure) => ProbeOutcome::Failed(failure),
    }
}

fn probe_working_directory() -> PathBuf {
    env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
}

fn log_probe(
    provider_id: &str,
    executable: &Path,
    arguments: &[&str],
    outcome: &ProbeOutcome,
) {
    let log_path = probe_log_path();
    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut file = match OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(file) => file,
        Err(_) => return,
    };
    let summary = match outcome {
        ProbeOutcome::Ran(output) => format!(
            "ran exit={} stdout={} stderr={}",
            output.status,
            output.stdout.len(),
            output.stderr.len()
        ),
        ProbeOutcome::Failed(failure) => format!("failed {:?}: {}", failure.kind, failure.message),
    };
    let _ = writeln!(
        file,
        "{} provider={} exe={} args={:?} {}",
        chrono::Utc::now().to_rfc3339(),
        provider_id,
        executable.display(),
        arguments,
        summary
    );
}

fn probe_log_path() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(|root| {
            Path::new(&root)
                .join("com.jakes.sprite-maker")
                .join("logs")
                .join("provider-probes.log")
        })
        .unwrap_or_else(|| env::temp_dir().join("sprite-studio-provider-probes.log"))
}
