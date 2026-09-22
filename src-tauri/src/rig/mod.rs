mod biped_template;
mod commands;
mod interpolate;
mod creature_templates;
mod fit;
mod ik;
mod mesh;
mod mirror;
mod object_templates;
mod render;
mod skin;
mod suggest;
mod suggestion;
mod types;
mod validate;

#[allow(unused_imports)]
pub use commands::AiRigSuggestionInput;
pub use commands::{
    __cmd__ai_suggest_rig_points, __cmd__analyze_rig_fit, __cmd__delete_rig, __cmd__list_rigs,
    __cmd__save_rig, __cmd__suggest_rig_points, __cmd__validate_rig_spec,
    __tauri_command_name_ai_suggest_rig_points, __tauri_command_name_analyze_rig_fit,
    __tauri_command_name_delete_rig, __tauri_command_name_list_rigs, __tauri_command_name_save_rig,
    __tauri_command_name_suggest_rig_points, __tauri_command_name_validate_rig_spec,
    ai_suggest_rig_points, analyze_rig_fit, delete_rig, list_rigs, save_rig, suggest_rig_points,
    validate_rig_spec,
};
#[allow(unused_imports)]
pub use fit::{MorphologyScore, RigFitReport};
pub use interpolate::{
    __cmd__interpolate_rig_animation_frames, __cmd__interpolate_rig_frames,
    __tauri_command_name_interpolate_rig_animation_frames,
    __tauri_command_name_interpolate_rig_frames, interpolate_rig_animation_frames,
    interpolate_rig_frames,
};
pub use render::{
    __cmd__render_rig_animation, __cmd__render_rig_preview,
    __tauri_command_name_render_rig_animation, __tauri_command_name_render_rig_preview,
    render_rig_animation, render_rig_preview,
};
#[allow(unused_imports)]
pub use suggestion::parse_rig_suggestion_text;
#[allow(unused_imports)]
pub use types::{
    Rig, RigBone, RigContact, RigFrame, RigInput, RigPoint, RigRenderResult, RigSuggestion,
    RigTransform, MORPHOLOGIES,
};
#[allow(unused_imports)]
pub use validate::validate_rig;

pub(crate) use commands::{capture_chat_suggestion, load_rig_by_id};
#[allow(unused_imports)]
pub(crate) use commands::{
    analyze_rig_fit_inner, render_rig_animation_blocking, save_rig_inner, suggest_rig_points_inner,
};
#[allow(unused_imports)]
pub(crate) use fit::{capsule_coverage, detect_morphology};
#[allow(unused_imports)]
pub(crate) use ik::{
    build_ownership, owned_bounds, point_positions, point_segment_distance, Affine,
};
#[allow(unused_imports)]
pub(crate) use mirror::{mirror_rig_frame, mirror_rig_frames, resolve_rig_for_animation};
#[allow(unused_imports)]
pub(crate) use render::render_rig_animation_inner;
#[allow(unused_imports)]
pub(crate) use interpolate::{
    interpolate_rig_frame, interpolate_rig_frame_at, interpolate_rig_frames_between,
    interpolate_rig_frames_inner, InterpolateRigFramesInput,
};
#[allow(unused_imports)]
pub(crate) use skin::render_frames;
#[allow(unused_imports)]
pub(crate) use suggest::{distance_transform, suggest_points};
#[allow(unused_imports)]
pub(crate) use suggestion::normalize_suggestion;
#[allow(unused_imports)]
pub(crate) use types::normalize_morphology;

#[cfg(test)]
mod render_frame;
#[cfg(test)]
pub(crate) use render_frame::render_frame;

#[cfg(test)]
mod interpolate_tests;
#[cfg(test)]
mod mirror_tests;
#[cfg(test)]
mod deform_tests;
#[cfg(test)]
mod ik_tests;
#[cfg(test)]
mod tests;
