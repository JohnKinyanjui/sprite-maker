use super::mirror::mirror_rig_frame;
use super::types::{RigContact, RigFrame, RigTransform};

#[test]
fn mirror_rig_frame_negates_horizontal_motion_and_contacts() {
    let frame = RigFrame {
        phase: None,
        hold: false,
        root_dx: 4.0,
        root_dy: 0.0,
        transforms: vec![RigTransform {
            bone: "thigh".into(),
            dx: 2.0,
            dy: 0.0,
            rotate: 0.5,
            scale_x: 1.0,
            scale_y: 1.0,
        }],
        contacts: vec![RigContact {
            bone: "foot".into(),
            x: 12.0,
            y: 20.0,
            bend: 0.25,
        }],
    };
    let mirrored = mirror_rig_frame(&frame, "horizontal", 64.0, 64.0);
    assert_eq!(mirrored.root_dx, -4.0);
    assert_eq!(mirrored.transforms[0].dx, -2.0);
    assert_eq!(mirrored.transforms[0].rotate, -0.5);
    assert_eq!(mirrored.contacts[0].x, 52.0);
    assert_eq!(mirrored.contacts[0].bend, -0.25);
}

#[test]
fn mirror_rig_frame_negates_vertical_motion() {
    let frame = RigFrame {
        phase: None,
        hold: false,
        root_dx: 2.0,
        root_dy: 5.0,
        transforms: vec![RigTransform {
            bone: "thigh".into(),
            dx: 1.0,
            dy: 3.0,
            rotate: 0.4,
            scale_x: 1.0,
            scale_y: 1.0,
        }],
        contacts: vec![RigContact {
            bone: "foot".into(),
            x: 20.0,
            y: 48.0,
            bend: 0.2,
        }],
    };
    let mirrored = mirror_rig_frame(&frame, "vertical", 64.0, 64.0);
    assert_eq!(mirrored.root_dy, -5.0);
    assert_eq!(mirrored.transforms[0].dy, -3.0);
    assert_eq!(mirrored.contacts[0].y, 16.0);
}
