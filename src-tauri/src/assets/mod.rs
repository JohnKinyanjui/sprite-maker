mod inspect;
mod pixel_normalize;
mod scan;

#[cfg(test)]
use inspect::safe_category;
pub use inspect::{
    __cmd__delete_asset, __cmd__import_asset, __cmd__list_asset_versions, __cmd__list_assets,
    __cmd__rename_asset, __tauri_command_name_delete_asset, __tauri_command_name_import_asset,
    __tauri_command_name_list_asset_versions, __tauri_command_name_list_assets,
    __tauri_command_name_rename_asset, delete_asset, get_asset, import_asset, list_asset_versions,
    list_assets, rename_asset,
};
pub(crate) use inspect::{inspect, list_assets_inner, upsert};
pub(crate) use pixel_normalize::{
    defringe, extract_palette, normalize_sprite_alpha, normalize_sprite_file, remove_orphan_pixels,
};
pub use scan::{
    __cmd__export_asset, __cmd__get_generation_fingerprint, __cmd__get_generation_manifest,
    __cmd__list_workspace_rig_specs, __cmd__scan_assets, __cmd__scan_generation_assets,
    __tauri_command_name_export_asset, __tauri_command_name_get_generation_fingerprint,
    __tauri_command_name_get_generation_manifest, __tauri_command_name_list_workspace_rig_specs,
    __tauri_command_name_scan_assets, __tauri_command_name_scan_generation_assets, export_asset,
    get_generation_fingerprint, get_generation_manifest, list_workspace_rig_specs, scan_assets,
    scan_generation_assets,
};
pub(crate) use scan::{
    export_asset_inner, read_generation_manifest, recover_manifest_from_imagegen_sources,
    scan_generation_assets_inner, write_generation_manifest,
};
#[cfg(test)]
use scan::{collect_workspace_rig_specs, generation_fingerprint};

#[cfg(test)]
mod tests;
