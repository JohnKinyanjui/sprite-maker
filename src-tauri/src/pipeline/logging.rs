use crate::error::CommandResult;
use chrono::Utc;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::Write,
    path::Path,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PipelineLogEntry {
    pub timestamp: String,
    pub workspace_id: String,
    pub event: String,
    pub stage: Option<String>,
    pub duration_ms: Option<u64>,
    pub animation_id: Option<String>,
    pub job_id: Option<String>,
    pub passed: Option<bool>,
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContractRetryMetrics {
    pub duration_ms: u64,
    pub outcome: String,
    pub deterministic_passes: u32,
    pub ai_attempts: u32,
    pub failure_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage_durations_ms: Option<BTreeMap<String, u64>>,
}

pub(crate) struct PipelineStageTimer {
    last_mark: Instant,
    stage_durations_ms: BTreeMap<String, u64>,
}

impl PipelineStageTimer {
    pub(crate) fn start() -> Self {
        Self {
            last_mark: Instant::now(),
            stage_durations_ms: BTreeMap::new(),
        }
    }

    pub(crate) fn mark(&mut self, stage: impl Into<String>) {
        let stage = stage.into();
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_mark).as_millis() as u64;
        self.stage_durations_ms.insert(stage, elapsed);
        self.last_mark = now;
    }

    pub(crate) fn stage_durations_ms(&self) -> &BTreeMap<String, u64> {
        &self.stage_durations_ms
    }
}

pub(crate) struct PipelineTimer {
    started: Instant,
}

impl PipelineTimer {
    pub(crate) fn start() -> Self {
        Self {
            started: Instant::now(),
        }
    }

    pub(crate) fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
}

pub(crate) fn append_pipeline_log(root: &Path, entry: PipelineLogEntry) -> CommandResult<()> {
    let directory = root.join(".sprite-studio");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("pipeline-log.jsonl");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| {
            crate::error::CommandError::new("pipeline_log_write_failed", error.to_string())
        })?;
    let line = serde_json::to_string(&entry).map_err(|error| {
        crate::error::CommandError::new("serialization_error", error.to_string())
    })?;
    writeln!(file, "{line}").map_err(|error| {
        crate::error::CommandError::new("pipeline_log_write_failed", error.to_string())
    })?;
    Ok(())
}

pub(crate) fn log_pipeline_event(
    root: &Path,
    workspace_id: &str,
    event: &str,
    stage: Option<&str>,
    duration: Option<Duration>,
    animation_id: Option<&str>,
    job_id: Option<&str>,
    passed: Option<bool>,
    error_code: Option<&str>,
    details: Option<serde_json::Value>,
) {
    let _ = append_pipeline_log(
        root,
        PipelineLogEntry {
            timestamp: Utc::now().to_rfc3339(),
            workspace_id: workspace_id.to_string(),
            event: event.to_string(),
            stage: stage.map(str::to_string),
            duration_ms: duration.map(|value| value.as_millis() as u64),
            animation_id: animation_id.map(str::to_string),
            job_id: job_id.map(str::to_string),
            passed,
            error_code: error_code.map(str::to_string),
            details,
        },
    );
}
