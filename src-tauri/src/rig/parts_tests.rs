use super::parts::{assemble, assemble_fresh_parts, load_for_master};
use super::parts_render::{render_master_frames, render_parts_frames, scale2x_for_tests};
use super::suggestion::parse_frames_for_skeleton;
use super::types::{Rig, RigFrame, RigSuggestion, RigTransform};
use image::{Rgba, RgbaImage};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const TORSO: Rgba<u8> = Rgba([200, 60, 60, 255]);
const HEAD: Rgba<u8> = Rgba([240, 200, 160, 255]);
const ARM_NEAR: Rgba<u8> = Rgba([60, 60, 200, 255]);
const ARM_FAR: Rgba<u8> = Rgba([30, 30, 120, 255]);
const THIGH_NEAR: Rgba<u8> = Rgba([60, 180, 60, 255]);
const THIGH_FAR: Rgba<u8> = Rgba([20, 90, 20, 255]);
const SHIN_NEAR: Rgba<u8> = Rgba([220, 220, 40, 255]);
const SHIN_FAR: Rgba<u8> = Rgba([120, 120, 20, 255]);

struct Workspace(PathBuf);

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn workspace() -> Workspace {
    let root = std::env::temp_dir().join(format!("sprite-parts-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join(".sprite-studio/parts/runner")).unwrap();
    Workspace(root)
}

/// A solid part with a transparent margin, like a keyed cut-out.
fn part(width: u32, height: u32, color: Rgba<u8>) -> RgbaImage {
    let mut image = RgbaImage::new(width + 4, height + 4);
    for y in 0..height {
        for x in 0..width {
            image.put_pixel(x + 2, y + 2, color);
        }
    }
    image
}

fn write_parts(dir: &Path, include_far: bool) {
    let mut files = vec![
        ("head", part(10, 10, HEAD)),
        ("torso", part(10, 20, TORSO)),
        ("upper_arm_near", part(4, 11, ARM_NEAR)),
        ("forearm_near", part(4, 11, ARM_NEAR)),
        ("thigh_near", part(5, 14, THIGH_NEAR)),
        ("shin_near", part(5, 14, SHIN_NEAR)),
    ];
    if include_far {
        files.extend([
            ("upper_arm_far", part(4, 11, ARM_FAR)),
            ("forearm_far", part(4, 11, ARM_FAR)),
            ("thigh_far", part(5, 14, THIGH_FAR)),
            ("shin_far", part(5, 14, SHIN_FAR)),
        ]);
    }
    let mut parts = serde_json::Map::new();
    for (name, image) in &files {
        image.save(dir.join(format!("{name}.png"))).unwrap();
        parts.insert(name.to_string(), json!({ "file": format!("{name}.png") }));
    }
    std::fs::write(
        dir.join("parts.json"),
        serde_json::to_vec(&json!({"kind": "biped-parts", "name": "runner", "facing": "right", "parts": parts})).unwrap(),
    )
    .unwrap();
}

fn count(image: &RgbaImage, color: Rgba<u8>) -> usize {
    image.pixels().filter(|pixel| **pixel == color).count()
}

fn rig_from(parts: &super::parts::PartsRig, frames: Vec<RigFrame>) -> Rig {
    Rig {
        id: "rig".into(),
        workspace_id: "ws".into(),
        worktree_id: None,
        asset_id: None,
        name: "runner".into(),
        morphology: "biped".into(),
        fps: 12.0,
        looping: true,
        points: parts.points.clone(),
        bones: parts.bones.clone(),
        frames,
        created_at: String::new(),
        updated_at: String::new(),
    }
}

fn frame(transforms: &[(&str, f64)]) -> RigFrame {
    RigFrame {
        phase: None,
        hold: false,
        root_dx: 0.0,
        root_dy: 0.0,
        transforms: transforms
            .iter()
            .map(|(bone, rotate)| RigTransform {
                bone: bone.to_string(),
                dx: 0.0,
                dy: 0.0,
                rotate: *rotate,
                scale_x: 1.0,
                scale_y: 1.0,
            })
            .collect(),
        contacts: Vec::new(),
    }
}

fn point(parts: &super::parts::PartsRig, name: &str) -> (f64, f64) {
    let point = parts.points.iter().find(|point| point.name == name).unwrap();
    (point.x, point.y)
}

#[test]
fn assembly_stands_the_figure_on_the_ground_with_exact_joints() {
    let ws = workspace();
    let dir = ws.0.join(".sprite-studio/parts/runner");
    write_parts(&dir, true);
    let assembly = assemble(&dir, Some((96, 96))).expect("assemble");
    assert_eq!(assembly.master.dimensions(), (96, 96));
    assert_eq!(assembly.rig.bones.len(), 10);
    assert_eq!(assembly.rig.layers.len(), 10);
    // Legs hang straight down from the hip, knees stack on thighs, and the
    // feet stand on a ground line near the bottom of the canvas.
    let (hip, knee, foot) = (point(&assembly.rig, "hip_r"), point(&assembly.rig, "knee_r"), point(&assembly.rig, "foot_r"));
    assert!((hip.0 - 48.0).abs() <= 1.0, "hip is centred: {hip:?}");
    assert!(hip.1 < knee.1 && knee.1 < foot.1);
    assert!((knee.0 - hip.0).abs() < 1.0 && (foot.0 - hip.0).abs() < 1.0);
    let ground = (0..96).rev().find(|y| (0..96).any(|x| assembly.master.get_pixel(x, *y)[3] > 0)).unwrap();
    assert!((88..=93).contains(&ground), "ground line at {ground}");
    assert!((foot.1 - (ground as f64 + 0.5)).abs() <= 1.0, "foot {foot:?} on ground {ground}");
    // Near parts cover far parts at rest, exactly like a side view.
    assert!(count(&assembly.master, THIGH_NEAR) > 0 && count(&assembly.master, THIGH_FAR) == 0);
    assert!(count(&assembly.master, HEAD) == 100);
}

#[test]
fn assembly_grows_a_canvas_that_is_too_small_and_shades_missing_far_parts() {
    let ws = workspace();
    let dir = ws.0.join(".sprite-studio/parts/runner");
    write_parts(&dir, false);
    let assembly = assemble(&dir, Some((32, 32))).expect("assemble");
    let (width, height) = assembly.master.dimensions();
    assert!(width > 32 && height > 32 && width % 8 == 0 && width == height);
    let far = assembly.rig.layers.iter().find(|layer| layer.bone == "thigh_l").unwrap();
    let pixel = far.image.pixels().find(|pixel| pixel[3] > 0).unwrap();
    assert!(pixel[1] < THIGH_NEAR[1], "far thigh reuses the near thigh, darker: {pixel:?}");
}

#[test]
fn assembly_rejects_missing_required_parts_and_escaping_files() {
    let ws = workspace();
    let dir = ws.0.join(".sprite-studio/parts/runner");
    write_parts(&dir, true);
    std::fs::remove_file(dir.join("torso.png")).unwrap();
    assert!(assemble(&dir, None).err().expect("missing torso").contains("torso"));
    std::fs::write(
        dir.join("parts.json"),
        serde_json::to_vec(&json!({"parts": {"torso": {"file": "../../secret.png"}}})).unwrap(),
    )
    .unwrap();
    assert!(assemble(&dir, None).err().expect("escaping file").contains("inside the parts folder"));
}

#[test]
fn rest_frame_reproduces_the_assembled_master_exactly() {
    let ws = workspace();
    let dir = ws.0.join(".sprite-studio/parts/runner");
    write_parts(&dir, true);
    let assembly = assemble(&dir, Some((96, 96))).unwrap();
    let rig = rig_from(&assembly.rig, vec![frame(&[]), frame(&[("thigh_r", -30.0)])]);
    let frames = render_parts_frames(assembly.master.dimensions(), &rig, &assembly.rig);
    assert_eq!(frames[0], assembly.master);
    assert_ne!(frames[1], assembly.master);
}

/// The core failure of the old renderer: a swinging near leg took the far
/// leg's pixels with it or left a hole. Whole parts cannot do that.
#[test]
fn swinging_the_near_leg_reveals_the_complete_far_leg() {
    let ws = workspace();
    let dir = ws.0.join(".sprite-studio/parts/runner");
    write_parts(&dir, true);
    let assembly = assemble(&dir, Some((96, 96))).unwrap();
    assert_eq!(count(&assembly.master, THIGH_FAR), 0, "hidden behind the near leg at rest");
    let far_thigh = assembly.rig.layers.iter().find(|layer| layer.bone == "thigh_l").unwrap();
    let far_pixels = count(&far_thigh.image, THIGH_FAR);
    let near_pixels = count(&assembly.master, THIGH_NEAR);
    let rig = rig_from(&assembly.rig, vec![frame(&[("thigh_r", -60.0), ("thigh_l", 35.0)])]);
    let stride = &render_parts_frames(assembly.master.dimensions(), &rig, &assembly.rig)[0];
    // Only the parts drawn in front (torso over the hip, shin over the knee
    // cap) still cover it; the thigh itself keeps its whole area when rotated.
    let revealed = count(stride, THIGH_FAR);
    assert!(revealed * 4 >= far_pixels, "far thigh revealed {revealed}/{far_pixels}");
    let alone = super::parts::PartsRig {
        points: assembly.rig.points.clone(),
        bones: assembly.rig.bones.clone(),
        layers: vec![super::parts::PartLayer { bone: far_thigh.bone.clone(), image: far_thigh.image.clone(), x: far_thigh.x, y: far_thigh.y }],
    };
    let isolated = count(&render_parts_frames(assembly.master.dimensions(), &rig, &alone)[0], THIGH_FAR);
    assert!(isolated * 10 >= far_pixels * 8 && isolated * 10 <= far_pixels * 12, "rotated far thigh {isolated}/{far_pixels}");
    let rotated = count(stride, THIGH_NEAR);
    assert!(rotated * 10 >= near_pixels * 8 && rotated * 10 <= near_pixels * 12, "near thigh {rotated} vs {near_pixels}");
    let (far_shin, shin_pixels) = (count(stride, SHIN_FAR), count(&assembly.master, SHIN_NEAR));
    assert!(far_shin * 10 >= shin_pixels * 8, "far shin visible {far_shin}/{shin_pixels}");
}

#[test]
fn scale2x_doubles_and_keeps_solid_regions() {
    let image = part(3, 3, TORSO);
    let doubled = scale2x_for_tests(&image);
    assert_eq!(doubled.dimensions(), (14, 14));
    // Scale2x rounds the four convex corners and keeps everything else.
    assert_eq!(count(&doubled, TORSO), 36 - 4);
    assert_eq!(*doubled.get_pixel(6, 6), TORSO);
    assert_eq!(doubled.get_pixel(0, 0)[3], 0);
}

#[test]
fn fresh_parts_become_the_master_manifest_and_a_findable_assembly() {
    let ws = workspace();
    let dir = ws.0.join(".sprite-studio/parts/runner");
    write_parts(&dir, true);
    let started = SystemTime::now() - Duration::from_secs(1);
    let outcome = assemble_fresh_parts(&ws.0, started, Some((96, 96))).unwrap().expect("fresh parts");
    assert_eq!(outcome.master_relative, "assets/characters/runner.png");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(ws.0.join(".sprite-studio/last-generation.json")).unwrap()).unwrap();
    assert_eq!(manifest["files"], json!(["assets/characters/runner.png"]));
    assert_eq!(manifest["category"], "characters");
    let master_path = ws.0.join(&outcome.master_relative);
    let parts = load_for_master(&master_path).expect("assembly found from the master");
    assert_eq!(parts.bones.len(), 10);
    let master = image::open(&master_path).unwrap().to_rgba8();
    let rig = rig_from(&parts, vec![frame(&[]), frame(&[("upper_arm_r", -45.0)])]);
    assert_eq!(render_master_frames(&master_path, &master, &rig)[0], master);
    // Registration cleanup re-saves the master with small edits; the parts
    // still apply. (Reproduced: the rewrite made every render fall back.)
    let mut cleaned = master.clone();
    let edge = cleaned.enumerate_pixels().find(|(_, _, pixel)| pixel[3] > 0).map(|(x, y, _)| (x, y)).unwrap();
    cleaned.put_pixel(edge.0, edge.1, Rgba([0, 0, 0, 0]));
    cleaned.put_pixel(0, 0, Rgba([1, 2, 3, 255]));
    cleaned.save(&master_path).unwrap();
    assert!(load_for_master(&master_path).is_some());
    // A repainted master no longer matches its parts; it falls back.
    let mut repainted = master.clone();
    for pixel in repainted.pixels_mut().filter(|pixel| pixel[3] > 0) {
        *pixel = Rgba([10, 10, 10, 255]);
    }
    repainted.save(&master_path).unwrap();
    assert!(load_for_master(&master_path).is_none());
}

#[test]
fn parts_from_an_earlier_request_are_not_reassembled() {
    let ws = workspace();
    write_parts(&ws.0.join(".sprite-studio/parts/runner"), true);
    let later = SystemTime::now() + Duration::from_secs(30);
    assert!(assemble_fresh_parts(&ws.0, later, None).unwrap().is_none());
    assert!(!ws.0.join(".sprite-studio/last-generation.json").exists());
}

#[test]
fn pose_answers_keep_the_exact_skeleton_and_drop_unknown_bones() {
    let ws = workspace();
    let dir = ws.0.join(".sprite-studio/parts/runner");
    write_parts(&dir, true);
    let assembly = assemble(&dir, Some((96, 96))).unwrap();
    let skeleton = RigSuggestion {
        morphology: "biped".into(),
        points: assembly.rig.points.clone(),
        bones: assembly.rig.bones.clone(),
        frames: Vec::new(),
        reasoning: String::new(),
        source: "parts".into(),
    };
    let answer = "```rig-suggestion\n{\"points\": [{\"name\": \"neck\", \"x\": 1, \"y\": 1}], \"frames\": [\
        {\"phase\": \"a\", \"transforms\": [{\"bone\": \"thigh_r\", \"rotate\": -30}, {\"bone\": \"tail\", \"rotate\": 9}]},\
        {\"phase\": \"b\", \"transforms\": [{\"bone\": \"thigh_r\", \"rotate\": 30}]}]}\n```";
    let parsed = parse_frames_for_skeleton(answer, 96, 96, skeleton).expect("frames");
    assert_eq!(parsed.frames.len(), 2);
    assert_eq!(parsed.frames[0].transforms.len(), 1, "unknown bone dropped");
    let neck = parsed.points.iter().find(|point| point.name == "neck").unwrap();
    assert_eq!((neck.x, neck.y), point(&assembly.rig, "neck"), "skeleton is not overwritten");
    assert_eq!(parsed.points.len(), assembly.rig.points.len());
    let single = "```rig-suggestion\n{\"frames\": [{\"transforms\": []}]}\n```";
    let skeleton = RigSuggestion { morphology: "biped".into(), points: assembly.rig.points.clone(), bones: assembly.rig.bones.clone(), frames: Vec::new(), reasoning: String::new(), source: "parts".into() };
    assert!(parse_frames_for_skeleton(single, 96, 96, skeleton).is_none());
}



#[test]
fn rigs_planned_on_another_skeleton_keep_the_classic_renderer() {
    let ws = workspace();
    let dir = ws.0.join(".sprite-studio/parts/runner");
    write_parts(&dir, true);
    let assembly = assemble(&dir, Some((96, 96))).unwrap();
    let exact = rig_from(&assembly.rig, vec![frame(&[])]);
    assert!(super::parts_render::rig_uses_parts(&exact, &assembly.rig));
    // Reproduced: an AI-invented skeleton shared eight bone names but not
    // the joint positions; moving parts about its pivots would tear them.
    let mut moved = exact.clone();
    moved.points.iter_mut().find(|point| point.name == "knee_r").unwrap().x += 6.0;
    assert!(!super::parts_render::rig_uses_parts(&moved, &assembly.rig));
    let mut renamed = exact.clone();
    renamed.bones.iter_mut().find(|bone| bone.name == "lower_arm_r").unwrap().name = "forearm_r".into();
    assert!(!super::parts_render::rig_uses_parts(&renamed, &assembly.rig));
}

