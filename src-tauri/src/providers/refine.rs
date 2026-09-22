use super::detect::provider_capabilities;
use super::discovery::find_executable;
use super::modes::provider_is_authenticated;
use super::prompt::run_agent_text_request;
use super::stream::{provider_auth_help, provider_display_name};
use crate::{
    conversations::{append_conversation_log, get_conversation},
    error::{CommandError, CommandResult},
    models::RefineGenerationPromptInput,
    references,
    sprite_harness::{clean_refined_prompt_response, refine_generation_prompt},
    workspace::workspace_path,
    AppState,
};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::State;
use tokio::sync::oneshot;

static REFINE_SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) struct RefineCancelEntry {
    pub cancel_tx: oneshot::Sender<()>,
    pub session_id: u64,
}

fn register_refine_canceller(
    state: &AppState,
    conversation_id: &str,
) -> CommandResult<(oneshot::Receiver<()>, u64)> {
    let session_id = REFINE_SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let mut cancellers = state.refine_cancellers.lock().map_err(|_| {
        CommandError::new("process_error", "Refine process registry is unavailable")
    })?;
    if let Some(existing) = cancellers.remove(conversation_id) {
        let _ = existing.cancel_tx.send(());
    }
    cancellers.insert(
        conversation_id.to_string(),
        RefineCancelEntry {
            cancel_tx,
            session_id,
        },
    );
    Ok((cancel_rx, session_id))
}

fn clear_refine_canceller(state: &AppState, conversation_id: &str, session_id: u64) {
    if let Ok(mut cancellers) = state.refine_cancellers.lock() {
        if cancellers
            .get(conversation_id)
            .is_some_and(|entry| entry.session_id == session_id)
        {
            cancellers.remove(conversation_id);
        }
    }
}

pub(crate) fn cancel_refine_generation_prompt_inner(
    conversation_id: &str,
    state: &AppState,
) -> CommandResult<()> {
    let sender = state
        .refine_cancellers
        .lock()
        .map_err(|_| {
            CommandError::new("process_error", "Refine process registry is unavailable")
        })?
        .remove(conversation_id)
        .map(|entry| entry.cancel_tx);
    if let Some(sender) = sender {
        let _ = sender.send(());
    } else {
        return Err(CommandError::new(
            "refine_not_running",
            "No prompt refinement is running for this chat",
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn refine_generation_prompt_command(
    input: RefineGenerationPromptInput,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    let draft = input.draft.trim().to_string();
    if draft.is_empty() {
        return Err(CommandError::new(
            "empty_prompt",
            "Write a draft before refining it",
        ));
    }
    let conversation_id = input.conversation_id.clone();
    let (cancel_rx, session_id) = register_refine_canceller(&state, &conversation_id)?;
    let result = refine_generation_prompt_inner(&input, &state, &draft, cancel_rx).await;
    clear_refine_canceller(&state, &conversation_id, session_id);
    result
}

async fn refine_generation_prompt_inner(
    input: &RefineGenerationPromptInput,
    state: &AppState,
    draft: &str,
    cancel_rx: oneshot::Receiver<()>,
) -> CommandResult<String> {
    let conversation = get_conversation(state, &input.conversation_id)?;
    let provider_id = conversation.provider.clone();
    if !matches!(
        provider_id.as_str(),
        "codex" | "claude" | "gemini" | "grok" | "cursor" | "antigravity"
    ) {
        return Err(CommandError::new(
            "provider_unsupported",
            "This conversation does not use a supported CLI provider",
        ));
    }
    let executable = find_executable(&provider_id).ok_or_else(|| {
        CommandError::new(
            "provider_unavailable",
            format!(
                "{} was not found. Install its CLI, authenticate it, then retry detection.",
                provider_display_name(&provider_id)
            ),
        )
    })?;
    if !provider_is_authenticated(&provider_id, &executable) {
        return Err(CommandError::new(
            "provider_unauthenticated",
            provider_auth_help(&provider_id),
        ));
    }
    let workspace = workspace_path(state, &conversation.workspace_id)?;
    let capabilities = provider_capabilities(&provider_id);
    let (reference_context, reference_paths) = references::prompt_context(
        state,
        &input.conversation_id,
        &input.reference_ids,
        capabilities.maximum_reference_images as usize,
    )?;
    let combined_context = [input.context.as_deref().unwrap_or(""), reference_context.as_str()]
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let meta_prompt = refine_generation_prompt(
        draft,
        (!combined_context.is_empty()).then_some(combined_context.as_str()),
        input.command.as_deref(),
        input.generation.as_ref(),
        input.animation_mode.as_deref(),
    );
    let _ = append_conversation_log(
        state,
        &input.conversation_id,
        None,
        "info",
        "refine",
        "refine_started",
        draft,
        serde_json::json!({
            "provider": provider_id,
            "model": input.model,
            "reasoningEffort": input.reasoning_effort,
            "command": input.command,
            "referenceCount": input.reference_ids.len(),
        }),
    );
    let response = run_agent_text_request(
        &provider_id,
        input.model.as_deref(),
        input.reasoning_effort.as_deref(),
        &workspace,
        &meta_prompt,
        &reference_paths,
        Some(cancel_rx),
    )
    .await;
    let response = match response {
        Ok(value) => value,
        Err(error) if error.code == "request_cancelled" => {
            let _ = append_conversation_log(
                state,
                &input.conversation_id,
                None,
                "warning",
                "refine",
                "refine_cancelled",
                "Prompt refinement cancelled",
                serde_json::json!({ "provider": provider_id }),
            );
            return Err(error);
        }
        Err(error) => {
            let _ = append_conversation_log(
                state,
                &input.conversation_id,
                None,
                "error",
                "refine",
                "refine_failed",
                &error.message,
                serde_json::json!({ "provider": provider_id, "code": error.code }),
            );
            return Err(error);
        }
    };
    let refined = clean_refined_prompt_response(&response);
    if refined.is_empty() {
        let error = CommandError::new(
            "provider_empty_response",
            "The provider returned an empty refined prompt. Try again.",
        );
        let _ = append_conversation_log(
            state,
            &input.conversation_id,
            None,
            "error",
            "refine",
            "refine_failed",
            &error.message,
            serde_json::json!({ "provider": provider_id, "code": error.code }),
        );
        return Err(error);
    }
    let _ = append_conversation_log(
        state,
        &input.conversation_id,
        None,
        "info",
        "refine",
        "refine_completed",
        &refined,
        serde_json::json!({ "provider": provider_id }),
    );
    Ok(refined)
}

#[tauri::command]
pub fn cancel_refine_generation_prompt_command(
    conversation_id: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    cancel_refine_generation_prompt_inner(&conversation_id, &state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_stale_refine_canceller_cancels_previous_session() {
        let (state, _) = AppState::open_headless().expect("open headless db");
        let (mut first_rx, first_session) =
            register_refine_canceller(&state, "conversation-a").expect("first session");
        let (_second_rx, second_session) =
            register_refine_canceller(&state, "conversation-a").expect("second session");
        assert_ne!(first_session, second_session);
        assert!(first_rx.try_recv().is_ok(), "stale session should be cancelled");
        clear_refine_canceller(&state, "conversation-a", first_session);
        cancel_refine_generation_prompt_inner("conversation-a", &state).expect("second session still active");
        clear_refine_canceller(&state, "conversation-a", second_session);
        assert!(
            cancel_refine_generation_prompt_inner("conversation-a", &state).is_err(),
            "all refine sessions should be cleared"
        );
    }
}
