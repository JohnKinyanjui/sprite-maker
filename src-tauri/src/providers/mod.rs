mod antigravity_stream;
mod arguments;
mod auth;
mod cursor_stream;
mod detect;
mod discovery;
mod execute;
mod execute_finish;
mod external_source;
mod headless;
mod image_providers;
mod modes;
mod probes;
mod prompt;
mod refine;
mod run;
mod stream;

pub use auth::{
    __cmd__authenticate_agent_provider, __cmd__install_agent_provider,
    __tauri_command_name_authenticate_agent_provider, __tauri_command_name_install_agent_provider,
    authenticate_agent_provider, install_agent_provider,
};
pub use detect::{
    __cmd__detect_providers, __tauri_command_name_detect_providers, detect_providers,
};
pub use image_providers::{
    __cmd__delete_image_provider, __cmd__save_image_provider, __cmd__test_image_provider,
    __tauri_command_name_delete_image_provider, __tauri_command_name_save_image_provider,
    __tauri_command_name_test_image_provider, delete_image_provider, save_image_provider,
    test_image_provider,
};
pub use refine::{
    __cmd__cancel_refine_generation_prompt_command, __cmd__refine_generation_prompt_command,
    __tauri_command_name_cancel_refine_generation_prompt_command,
    __tauri_command_name_refine_generation_prompt_command,
    cancel_refine_generation_prompt_command, refine_generation_prompt_command,
};
pub use run::{
    __cmd__cancel_provider_request, __cmd__start_provider_message,
    __tauri_command_name_cancel_provider_request, __tauri_command_name_start_provider_message,
    cancel_provider_request, start_provider_message,
};

pub(crate) use refine::RefineCancelEntry;
pub(crate) use detect::detect_providers_inner;
pub(crate) use headless::{apply_std_headless_flags, apply_tokio_headless_flags};
pub(crate) use prompt::run_agent_text_request;
pub(crate) use run::{cancel_provider_request_inner, start_provider_run};

#[cfg(test)]
use arguments::{
    codex_arguments, provider_arguments, provider_stdin_bytes, validate_provider_options,
};
#[cfg(test)]
use auth::{antigravity_cli_install_command, cursor_cli_install_command};
#[cfg(test)]
use discovery::{executable_lookup_names, merge_provider_paths, provider_process_path};
#[cfg(test)]
use image_providers::is_provider_native_image;
#[cfg(test)]
use modes::{antigravity_modes_from_text, cursor_auth_from_status_json};
#[cfg(test)]
use stream::{
    append_stream_text, parse_codex_line, parse_stream_line, provider_failure_message,
    response_reports_generation_failure, with_optional_auth_hint,
};

#[cfg(all(test, unix))]
use discovery::{login_shell_path, login_shell_path_with_timeout, provider_environment_path};

#[cfg(test)]
mod discovery_tests;
#[cfg(test)]
mod stream_tests;
