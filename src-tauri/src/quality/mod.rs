mod alignment;
mod analysis;
mod contract_bridge;
mod rig_bridge;
mod frame_checks;
mod interpolation;
mod metrics;
mod motion_checks;
mod repair;
mod reports;

pub use alignment::{
    __cmd__repair_animation_alignment, __tauri_command_name_repair_animation_alignment,
    repair_animation_alignment,
};
pub use interpolation::{
    __cmd__optimize_animation_frames, __tauri_command_name_optimize_animation_frames,
    optimize_animation_frames,
};
pub use repair::{
    __cmd__repair_animation_transparency, __tauri_command_name_repair_animation_transparency,
    repair_animation_transparency,
};
pub use reports::{
    __cmd__acknowledge_quality_check, __cmd__get_quality_report, __cmd__queue_quality_analysis,
    __tauri_command_name_acknowledge_quality_check, __tauri_command_name_get_quality_report,
    __tauri_command_name_queue_quality_analysis, acknowledge_quality_check, get_quality_report,
    queue_quality_analysis,
};
pub(crate) use metrics::{bounds_area, centroid_distance, compute_metrics, pixel_difference};
pub(crate) use reports::{get_quality_report_inner, queue_quality_analysis_inner};

#[cfg(test)]
mod tests;
