//! Parts-based biped rigs.
//!
//! Instead of guessing which pixels of one finished pose belong to each bone,
//! the agent delivers every body part as its own transparent PNG (near and far
//! limbs drawn separately, with rounded joint caps). Sprite Studio assembles
//! them into an exact rest pose: the master image, the joint points, and the
//! bones all come from the same placement, so nothing is inferred. Rendering
//! then moves whole parts (see `parts_render`), so limbs never tear, merge, or
//! leave holes where hidden pixels used to be.
//!
//! Layout on disk, inside the workspace:
//! `.sprite-studio/parts/<slug>/parts.json` plus the part PNGs (agent output),
//! and `assembly.json` plus `layers/*.png` (written here). The assembly is
//! found again from the master image path, and is ignored once the master's
//! bytes no longer match, so a repainted master falls back to the classic rig.

use super::types::{RigBone, RigPoint};
use crate::models::GenerationManifest;
use chrono::Utc;
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    time::SystemTime,
};

pub(crate) const PARTS_DIR: &str = ".sprite-studio/parts";
const MANIFEST_FILE: &str = "parts.json";
const ASSEMBLY_FILE: &str = "assembly.json";
const LAYERS_DIR: &str = "layers";
const MAX_CANVAS: u32 = 512;
/// Brightness applied to a near part reused for a missing far part.
const FAR_SHADE: f32 = 0.78;

/// One part of the fixed biped skeleton.
struct PartSpec {
    part: &'static str,
    /// Near parts to reuse (shaded) when a far part is missing.
    fallback: Option<&'static str>,
    bone: &'static str,
    start: &'static str,
    end: &'static str,
    parent: Option<&'static str>,
    z: i64,
    kind: PartKind,
}

#[derive(Clone, Copy, PartialEq)]
enum PartKind {
    Torso,
    Head,
    UpperArm,
    Forearm,
    Thigh,
    Shin,
}

/// Parents precede children so equal-z children draw over their parents.
const PARTS: &[PartSpec] = &[
    PartSpec { part: "torso", fallback: None, bone: "torso", start: "neck", end: "hip", parent: None, z: 5, kind: PartKind::Torso },
    PartSpec { part: "head", fallback: None, bone: "head", start: "neck", end: "head_top", parent: Some("torso"), z: 6, kind: PartKind::Head },
    PartSpec { part: "upper_arm_far", fallback: Some("upper_arm_near"), bone: "upper_arm_l", start: "shoulder_l", end: "elbow_l", parent: Some("torso"), z: 1, kind: PartKind::UpperArm },
    PartSpec { part: "forearm_far", fallback: Some("forearm_near"), bone: "lower_arm_l", start: "elbow_l", end: "hand_l", parent: Some("upper_arm_l"), z: 1, kind: PartKind::Forearm },
    PartSpec { part: "thigh_far", fallback: Some("thigh_near"), bone: "thigh_l", start: "hip_l", end: "knee_l", parent: Some("torso"), z: 2, kind: PartKind::Thigh },
    PartSpec { part: "shin_far", fallback: Some("shin_near"), bone: "shin_l", start: "knee_l", end: "foot_l", parent: Some("thigh_l"), z: 2, kind: PartKind::Shin },
    PartSpec { part: "thigh_near", fallback: None, bone: "thigh_r", start: "hip_r", end: "knee_r", parent: Some("torso"), z: 7, kind: PartKind::Thigh },
    PartSpec { part: "shin_near", fallback: None, bone: "shin_r", start: "knee_r", end: "foot_r", parent: Some("thigh_r"), z: 7, kind: PartKind::Shin },
    PartSpec { part: "upper_arm_near", fallback: None, bone: "upper_arm_r", start: "shoulder_r", end: "elbow_r", parent: Some("torso"), z: 9, kind: PartKind::UpperArm },
    PartSpec { part: "forearm_near", fallback: None, bone: "lower_arm_r", start: "elbow_r", end: "hand_r", parent: Some("upper_arm_r"), z: 9, kind: PartKind::Forearm },
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartsManifest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    canvas: Option<[u32; 2]>,
    parts: HashMap<String, PartEntry>,
}

#[derive(Debug, Deserialize)]
struct PartEntry {
    file: String,
    #[serde(default)]
    joints: HashMap<String, [f64; 2]>,
}

/// A part image placed on the rest-pose canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LayerPlacement {
    pub bone: String,
    pub file: String,
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssemblyRecord {
    pub version: u32,
    pub master: String,
    pub master_hash: String,
    pub canvas: [u32; 2],
    pub points: Vec<RigPoint>,
    pub bones: Vec<RigBone>,
    pub layers: Vec<LayerPlacement>,
}

/// A decoded part layer positioned on the rest-pose canvas.
pub(crate) struct PartLayer {
    pub bone: String,
    pub image: RgbaImage,
    pub x: i64,
    pub y: i64,
}

pub(crate) struct PartsRig {
    pub points: Vec<RigPoint>,
    pub bones: Vec<RigBone>,
    pub layers: Vec<PartLayer>,
}

pub(crate) struct Assembly {
    pub master: RgbaImage,
    pub rig: PartsRig,
}

#[derive(Debug)]
pub(crate) struct AssemblyOutcome {
    pub name: String,
    pub master_relative: String,
    pub part_count: usize,
}

/// A trimmed part image with joints in its own pixel space.
struct Part {
    image: RgbaImage,
    joints: HashMap<&'static str, (f64, f64)>,
}

fn opaque(pixel: &Rgba<u8>) -> bool {
    pixel[3] > 0
}

/// Crop to opaque pixels plus one transparent pixel of padding, so rotation
/// sampling has a clean edge. Returns the image and the crop offset.
fn trim(image: &RgbaImage) -> Option<(RgbaImage, (i64, i64))> {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (u32::MAX, u32::MAX, 0, 0);
    let mut any = 0usize;
    for (x, y, pixel) in image.enumerate_pixels() {
        if opaque(pixel) {
            any += 1;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if any < 4 {
        return None;
    }
    let width = max_x - min_x + 3;
    let height = max_y - min_y + 3;
    let mut out = RgbaImage::new(width, height);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            out.put_pixel(x - min_x + 1, y - min_y + 1, *image.get_pixel(x, y));
        }
    }
    Some((out, (min_x as i64 - 1, min_y as i64 - 1)))
}

/// Opaque x-span `(min, max + 1)` within a row band.
fn band_span(image: &RgbaImage, rows: std::ops::Range<u32>) -> Option<(f64, f64)> {
    let mut span: Option<(u32, u32)> = None;
    for y in rows {
        for x in 0..image.width() {
            if opaque(image.get_pixel(x, y)) {
                let entry = span.get_or_insert((x, x));
                entry.0 = entry.0.min(x);
                entry.1 = entry.1.max(x);
            }
        }
    }
    span.map(|(min, max)| (min as f64, max as f64 + 1.0))
}

fn opaque_rows(image: &RgbaImage) -> (u32, u32) {
    let rows: Vec<u32> = (0..image.height())
        .filter(|y| (0..image.width()).any(|x| opaque(image.get_pixel(x, *y))))
        .collect();
    (rows[0], rows[rows.len() - 1] + 1)
}

/// Joint positions implied by the drawing convention: limbs hang straight
/// down with the joint nearest the body at the top, and every limb joint sits
/// one cap-radius inside its end so neighbouring parts overlap.
fn default_joints(kind: PartKind, image: &RgbaImage) -> HashMap<&'static str, (f64, f64)> {
    let (top, bottom) = opaque_rows(image);
    let height = (bottom - top) as f64;
    let band = ((height * 0.12).round() as u32).max(2).min(bottom - top);
    let (top_min, top_max) = band_span(image, top..top + band).unwrap_or((0.0, 1.0));
    let (bottom_min, bottom_max) =
        band_span(image, bottom - band..bottom).unwrap_or((0.0, 1.0));
    let top_center = (top_min + top_max) / 2.0;
    let bottom_center = (bottom_min + bottom_max) / 2.0;
    let top_cap = ((top_max - top_min) / 2.0).min(height * 0.25);
    let bottom_cap = ((bottom_max - bottom_min) / 2.0).min(height * 0.25);
    let proximal = (top_center, top as f64 + top_cap);
    let distal = (bottom_center, bottom as f64 - bottom_cap);
    let mut joints = HashMap::new();
    match kind {
        PartKind::Torso => {
            joints.insert("neck", (top_center, top as f64 + 1.0));
            joints.insert("shoulder", (top_center, top as f64 + height * 0.18));
            // Hip sockets sit just inside the bottom edge: any deeper and the
            // torso, drawn over the far leg, hides the top of the far thigh.
            joints.insert("hip", (bottom_center, bottom as f64 - (height * 0.06).max(1.0)));
        }
        PartKind::Head => {
            joints.insert("neck", (bottom_center, bottom as f64 - 1.0));
            joints.insert("top", (top_center, top as f64));
        }
        PartKind::UpperArm => {
            joints.insert("shoulder", proximal);
            joints.insert("elbow", distal);
        }
        PartKind::Forearm => {
            joints.insert("elbow", proximal);
            joints.insert("hand", distal);
        }
        PartKind::Thigh => {
            joints.insert("hip", proximal);
            joints.insert("knee", distal);
        }
        PartKind::Shin => {
            joints.insert("knee", proximal);
            // The foot point is the planted contact, so it sits on the sole.
            joints.insert("foot", (bottom_center, bottom as f64 - 0.5));
        }
    }
    joints
}

fn joint_names(kind: PartKind) -> &'static [&'static str] {
    match kind {
        PartKind::Torso => &["neck", "shoulder", "hip"],
        PartKind::Head => &["neck", "top"],
        PartKind::UpperArm => &["shoulder", "elbow"],
        PartKind::Forearm => &["elbow", "hand"],
        PartKind::Thigh => &["hip", "knee"],
        PartKind::Shin => &["knee", "foot"],
    }
}

fn shade(image: &RgbaImage, factor: f32) -> RgbaImage {
    let mut out = image.clone();
    for pixel in out.pixels_mut() {
        for channel in 0..3 {
            pixel[channel] = (pixel[channel] as f32 * factor).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// A relative file path that cannot leave the parts directory.
fn confined(dir: &Path, file: &str) -> Result<PathBuf, String> {
    let relative = Path::new(file);
    if file.trim().is_empty()
        || !relative.components().all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(format!("part file `{file}` must be a plain path inside the parts folder"));
    }
    let full = dir.join(relative);
    let root = dir.canonicalize().map_err(|error| error.to_string())?;
    let resolved = full
        .canonicalize()
        .map_err(|_| format!("part file `{file}` is missing"))?;
    if !resolved.starts_with(&root) || !resolved.is_file() {
        return Err(format!("part file `{file}` escapes the parts folder"));
    }
    Ok(resolved)
}

fn load_part(dir: &Path, spec: &PartSpec, entry: &PartEntry) -> Result<Part, String> {
    let path = confined(dir, &entry.file)?;
    let decoded = image::open(&path)
        .map_err(|error| format!("part `{}` is not a readable PNG: {error}", spec.part))?
        .to_rgba8();
    let (image, (offset_x, offset_y)) = trim(&decoded)
        .ok_or_else(|| format!("part `{}` has no visible pixels", spec.part))?;
    if image.width() > MAX_CANVAS || image.height() > MAX_CANVAS {
        return Err(format!(
            "part `{}` is {}×{} pixels; draw parts at final sprite scale",
            spec.part,
            image.width(),
            image.height()
        ));
    }
    let mut joints = default_joints(spec.kind, &image);
    for name in joint_names(spec.kind) {
        if let Some([x, y]) = entry.joints.get(*name) {
            if !(x.is_finite() && y.is_finite()) {
                return Err(format!("part `{}` joint `{name}` is not a number", spec.part));
            }
            joints.insert(*name, (x - offset_x as f64, y - offset_y as f64));
        }
    }
    Ok(Part { image, joints })
}

fn load_parts(dir: &Path, manifest: &PartsManifest) -> Result<HashMap<&'static str, Part>, String> {
    let mut parts = HashMap::new();
    for spec in PARTS {
        if let Some(entry) = manifest.parts.get(spec.part) {
            parts.insert(spec.part, load_part(dir, spec, entry)?);
        }
    }
    for spec in PARTS {
        if parts.contains_key(spec.part) {
            continue;
        }
        let reused = spec.fallback.and_then(|near| parts.get(near)).map(|near| Part {
            image: shade(&near.image, FAR_SHADE),
            joints: near.joints.clone(),
        });
        match reused {
            Some(part) => {
                parts.insert(spec.part, part);
            }
            None => return Err(format!("the parts sheet is missing `{}`", spec.part)),
        }
    }
    Ok(parts)
}

fn add(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 + b.0, a.1 + b.1)
}

fn sub(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 - b.0, a.1 - b.1)
}

fn round_offset(value: (f64, f64)) -> (i64, i64) {
    (value.0.round() as i64, value.1.round() as i64)
}

/// Assemble a parts folder into an exact rest pose. `requested` is the canvas
/// the user asked for; a larger one is chosen when the figure would not fit.
pub(crate) fn assemble(dir: &Path, requested: Option<(u32, u32)>) -> Result<Assembly, String> {
    let text = std::fs::read_to_string(dir.join(MANIFEST_FILE))
        .map_err(|_| "the parts folder has no parts.json".to_string())?;
    let manifest: PartsManifest =
        serde_json::from_str(&text).map_err(|error| format!("parts.json is invalid: {error}"))?;
    let parts = load_parts(dir, &manifest)?;
    let joint = |part: &str, name: &str| parts[part].joints[name];

    // Figure space: the hip joint is the origin and every limb hangs straight
    // down. Offsets are rounded so part pixels stay on the pixel grid, and each
    // joint is read back through its rounded offset so points match the pixels.
    let mut offsets: HashMap<&'static str, (i64, i64)> = HashMap::new();
    let mut points: HashMap<&'static str, (f64, f64)> = HashMap::new();
    let place = |offsets: &mut HashMap<&'static str, (i64, i64)>, part: &'static str, anchor: (f64, f64), local: (f64, f64)| {
        let offset = round_offset(sub(anchor, local));
        offsets.insert(part, offset);
        (offset.0 as f64, offset.1 as f64)
    };
    let torso = place(&mut offsets, "torso", (0.0, 0.0), joint("torso", "hip"));
    let neck = add(joint("torso", "neck"), torso);
    let shoulder = add(joint("torso", "shoulder"), torso);
    let hip = add(joint("torso", "hip"), torso);
    points.insert("neck", neck);
    points.insert("hip", hip);
    let head = place(&mut offsets, "head", neck, joint("head", "neck"));
    points.insert("head_top", add(joint("head", "top"), head));
    for (side, upper, fore) in [("l", "upper_arm_far", "forearm_far"), ("r", "upper_arm_near", "forearm_near")] {
        let upper_offset = place(&mut offsets, upper, shoulder, joint(upper, "shoulder"));
        let elbow = add(joint(upper, "elbow"), upper_offset);
        let fore_offset = place(&mut offsets, fore, elbow, joint(fore, "elbow"));
        points.insert(if side == "l" { "shoulder_l" } else { "shoulder_r" }, add(joint(upper, "shoulder"), upper_offset));
        points.insert(if side == "l" { "elbow_l" } else { "elbow_r" }, elbow);
        points.insert(if side == "l" { "hand_l" } else { "hand_r" }, add(joint(fore, "hand"), fore_offset));
    }
    for (side, thigh, shin) in [("l", "thigh_far", "shin_far"), ("r", "thigh_near", "shin_near")] {
        let thigh_offset = place(&mut offsets, thigh, hip, joint(thigh, "hip"));
        let knee = add(joint(thigh, "knee"), thigh_offset);
        let shin_offset = place(&mut offsets, shin, knee, joint(shin, "knee"));
        points.insert(if side == "l" { "hip_l" } else { "hip_r" }, add(joint(thigh, "hip"), thigh_offset));
        points.insert(if side == "l" { "knee_l" } else { "knee_r" }, knee);
        points.insert(if side == "l" { "foot_l" } else { "foot_r" }, add(joint(shin, "foot"), shin_offset));
    }

    // Union bounds of every placed part (inclusive min, exclusive max).
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
    for spec in PARTS {
        let (x, y) = offsets[spec.part];
        let image = &parts[spec.part].image;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + image.width() as i64);
        max_y = max_y.max(y + image.height() as i64);
    }
    let figure_height = (max_y - min_y) as f64;
    let figure_width = (max_x - min_x) as f64;
    // Running and walking swing limbs out to roughly one leg length on each
    // side, so the canvas needs horizontal room as well as headroom.
    let leg = points["foot_r"].1 - points["hip"].1;
    let needed_width = figure_width.max(leg * 2.4) + 4.0;
    let needed_height = figure_height / 0.86;
    let fits = |(width, height): (u32, u32)| width as f64 >= needed_width && height as f64 >= needed_height;
    let canvas = requested
        .or(manifest.canvas.map(|[width, height]| (width, height)))
        .filter(|size| fits(*size))
        .unwrap_or_else(|| {
            let side = (needed_width.max(needed_height) / 8.0).ceil() as u32 * 8;
            (side, side)
        });
    if canvas.0 > MAX_CANVAS || canvas.1 > MAX_CANVAS || canvas.0 < 8 || canvas.1 < 8 {
        return Err(format!(
            "the assembled figure needs a {}×{} canvas; draw the parts at final sprite scale",
            canvas.0, canvas.1
        ));
    }
    // Feet on a ground line near the bottom, hip centred horizontally.
    let margin = ((canvas.1 as f64) * 0.04).round().max(1.0) as i64;
    let shift = (
        (canvas.0 as f64 / 2.0).round() as i64 - hip.0.round() as i64,
        canvas.1 as i64 - margin - max_y,
    );
    let shift_f = (shift.0 as f64, shift.1 as f64);

    let mut master = RgbaImage::new(canvas.0, canvas.1);
    let mut layers = Vec::new();
    let mut order: Vec<usize> = (0..PARTS.len()).collect();
    order.sort_by_key(|index| (PARTS[*index].z, *index));
    for index in order {
        let spec = &PARTS[index];
        let (x, y) = offsets[spec.part];
        let (x, y) = (x + shift.0, y + shift.1);
        let image = &parts[spec.part].image;
        for (px, py, pixel) in image.enumerate_pixels() {
            let (cx, cy) = (x + px as i64, y + py as i64);
            if opaque(pixel) && cx >= 0 && cy >= 0 && cx < canvas.0 as i64 && cy < canvas.1 as i64 {
                let existing = *master.get_pixel(cx as u32, cy as u32);
                master.put_pixel(cx as u32, cy as u32, super::skin::blend_rgba(existing, *pixel));
            }
        }
        layers.push((index, x, y));
    }
    let point_kind = |name: &str| match name {
        "head_top" | "hand_l" | "hand_r" => "anchor",
        "foot_l" | "foot_r" => "contact",
        _ => "joint",
    };
    let point_order = [
        "head_top", "neck", "shoulder_l", "shoulder_r", "elbow_l", "elbow_r", "hand_l", "hand_r",
        "hip", "hip_l", "hip_r", "knee_l", "knee_r", "foot_l", "foot_r",
    ];
    let rig_points = point_order
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let (x, y) = add(points[name], shift_f);
            RigPoint {
                id: format!("p{}", index + 1),
                name: name.to_string(),
                kind: point_kind(name).to_string(),
                x,
                y,
                confidence: 1.0,
                source: "parts".to_string(),
                note: None,
            }
        })
        .collect();
    let bones = PARTS
        .iter()
        .enumerate()
        .map(|(index, spec)| RigBone {
            id: format!("b{}", index + 1),
            name: spec.bone.to_string(),
            start_point: spec.start.to_string(),
            end_point: spec.end.to_string(),
            radius: (parts[spec.part].image.width() as f64 / 2.0).max(1.0),
            parent: spec.parent.map(str::to_string),
            z: spec.z,
        })
        .collect();
    let layers = layers
        .into_iter()
        .map(|(index, x, y)| PartLayer {
            bone: PARTS[index].bone.to_string(),
            image: parts[PARTS[index].part].image.clone(),
            x,
            y,
        })
        .collect();
    Ok(Assembly {
        master,
        rig: PartsRig { points: rig_points, bones, layers },
    })
}

fn master_hash(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|bytes| blake3::hash(&bytes).to_hex().to_string())
}

/// Save the layers and the assembly record next to the parts.
fn write_assembly(dir: &Path, assembly: &Assembly, master_relative: &str, master_path: &Path) -> Result<(), String> {
    let layers_dir = dir.join(LAYERS_DIR);
    std::fs::create_dir_all(&layers_dir).map_err(|error| error.to_string())?;
    let mut placements = Vec::new();
    for layer in &assembly.rig.layers {
        let file = format!("{LAYERS_DIR}/{}.png", layer.bone);
        layer.image.save(dir.join(&file)).map_err(|error| error.to_string())?;
        placements.push(LayerPlacement { bone: layer.bone.clone(), file, x: layer.x, y: layer.y });
    }
    let record = AssemblyRecord {
        version: 1,
        master: master_relative.to_string(),
        master_hash: master_hash(master_path).ok_or("the assembled master could not be read")?,
        canvas: [assembly.master.width(), assembly.master.height()],
        points: assembly.rig.points.clone(),
        bones: assembly.rig.bones.clone(),
        layers: placements,
    };
    let json = serde_json::to_vec_pretty(&record).map_err(|error| error.to_string())?;
    std::fs::write(dir.join(ASSEMBLY_FILE), json).map_err(|error| error.to_string())
}

fn slug(value: &str) -> String {
    let slug: String = value
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() { character.to_ascii_lowercase() } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() { "character".to_string() } else { slug }
}

/// The newest parts folder whose parts.json was written since `since`.
fn fresh_parts_dir(workspace: &Path, since: SystemTime) -> Option<PathBuf> {
    let threshold = since.checked_sub(std::time::Duration::from_secs(2)).unwrap_or(since);
    std::fs::read_dir(workspace.join(PARTS_DIR))
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let dir = entry.path();
            let modified = std::fs::symlink_metadata(dir.join(MANIFEST_FILE)).ok()?.modified().ok()?;
            (entry.file_type().ok()?.is_dir() && modified >= threshold).then_some((modified, dir))
        })
        .max()
        .map(|(_, dir)| dir)
}

/// After a master-only provider pass: assemble the parts the agent saved for
/// this request, write the master and a fresh generation manifest, and record
/// the assembly. `Ok(None)` means the agent delivered no parts this time.
pub(crate) fn assemble_fresh_parts(
    workspace: &Path,
    since: SystemTime,
    requested: Option<(u32, u32)>,
) -> Result<Option<AssemblyOutcome>, String> {
    let Some(dir) = fresh_parts_dir(workspace, since) else {
        return Ok(None);
    };
    let assembly = assemble(&dir, requested)?;
    let folder = dir.file_name().and_then(|name| name.to_str()).unwrap_or("character");
    let manifest_name = std::fs::read_to_string(dir.join(MANIFEST_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<PartsManifest>(&text).ok())
        .and_then(|manifest| manifest.name)
        .unwrap_or_else(|| folder.to_string());
    let name = slug(&manifest_name);
    // The master is named after the parts folder so it can be found again.
    let master_relative = format!("assets/characters/{}.png", slug(folder));
    let master_path = workspace.join(&master_relative);
    std::fs::create_dir_all(workspace.join("assets/characters")).map_err(|error| error.to_string())?;
    assembly.master.save(&master_path).map_err(|error| error.to_string())?;
    write_assembly(&dir, &assembly, &master_relative, &master_path)?;
    let manifest = GenerationManifest {
        kind: Some("sprite".to_string()),
        name: name.clone(),
        category: "characters".to_string(),
        fps: 1.0,
        files: vec![master_relative.clone()],
        generated_at: Utc::now().to_rfc3339(),
        rig: None,
        rig_id: None,
        source: Some(master_relative.clone()),
        quality: None,
        direction_family: None,
        facing: None,
        mirrored_from: None,
        anchor_slug: None,
    };
    let json = serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?;
    std::fs::write(workspace.join(".sprite-studio/last-generation.json"), json)
        .map_err(|error| error.to_string())?;
    Ok(Some(AssemblyOutcome {
        name,
        master_relative,
        part_count: PARTS.len(),
    }))
}

fn workspace_root(master_path: &Path) -> Option<PathBuf> {
    master_path
        .ancestors()
        .skip(1)
        .find(|dir| dir.join(".sprite-studio").is_dir())
        .map(Path::to_path_buf)
}

/// The parts rig assembled for this exact master image, if any.
pub(crate) fn load_for_master(master_path: &Path) -> Option<PartsRig> {
    let workspace = workspace_root(master_path)?;
    let relative = master_path.strip_prefix(&workspace).ok()?.to_string_lossy().replace('\\', "/");
    let stem = master_path.file_stem()?.to_str()?;
    let dir = workspace.join(PARTS_DIR).join(stem);
    let record: AssemblyRecord =
        serde_json::from_slice(&std::fs::read(dir.join(ASSEMBLY_FILE)).ok()?).ok()?;
    if record.master != relative {
        return None;
    }
    let mut layers = Vec::new();
    for placement in &record.layers {
        let path = confined(&dir, &placement.file).ok()?;
        layers.push(PartLayer {
            bone: placement.bone.clone(),
            image: image::open(path).ok()?.to_rgba8(),
            x: placement.x,
            y: placement.y,
        });
    }
    // Registration re-saves new sprites after alpha and palette cleanup, so
    // identical bytes are only the fast path; otherwise the master must still
    // look like its own parts.
    if Some(record.master_hash.as_str()) != master_hash(master_path).as_deref() {
        let master = image::open(master_path).ok()?.to_rgba8();
        if master.dimensions() != (record.canvas[0], record.canvas[1]) || !resembles_parts(&master, &layers) {
            return None;
        }
    }
    Some(PartsRig { points: record.points, bones: record.bones, layers })
}

/// Layers are stored in draw order, so compositing them rebuilds the rest pose.
fn compose(size: (u32, u32), layers: &[PartLayer]) -> RgbaImage {
    let mut canvas = RgbaImage::new(size.0, size.1);
    for layer in layers {
        for (px, py, pixel) in layer.image.enumerate_pixels() {
            let (x, y) = (layer.x + px as i64, layer.y + py as i64);
            if opaque(pixel) && x >= 0 && y >= 0 && x < size.0 as i64 && y < size.1 as i64 {
                let existing = *canvas.get_pixel(x as u32, y as u32);
                canvas.put_pixel(x as u32, y as u32, super::skin::blend_rgba(existing, *pixel));
            }
        }
    }
    canvas
}

/// True when at most 5% of the figure changed: cleanup only nudges edge alpha
/// and snaps colours, while a repaint changes far more.
fn resembles_parts(master: &RgbaImage, layers: &[PartLayer]) -> bool {
    let rest = compose(master.dimensions(), layers);
    let (mut figure, mut changed) = (0usize, 0usize);
    for (current, original) in master.pixels().zip(rest.pixels()) {
        let (a, b) = (opaque(current), opaque(original));
        if !(a || b) {
            continue;
        }
        figure += 1;
        let recoloured = a && b && (0..3).any(|channel| current[channel].abs_diff(original[channel]) > 48);
        if a != b || recoloured {
            changed += 1;
        }
    }
    figure > 0 && changed * 20 <= figure
}
