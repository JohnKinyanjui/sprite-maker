use super::arguments::validate_provider_options;
use super::detect::provider_capabilities;
use super::discovery::find_executable;
use super::execute::{run_provider, ProviderRun};
use super::image_providers::{is_provider_native_image, load_image_provider};
use super::modes::provider_is_authenticated;
use super::stream::{provider_auth_help, provider_display_name};
use crate::{
    conversations::{add_message, append_conversation_log, get_conversation},
    error::{CommandError, CommandResult},
    models::ProviderRequestOptions,
    references,
    pipeline::anchor_contract_text,
    sprite_harness::studio_prompt,
    workspace::workspace_path,
    AppState,
};
use tauri::{AppHandle, State};
use tokio::sync::oneshot;
use uuid::Uuid;

#[tauri::command]
pub fn start_provider_message(
    conversation_id: String,
    prompt: String,
    context: Option<String>,
    options: Option<ProviderRequestOptions>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    start_provider_run(conversation_id, prompt, context, options, Some(app), &state)
}

pub(crate) fn start_provider_run(
    conversation_id: String,
    prompt: String,
    context: Option<String>,
    options: Option<ProviderRequestOptions>,
    app: Option<AppHandle>,
    state: &AppState,
) -> CommandResult<String> {
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Err(CommandError::new(
            "empty_prompt",
            "Write a message before sending",
        ));
    }
    let conversation = get_conversation(state, &conversation_id)?;
    if !matches!(
        conversation.provider.as_str(),
        "codex" | "claude" | "gemini" | "grok" | "cursor" | "antigravity"
    ) {
        return Err(CommandError::new(
            "provider_unsupported",
            "This conversation does not use a supported CLI provider",
        ));
    }
    let provider_id = conversation.provider.clone();
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
    let options = options.unwrap_or_default();
    validate_provider_options(&options)?;
    let capabilities = provider_capabilities(&provider_id);
    if !options.reference_ids.is_empty() && !capabilities.image_input {
        return Err(CommandError::new(
            "provider_image_input_unsupported",
            "The selected provider cannot accept reference images",
        ));
    }
    let (reference_context, reference_paths) = references::prompt_context(
        state,
        &conversation_id,
        &options.reference_ids,
        capabilities.maximum_reference_images as usize,
    )?;
    let combined_context = [context.as_deref().unwrap_or(""), reference_context.as_str()]
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let image_provider_id = options.image_provider_id.as_deref().unwrap_or("imagegen");
    // Chats created before the provider-native option inherited Codex's
    // `imagegen` setting. Keep those chats usable: non-Codex CLIs can work
    // directly, while Codex continues to use its existing ImageGen path.
    // Cursor + cursor-image and Antigravity + antigravity-image follow the
    // same agent-native pattern.
    let provider_native_image = is_provider_native_image(image_provider_id, &provider_id);
    if image_provider_id == "midjourney" {
        return Err(CommandError::new(
            "provider_unsupported",
            "Midjourney does not provide a public API for this integration",
        ));
    }
    let image_provider = if provider_native_image {
        None
    } else {
        load_image_provider(state, image_provider_id)?
    };
    if !provider_native_image
        && image_provider_id != "imagegen"
        && image_provider_id != "cursor-image"
        && image_provider_id != "antigravity-image"
        && image_provider.is_none()
    {
        return Err(CommandError::new(
            "provider_unavailable",
            "Configure the selected image provider in Settings before generating",
        ));
    }
    workspace_path(state, &conversation.workspace_id)?;
    add_message(
        state,
        &conversation_id,
        "user",
        "text",
        &prompt,
        "completed",
    )?;
    let assistant = add_message(state, &conversation_id, "assistant", "text", "", "running")?;
    let request_id = Uuid::new_v4().to_string();
    append_conversation_log(
        state,
        &conversation_id,
        Some(&request_id),
        "info",
        "chat",
        "user_message",
        &prompt,
        serde_json::json!({
            "provider": provider_id,
            "model": options.model,
            "reasoningEffort": options.reasoning_effort,
            "command": options.command,
            "referenceCount": options.reference_ids.len(),
            "assistantMessageId": assistant.id,
            "imageProviderId": options.image_provider_id,
        }),
    )?;
    let (cancel_tx, cancel_rx) = oneshot::channel();
    state
        .cancellers
        .lock()
        .map_err(|_| {
            CommandError::new("process_error", "Provider process registry is unavailable")
        })?
        .insert(request_id.clone(), cancel_tx);

    state.track_generation(
        &request_id,
        &conversation_id,
        &assistant.id,
        &conversation.workspace_id,
    );
    let task_app = app;
    let task_state = state.clone();
    let task_request_id = request_id.clone();
    let image_prompt = format!("{prompt}\n\n{combined_context}\n\nCreate one clean, centered, motion-ready game-art source master. Use a plain removable background, clear silhouette, and no text, labels, contact sheet, or multiple poses.");
    let anchor_contract = anchor_contract_text(&conversation.workspace_id, state);
    let anchor_section = if anchor_contract.is_empty() {
        None
    } else {
        Some(anchor_contract)
    };
    let run = ProviderRun {
        request_id: task_request_id,
        conversation_id,
        workspace_id: conversation.workspace_id,
        session_id: conversation.provider_session_id,
        assistant_id: assistant.id,
        prompt: studio_prompt(
            &prompt,
            (!combined_context.is_empty()).then_some(combined_context.as_str()),
            options.generation.as_ref(),
            options.command.as_deref(),
            Some(provider_id.as_str()),
            anchor_section.as_deref(),
            options.native_rig_master_only,
        ),
        model: options.model,
        reasoning_effort: options.reasoning_effort,
        reference_paths,
        executable,
        image_provider,
        image_prompt,
        provider_id,
    };
    tauri::async_runtime::spawn(async move {
        run_provider(task_app, task_state, run, cancel_rx).await;
    });
    Ok(request_id)
}

#[tauri::command]
pub fn cancel_provider_request(
    request_id: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    cancel_provider_request_inner(&request_id, &state)
}

pub(crate) fn cancel_provider_request_inner(
    request_id: &str,
    state: &AppState,
) -> CommandResult<()> {
    let sender = state
        .cancellers
        .lock()
        .map_err(|_| {
            CommandError::new("process_error", "Provider process registry is unavailable")
        })?
        .remove(request_id);
    if let Some(sender) = sender {
        let _ = sender.send(());
    } else {
        return Err(CommandError::new(
            "request_not_found",
            "The provider request is no longer running",
        ));
    }
    Ok(())
}
