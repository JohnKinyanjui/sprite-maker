mod persist;
mod spritesheet;
mod spritesheet_render;
mod vfx_abilities;
mod vfx_draw;
mod vfx_queue;

pub use persist::{
    __cmd__cancel_job, __cmd__list_jobs, __tauri_command_name_cancel_job,
    __tauri_command_name_list_jobs, cancel_job, list_jobs,
};
pub(crate) use persist::{
    cancellation_requested, load_job, request_job_cancellation, set_job_metadata, set_job_state,
    JobProgress,
};
pub(crate) use persist::emit_job;
pub(crate) use spritesheet::queue_sprite_sheet_inner;
pub use spritesheet::{
    __cmd__delete_sprite_sheet, __cmd__list_sprite_sheets, __cmd__queue_sprite_sheet,
    __tauri_command_name_delete_sprite_sheet, __tauri_command_name_list_sprite_sheets,
    __tauri_command_name_queue_sprite_sheet, delete_sprite_sheet, list_sprite_sheets,
    queue_sprite_sheet,
};
pub(crate) use vfx_queue::queue_procedural_vfx_inner;
pub use vfx_queue::{
    __cmd__list_vfx_effects, __cmd__queue_procedural_vfx, __tauri_command_name_list_vfx_effects,
    __tauri_command_name_queue_procedural_vfx, list_vfx_effects, queue_procedural_vfx,
};

#[cfg(test)]
mod tests;
