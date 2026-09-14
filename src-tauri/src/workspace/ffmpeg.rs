use crate::error::CommandResult;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegRuntimeStatus {
    pub available: bool,
    pub command: Option<String>,
    pub bundled: bool,
    pub detail: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct FfmpegRuntimeManifest {
    path: String,
}

pub(crate) const FFMPEG_MISSING_DETAIL: &str =
    "ffmpeg was not found. Reinstall Sprite Studio or install ffmpeg and add it to PATH.";

const RUNTIME_MANIFEST: &str = "ffmpeg-runtime.json";
const ENV_OVERRIDE: &str = "SPRITE_STUDIO_FFMPEG";

pub(crate) fn initialize_ffmpeg_runtime() {
    if let Some(path) = resolve_bundled_adjacent_to_executable() {
        let _ = persist_ffmpeg_runtime_path(&path);
    }
}

pub fn resolve_ffmpeg_executable() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var(ENV_OVERRIDE) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    if let Some(path) = read_runtime_manifest() {
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(path) = resolve_bundled_adjacent_to_executable() {
        return Some(path);
    }
    which::which("ffmpeg").ok()
}

pub fn ffmpeg_runtime_status() -> FfmpegRuntimeStatus {
    match resolve_ffmpeg_executable() {
        Some(path) => {
            let bundled = resolve_bundled_adjacent_to_executable()
                .is_some_and(|bundled| bundled == path);
            FfmpegRuntimeStatus {
                available: true,
                command: Some(path.display().to_string()),
                bundled,
                detail: if bundled {
                    format!("Bundled ffmpeg ready: {}", path.display())
                } else {
                    format!("ffmpeg ready: {}", path.display())
                },
            }
        }
        None => FfmpegRuntimeStatus {
            available: false,
            command: None,
            bundled: false,
            detail: FFMPEG_MISSING_DETAIL.into(),
        },
    }
}

fn runtime_manifest_path() -> PathBuf {
    crate::app_data_dir().join(RUNTIME_MANIFEST)
}

fn read_runtime_manifest() -> Option<PathBuf> {
    let path = runtime_manifest_path();
    if !path.is_file() {
        return None;
    }
    let raw = std::fs::read_to_string(&path).ok()?;
    let manifest = serde_json::from_str::<FfmpegRuntimeManifest>(&raw).ok()?;
    Some(PathBuf::from(manifest.path))
}

fn persist_ffmpeg_runtime_path(path: &Path) -> CommandResult<()> {
    let manifest = FfmpegRuntimeManifest {
        path: path.display().to_string(),
    };
    let payload = serde_json::to_string_pretty(&manifest).map_err(|error| {
        crate::error::CommandError::new("ffmpeg_runtime_json", error.to_string())
    })?;
    std::fs::create_dir_all(crate::app_data_dir())?;
    std::fs::write(runtime_manifest_path(), payload)?;
    Ok(())
}

fn resolve_bundled_adjacent_to_executable() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let directory = executable.parent()?;
    for name in ffmpeg_filenames() {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn ffmpeg_filenames() -> [&'static str; 2] {
    if cfg!(windows) {
        ["ffmpeg.exe", "ffmpeg"]
    } else {
        ["ffmpeg", "ffmpeg.exe"]
    }
}

#[tauri::command]
pub fn check_ffmpeg_runtime() -> FfmpegRuntimeStatus {
    ffmpeg_runtime_status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffmpeg_runtime_status_reports_missing_when_unavailable() {
        let previous = std::env::var(ENV_OVERRIDE).ok();
        std::env::remove_var(ENV_OVERRIDE);
        if resolve_ffmpeg_executable().is_some() {
            let status = ffmpeg_runtime_status();
            assert!(status.available);
            if previous.is_some() {
                std::env::set_var(ENV_OVERRIDE, previous.unwrap());
            }
            return;
        }
        let status = ffmpeg_runtime_status();
        assert!(!status.available);
        assert!(status.detail.contains("ffmpeg"));
        if let Some(value) = previous {
            std::env::set_var(ENV_OVERRIDE, value);
        }
    }
}
