use super::discovery::find_executable;
use super::image_providers::{image_provider_status, stored_image_providers, StoredImageProvider};
use super::modes::{
    antigravity_modes_from_output, check_provider_auth, codex_modes, cursor_modes_from_output,
    grok_modes_from_output,
};
use super::probes::{command_output, AuthCheck, ProbeOutcome};
use crate::error::{CommandError, CommandResult};
use crate::models::{ProviderCapabilities, ProviderStatus};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn detect_providers(state: State<'_, AppState>) -> CommandResult<Vec<ProviderStatus>> {
    let state = state.inner().clone();
    let providers = tauri::async_runtime::spawn_blocking(move || detect_providers_inner(&state))
        .await
        .map_err(|error| {
            CommandError::new(
                "process_error",
                format!("Provider detection interrupted: {error}"),
            )
        })?;
    Ok(providers)
}

pub(crate) fn detect_providers_inner(state: &AppState) -> Vec<ProviderStatus> {
    let mut providers: Vec<ProviderStatus> = [
        ("codex", "Codex CLI"),
        ("claude", "Claude Code"),
        ("gemini", "Gemini CLI"),
        ("grok", "Grok CLI"),
        ("cursor", "Cursor CLI"),
        ("antigravity", "Antigravity CLI"),
    ]
    .into_iter()
    .map(|(id, name)| {
        let executable = find_executable(id);
        let installed = executable.is_some();
        // Grok's authenticated `models` probe used to run twice here and can
        // take over a minute when its session or network is unhealthy. Probe it
        // once, with the same hard timeout as every other status command.
        // Antigravity uses the same `models` success check for auth.
        let auth_check = executable
            .as_deref()
            .map(|path| check_provider_auth(id, path))
            .unwrap_or(AuthCheck::NotLoggedIn);
        let authenticated = auth_check == AuthCheck::Authenticated;
        let models_probe = if matches!(id, "grok" | "antigravity") {
            executable
                .as_deref()
                .and_then(|path| match command_output(id, path, &["models"]) {
                    ProbeOutcome::Ran(output) => Some(output),
                    ProbeOutcome::Failed(_) => None,
                })
        } else {
            None
        };
        let modes = match (id, executable.as_deref(), authenticated) {
            ("codex", Some(path), true) => codex_modes(path),
            ("grok", Some(_), true) => models_probe
                .as_ref()
                .map(grok_modes_from_output)
                .unwrap_or_default(),
            ("antigravity", Some(_), true) => models_probe
                .as_ref()
                .map(antigravity_modes_from_output)
                .unwrap_or_default(),
            ("cursor", Some(path), true) => match command_output(id, path, &["models"]) {
                ProbeOutcome::Ran(output) => cursor_modes_from_output(&output),
                ProbeOutcome::Failed(_) => Vec::new(),
            },
            _ => Vec::new(),
        };
        let (status, detail) = match (id, installed, &auth_check) {
            (_, false, _) => (
                "not_installed",
                match id {
                    "codex" => "Install the Codex CLI, then run `codex login`",
                    "claude" => "Install Claude Code, then run `claude auth login`",
                    "gemini" => "Install @google/gemini-cli, run `gemini`, and complete authentication",
                    "grok" => "Install Grok Build, then run `grok login`",
                    "cursor" => {
                        "Press Install Cursor CLI below. After installation completes, run Detect again and sign in with `agent login`."
                    }
                    "antigravity" => {
                        "Press Install Antigravity CLI below. After installation completes, run Detect again and sign in with `agy`."
                    }
                    _ => "Install and authenticate this provider's CLI",
                },
            ),
            ("claude", true, AuthCheck::Authenticated) => (
                "detected",
                "CLI reports signed in. Claude validates the stored credentials on the first headless request; run `claude auth login` if that request returns 401.",
            ),
            ("gemini", true, _) => (
                "detected",
                "CLI detected. Authentication is verified by the first headless request.",
            ),
            (_, true, AuthCheck::ProbeFailed { detail }) => ("probe_failed", detail.as_str()),
            (_, true, AuthCheck::NotLoggedIn) => (
                "needs_auth",
                match id {
                    "codex" => "CLI detected, but `codex login status` did not confirm a session. Run `codex login`.",
                    "claude" => "CLI detected, but `claude auth status` reports signed out. Run `claude auth login`.",
                    "grok" => "CLI detected, but `grok models` could not verify a session. Run `grok login`.",
                    "cursor" => "Sign in below with `agent login`. Cursor IDE sign-in is separate from the Cursor CLI.",
                    "antigravity" => "Sign in below with `agy`. A Gemini API key alone does not authenticate Antigravity CLI.",
                    _ => "CLI detected, but authentication could not be verified.",
                },
            ),
            (_, true, AuthCheck::Authenticated) => (
                "ready",
                match id {
                    "codex" => "Installed, authenticated, and ready for workspace conversations.",
                    "claude" => "Installed and authenticated. Uses Claude Code's supported headless event stream.",
                    "grok" => "Installed and authenticated. Uses Grok Build's supported single-turn event stream.",
                    "cursor" => "Installed and authenticated. Uses Cursor CLI's supported headless stream-json stream.",
                    "antigravity" => "Installed and authenticated. Uses Antigravity CLI's supported headless stream-json stream and native generate_image.",
                    _ => "Installed and ready.",
                },
            ),
        };
        ProviderStatus {
            id: id.into(),
            name: name.into(),
            kind: "agent".into(),
            installed,
            executable: executable.map(|path| path.to_string_lossy().into_owned()),
            status: status.into(),
            detail: detail.into(),
            modes,
            capabilities: provider_capabilities(id),
            configurable: false,
            has_api_key: false,
            base_url: None,
            model: None,
        }
    })
    .collect();
    providers.push(ProviderStatus {
        id: "imagegen".into(),
        name: "OpenAI ImageGen".into(),
        kind: "image".into(),
        installed: providers
            .iter()
            .any(|provider| provider.id == "codex" && provider.status == "ready"),
        executable: None,
        status: if providers
            .iter()
            .any(|provider| provider.id == "codex" && provider.status == "ready")
        {
            "ready"
        } else {
            "needs_codex"
        }
        .into(),
        detail: "Provided through the authenticated Codex workflow; no separate image API key is required.".into(),
        modes: Vec::new(),
        capabilities: provider_capabilities("codex"),
        configurable: false,
        has_api_key: false,
        base_url: None,
        model: None,
    });
    let cursor_ready = providers
        .iter()
        .any(|provider| provider.id == "cursor" && provider.status == "ready");
    providers.push(ProviderStatus {
        id: "cursor-image".into(),
        name: "Cursor Image".into(),
        kind: "image".into(),
        installed: cursor_ready,
        executable: None,
        status: if cursor_ready {
            "ready".into()
        } else {
            "needs_cursor".into()
        },
        detail: "Native image generation through the authenticated Cursor agent (GenerateImage)."
            .into(),
        modes: Vec::new(),
        capabilities: provider_capabilities("cursor"),
        configurable: false,
        has_api_key: false,
        base_url: None,
        model: None,
    });
    let antigravity_ready = providers
        .iter()
        .any(|provider| provider.id == "antigravity" && provider.status == "ready");
    providers.push(ProviderStatus {
        id: "antigravity-image".into(),
        name: "Antigravity Image".into(),
        kind: "image".into(),
        installed: antigravity_ready,
        executable: None,
        status: if antigravity_ready {
            "ready".into()
        } else {
            "needs_antigravity".into()
        },
        detail:
            "Native image generation through the authenticated Antigravity agent (generate_image)."
                .into(),
        modes: Vec::new(),
        capabilities: provider_capabilities("antigravity"),
        configurable: false,
        has_api_key: false,
        base_url: None,
        model: None,
    });
    let mut configured = stored_image_providers(state).unwrap_or_default();
    if !configured
        .iter()
        .any(|provider| provider.id == "grok-image")
    {
        configured.push(StoredImageProvider {
            id: "grok-image".into(),
            name: "Grok Imagine".into(),
            provider_type: "grok".into(),
            base_url: "https://api.x.ai/v1".into(),
            api_key: String::new(),
            model: "grok-imagine-image-2.0".into(),
        });
    }
    providers.extend(configured.iter().map(image_provider_status));
    providers.push(ProviderStatus {
        id: "midjourney".into(),
        name: "Midjourney".into(),
        kind: "image".into(),
        installed: false,
        executable: None,
        status: "unsupported".into(),
        detail: "Use an authorized gateway. Midjourney does not offer a general public API, and Sprite Studio never automates its website or Discord bot.".into(),
        modes: Vec::new(),
        capabilities: provider_capabilities("unknown"),
        configurable: true,
        has_api_key: false,
        base_url: None,
        model: None,
    });
    providers
}

pub(crate) fn provider_capabilities(id: &str) -> ProviderCapabilities {
    match id {
        "codex" => ProviderCapabilities {
            text_input: true,
            image_input: true,
            multiple_image_input: true,
            image_editing: true,
            masks: false,
            transparency: true,
            structured_output: true,
            video_animation: false,
            image_to_image: true,
            maximum_reference_images: 5,
        },
        "claude" | "gemini" | "grok" | "cursor" | "antigravity" => ProviderCapabilities {
            text_input: true,
            image_input: true,
            multiple_image_input: true,
            image_editing: false,
            masks: false,
            transparency: false,
            structured_output: true,
            video_animation: false,
            image_to_image: false,
            maximum_reference_images: 10,
        },
        _ => ProviderCapabilities {
            text_input: true,
            image_input: false,
            multiple_image_input: false,
            image_editing: false,
            masks: false,
            transparency: false,
            structured_output: false,
            video_animation: false,
            image_to_image: false,
            maximum_reference_images: 0,
        },
    }
}
