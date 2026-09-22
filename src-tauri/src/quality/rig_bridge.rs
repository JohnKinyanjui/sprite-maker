use crate::{
    assets::{get_asset, read_generation_manifest},
    error::CommandResult,
    models::Animation,
    rig::{interpolate_rig_frame, load_rig_by_id, render_frames, Rig},
    workspace::workspace_path,
    AppState,
};
use image::RgbaImage;

fn resolve_rig_for_animation(
    state: &AppState,
    animation: &Animation,
) -> Option<Rig> {
    let root = match workspace_path(state, &animation.workspace_id) {
        Ok(value) => value,
        Err(_) => return None,
    };
    let manifest = match read_generation_manifest(&root) {
        Ok(value) => value,
        Err(_) => return None,
    };
    let manifest = match manifest {
        Some(value) => value,
        None => return None,
    };
    let rig_id = match manifest.rig_id {
        Some(value) => value,
        None => return None,
    };
    match load_rig_by_id(state, &rig_id) {
        Ok(value) => value,
        Err(_) => None,
    }
}

pub(crate) fn try_render_rig_transition(
    state: &AppState,
    animation: &Animation,
    first_index: usize,
    second_index: usize,
) -> CommandResult<Option<RgbaImage>> {
    let rig = match resolve_rig_for_animation(state, animation) {
        Some(value) => value,
        None => return Ok(None),
    };
    if rig.frames.is_empty()
        || first_index >= rig.frames.len()
        || second_index >= rig.frames.len()
    {
        return Ok(None);
    }
    let master_asset_id = match rig.asset_id.as_ref() {
        Some(value) => value,
        None => return Ok(None),
    };
    let asset = match get_asset(state, master_asset_id) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let master = match image::open(&asset.path) {
        Ok(value) => value.to_rgba8(),
        Err(_) => return Ok(None),
    };
    let midpoint =
        interpolate_rig_frame(&rig.frames[first_index], &rig.frames[second_index]);
    let mut render_rig = rig;
    render_rig.frames = vec![midpoint];
    Ok(render_frames(&master, &render_rig).into_iter().next())
}
