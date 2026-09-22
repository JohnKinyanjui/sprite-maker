use crate::{
    error::{CommandError, CommandResult},
    providers::apply_tokio_headless_flags,
    workspace::{python::resolve_python_launcher, workspace_path},
    AppState,
};
use serde_json::Value;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use tauri::State;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

#[tauri::command]
pub async fn run_sprite_polish(
    workspace_id: String,
    master: String,
    rough: String,
    input: String,
    output: String,
    state: State<'_, AppState>,
) -> CommandResult<Value> {
    let root = workspace_path(&state, &workspace_id)?;
    let script = root.join(".sprite-studio/sprite_polish.py");
    if !script.is_file() {
        return Err(CommandError::new(
            "polish_tool_missing",
            "Bundled sprite_polish.py is not available in this workspace",
        ));
    }
    let master = workspace_relative_arg(&root, &master)?;
    let rough = workspace_relative_arg(&root, &rough)?;
    let input = workspace_relative_arg(&root, &input)?;
    let output = workspace_relative_arg(&root, &output)?;
    let launcher = resolve_python_launcher().ok_or_else(|| {
        CommandError::new(
            "polish_python_missing",
            crate::workspace::python::PYTHON_MISSING_DETAIL,
        )
    })?;
    let mut command = Command::new(&launcher.program);
    command
        .args(&launcher.args)
        .arg(&script)
        .arg("--master")
        .arg(&master)
        .arg("--rough")
        .arg(&rough)
        .arg("--input")
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    apply_tokio_headless_flags(&mut command);
    let output_process = timeout(Duration::from_secs(120), command.output())
        .await
        .map_err(|_| {
            CommandError::new(
                "polish_timeout",
                "sprite_polish took longer than 120 seconds",
            )
        })?
        .map_err(|error| CommandError::new("polish_failed", error.to_string()))?;
    if !output_process.status.success() {
        let stderr = String::from_utf8_lossy(&output_process.stderr);
        let stdout = String::from_utf8_lossy(&output_process.stdout);
        let message = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else {
            stdout.trim().to_string()
        };
        return Err(CommandError::new("polish_rejected", message));
    }
    let stdout = String::from_utf8_lossy(&output_process.stdout);
    let parsed = serde_json::from_str(stdout.trim()).map_err(|error| {
        CommandError::new(
            "polish_output_invalid",
            format!("sprite_polish returned invalid JSON: {error}"),
        )
    })?;
    let output_path = resolve_workspace_path(&root, &output)?;
    crate::allow_asset_file(None, &output_path)?;
    Ok(parsed)
}

#[tauri::command]
pub async fn archive_sprite_paths(
    workspace_id: String,
    paths: Vec<String>,
    label: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<String>> {
    let root = workspace_path(&state, &workspace_id)?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%fZ");
    let archive_root = root
        .join(".sprite-studio")
        .join("polish-rough")
        .join(format!("{}-{}", label, stamp));
    std::fs::create_dir_all(&archive_root)?;
    let mut archived = Vec::with_capacity(paths.len());
    for relative in paths {
        let source = resolve_workspace_path(&root, &relative)?;
        if !source.is_file() {
            return Err(CommandError::new(
                "rough_frame_missing",
                format!("Rough frame is missing: {relative}"),
            ));
        }
        let destination = archive_root.join(
            Path::new(&relative)
                .file_name()
                .ok_or_else(|| CommandError::new("invalid_path", format!("Invalid path: {relative}")))?,
        );
        std::fs::copy(&source, &destination)?;
        archived.push(
            destination
                .strip_prefix(&root)
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| destination.to_string_lossy().into_owned()),
        );
    }
    Ok(archived)
}

#[tauri::command]
pub async fn restore_sprite_paths(
    workspace_id: String,
    backup_paths: Vec<String>,
    target_paths: Vec<String>,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    if backup_paths.len() != target_paths.len() {
        return Err(CommandError::new(
            "restore_mismatch",
            "Backup and target path lists must match",
        ));
    }
    let root = workspace_path(&state, &workspace_id)?;
    for (backup, target) in backup_paths.into_iter().zip(target_paths) {
        let source = resolve_workspace_path(&root, &backup)?;
        let destination = resolve_workspace_path(&root, &target)?;
        if !source.is_file() {
            return Err(CommandError::new(
                "backup_missing",
                format!("Backup frame is missing: {backup}"),
            ));
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&source, &destination)?;
        crate::allow_asset_file(None, &destination)?;
    }
    Ok(())
}

fn workspace_relative_arg(root: &Path, value: &str) -> CommandResult<String> {
    let resolved = resolve_workspace_path(root, value)?;
    Ok(resolved
        .strip_prefix(root)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| value.replace('\\', "/")))
}

fn resolve_workspace_path(root: &Path, value: &str) -> CommandResult<PathBuf> {
    if Path::new(value).is_absolute() {
        return Err(CommandError::new(
            "path_outside_workspace",
            "Workspace-relative paths are required for polish operations",
        ));
    }
    let normalized = value.replace('\\', "/");
    if normalized
        .split('/')
        .any(|part| part == ".." || part.is_empty() && normalized.contains("//"))
    {
        return Err(CommandError::new(
            "path_outside_workspace",
            "Parent directory traversal is not allowed",
        ));
    }
    let path = sanitize_workspace_join(root, &normalized)?;
    let canonical_root = root.canonicalize().map_err(CommandError::from)?;
    if path.exists() {
        let canonical = path.canonicalize().map_err(CommandError::from)?;
        if !canonical.starts_with(&canonical_root) {
            return Err(CommandError::new(
                "path_outside_workspace",
                "Path is outside the workspace",
            ));
        }
        return Ok(canonical);
    }
    if let Some(parent) = path.parent() {
        if parent.exists() {
            let canonical_parent = parent.canonicalize().map_err(CommandError::from)?;
            if !canonical_parent.starts_with(&canonical_root) {
                return Err(CommandError::new(
                    "path_outside_workspace",
                    "Path is outside the workspace",
                ));
            }
        }
    }
    path.strip_prefix(root).map_err(|_| {
        CommandError::new("path_outside_workspace", "Path is outside the workspace")
    })?;
    Ok(path)
}

fn sanitize_workspace_join(root: &Path, value: &str) -> CommandResult<PathBuf> {
    let mut joined = root.to_path_buf();
    for component in Path::new(value).components() {
        match component {
            Component::Normal(part) => joined.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(CommandError::new(
                    "path_outside_workspace",
                    "Parent directory traversal is not allowed",
                ));
            }
            _ => {
                return Err(CommandError::new(
                    "path_outside_workspace",
                    "Absolute or prefix paths are not allowed",
                ));
            }
        }
    }
    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::{resolve_workspace_path, sanitize_workspace_join};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn rejects_parent_traversal_paths() {
        let root = std::env::temp_dir().join(format!("sprite-polish-root-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("temp root");
        let error = resolve_workspace_path(&root, "assets/../outside.png")
            .expect_err("traversal should fail");
        assert_eq!(error.code, "path_outside_workspace");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn resolves_workspace_relative_paths() {
        let root = std::env::temp_dir().join(format!("sprite-polish-root-{}", uuid::Uuid::new_v4()));
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let file = assets.join("hero.png");
        fs::write(&file, b"png").expect("write file");
        let resolved = resolve_workspace_path(&root, "assets/hero.png").expect("resolve path");
        assert!(resolved.is_file());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn sanitize_join_rejects_prefix_paths() {
        let root = PathBuf::from("C:\\workspace");
        let error = sanitize_workspace_join(&root, "/etc/passwd").expect_err("prefix path");
        assert_eq!(error.code, "path_outside_workspace");
    }
}
