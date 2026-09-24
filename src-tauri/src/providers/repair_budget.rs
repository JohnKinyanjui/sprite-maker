//! Runtime-enforced rig repair budget for one provider request.
//!
//! The prompt asks the agent to stop after a few validate→fix rounds, but a
//! provider can ignore prose. `run_provider` feeds every raw stream line to a
//! fresh [`RepairBudget`]; it counts rig validation and render tool calls as
//! the CLI announces them and reports exhaustion the moment the agent starts
//! one call too many. The runner then stops the provider process and
//! `budget_finalize` publishes the best structurally valid candidate itself.

use serde_json::Value;
use std::collections::HashSet;

/// Validate→fix rounds allowed before the first render attempt.
pub(crate) const MAX_VALIDATIONS_BEFORE_RENDER: u32 = 3;
/// Validator runs allowed after the first render (inspection, one repair check, final check).
pub(crate) const MAX_VALIDATIONS_AFTER_RENDER: u32 = 3;
/// The first render plus exactly one post-render repair rerender.
pub(crate) const MAX_RENDER_ATTEMPTS: u32 = 2;

const VALIDATE_TOOLS: &[&str] = &[
    "sprite_rig_validate",
    "sprite_rig_analyze_motion",
    "sprite_rig_analyze_walk",
    "analyze_rig_fit",
];
const RENDER_TOOLS: &[&str] = &["sprite_rig_render", "render_rig_animation"];
const NAME_KEYS: &[&str] = &["name", "tool", "tool_name", "toolName"];
const COMMAND_KEYS: &[&str] = &["command", "cmd"];
const ID_KEYS: &[&str] = &[
    "id",
    "call_id",
    "callId",
    "tool_id",
    "toolId",
    "tool_call_id",
    "toolCallId",
];
/// Tool output never counts as a new call, even when it echoes a command.
const OUTPUT_KEYS: &[&str] = &[
    "aggregated_output",
    "output",
    "result",
    "structured_content",
    "stdout",
    "stderr",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RigToolCall {
    Validate,
    Render,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BudgetExhaustion {
    /// A fourth validation started before any render.
    ValidationsBeforeRender,
    /// Validation kept looping after the first render without a rerender.
    ValidationsAfterRender,
    /// A second repair rerender (third render attempt) started.
    Rerenders,
}

impl BudgetExhaustion {
    pub(crate) fn describe(self) -> String {
        match self {
            Self::ValidationsBeforeRender => format!(
                "the rig repair budget ({MAX_VALIDATIONS_BEFORE_RENDER} validate→fix rounds before the first render) was spent"
            ),
            Self::ValidationsAfterRender => format!(
                "the post-render repair budget ({MAX_VALIDATIONS_AFTER_RENDER} validator runs after the first render) was spent"
            ),
            Self::Rerenders => {
                "the post-render repair budget (one repair rerender) was spent".to_string()
            }
        }
    }
}

/// Per-request counters. Create one per provider run; nothing is shared
/// between requests, so a spent budget cannot leak into the next turn.
#[derive(Debug, Default)]
pub(crate) struct RepairBudget {
    validations_before_render: u32,
    validations_after_render: u32,
    render_attempts: u32,
    seen: HashSet<String>,
    exhausted: Option<BudgetExhaustion>,
}

impl RepairBudget {
    /// Count the rig tool calls announced by one raw provider stream line.
    /// Returns the exhaustion reason the first time the budget is exceeded.
    pub(crate) fn observe_line(&mut self, line: &str) -> Option<BudgetExhaustion> {
        if !(line.contains("sprite_rig") || line.contains("rig_animation") || line.contains("analyze_rig_fit")) {
            return None;
        }
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            return None;
        };
        let mut found = Vec::new();
        collect_calls(&value, None, "", &mut found);
        for (key, call) in found {
            if key.is_some_and(|key| !self.seen.insert(key)) {
                continue;
            }
            if let Some(exhaustion) = self.record(call) {
                return Some(exhaustion);
            }
        }
        None
    }

    pub(crate) fn record(&mut self, call: RigToolCall) -> Option<BudgetExhaustion> {
        if self.exhausted.is_some() {
            return None;
        }
        let exhausted = match call {
            RigToolCall::Validate if self.render_attempts == 0 => {
                self.validations_before_render += 1;
                (self.validations_before_render > MAX_VALIDATIONS_BEFORE_RENDER)
                    .then_some(BudgetExhaustion::ValidationsBeforeRender)
            }
            RigToolCall::Validate => {
                self.validations_after_render += 1;
                (self.validations_after_render > MAX_VALIDATIONS_AFTER_RENDER)
                    .then_some(BudgetExhaustion::ValidationsAfterRender)
            }
            RigToolCall::Render => {
                self.render_attempts += 1;
                (self.render_attempts > MAX_RENDER_ATTEMPTS).then_some(BudgetExhaustion::Rerenders)
            }
        };
        self.exhausted = exhausted;
        exhausted
    }

    pub(crate) fn exhausted(&self) -> Option<BudgetExhaustion> {
        self.exhausted
    }
}

/// Walk one provider event and collect rig tool invocations with a stable
/// dedupe key (call id + JSON path) so started/updated/completed events for
/// the same call count once. Calls without any id are counted every time.
fn collect_calls(
    value: &Value,
    inherited_id: Option<&str>,
    path: &str,
    found: &mut Vec<(Option<String>, RigToolCall)>,
) {
    match value {
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_calls(item, inherited_id, &format!("{path}/{index}"), found);
            }
        }
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("tool_result")
                || map.contains_key("tool_use_id")
            {
                return;
            }
            let id = ID_KEYS
                .iter()
                .find_map(|key| map.get(*key).and_then(Value::as_str))
                .or(inherited_id);
            let mut calls = Vec::new();
            for key in NAME_KEYS {
                if let Some(name) = map.get(*key).and_then(Value::as_str) {
                    calls.extend(tool_name_call(name));
                }
            }
            for key in COMMAND_KEYS {
                match map.get(*key) {
                    Some(Value::String(command)) => calls.extend(shell_rig_calls(command)),
                    Some(Value::Array(parts)) => {
                        let joined = parts
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(" ");
                        calls.extend(shell_rig_calls(&joined));
                    }
                    _ => {}
                }
            }
            for (index, call) in calls.into_iter().enumerate() {
                found.push((id.map(|id| format!("{id}|{path}|{index}")), call));
            }
            for (key, child) in map {
                if !OUTPUT_KEYS.contains(&key.as_str()) {
                    collect_calls(child, id, &format!("{path}/{key}"), found);
                }
            }
        }
        _ => {}
    }
}

/// MCP tool names arrive bare (`sprite_rig_validate`) or namespaced
/// (`mcp__sprite-rig__sprite_rig_validate`, `sprite-rig.sprite_rig_render`).
fn tool_name_call(name: &str) -> Option<RigToolCall> {
    let matches = |tool: &&str| {
        name == *tool
            || [
                format!("__{tool}"),
                format!(".{tool}"),
                format!("/{tool}"),
                format!(":{tool}"),
            ]
            .iter()
            .any(|suffix| name.ends_with(suffix.as_str()))
    };
    if VALIDATE_TOOLS.iter().any(matches) {
        Some(RigToolCall::Validate)
    } else if RENDER_TOOLS.iter().any(matches) {
        Some(RigToolCall::Render)
    } else {
        None
    }
}

/// Rig engine runs inside one shell command, in order. Reading or grepping
/// the script is not a run; only a python launch (or a direct/shebang launch
/// at the start of a command segment) with a rig argument counts.
pub(crate) fn shell_rig_calls(command: &str) -> Vec<RigToolCall> {
    let tokens = command.split_whitespace().collect::<Vec<_>>();
    let mut calls = Vec::new();
    for (index, raw) in tokens.iter().enumerate() {
        let token = trim_shell_punctuation(raw);
        if !(token == "sprite_rig.py"
            || token.ends_with("/sprite_rig.py")
            || token.ends_with("\\sprite_rig.py"))
        {
            continue;
        }
        let launched = index == 0 || {
            let previous = tokens[index - 1];
            is_python_launcher(trim_shell_punctuation(previous))
                || starts_command_segment(previous)
        };
        if !launched {
            continue;
        }
        let Some(next) = tokens.get(index + 1).map(|value| trim_shell_punctuation(value)) else {
            continue;
        };
        if next.starts_with("--validate") || next.starts_with("--check") {
            calls.push(RigToolCall::Validate);
        } else if next.starts_with("--finalize-degraded") || is_rig_argument(next) {
            calls.push(RigToolCall::Render);
        }
    }
    calls
}

fn trim_shell_punctuation(token: &str) -> &str {
    token.trim_matches(|character| matches!(character, '\'' | '"' | '(' | ')' | '`' | ';'))
}

fn is_python_launcher(token: &str) -> bool {
    let name = token.rsplit(['/', '\\']).next().unwrap_or(token).to_ascii_lowercase();
    name.starts_with("python") || matches!(name.as_str(), "py" | "py.exe" | "-3" | "-u" | "-b")
}

fn starts_command_segment(previous: &str) -> bool {
    matches!(previous, "&&" | "||" | ";" | "|" | "then" | "do" | "-c" | "-lc" | "exec")
        || previous.ends_with(';')
        || previous.ends_with("&&")
}

fn is_rig_argument(token: &str) -> bool {
    !token.is_empty()
        && !token.contains(['>', '<', '|', '&'])
        && token
            .chars()
            .next()
            .is_some_and(|first| first.is_alphanumeric() || matches!(first, '.' | '/' | '$' | '~' | '_'))
}
