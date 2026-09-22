use crate::error::CommandResult;
use crate::providers::apply_std_headless_flags;
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    process::{Command, Stdio},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonLauncher {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonRuntimeStatus {
    pub available: bool,
    pub command: Option<String>,
    pub detail: String,
}

pub(crate) const PYTHON_MISSING_DETAIL: &str =
    "Python 3 was not found on PATH. Install CPython from python.org or ensure `py -3` works. Without it, `.sprite-studio/sprite_tool.py` and `sprite_rig.py` cannot run.";

pub fn resolve_python_launcher() -> Option<PythonLauncher> {
    let candidates: [(&str, Vec<String>); 3] = [
        ("py", vec!["-3".to_string()]),
        ("python3", Vec::new()),
        ("python", Vec::new()),
    ];
    for (program, args) in candidates {
        if verify_python(program, &args) {
            return Some(PythonLauncher {
                program: program.to_string(),
                args,
            });
        }
    }
    None
}

pub fn python_runtime_status() -> PythonRuntimeStatus {
    match resolve_python_launcher() {
        Some(launcher) => PythonRuntimeStatus {
            available: true,
            command: Some(launcher_command_line(&launcher)),
            detail: format!(
                "Python runtime ready: {}",
                launcher_command_line(&launcher)
            ),
        },
        None => PythonRuntimeStatus {
            available: false,
            command: None,
            detail: PYTHON_MISSING_DETAIL.into(),
        },
    }
}

pub fn launcher_command_line(launcher: &PythonLauncher) -> String {
    if launcher.args.is_empty() {
        launcher.program.clone()
    } else {
        format!("{} {}", launcher.program, launcher.args.join(" "))
    }
}

pub fn inject_python_commands(text: &str, launcher: &PythonLauncher) -> String {
    let prefix = launcher_command_line(launcher);
    let replacement = format!("{prefix} .sprite-studio/");
    text.replace("python3 .sprite-studio/", &replacement)
        .replace("python .sprite-studio/", &replacement)
}

pub(crate) fn write_python_launcher_sidecar(
    metadata_dir: &Path,
    launcher: &PythonLauncher,
) -> CommandResult<()> {
    let payload = serde_json::to_string_pretty(launcher).map_err(|error| {
        crate::error::CommandError::new("python_launcher_json", error.to_string())
    })?;
    std::fs::write(metadata_dir.join("python_launcher.json"), payload)?;
    Ok(())
}

fn verify_python(program: &str, args: &[String]) -> bool {
    let mut command = Command::new(program);
    command
        .args(args)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    apply_std_headless_flags(&mut command);
    let output = match command.output() {
        Ok(output) => output,
        Err(_) => return false,
    };
    if !output.status.success() {
        return false;
    }
    let version = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    version.contains("Python 3.")
}

#[tauri::command]
pub fn check_python_runtime() -> PythonRuntimeStatus {
    python_runtime_status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_runtime_status_reports_missing_when_unavailable() {
        if resolve_python_launcher().is_some() {
            let status = python_runtime_status();
            assert!(status.available);
            assert!(status.command.is_some());
            return;
        }
        let status = python_runtime_status();
        assert!(!status.available);
        assert!(status.detail.contains("Python 3"));
    }

    #[test]
    fn inject_python_commands_replaces_workspace_prefix() {
        let launcher = PythonLauncher {
            program: "py".into(),
            args: vec!["-3".into()],
        };
        let text = "run python3 .sprite-studio/sprite_tool.py spec.json";
        assert_eq!(
            inject_python_commands(text, &launcher),
            "run py -3 .sprite-studio/sprite_tool.py spec.json"
        );
        let bare = "run python .sprite-studio/sprite_rig.py rig.json";
        assert_eq!(
            inject_python_commands(bare, &launcher),
            "run py -3 .sprite-studio/sprite_rig.py rig.json"
        );
        let mixed = "python3 .sprite-studio/sprite_tool.py a.json then python .sprite-studio/sprite_rig.py b.json";
        assert_eq!(
            inject_python_commands(mixed, &launcher),
            "py -3 .sprite-studio/sprite_tool.py a.json then py -3 .sprite-studio/sprite_rig.py b.json"
        );
    }
}
