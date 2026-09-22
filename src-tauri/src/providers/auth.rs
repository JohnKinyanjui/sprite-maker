use super::discovery::{current_provider_environment_path, find_executable};
use super::headless::apply_std_headless_flags;
use super::modes::provider_is_authenticated;
use crate::error::{CommandError, CommandResult};
use crate::models::ProviderInstallResult;
use std::path::Path;
use std::process::{Command as StdCommand, Stdio};

pub(crate) fn antigravity_cli_install_command() -> StdCommand {
    #[cfg(windows)]
    {
        let mut command = StdCommand::new("powershell");
        command.args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "irm https://antigravity.google/cli/install.ps1 | iex",
        ]);
        command
    }
    #[cfg(not(windows))]
    {
        let mut command = StdCommand::new("bash");
        command.args([
            "-lc",
            "curl -fsSL https://antigravity.google/cli/install.sh | bash",
        ]);
        command
    }
}

pub(crate) fn cursor_cli_install_command() -> StdCommand {
    #[cfg(windows)]
    {
        let mut command = StdCommand::new("powershell");
        command.args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "irm 'https://cursor.com/install?win32=true' | iex",
        ]);
        command
    }
    #[cfg(not(windows))]
    {
        let mut command = StdCommand::new("bash");
        command.args(["-lc", "curl https://cursor.com/install -fsS | bash"]);
        command
    }
}

fn agent_cli_display_name(provider_id: &str) -> &'static str {
    match provider_id {
        "cursor" => "Cursor CLI",
        "antigravity" => "Antigravity CLI",
        _ => "Agent CLI",
    }
}

fn run_agent_cli_install(provider_id: &str) -> CommandResult<String> {
    let (mut command, name) = match provider_id {
        "cursor" => (cursor_cli_install_command(), "Cursor CLI"),
        "antigravity" => (antigravity_cli_install_command(), "Antigravity CLI"),
        _ => {
            return Err(CommandError::new(
                "unsupported_provider",
                "Sprite Studio can only install Cursor CLI or Antigravity CLI from Settings",
            ))
        }
    };
    apply_std_headless_flags(&mut command);
    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| {
            CommandError::new(
                "install_failed",
                format!("Could not start the {name} installer: {error}"),
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = [stderr, stdout]
            .into_iter()
            .filter(|chunk| !chunk.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        return Err(CommandError::new(
            "install_failed",
            if detail.is_empty() {
                format!("{name} installation failed. Try running the official installer, then Detect again.")
            } else {
                format!("{name} installation failed: {detail}")
            },
        ));
    }
    Ok(if find_executable(provider_id).is_some() {
        match provider_id {
            "cursor" => "Cursor CLI installed. Sign in with `agent login` when you are ready.".into(),
            _ => "Antigravity CLI installed. Open a terminal with `agy` and complete Google sign-in when you are ready.".into(),
        }
    } else {
        format!("{name} installer finished. If Settings still shows it as missing, restart Sprite Studio and run Detect again.")
    })
}

#[tauri::command]
pub async fn install_agent_provider(provider_id: String) -> CommandResult<ProviderInstallResult> {
    if !matches!(provider_id.as_str(), "cursor" | "antigravity") {
        return Err(CommandError::new(
            "unsupported_provider",
            "Sprite Studio can only install Cursor CLI or Antigravity CLI from Settings right now",
        ));
    }
    let name = agent_cli_display_name(&provider_id);
    if find_executable(&provider_id).is_some() {
        return Ok(ProviderInstallResult {
            detail: format!("{name} is already installed. Run Detect again if Settings still shows it as missing."),
            installed: true,
        });
    }
    let install_id = provider_id.clone();
    let detail = tauri::async_runtime::spawn_blocking(move || run_agent_cli_install(&install_id))
        .await
        .map_err(|error| {
            CommandError::new(
                "install_failed",
                format!("{name} installation was interrupted: {error}"),
            )
        })??;
    Ok(ProviderInstallResult {
        installed: find_executable(&provider_id).is_some(),
        detail,
    })
}

fn spawn_agent_login(provider_id: &str, executable: &Path) -> CommandResult<()> {
    let mut command = StdCommand::new(executable);
    command.env("PATH", current_provider_environment_path());
    if provider_id == "cursor" {
        command.arg("login");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x00000010;
        command.creation_flags(CREATE_NEW_CONSOLE);
    }
    #[cfg(not(windows))]
    {
        command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
    }
    command.spawn().map_err(|error| {
        CommandError::new(
            "login_failed",
            format!(
                "Could not start {} sign-in: {error}",
                agent_cli_display_name(provider_id)
            ),
        )
    })?;
    Ok(())
}

#[tauri::command]
pub fn authenticate_agent_provider(provider_id: String) -> CommandResult<ProviderInstallResult> {
    if !matches!(provider_id.as_str(), "cursor" | "antigravity") {
        return Err(CommandError::new(
            "unsupported_provider",
            "Sprite Studio can only sign in to Cursor CLI or Antigravity CLI from Settings right now",
        ));
    }
    let name = agent_cli_display_name(&provider_id);
    let executable = find_executable(&provider_id).ok_or_else(|| {
        CommandError::new(
            "provider_unavailable",
            format!("Install {name} first, then sign in"),
        )
    })?;
    if provider_is_authenticated(&provider_id, &executable) {
        return Ok(ProviderInstallResult {
            detail: format!("{name} is already authenticated. Run Detect again if Settings still shows sign-in required."),
            installed: true,
        });
    }
    spawn_agent_login(&provider_id, &executable)?;
    Ok(ProviderInstallResult {
        installed: provider_is_authenticated(&provider_id, &executable),
        detail: "Complete sign-in in the terminal window that opened, then click Detect again."
            .into(),
    })
}
