use super::repair_budget::{
    shell_rig_calls, BudgetExhaustion, RepairBudget, RigToolCall, MAX_RENDER_ATTEMPTS,
    MAX_VALIDATIONS_AFTER_RENDER, MAX_VALIDATIONS_BEFORE_RENDER,
};
use serde_json::json;

const VALIDATE: &str =
    "/bin/zsh -lc 'python3 .sprite-studio/sprite_rig.py --validate .sprite-studio/rigs/running_man.json'";
const RENDER: &str =
    "/bin/zsh -lc 'python3 .sprite-studio/sprite_rig.py .sprite-studio/rigs/running_man.json'";

fn codex_command(event: &str, id: &str, command: &str, output: &str) -> String {
    json!({
        "type": event,
        "item": {
            "id": id,
            "type": "command_execution",
            "command": command,
            "aggregated_output": output,
            "status": if event == "item.started" { "in_progress" } else { "completed" },
        }
    })
    .to_string()
}

fn codex_rig_edit(id: &str) -> String {
    json!({
        "type": "item.completed",
        "item": {
            "id": id,
            "type": "file_change",
            "changes": [{"path": ".sprite-studio/rigs/running_man.json", "kind": "update"}],
        }
    })
    .to_string()
}

fn claude_tool_use(id: &str, name: &str, input: serde_json::Value) -> String {
    json!({
        "type": "assistant",
        "message": {
            "id": "msg_1",
            "content": [{"type": "tool_use", "id": id, "name": name, "input": input}],
        }
    })
    .to_string()
}

fn claude_tool_result(id: &str, output: &str) -> String {
    json!({
        "type": "user",
        "message": {"content": [{"type": "tool_result", "tool_use_id": id, "content": output}]},
    })
    .to_string()
}

/// Replays the live failure: "make a running man" kept looping
/// edit → `sprite_rig.py --validate` (polygon ownership, scaleX, foot target,
/// mask polygon, root dx, role naming, ...). The runtime must stop it at the
/// fourth pre-render validation no matter what the prompt said.
#[test]
fn live_running_man_validate_loop_is_stopped_at_the_fourth_validation() {
    let failure = "sprite_rig: parts 'torso' and 'left_upper' overlap 14 visible pixels but are not a declared parent-child joint";
    let mut budget = RepairBudget::default();
    let mut stopped_at = None;
    for round in 1..=6 {
        let id = format!("item_{round}");
        let lines = [
            codex_rig_edit(&format!("edit_{round}")),
            codex_command("item.started", &id, VALIDATE, ""),
            codex_command("item.updated", &id, VALIDATE, ""),
            codex_command("item.completed", &id, VALIDATE, failure),
        ];
        for line in &lines {
            if let Some(exhaustion) = budget.observe_line(line) {
                stopped_at = Some((round, exhaustion));
                break;
            }
        }
        if stopped_at.is_some() {
            break;
        }
    }
    assert_eq!(
        stopped_at,
        Some((
            MAX_VALIDATIONS_BEFORE_RENDER as usize + 1,
            BudgetExhaustion::ValidationsBeforeRender
        )),
        "the fourth validation must trip the budget as soon as it starts"
    );
    assert_eq!(budget.exhausted(), Some(BudgetExhaustion::ValidationsBeforeRender));
}

#[test]
fn claude_code_validation_loop_is_bounded_and_tool_results_are_ignored() {
    let mut budget = RepairBudget::default();
    for round in 1..=MAX_VALIDATIONS_BEFORE_RENDER {
        let id = format!("toolu_{round}");
        let command = "python3 .sprite-studio/sprite_rig.py --check .sprite-studio/rigs/running_man.json";
        assert_eq!(
            budget.observe_line(&claude_tool_use(&id, "Bash", json!({"command": command}))),
            None
        );
        // Output that echoes the command must not count as another call.
        assert_eq!(
            budget.observe_line(&claude_tool_result(&id, &format!("$ {command}\nsprite_rig: loop seam pops"))),
            None
        );
        assert_eq!(
            budget.observe_line(&claude_tool_use(
                &format!("edit_{round}"),
                "Edit",
                json!({"file_path": ".sprite-studio/rigs/running_man.json", "old_string": "a", "new_string": "b"}),
            )),
            None
        );
    }
    assert_eq!(
        budget.observe_line(&claude_tool_use(
            "toolu_4",
            "mcp__sprite-rig__sprite_rig_validate",
            json!({"rig": "running_man.json"}),
        )),
        Some(BudgetExhaustion::ValidationsBeforeRender)
    );
}

#[test]
fn mcp_validation_and_analysis_tools_share_one_budget() {
    let mut budget = RepairBudget::default();
    let calls = [
        ("m1", "sprite_rig_validate"),
        ("m2", "sprite_rig_analyze_motion"),
        ("m3", "analyze_rig_fit"),
    ];
    for (id, tool) in calls {
        let line = json!({"type": "item.started", "item": {"id": id, "type": "mcp_tool_call", "server": "sprite-rig", "tool": tool, "arguments": {"rig": "running_man.json"}}}).to_string();
        assert_eq!(budget.observe_line(&line), None);
    }
    let gemini = json!({"type": "tool_use", "tool_name": "sprite_rig_validate", "tool_id": "g1", "parameters": {"rig": "running_man.json"}}).to_string();
    assert_eq!(
        budget.observe_line(&gemini),
        Some(BudgetExhaustion::ValidationsBeforeRender)
    );
}

#[test]
fn post_render_allows_exactly_one_repair_rerender() {
    let mut budget = RepairBudget::default();
    for round in 1..=MAX_VALIDATIONS_BEFORE_RENDER {
        assert_eq!(budget.observe_line(&codex_command("item.started", &format!("v{round}"), VALIDATE, "")), None);
    }
    assert_eq!(budget.observe_line(&codex_command("item.started", "r1", RENDER, "")), None);
    assert_eq!(budget.observe_line(&codex_command("item.started", "v4", VALIDATE, "")), None);
    assert_eq!(budget.observe_line(&codex_command("item.started", "r2", RENDER, "")), None);
    assert_eq!(
        budget.observe_line(&codex_command("item.started", "r3", RENDER, "")),
        Some(BudgetExhaustion::Rerenders)
    );
    assert_eq!(MAX_RENDER_ATTEMPTS, 2);
}

#[test]
fn post_render_validation_loop_without_rerender_is_bounded() {
    let mut budget = RepairBudget::default();
    assert_eq!(budget.record(RigToolCall::Render), None);
    for _ in 0..MAX_VALIDATIONS_AFTER_RENDER {
        assert_eq!(budget.record(RigToolCall::Validate), None);
    }
    assert_eq!(
        budget.record(RigToolCall::Validate),
        Some(BudgetExhaustion::ValidationsAfterRender)
    );
}

#[test]
fn a_validated_render_within_budget_never_trips() {
    let mut budget = RepairBudget::default();
    for call in [
        RigToolCall::Validate,
        RigToolCall::Validate,
        RigToolCall::Render,
        RigToolCall::Validate,
    ] {
        assert_eq!(budget.record(call), None);
    }
    assert_eq!(budget.exhausted(), None);
}

#[test]
fn each_request_starts_with_a_fresh_budget() {
    let mut first = RepairBudget::default();
    for _ in 0..=MAX_VALIDATIONS_BEFORE_RENDER {
        first.record(RigToolCall::Validate);
    }
    assert!(first.exhausted().is_some());
    // Codex restarts item ids in a new process; nothing from the earlier
    // request may pre-spend or dedupe the next request's calls.
    let mut second = RepairBudget::default();
    for round in 1..=MAX_VALIDATIONS_BEFORE_RENDER {
        assert_eq!(second.observe_line(&codex_command("item.started", &format!("item_{round}"), VALIDATE, "")), None);
    }
    assert_eq!(second.exhausted(), None);
}

#[test]
fn cursor_started_and_completed_events_count_once() {
    let mut budget = RepairBudget::default();
    for round in 1..=MAX_VALIDATIONS_BEFORE_RENDER + 1 {
        let call_id = format!("call_{round}");
        let started = json!({"type": "tool_call", "subtype": "started", "call_id": call_id, "tool_call": {"shellToolCall": {"args": {"command": "python3 .sprite-studio/sprite_rig.py --validate .sprite-studio/rigs/r.json"}}}}).to_string();
        let completed = json!({"type": "tool_call", "subtype": "completed", "call_id": call_id, "tool_call": {"shellToolCall": {"args": {"command": "python3 .sprite-studio/sprite_rig.py --validate .sprite-studio/rigs/r.json"}, "result": {"success": {"stdout": "sprite_rig.py --validate"}}}}}).to_string();
        let first = budget.observe_line(&started);
        let second = budget.observe_line(&completed);
        assert_eq!(second, None, "a completed event must not recount its call");
        if round <= MAX_VALIDATIONS_BEFORE_RENDER {
            assert_eq!(first, None);
        } else {
            assert_eq!(first, Some(BudgetExhaustion::ValidationsBeforeRender));
        }
    }
}

#[test]
fn shell_classification_ignores_reading_the_engine() {
    for command in [
        "cat .sprite-studio/sprite_rig.py",
        "sed -n 1,80p .sprite-studio/sprite_rig.py",
        "grep -n validate .sprite-studio/sprite_rig.py | head",
        "rg 'sprite_rig.py --validate' .sprite-studio",
        "python3 .sprite-studio/sprite_rig.py 2>&1",
    ] {
        assert!(shell_rig_calls(command).is_empty(), "{command} is not a rig run");
    }
    assert_eq!(
        shell_rig_calls(
            "cd ws && python3 .sprite-studio/sprite_rig.py --validate .sprite-studio/rigs/a.json && py -3 .sprite-studio/sprite_rig.py .sprite-studio/rigs/a.json"
        ),
        vec![RigToolCall::Validate, RigToolCall::Render]
    );
    assert_eq!(
        shell_rig_calls("bash -lc '.sprite-studio/sprite_rig.py --check .sprite-studio/rigs/a.json'"),
        vec![RigToolCall::Validate]
    );
}

#[test]
fn chained_validate_and_render_in_one_command_counts_both_in_order() {
    let mut budget = RepairBudget::default();
    for round in 1..=MAX_VALIDATIONS_BEFORE_RENDER {
        assert_eq!(budget.observe_line(&codex_command("item.started", &format!("v{round}"), VALIDATE, "")), None);
    }
    let chained = "python3 .sprite-studio/sprite_rig.py --validate .sprite-studio/rigs/r.json && python3 .sprite-studio/sprite_rig.py .sprite-studio/rigs/r.json";
    assert_eq!(
        budget.observe_line(&codex_command("item.started", "chain", chained, "")),
        Some(BudgetExhaustion::ValidationsBeforeRender),
        "the fourth validation trips before the chained render can run"
    );
}
