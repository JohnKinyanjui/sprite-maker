use super::types::{Rig, RigContact, RigFrame, RigTransform};

pub(crate) fn mirror_rig_frame(
    frame: &RigFrame,
    axis: &str,
    canvas_width: f64,
    canvas_height: f64,
) -> RigFrame {
    match axis {
        "vertical" => RigFrame {
            phase: frame.phase.clone(),
            hold: frame.hold,
            root_dx: frame.root_dx,
            root_dy: -frame.root_dy,
            transforms: frame
                .transforms
                .iter()
                .map(|transform| mirror_transform_vertical(transform))
                .collect(),
            contacts: frame
                .contacts
                .iter()
                .map(|contact| mirror_contact_vertical(contact, canvas_height))
                .collect(),
        },
        _ => RigFrame {
            phase: frame.phase.clone(),
            hold: frame.hold,
            root_dx: -frame.root_dx,
            root_dy: frame.root_dy,
            transforms: frame
                .transforms
                .iter()
                .map(|transform| mirror_transform_horizontal(transform))
                .collect(),
            contacts: frame
                .contacts
                .iter()
                .map(|contact| mirror_contact_horizontal(contact, canvas_width))
                .collect(),
        },
    }
}

pub(crate) fn mirror_rig_frames(
    rig: &Rig,
    axis: &str,
    canvas_width: f64,
    canvas_height: f64,
) -> Vec<RigFrame> {
    rig.frames
        .iter()
        .map(|frame| mirror_rig_frame(frame, axis, canvas_width, canvas_height))
        .collect()
}

fn mirror_transform_horizontal(transform: &RigTransform) -> RigTransform {
    RigTransform {
        bone: transform.bone.clone(),
        dx: -transform.dx,
        dy: transform.dy,
        rotate: -transform.rotate,
        scale_x: transform.scale_x,
        scale_y: transform.scale_y,
    }
}

fn mirror_transform_vertical(transform: &RigTransform) -> RigTransform {
    RigTransform {
        bone: transform.bone.clone(),
        dx: transform.dx,
        dy: -transform.dy,
        rotate: -transform.rotate,
        scale_x: transform.scale_x,
        scale_y: transform.scale_y,
    }
}

fn mirror_contact_horizontal(contact: &RigContact, canvas_width: f64) -> RigContact {
    RigContact {
        bone: contact.bone.clone(),
        x: canvas_width - contact.x,
        y: contact.y,
        bend: -contact.bend,
    }
}

fn mirror_contact_vertical(contact: &RigContact, canvas_height: f64) -> RigContact {
    RigContact {
        bone: contact.bone.clone(),
        x: contact.x,
        y: canvas_height - contact.y,
        bend: -contact.bend,
    }
}

pub(crate) fn resolve_rig_for_animation(
    state: &crate::AppState,
    workspace_id: &str,
    worktree_id: Option<&str>,
    animation_name: &str,
    rig_id: Option<&str>,
) -> Option<Rig> {
    if let Some(rig_id) = rig_id.filter(|value| !value.trim().is_empty()) {
        return super::load_rig_by_id(state, rig_id).ok().flatten();
    }
    let connection = match state.db.lock() {
        Ok(value) => value,
        Err(_) => return None,
    };
    let base_name = animation_name
        .rsplit_once('-')
        .map(|(stem, _)| stem)
        .unwrap_or(animation_name);
    let query = if worktree_id.is_some() {
        "SELECT id FROM rigs WHERE workspace_id=?1 AND worktree_id=?2 AND lower(name)=lower(?3) LIMIT 1"
    } else {
        "SELECT id FROM rigs WHERE workspace_id=?1 AND worktree_id IS NULL AND lower(name)=lower(?2) LIMIT 1"
    };
    let rig_id: Option<String> = if let Some(worktree_id) = worktree_id {
        connection
            .query_row(query, rusqlite::params![workspace_id, worktree_id, base_name], |row| {
                row.get(0)
            })
            .ok()
    } else {
        connection
            .query_row(query, rusqlite::params![workspace_id, base_name], |row| row.get(0))
            .ok()
    };
    rig_id.and_then(|id| super::load_rig_by_id(state, &id).ok().flatten())
}
