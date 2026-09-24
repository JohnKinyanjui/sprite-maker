//! Deterministic finalization after the runtime stops a provider whose
//! repair budget is spent. The agent is no longer involved: Sprite Studio
//! accepts this request's own rendered candidate, or renders the latest rig
//! with visual-quality gates downgraded to warnings. Structural integrity
//! (decodable frames, uniform canvas, workspace confinement, routed category,
//! two distinct frames, fresh manifest) still blocks publication.

use super::headless::apply_tokio_headless_flags;
use super::repair_budget::BudgetExhaustion;
use crate::workspace::resolve_python_launcher;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::{Duration, SystemTime},
};
use tokio::process::Command;

const CATEGORIES: &[&str] = &["characters", "creatures", "terrain", "props", "effects"];
const MANIFEST: &str = ".sprite-studio/last-generation.json";
const RENDER_TIMEOUT: Duration = Duration::from_secs(180);
const LOCK_RETRIES: u32 = 10;

#[derive(Debug)]
pub(crate) struct BudgetFinalization {
    pub response: String,
    pub published: bool,
}

#[derive(Debug, PartialEq)]
pub(crate) struct VerifiedManifest {
    pub name: String,
    pub category: String,
    pub files: Vec<String>,
    pub quality_warnings: Vec<String>,
}

pub(crate) async fn finalize_spent_budget(
    workspace: &Path,
    started_at: SystemTime,
    expected_category: Option<&str>,
    exhaustion: BudgetExhaustion,
) -> BudgetFinalization {
    let reason = exhaustion.describe();
    if let Ok(manifest) = verify_fresh_manifest(workspace, started_at, expected_category) {
        return published(&manifest, &reason);
    }
    let Some(rig) = latest_request_rig(workspace, started_at) else {
        return failed(format!(
            "{reason}, and this request saved no rig that could be rendered."
        ));
    };
    if let Err(error) = render_degraded(workspace, &rig).await {
        return failed(format!(
            "{reason}, and the latest rig could not produce a structurally valid animation: {error}"
        ));
    }
    match verify_fresh_manifest(workspace, started_at, expected_category) {
        Ok(manifest) => published(&manifest, &reason),
        Err(error) => failed(format!(
            "{reason}, and the final render failed integrity checks: {error}"
        )),
    }
}

fn published(manifest: &VerifiedManifest, reason: &str) -> BudgetFinalization {
    let mut limitation = format!("{reason}; remaining motion-quality issues were accepted as-is");
    let details = manifest
        .quality_warnings
        .iter()
        .take(3)
        .map(|warning| warning.chars().take(160).collect::<String>())
        .collect::<Vec<_>>();
    if !details.is_empty() {
        limitation.push_str(&format!(" ({})", details.join("; ")));
    }
    BudgetFinalization {
        response: format!(
            "Published `{}` — {} frames in `assets/{}/` (first frame `{}`).\n\nGENERATION_WARNING: {limitation}.",
            manifest.name,
            manifest.files.len(),
            manifest.category,
            manifest.files[0],
        ),
        published: true,
    }
}

fn failed(detail: String) -> BudgetFinalization {
    BudgetFinalization {
        response: format!("GENERATION_FAILED: {detail}"),
        published: false,
    }
}

/// The request's own manifest, verified for integrity. Anything written
/// before the request started belongs to an older turn and is rejected.
pub(crate) fn verify_fresh_manifest(
    workspace: &Path,
    started_at: SystemTime,
    expected_category: Option<&str>,
) -> Result<VerifiedManifest, String> {
    let path = workspace.join(MANIFEST);
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| "no generation manifest".to_string())?;
    if !metadata.is_file() {
        return Err("generation manifest is not a regular file".into());
    }
    let value: Value = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or("generation manifest is not valid JSON")?;
    let generated_at = value
        .get("generatedAt")
        .and_then(Value::as_str)
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .ok_or("generation manifest has no valid generatedAt")?
        .with_timezone(&Utc);
    let started = DateTime::<Utc>::from(started_at) - chrono::Duration::seconds(2);
    if generated_at < started {
        return Err("generation manifest predates this request".into());
    }
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .ok_or("generation manifest has no name")?
        .to_string();
    let category = value
        .get("category")
        .and_then(Value::as_str)
        .filter(|category| CATEGORIES.contains(category))
        .ok_or("generation manifest has an unsupported category")?
        .to_string();
    if let Some(expected) = expected_category {
        if category != expected {
            return Err(format!(
                "generation manifest category `{category}` does not match the routed category `{expected}`"
            ));
        }
    }
    let files = value
        .get("files")
        .and_then(Value::as_array)
        .ok_or("generation manifest lists no files")?
        .iter()
        .map(|file| file.as_str().map(str::to_string))
        .collect::<Option<Vec<_>>>()
        .ok_or("generation manifest files must be paths")?;
    if files.len() < 2 {
        return Err("an animation needs at least two frames".into());
    }
    let root = workspace
        .canonicalize()
        .map_err(|error| format!("workspace is unavailable: {error}"))?;
    let folder = Path::new("assets").join(&category);
    let mut dimensions = None;
    let mut distinct = HashSet::new();
    for file in &files {
        let relative = Path::new(file);
        if !relative.components().all(|part| matches!(part, Component::Normal(_)))
            || !relative.starts_with(&folder)
            || relative.extension().and_then(|value| value.to_str()) != Some("png")
        {
            return Err(format!("frame `{file}` is outside `{}`", folder.display()));
        }
        let full = workspace.join(relative);
        let is_regular = std::fs::symlink_metadata(&full).is_ok_and(|meta| meta.is_file());
        let confined = full.canonicalize().is_ok_and(|path| path.starts_with(&root));
        if !is_regular || !confined {
            return Err(format!("frame `{file}` is missing or escapes the workspace"));
        }
        let image = image::open(&full)
            .map_err(|error| format!("frame `{file}` is not a decodable PNG: {error}"))?
            .to_rgba8();
        let size = image.dimensions();
        if *dimensions.get_or_insert(size) != size {
            return Err(format!("frame `{file}` has a mismatched canvas size"));
        }
        let mut hasher = DefaultHasher::new();
        image.as_raw().hash(&mut hasher);
        distinct.insert(hasher.finish());
    }
    if distinct.len() < 2 {
        return Err("an animation needs at least two distinct frames".into());
    }
    let quality_warnings = value
        .get("qualityWarnings")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    Ok(VerifiedManifest {
        name,
        category,
        files,
        quality_warnings,
    })
}

/// The rig this request was editing: the newest rig JSON written since it started.
pub(crate) fn latest_request_rig(workspace: &Path, started_at: SystemTime) -> Option<String> {
    let threshold = started_at.checked_sub(Duration::from_secs(1)).unwrap_or(started_at);
    std::fs::read_dir(workspace.join(".sprite-studio/rigs"))
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let metadata = entry.path().symlink_metadata().ok()?;
            let modified = metadata.modified().ok()?;
            (name.ends_with(".json")
                && !name.starts_with('.')
                && metadata.is_file()
                && modified >= threshold)
                .then_some((modified, name))
        })
        .max()
        .map(|(_, name)| format!(".sprite-studio/rigs/{name}"))
}

async fn render_degraded(workspace: &Path, rig: &str) -> Result<(), String> {
    let launcher = resolve_python_launcher().ok_or("Python 3 is not available")?;
    let script = PathBuf::from(".sprite-studio").join("sprite_rig.py");
    for attempt in 0..=LOCK_RETRIES {
        let mut command = Command::new(&launcher.program);
        command
            .args(&launcher.args)
            .arg(&script)
            .arg("--finalize-degraded")
            .arg(rig)
            .current_dir(workspace)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        apply_tokio_headless_flags(&mut command);
        let output = tokio::time::timeout(RENDER_TIMEOUT, command.output())
            .await
            .map_err(|_| "the rig render timed out".to_string())?
            .map_err(|error| format!("could not run the rig engine: {error}"))?;
        if output.status.success() {
            return Ok(());
        }
        let diagnostic = String::from_utf8_lossy(&output.stderr).trim().to_string();
        // A validator or render the stopped provider launched may still hold
        // the render lock for a moment; wait for it rather than failing.
        if diagnostic.contains("another rig render is already committing") && attempt < LOCK_RETRIES {
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        }
        return Err(diagnostic
            .strip_prefix("sprite_rig: ")
            .unwrap_or(&diagnostic)
            .chars()
            .take(400)
            .collect());
    }
    Err("the rig render lock stayed busy".into())
}
