use super::budget_finalize::BudgetFinalization;
use super::stream::{
    emit, provider_display_name, provider_failure_message, response_reports_generation_failure,
    with_optional_auth_hint,
};
use crate::{
    conversations::{get_conversation, update_message},
    AppState,
};
use tauri::AppHandle;

pub(crate) struct FinishProviderRun<'a> {
    pub app: Option<&'a AppHandle>,
    pub state: &'a AppState,
    pub request_id: &'a str,
    pub conversation_id: &'a str,
    pub workspace_id: &'a str,
    pub assistant_id: &'a str,
    pub provider_id: &'a str,
    pub response: String,
    pub cancelled: bool,
    pub status: Result<std::process::ExitStatus, std::io::Error>,
    pub stderr_output: &'a str,
    /// Set when the runtime stopped the provider for a spent repair budget.
    pub budget_finalization: Option<BudgetFinalization>,
}

pub(crate) fn finish_provider_run(args: FinishProviderRun<'_>) {
    let FinishProviderRun {
        app,
        state,
        request_id,
        conversation_id,
        workspace_id,
        assistant_id,
        provider_id,
        mut response,
        cancelled,
        status,
        stderr_output,
        budget_finalization,
    } = args;
    if cancelled {
        let message = if response.is_empty() {
            "Request cancelled"
        } else {
            &response
        };
        let _ = update_message(state, assistant_id, message, "cancelled");
        emit(
            app,
            state,
            request_id,
            conversation_id,
            "cancelled",
            "Request cancelled",
        );
        return;
    }
    if let Some(finalization) = budget_finalization {
        // The provider was stopped on purpose, so its exit status and partial
        // narration are irrelevant; the runtime's own outcome is the result.
        let (message_status, event) = if finalization.published {
            ("completed", "completed")
        } else {
            ("failed", "failed")
        };
        let _ = update_message(state, assistant_id, &finalization.response, message_status);
        emit(app, state, request_id, conversation_id, event, finalization.response);
        return;
    }
    match status {
        Ok(exit) if exit.success() => {
            if response.trim().is_empty() {
                response = format!(
                    "{} completed without returning a text response.",
                    provider_display_name(provider_id)
                );
            }
            // A /rig turn (or any reply carrying the rig-suggestion contract)
            // is captured here so the Rig editor can pick it up immediately.
            if let Ok(conversation) = get_conversation(state, conversation_id) {
                if let Ok(Some(name)) = crate::rig::capture_chat_suggestion(
                    state,
                    workspace_id,
                    conversation.worktree_id.as_deref(),
                    &response,
                ) {
                    emit(
                        app,
                        state,
                        request_id,
                        conversation_id,
                        "activity",
                        format!("{name} captured — open the Rig tab to review and render it"),
                    );
                }
            }
            if response_reports_generation_failure(&response) {
                let _ = update_message(state, assistant_id, &response, "failed");
                emit(app, state, request_id, conversation_id, "failed", response);
            } else {
                let _ = update_message(state, assistant_id, &response, "completed");
                emit(
                    app,
                    state,
                    request_id,
                    conversation_id,
                    "completed",
                    response,
                );
            }
        }
        Ok(exit) => {
            let message = with_optional_auth_hint(
                provider_id,
                &provider_failure_message(
                    provider_id,
                    &exit.to_string(),
                    stderr_output,
                    &response,
                ),
            );
            let _ = update_message(state, assistant_id, &message, "failed");
            emit(app, state, request_id, conversation_id, "failed", message);
        }
        Err(error) => {
            let message = format!(
                "Could not observe the {} process: {error}",
                provider_display_name(provider_id)
            );
            let _ = update_message(state, assistant_id, &message, "failed");
            emit(app, state, request_id, conversation_id, "failed", message);
        }
    }
}
