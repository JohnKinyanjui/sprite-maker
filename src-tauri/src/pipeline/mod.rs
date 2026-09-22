mod align;
mod anchors;
mod character_contract;
mod character_pack;
mod character_profile;
mod clean_alpha;
mod contract;
mod contract_retry;
mod contract_retry_job;
mod direction_meta;
mod direction_set;
mod facing;
mod frame_score;
mod harden;
mod logging;
mod mirror;
mod motion_batch;
mod motion_presets;
mod sessions;
#[cfg(test)]
mod sessions_tests;
mod optical_flow;
mod region_regen;
mod paint;
mod shared_palette;
mod normalize;
mod production_score;
mod qc;
mod segment;
mod snap_grid;
mod strip;

pub(crate) use align::{blit_with_offset, nudge_animation_frames_inner};
pub(crate) use anchors::{
    anchor_contract_text, get_anchor_inner, list_anchors_inner, promote_anchor_inner,
    remove_anchors_for_asset_inner,
};
pub(crate) use character_contract::character_contract_check_inner;
pub(crate) use character_profile::get_character_profile_inner;
pub(crate) use contract::{
    ensure_export_allowed, set_animation_review_status_inner, size_contract_check_inner,
};
pub(crate) use contract_retry::{
    finalize_contract_retry_inner, retry_size_contract_inner,
};
pub(crate) use clean_alpha::clean_alpha_animation_inner;
pub(crate) use contract_retry_job::queue_contract_retry_inner;
pub(crate) use direction_set::queue_direction_set_inner;
pub(crate) use character_pack::export_character_pack_inner;
pub(crate) use facing::{
    detect_anchor_facing_inner, list_facing_checks_inner, orient_anchor_inner,
    ListFacingChecksInput,
};
pub(crate) use frame_score::score_animation_frames_inner;
pub(crate) use motion_batch::queue_motion_batch_inner;
pub(crate) use production_score::production_score_inner;
pub(crate) use region_regen::queue_region_regen_inner;
pub(crate) use harden::harden_animation_inner;
pub(crate) use mirror::mirror_animation_inner;
pub(crate) use normalize::normalize_animation_inner;
pub(crate) use snap_grid::snap_animation_offsets_inner;
pub(crate) use sessions::{append_session_from_manifest, ManifestSessionHook};
pub(crate) use paint::{paint_frame_alpha_inner, restore_asset_version_inner};
pub(crate) use shared_palette::quantize_worktree_palette_inner;
pub(crate) use strip::{
    extract_video_frames_inner, score_strip_inner, split_sprite_strip_inner,
    subsample_video_frames_inner,
};

pub use align::{
    __cmd__nudge_animation_frames, __cmd__reset_animation_alignment_offsets,
    __tauri_command_name_nudge_animation_frames, __tauri_command_name_reset_animation_alignment_offsets,
    nudge_animation_frames, reset_animation_alignment_offsets,
};
pub use anchors::{
    __cmd__get_anchor, __cmd__list_anchors, __cmd__promote_anchor,
    __tauri_command_name_get_anchor, __tauri_command_name_list_anchors,
    __tauri_command_name_promote_anchor, get_anchor, list_anchors, promote_anchor,
};
pub use character_contract::{
    __cmd__check_character_contract, __tauri_command_name_check_character_contract,
    check_character_contract,
};
pub use character_pack::{
    __cmd__export_character_pack, __tauri_command_name_export_character_pack,
    export_character_pack,
};
pub use character_profile::{
    __cmd__export_character_profile, __cmd__get_character_profile,
    __cmd__import_character_profile, __cmd__refresh_character_profile,
    __tauri_command_name_export_character_profile, __tauri_command_name_get_character_profile,
    __tauri_command_name_import_character_profile, __tauri_command_name_refresh_character_profile,
    export_character_profile, get_character_profile, import_character_profile,
    refresh_character_profile,
};
pub use production_score::{
    __cmd__get_production_score, __tauri_command_name_get_production_score, get_production_score,
};
pub use contract::{
    __cmd__check_size_contract, __cmd__set_animation_review_status,
    __tauri_command_name_check_size_contract, __tauri_command_name_set_animation_review_status,
    check_size_contract, set_animation_review_status,
};
pub use contract_retry::{
    __cmd__finalize_contract_retry, __cmd__retry_size_contract,
    __tauri_command_name_finalize_contract_retry, __tauri_command_name_retry_size_contract,
    finalize_contract_retry, retry_size_contract,
};
pub use clean_alpha::{
    __cmd__clean_alpha_animation, __tauri_command_name_clean_alpha_animation, clean_alpha_animation,
};
pub use contract_retry_job::{
    __cmd__queue_contract_retry, __tauri_command_name_queue_contract_retry, queue_contract_retry,
};
pub use direction_meta::{
    __cmd__list_direction_meta, __tauri_command_name_list_direction_meta, list_direction_meta,
};
pub use direction_set::{
    __cmd__queue_direction_set, __tauri_command_name_queue_direction_set, queue_direction_set,
};
pub use facing::{
    __cmd__detect_anchor_facing, __cmd__list_facing_checks, __cmd__orient_anchor,
    __tauri_command_name_detect_anchor_facing, __tauri_command_name_list_facing_checks,
    __tauri_command_name_orient_anchor, detect_anchor_facing, list_facing_checks, orient_anchor,
};
pub use frame_score::{
    __cmd__score_animation_frames, __tauri_command_name_score_animation_frames,
    score_animation_frames,
};
pub use harden::{
    __cmd__harden_animation, __tauri_command_name_harden_animation, harden_animation,
};
pub use mirror::{
    __cmd__mirror_animation, __tauri_command_name_mirror_animation, mirror_animation,
};
pub use motion_batch::{
    __cmd__queue_motion_batch, __tauri_command_name_queue_motion_batch, queue_motion_batch,
};
pub use region_regen::{
    __cmd__queue_region_regen, __tauri_command_name_queue_region_regen, queue_region_regen,
};
pub use motion_presets::{
    __cmd__list_missing_motions, __cmd__list_motion_presets, __cmd__save_motion_presets,
    __tauri_command_name_list_missing_motions, __tauri_command_name_list_motion_presets,
    __tauri_command_name_save_motion_presets, list_missing_motions, list_motion_presets,
    save_motion_presets,
};
pub use sessions::{
    __cmd__list_generation_sessions, __tauri_command_name_list_generation_sessions,
    list_generation_sessions,
};
pub use normalize::{
    __cmd__normalize_animation, __tauri_command_name_normalize_animation, normalize_animation,
};
pub use snap_grid::{
    __cmd__snap_to_pixel_grid, __tauri_command_name_snap_to_pixel_grid, snap_to_pixel_grid,
};
pub use paint::{
    __cmd__paint_frame_alpha, __cmd__restore_asset_version,
    __tauri_command_name_paint_frame_alpha, __tauri_command_name_restore_asset_version,
    paint_frame_alpha, restore_asset_version,
};
pub use shared_palette::{
    __cmd__quantize_worktree_palette, __tauri_command_name_quantize_worktree_palette,
    quantize_worktree_palette,
};
pub use strip::{
    __cmd__extract_video_frames, __cmd__score_sprite_strip, __cmd__split_sprite_strip,
    __cmd__subsample_video_frames, __tauri_command_name_extract_video_frames,
    __tauri_command_name_score_sprite_strip, __tauri_command_name_split_sprite_strip,
    __tauri_command_name_subsample_video_frames, extract_video_frames, score_sprite_strip,
    split_sprite_strip, subsample_video_frames,
};

#[cfg(test)]
mod align_tests;
#[cfg(test)]
mod clean_alpha_tests;
#[cfg(test)]
mod character_contract_tests;
#[cfg(test)]
mod character_pack_tests;
#[cfg(test)]
mod character_profile_tests;
#[cfg(test)]
mod production_score_tests;
#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod contract_retry_tests;
#[cfg(test)]
mod direction_set_tests;
#[cfg(test)]
mod facing_tests;
#[cfg(test)]
mod frame_score_tests;
#[cfg(test)]
mod mirror_tests;
#[cfg(test)]
mod motion_batch_tests;
#[cfg(test)]
mod motion_presets_tests;
#[cfg(test)]
mod optical_flow_tests;
#[cfg(test)]
mod region_regen_tests;
#[cfg(test)]
mod qc_tests;
#[cfg(test)]
mod test_fixtures;
#[cfg(test)]
mod fixture_regression_tests;
#[cfg(test)]
mod e2e_tests;
#[cfg(test)]
mod logging_tests;
#[cfg(test)]
mod harden_tests;
#[cfg(test)]
mod segment_tests;
#[cfg(test)]
mod snap_grid_tests;
#[cfg(test)]
mod strip_subsample_tests;
#[cfg(test)]
mod paint_tests;
#[cfg(test)]
mod tests;
