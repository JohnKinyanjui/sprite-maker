use super::ik::{bone_world_affine, identity_transform, solve_contact_ik};
use super::{RigBone, RigPoint};
use std::collections::HashMap;

fn point(name: &str, x: f64, y: f64) -> RigPoint {
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

fn two_bone_leg() -> (Vec<RigBone>, HashMap<String, (f64, f64)>) {
    let points = vec![
        point("hip", 26.5, 25.5),
        point("knee", 26.5, 34.0),
        point("foot", 26.5, 42.5),
    ];
    let bone = |name: &str, start: &str, end: &str, parent: Option<&str>| RigBone {
        id: format!("b-{name}"),
        name: name.into(),
        start_point: start.into(),
        end_point: end.into(),
        radius: 2.5,
        parent: parent.map(Into::into),
        z: 1,
    };
    let bones = vec![
        bone("thigh", "hip", "knee", None),
        bone("shin", "knee", "foot", Some("thigh")),
    ];
    let positions = points
        .iter()
        .map(|p| (p.name.clone(), (p.x, p.y)))
        .collect();
    (bones, positions)
}

/// Solve, then run the deltas through the renderer's own FK. Returns (knee, foot).
fn pose_leg(target: (f64, f64), bend: f64) -> ((f64, f64), (f64, f64)) {
    let (bones, positions) = two_bone_leg();
    let (parent_delta, child_delta) = solve_contact_ik(&bones, 1, &positions, target, bend);
    let mut transforms = HashMap::new();
    let mut thigh = identity_transform("thigh");
    thigh.rotate = parent_delta;
    let mut shin = identity_transform("shin");
    shin.rotate = child_delta;
    transforms.insert("thigh".to_string(), thigh);
    transforms.insert("shin".to_string(), shin);
    let mut cache = HashMap::new();
    let knee = bone_world_affine(0, &bones, &positions, &transforms, &mut cache, 0).apply(
        26.5,
        34.0,
    );
    let foot = bone_world_affine(1, &bones, &positions, &transforms, &mut cache, 0).apply(
        26.5,
        42.5,
    );
    (knee, foot)
}

#[test]
fn contact_ik_reaches_a_bent_target_for_both_bend_directions() {
    let hip = (26.5, 25.5);
    let target = (28.5, 41.5);
    let mut sides = Vec::new();
    for bend in [1.0, -1.0] {
        let (knee, foot) = pose_leg(target, bend);
        let miss = (foot.0 - target.0).hypot(foot.1 - target.1);
        assert!(
            miss < 1e-6,
            "bend {bend}: foot {foot:?} misses {target:?} by {miss:.3} px"
        );
        let cross = (target.0 - hip.0) * (knee.1 - hip.1) - (target.1 - hip.1) * (knee.0 - hip.0);
        sides.push(cross.signum());
    }
    assert_ne!(
        sides[0],
        sides[1],
        "bend +1 and -1 must put the knee on opposite sides"
    );
}

#[test]
fn contact_ik_straight_target_lands() {
    for bend in [1.0, -1.0] {
        let (_, foot) = pose_leg((26.5, 42.5), bend);
        assert!(
            (foot.0 - 26.5).hypot(foot.1 - 42.5) < 0.05,
            "bend {bend}: {foot:?}"
        );
    }
}
