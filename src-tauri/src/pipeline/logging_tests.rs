use super::logging::{append_pipeline_log, PipelineLogEntry, PipelineStageTimer};
use std::thread;
use std::time::Duration;
use uuid::Uuid;

#[test]
fn pipeline_log_appends_jsonl_lines() {
    let root = std::env::temp_dir().join(format!("sprite-pipeline-log-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("root");
    append_pipeline_log(
        &root,
        PipelineLogEntry {
            timestamp: "2026-01-01T00:00:00Z".into(),
            workspace_id: "ws".into(),
            event: "harden_animation".into(),
            stage: Some("completed".into()),
            duration_ms: Some(12),
            animation_id: Some("anim".into()),
            job_id: None,
            passed: Some(true),
            error_code: None,
            details: None,
        },
    )
    .expect("append");
    let content =
        std::fs::read_to_string(root.join(".sprite-studio/pipeline-log.jsonl")).expect("log");
    assert!(content.contains("\"event\":\"harden_animation\""));
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn pipeline_stage_timer_records_durations() {
    let mut timer = PipelineStageTimer::start();
    thread::sleep(Duration::from_millis(2));
    timer.mark("load_context");
    thread::sleep(Duration::from_millis(2));
    timer.mark("clean_alpha");
    let durations = timer.stage_durations_ms();
    assert_eq!(durations.len(), 2);
    assert!(durations.contains_key("load_context"));
    assert!(durations.contains_key("clean_alpha"));
}
