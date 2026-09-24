use super::{
    append_stream_text, codex_arguments, parse_codex_line, parse_stream_line, provider_arguments,
    provider_failure_message, provider_stdin_bytes, response_reports_generation_failure,
    validate_provider_options, with_optional_auth_hint,
};
use crate::models::{GenerationOptions, ProviderRequestOptions};
use std::path::Path;

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
fn preserves_recoverable_codex_item_error_detail_without_false_failure_label() {
    let line = r#"{"type":"item.completed","item":{"type":"error","message":"Skill descriptions were shortened to fit the context budget."}}"#;
    let (content, activity, _) = parse_codex_line(line);

    assert!(content.is_none());
    assert_eq!(
        activity.as_deref(),
        Some("Codex notice — Skill descriptions were shortened to fit the context budget.")
    );
}

#[test]
fn auth_keyword_appends_help_without_dropping_original() {
    let message = with_optional_auth_hint(
        "claude",
        "Sprite Studio's tool runner was unavailable during login probe",
    );
    assert!(message.contains("tool runner was unavailable"));
    assert!(message.contains("If this looks like a sign-in problem"));
    assert!(message.contains("claude auth login"));
}

#[test]
fn non_auth_failure_is_unchanged() {
    let message = with_optional_auth_hint("codex", "resume payload is invalid");
    assert_eq!(message, "resume payload is invalid");
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
fn withheld_result_without_marker_is_a_failure() {
    // Reproduced: the agent could not open a browser preview and withheld the
    // run cycle without `GENERATION_FAILED:`; the turn must not read as done.
    assert!(response_reports_generation_failure(
        "Generated the running man master, draft rig, and 8 frames, but the three-cycle playback approval was denied, so I marked the generated result unpublished."
    ));
    assert!(response_reports_generation_failure(
        "The new frames were not published."
    ));
    assert!(!response_reports_generation_failure(
        "Published running_man_run with 8 frames. GENERATION_WARNING: a one-pixel knee seam remains."
    ));
}

#[test]
fn parses_claude_partial_text_and_session() {
    let line =
        r#"{"type":"stream_event","session_id":"session-1","event":{"delta":{"text":"hello"}}}"#;
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

    let mut antigravity = String::new();
    append_stream_text(&mut antigravity, "antigravity", "Start");
    append_stream_text(&mut antigravity, "antigravity", " by reading");
    append_stream_text(&mut antigravity, "antigravity", " the project.");
    assert_eq!(antigravity, "Start by reading the project.");

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
    let codex = provider_arguments("codex", None, None, None, &[], None);
    let expected_codex_sandbox = if cfg!(windows) {
        "danger-full-access"
    } else {
        "workspace-write"
    };
    assert!(codex
        .windows(2)
        .any(|pair| pair == ["--sandbox", expected_codex_sandbox]));

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

    let cursor = provider_arguments(
        "cursor",
        Some("chat-123"),
        Some("composer-2.5"),
        None,
        &["/tmp/master.png".into()],
        None,
    );
    assert!(cursor
        .windows(2)
        .any(|pair| pair == ["--output-format", "stream-json"]));
    assert!(cursor.contains(&"-p".into()));
    assert!(cursor.contains(&"--stream-partial-output".into()));
    assert!(cursor.contains(&"--force".into()));
    assert!(cursor.contains(&"--trust".into()));
    assert!(cursor
        .windows(2)
        .any(|pair| pair == ["--resume", "chat-123"]));
    assert!(cursor
        .windows(2)
        .any(|pair| pair == ["--model", "composer-2.5"]));
    assert!(cursor
        .windows(2)
        .any(|pair| pair == ["--image", "/tmp/master.png"]));

    let antigravity = provider_arguments(
        "antigravity",
        Some("conv-9"),
        Some("gemini-3.1-pro-high"),
        Some("high"),
        &["/tmp/master.png".into()],
        None,
    );
    assert!(antigravity
        .windows(2)
        .any(|pair| pair == ["--output-format", "stream-json"]));
    assert!(antigravity
        .windows(2)
        .any(|pair| pair == ["--input-format", "stream-json"]));
    assert!(antigravity.contains(&"--dangerously-skip-permissions".into()));
    assert!(antigravity
        .windows(2)
        .any(|pair| pair == ["--print-timeout", "45m"]));
    assert!(antigravity
        .windows(2)
        .any(|pair| pair == ["--conversation", "conv-9"]));
    assert!(antigravity
        .windows(2)
        .any(|pair| pair == ["--model", "gemini-3.1-pro-high"]));
    assert!(antigravity
        .windows(2)
        .any(|pair| pair == ["--effort", "high"]));
    assert!(!antigravity.contains(&"--image".into()));
    assert!(!antigravity.contains(&"-p".into()));
    let stdin = String::from_utf8(provider_stdin_bytes("antigravity", "draw a fox")).unwrap();
    assert!(stdin.contains(r#""event":"user""#));
    assert!(stdin.contains("draw a fox"));
}

#[test]
fn parses_cursor_stream_json_deltas_and_skips_flushes() {
    let delta = r#"{"type":"assistant","timestamp_ms":1,"message":{"content":[{"type":"text","text":"Hello"}]},"session_id":"chat-9"}"#;
    let (content, _, session) = parse_stream_line("cursor", delta, false);
    assert_eq!(content.as_deref(), Some("Hello"));
    assert_eq!(session.as_deref(), Some("chat-9"));

    let flush = r#"{"type":"assistant","model_call_id":"call-1","timestamp_ms":2,"message":{"content":[{"type":"text","text":"Hello"}]}}"#;
    let (content, _, _) = parse_stream_line("cursor", flush, true);
    assert!(content.is_none());

    let tool = r#"{"type":"tool_call","subtype":"started","tool_call":{"writeToolCall":{"args":{"path":"assets/hero.png"}}}}"#;
    let (_, activity, _) = parse_stream_line("cursor", tool, false);
    assert_eq!(activity.as_deref(), Some("Writing file: assets/hero.png"));

    let glob = r#"{"type":"tool_call","subtype":"started","tool_call":{"globToolCall":{"args":{"pattern":"**/*.png"}}}}"#;
    let (_, activity, _) = parse_stream_line("cursor", glob, false);
    assert_eq!(
        activity.as_deref(),
        Some("Searching files: pattern `**/*.png`")
    );

    let noise = r#"{"type":"tool_call","subtype":"completed","tool_call":{"globToolCall":{},"completedAtMs":123}}"#;
    let (_, activity, _) = parse_stream_line("cursor", noise, false);
    assert!(activity.is_none());

    let init = r#"{"type":"system","subtype":"init","session_id":"chat-9"}"#;
    let (_, activity, session) = parse_stream_line("cursor", init, false);
    assert_eq!(activity.as_deref(), Some("Cursor CLI session started"));
    assert_eq!(session.as_deref(), Some("chat-9"));
}

#[test]
fn parses_antigravity_stream_json_deltas_tools_and_result() {
    let init = r#"{"event":"init","conversation_id":"conv-1","init":{"cwd":"/tmp","tools":["generate_image"],"permission_mode":"always-proceed"}}"#;
    let (_, activity, session) = parse_stream_line("antigravity", init, false);
    assert_eq!(activity.as_deref(), Some("Antigravity CLI session started"));
    assert_eq!(session.as_deref(), Some("conv-1"));

    let delta = r#"{"event":"step_update","step_update":{"conversation_id":"conv-1","step_index":2,"state":"ACTIVE","step_type":"agent_response","text_delta":"Hello"}}"#;
    let (content, _, session) = parse_stream_line("antigravity", delta, false);
    assert_eq!(content.as_deref(), Some("Hello"));
    assert_eq!(session.as_deref(), Some("conv-1"));

    let done = r#"{"event":"step_update","step_update":{"conversation_id":"conv-1","step_index":2,"state":"DONE","step_type":"agent_response","text_delta":"Hello\n"}}"#;
    let (content, _, _) = parse_stream_line("antigravity", done, true);
    assert!(content.is_none());

    let tool = r#"{"event":"step_update","step_update":{"conversation_id":"conv-1","step_index":4,"state":"ACTIVE","step_type":"tool","tool_name":"generate_image","tool_info":{"name":"generate_image","parameters":{"prompt":"a pixel fox"}}}}"#;
    let (_, activity, _) = parse_stream_line("antigravity", tool, false);
    assert_eq!(activity.as_deref(), Some("Generating image: a pixel fox"));

    let write = r#"{"event":"step_update","step_update":{"step_index":5,"state":"ACTIVE","step_type":"tool","tool_name":"write_to_file","tool_info":{"name":"write_to_file","parameters":{"path":".sprite-studio/imagegen-sources/fox/master.png"}}}}"#;
    let (_, activity, _) = parse_stream_line("antigravity", write, false);
    assert_eq!(activity.as_deref(), Some("Writing file: fox/master.png"));

    let command = r#"{"event":"step_update","step_update":{"state":"ACTIVE","step_type":"tool","tool_name":"run_command","tool_info":{"name":"run_command","parameters":{"CommandLine":"echo hello"}}}}"#;
    let (_, activity, _) = parse_stream_line("antigravity", command, false);
    assert_eq!(activity.as_deref(), Some("Running command: echo hello"));

    let result = r#"{"event":"result","result":{"conversation_id":"conv-1","status":"SUCCESS","response":"Done\n"}}"#;
    let (content, _, _) = parse_stream_line("antigravity", result, true);
    assert!(content.is_none());

    let result_only = r#"{"event":"result","result":{"conversation_id":"conv-1","status":"SUCCESS","response":"Only result text"}}"#;
    let (content, _, session) = parse_stream_line("antigravity", result_only, false);
    assert_eq!(content.as_deref(), Some("Only result text"));
    assert_eq!(session.as_deref(), Some("conv-1"));

    let failed = r#"{"event":"result","result":{"conversation_id":"conv-1","status":"ERROR","error":"authentication required"}}"#;
    let (content, activity, _) = parse_stream_line("antigravity", failed, false);
    assert_eq!(
        content.as_deref(),
        Some("GENERATION_FAILED: authentication required"),
    );
    assert!(activity.is_none());
    assert!(response_reports_generation_failure(
        "GENERATION_FAILED: authentication required",
    ));
}

#[test]
fn resumes_a_persisted_codex_session() {
    let sandbox_mode = if cfg!(windows) {
        "danger-full-access"
    } else {
        "workspace-write"
    };
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
            sandbox_mode,
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
        native_rig_master_only: false,
    };
    assert!(validate_provider_options(&options).is_ok());
    let mut pack_options = options;
    pack_options.command = Some("pack".into());
    assert!(validate_provider_options(&pack_options).is_ok());
}
