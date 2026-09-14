use super::interpolate::{
    interpolate_rig_animation_frames_inner, interpolate_rig_frame, interpolate_rig_frame_at,
    interpolate_rig_frames_between, InterpolateRigAnimationFramesInput,
};
use crate::animations::load_animation_by_id;
use crate::animations::save_animation_inner;
use crate::assets::{inspect, upsert};
use crate::models::{AnimationFrame, AnimationInput};
use crate::rig::RigInput;
use crate::workspace::create_workspace_inner;
use crate::AppState;
use super::types::{RigBone, RigContact, RigFrame, RigPoint, RigTransform};
use image::{Rgba, RgbaImage};

fn sample_left() -> RigFrame {
    RigFrame {
        phase: Some("start".into()),
        hold: false,
        root_dx: 0.0,
        root_dy: 0.0,
        transforms: vec![RigTransform {
            bone: "arm".into(),
            dx: 0.0,
            dy: 0.0,
            rotate: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
        }],
        contacts: vec![RigContact {
            bone: "foot".into(),
            x: 10.0,
            y: 20.0,
            bend: 1.0,
        }],
    }
}

fn sample_right() -> RigFrame {
    RigFrame {
        phase: Some("end".into()),
        hold: false,
        root_dx: 4.0,
        root_dy: -2.0,
        transforms: vec![RigTransform {
            bone: "arm".into(),
            dx: 8.0,
            dy: -4.0,
            rotate: 30.0,
            scale_x: 1.1,
            scale_y: 0.9,
        }],
        contacts: vec![RigContact {
            bone: "foot".into(),
            x: 14.0,
            y: 18.0,
            bend: -1.0,
        }],
    }
}

#[test]
fn interpolate_rig_frames_between_produces_n_frames() {
    let left = sample_left();
    let right = sample_right();
    let frames = interpolate_rig_frames_between(&left, &right, 3);
    assert_eq!(frames.len(), 3);
}

#[test]
fn midpoint_matches_half_lerp() {
    let left = sample_left();
    let right = sample_right();
    let midpoint = interpolate_rig_frame(&left, &right);
    let explicit = interpolate_rig_frame_at(&left, &right, 0.5);
    assert_eq!(midpoint.root_dx, explicit.root_dx);
    assert_eq!(midpoint.transforms[0].rotate, explicit.transforms[0].rotate);
    assert_eq!(midpoint.contacts[0].x, explicit.contacts[0].x);
}

#[test]
fn interpolation_endpoints_are_not_included() {
    let left = sample_left();
    let right = sample_right();
    let frames = interpolate_rig_frames_between(&left, &right, 1);
    assert_eq!(frames.len(), 1);
    assert_ne!(frames[0].root_dx, left.root_dx);
    assert_ne!(frames[0].root_dx, right.root_dx);
}

#[test]
fn interpolate_animation_frames_rejects_zero_steps() {
    let root = std::env::temp_dir().join(format!("rig-interp-{}", uuid::Uuid::new_v4()));
    let connection = crate::database::open(&root.join("app.sqlite3")).expect("db");
    let state = crate::AppState::from_connection(connection);
    let error = interpolate_rig_animation_frames_inner(
        InterpolateRigAnimationFramesInput {
            rig: RigInput {
                id: None,
                workspace_id: "workspace".into(),
                worktree_id: None,
                asset_id: None,
                name: "rig".into(),
                morphology: "biped".into(),
                fps: 8.0,
                looping: true,
                points: vec![],
                bones: vec![],
                frames: vec![],
            },
            from_index: 0,
            to_index: 1,
            steps: 0,
            animation_id: Some("animation".into()),
            workspace_id: "workspace".into(),
            worktree_id: None,
        },
        None,
        &state,
    )
    .expect_err("zero steps");
    assert_eq!(error.code, "invalid_steps");
}

fn rig_point(name: &str, x: f64, y: f64) -> RigPoint {
    RigPoint {
        id: format!("p-{name}"),
        name: name.into(),
        kind: "joint".into(),
        x,
        y,
        confidence: 1.0,
        source: "user".into(),
        note: None,
    }
}

fn sample_rig_input(workspace_id: &str, asset_id: &str) -> RigInput {
    RigInput {
        id: None,
        workspace_id: workspace_id.into(),
        worktree_id: None,
        asset_id: Some(asset_id.into()),
        name: "rig walk".into(),
        morphology: "biped".into(),
        fps: 8.0,
        looping: true,
        points: vec![rig_point("a", 8.0, 2.0), rig_point("b", 8.0, 14.0)],
        bones: vec![RigBone {
            id: "b1".into(),
            name: "limb".into(),
            start_point: "a".into(),
            end_point: "b".into(),
            radius: 4.0,
            parent: None,
            z: 1,
        }],
        frames: vec![sample_left(), sample_right()],
    }
}

#[test]
fn interpolate_animation_frames_creates_assets_and_extends_animation() {
    let root = std::env::temp_dir().join(format!("rig-interp-anim-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("dir");
    let connection = crate::database::open(&root.join("app.sqlite3")).expect("db");
    let state = AppState::from_connection(connection);
    let project = root.join("game");
    std::fs::create_dir_all(project.join("assets/characters")).expect("dirs");
    let workspace = create_workspace_inner(
        "Game".into(),
        project.to_string_lossy().into_owned(),
        &state,
    )
    .expect("workspace");
    let ws_root = crate::workspace::workspace_path(&state, &workspace.id).expect("root");
    let master_path = ws_root.join("assets/characters/master.png");
    RgbaImage::from_pixel(16, 16, Rgba([80, 120, 200, 255]))
        .save(&master_path)
        .expect("save");
    let asset = inspect(&workspace.id, &ws_root, &master_path, None).expect("inspect");
    upsert(&state, &asset, "test").expect("upsert");

    let animation = save_animation_inner(
        AnimationInput {
            id: None,
            workspace_id: workspace.id.clone(),
            worktree_id: None,
            name: "Walk".into(),
            fps: 8.0,
            looping: true,
            frames: vec![
                AnimationFrame {
                    asset_id: asset.id.clone(),
                    duration_ms: None,
                    offset_x: 0,
                    offset_y: 0,
                },
                AnimationFrame {
                    asset_id: asset.id.clone(),
                    duration_ms: None,
                    offset_x: 0,
                    offset_y: 0,
                },
            ],
            motion_plan: None,
            review_status: Some("draft".into()),
        },
        &state,
    )
    .expect("animation");

    let result = interpolate_rig_animation_frames_inner(
        InterpolateRigAnimationFramesInput {
            rig: sample_rig_input(&workspace.id, &asset.id),
            from_index: 0,
            to_index: 1,
            steps: 2,
            animation_id: Some(animation.id.clone()),
            workspace_id: workspace.id.clone(),
            worktree_id: None,
        },
        None,
        &state,
    )
    .expect("interpolate");

    assert_eq!(result.inserted_count, 2);
    assert_eq!(result.asset_ids.len(), 2);
    assert!(result.job_id.is_none());
    let updated = load_animation_by_id(&state, &animation.id).expect("reload");
    assert_eq!(updated.frames.len(), 4);
    let asset_count: i64 = state
        .db
        .lock()
        .expect("lock")
        .query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))
        .expect("count");
    assert_eq!(asset_count, 3, "master plus two rendered in-betweens");

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}
