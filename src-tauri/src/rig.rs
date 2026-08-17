use crate::{
    animations::save_animation_inner,
    assets,
    error::{CommandError, CommandResult},
    models::{AnimationFrame, GenerationManifest},
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use image::RgbaImage;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};
use tauri::State;
use uuid::Uuid;

pub const MORPHOLOGIES: [&str; 6] = [
    "biped",
    "quadruped",
    "winged",
    "serpentine",
    "object",
    "amorphous",
];

pub(crate) fn normalize_morphology(value: Option<&str>) -> String {
    let lowered = value.unwrap_or("biped").trim().to_ascii_lowercase();
    if MORPHOLOGIES.contains(&lowered.as_str()) {
        lowered
    } else {
        "biped".to_string()
    }
}

fn default_one() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigPoint {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub confidence: f64,
    pub source: String,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigBone {
    pub id: String,
    pub name: String,
    pub start_point: String,
    pub end_point: String,
    pub radius: f64,
    #[serde(default)]
    pub parent: Option<String>,
    pub z: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigTransform {
    pub bone: String,
    #[serde(default)]
    pub dx: f64,
    #[serde(default)]
    pub dy: f64,
    #[serde(default)]
    pub rotate: f64,
    #[serde(default = "default_one")]
    pub scale_x: f64,
    #[serde(default = "default_one")]
    pub scale_y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigContact {
    pub bone: String,
    pub x: f64,
    pub y: f64,
    #[serde(default = "default_one")]
    pub bend: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigFrame {
    #[serde(default)]
    pub phase: Option<String>,
    #[serde(default)]
    pub hold: bool,
    #[serde(default)]
    pub root_dx: f64,
    #[serde(default)]
    pub root_dy: f64,
    #[serde(default)]
    pub transforms: Vec<RigTransform>,
    #[serde(default)]
    pub contacts: Vec<RigContact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RigSpec {
    #[serde(default)]
    points: Vec<RigPoint>,
    #[serde(default)]
    bones: Vec<RigBone>,
    #[serde(default)]
    frames: Vec<RigFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rig {
    pub id: String,
    pub workspace_id: String,
    pub worktree_id: Option<String>,
    pub asset_id: Option<String>,
    pub name: String,
    pub morphology: String,
    pub fps: f64,
    pub looping: bool,
    pub points: Vec<RigPoint>,
    pub bones: Vec<RigBone>,
    pub frames: Vec<RigFrame>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigInput {
    pub id: Option<String>,
    pub workspace_id: String,
    pub worktree_id: Option<String>,
    pub asset_id: Option<String>,
    pub name: String,
    pub morphology: String,
    pub fps: f64,
    pub looping: bool,
    #[serde(default)]
    pub points: Vec<RigPoint>,
    #[serde(default)]
    pub bones: Vec<RigBone>,
    #[serde(default)]
    pub frames: Vec<RigFrame>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RigSuggestion {
    pub morphology: String,
    pub points: Vec<RigPoint>,
    pub bones: Vec<RigBone>,
    pub frames: Vec<RigFrame>,
    pub reasoning: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RigRenderResult {
    pub animation: crate::models::Animation,
    pub frame_paths: Vec<String>,
    pub asset_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// Affine transform (canvas-style row-major: x' = a*x + c*y + e, y' = b*x + d*y + f)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub(crate) struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    pub(crate) fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    pub(crate) fn translation(dx: f64, dy: f64) -> Self {
        Self {
            e: dx,
            f: dy,
            ..Self::identity()
        }
    }

    pub(crate) fn scaling(sx: f64, sy: f64) -> Self {
        Self {
            a: sx,
            d: sy,
            ..Self::identity()
        }
    }

    pub(crate) fn rotation_degrees(degrees: f64) -> Self {
        let radians = degrees.to_radians();
        let (sin, cos) = radians.sin_cos();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        }
    }

    /// self followed by next (next ∘ self).
    pub(crate) fn chain(&self, next: &Affine) -> Affine {
        Affine {
            a: next.a * self.a + next.c * self.b,
            b: next.b * self.a + next.d * self.b,
            c: next.a * self.c + next.c * self.d,
            d: next.b * self.c + next.d * self.d,
            e: next.a * self.e + next.c * self.f + next.e,
            f: next.b * self.e + next.d * self.f + next.f,
        }
    }

    pub(crate) fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    pub(crate) fn inverse(&self) -> Affine {
        let determinant = self.a * self.d - self.b * self.c;
        if determinant.abs() < f64::EPSILON {
            return Affine::identity();
        }
        let inverse = 1.0 / determinant;
        Affine {
            a: self.d * inverse,
            b: -self.b * inverse,
            c: -self.c * inverse,
            d: self.a * inverse,
            e: (self.c * self.f - self.d * self.e) * inverse,
            f: (self.b * self.e - self.a * self.f) * inverse,
        }
    }
}

fn local_bone_affine(start: (f64, f64), transform: &RigTransform) -> Affine {
    Affine::translation(-start.0, -start.1)
        .chain(&Affine::scaling(transform.scale_x, transform.scale_y))
        .chain(&Affine::rotation_degrees(transform.rotate))
        .chain(&Affine::translation(start.0, start.1))
        .chain(&Affine::translation(transform.dx, transform.dy))
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

pub(crate) fn point_segment_distance(
    px: f64,
    py: f64,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
) -> f64 {
    let dx = bx - ax;
    let dy = by - ay;
    let length_sq = dx * dx + dy * dy;
    if length_sq <= f64::EPSILON {
        ((px - ax).powi(2) + (py - ay).powi(2)).sqrt()
    } else {
        let t = (((px - ax) * dx + (py - ay) * dy) / length_sq).clamp(0.0, 1.0);
        let cx = ax + t * dx;
        let cy = ay + t * dy;
        ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
    }
}

pub(crate) fn point_positions(rig: &Rig, width: u32, height: u32) -> HashMap<String, (f64, f64)> {
    let mut positions = HashMap::new();
    for point in &rig.points {
        positions.insert(
            point.name.clone(),
            (
                point.x.clamp(0.0, (width.saturating_sub(1)) as f64),
                point.y.clamp(0.0, (height.saturating_sub(1)) as f64),
            ),
        );
    }
    positions
}

fn better_candidate(current: Option<(f64, i64, usize)>, candidate: (f64, i64, usize)) -> bool {
    match current {
        None => true,
        Some((distance, z, _)) => {
            candidate.0 < distance || (candidate.0 == distance && candidate.1 < z)
        }
    }
}

struct Capsule {
    start: (f64, f64),
    end: (f64, f64),
    radius: f64,
    z: i64,
}

/// Assign every opaque master pixel to exactly one bone. Pixels inside a bone
/// capsule go to the closest capsule; leftover pixels fall to the nearest bone
/// by segment distance so the rig always re-renders the whole silhouette.
pub(crate) fn build_ownership(
    master: &RgbaImage,
    rig: &Rig,
    positions: &HashMap<String, (f64, f64)>,
) -> Vec<i16> {
    let width = master.width() as usize;
    let height = master.height() as usize;
    let mut assignment = vec![-1i16; width * height];
    if rig.bones.is_empty() {
        return assignment;
    }
    let capsules: Vec<Capsule> = rig
        .bones
        .iter()
        .map(|bone| Capsule {
            start: positions
                .get(&bone.start_point)
                .copied()
                .unwrap_or((0.0, 0.0)),
            end: positions
                .get(&bone.end_point)
                .copied()
                .unwrap_or((0.0, 0.0)),
            radius: bone.radius.max(0.5),
            z: bone.z,
        })
        .collect();
    for y in 0..height {
        for x in 0..width {
            if master.get_pixel(x as u32, y as u32)[3] == 0 {
                continue;
            }
            let px = x as f64 + 0.5;
            let py = y as f64 + 0.5;
            let mut best_capped: Option<(f64, i64, usize)> = None;
            let mut best_any: Option<(f64, i64, usize)> = None;
            for (index, capsule) in capsules.iter().enumerate() {
                let distance = point_segment_distance(
                    px,
                    py,
                    capsule.start.0,
                    capsule.start.1,
                    capsule.end.0,
                    capsule.end.1,
                );
                let candidate = (distance, capsule.z, index);
                if distance <= capsule.radius && better_candidate(best_capped, candidate) {
                    best_capped = Some(candidate);
                }
                if better_candidate(best_any, candidate) {
                    best_any = Some(candidate);
                }
            }
            if let Some((_, _, index)) = best_capped.or(best_any) {
                assignment[y * width + x] = index as i16;
            }
        }
    }
    assignment
}

/// Two-bone analytic IK. Rotates the contact bone's chain so the bone's end
/// point lands on the canvas-space target, with `bend` choosing the elbow or
/// knee direction. Returns rotation deltas in degrees for (parent, bone).
fn solve_contact_ik(
    bones: &[RigBone],
    bone_index: usize,
    positions: &HashMap<String, (f64, f64)>,
    target: (f64, f64),
    bend: f64,
) -> (f64, f64) {
    let bone = &bones[bone_index];
    let joint = positions.get(&bone.start_point).copied().unwrap_or((0.0, 0.0));
    let end = positions.get(&bone.end_point).copied().unwrap_or((0.0, 0.0));
    let l2 = ((end.0 - joint.0).powi(2) + (end.1 - joint.1).powi(2)).sqrt();
    if l2 <= f64::EPSILON {
        return (0.0, 0.0);
    }
    let child_angle = (end.1 - joint.1).atan2(end.0 - joint.0);
    let Some(parent_index) = bones
        .iter()
        .position(|candidate| Some(candidate.name.clone()) == bone.parent)
    else {
        // Single-bone chain: point the bone straight at the target.
        let target_angle = (target.1 - joint.1).atan2(target.0 - joint.0);
        return (0.0, (target_angle - child_angle).to_degrees());
    };
    let parent = &bones[parent_index];
    let parent_start = positions
        .get(&parent.start_point)
        .copied()
        .unwrap_or((0.0, 0.0));
    let l1 = ((joint.0 - parent_start.0).powi(2) + (joint.1 - parent_start.1).powi(2)).sqrt();
    if l1 <= f64::EPSILON {
        return (0.0, 0.0);
    }
    let parent_angle = (joint.1 - parent_start.1).atan2(joint.0 - parent_start.0);
    let reach = ((target.0 - parent_start.0).powi(2) + (target.1 - parent_start.1).powi(2)).sqrt();
    let reach = reach.clamp((l1 - l2).abs() + 1e-3, l1 + l2 - 1e-3);
    let beta = ((l1 * l1 + reach * reach - l2 * l2) / (2.0 * l1 * reach))
        .clamp(-1.0, 1.0)
        .acos();
    let gamma = ((l1 * l1 + l2 * l2 - reach * reach) / (2.0 * l1 * l2))
        .clamp(-1.0, 1.0)
        .acos();
    let target_angle = (target.1 - parent_start.1).atan2(target.0 - parent_start.0);
    let bend = if bend >= 0.0 { 1.0 } else { -1.0 };
    let parent_delta = target_angle + bend * beta - parent_angle;
    let child_delta = bend * (std::f64::consts::PI - gamma) + parent_angle - child_angle;
    (parent_delta.to_degrees(), child_delta.to_degrees())
}

fn identity_transform(bone: &str) -> RigTransform {
    RigTransform {
        bone: bone.to_string(),
        dx: 0.0,
        dy: 0.0,
        rotate: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
    }
}

fn bone_world_affine(
    index: usize,
    bones: &[RigBone],
    positions: &HashMap<String, (f64, f64)>,
    transforms: &HashMap<String, RigTransform>,
    cache: &mut HashMap<usize, Affine>,
    depth: usize,
) -> Affine {
    if let Some(cached) = cache.get(&index) {
        return *cached;
    }
    // Cycle guard: a malformed parent loop renders each bone with identity
    // ancestors instead of recursing forever.
    if depth > bones.len() {
        return Affine::identity();
    }
    let bone = &bones[index];
    let transform = transforms
        .get(&bone.name)
        .cloned()
        .unwrap_or_else(|| identity_transform(&bone.name));
    let start = positions
        .get(&bone.start_point)
        .copied()
        .unwrap_or((0.0, 0.0));
    let local = local_bone_affine(start, &transform);
    let world = match bones
        .iter()
        .position(|candidate| Some(candidate.name.clone()) == bone.parent)
    {
        Some(parent_index) if parent_index != index => local.chain(&bone_world_affine(
            parent_index,
            bones,
            positions,
            transforms,
            cache,
            depth + 1,
        )),
        _ => local,
    };
    cache.insert(index, world);
    world
}

type OwnedBounds = Vec<Option<((i64, i64), (i64, i64))>>;

#[derive(Debug, Clone)]
struct MeshVertex {
    source: (f64, f64),
    influences: Vec<(usize, f64)>,
}

#[derive(Debug, Clone, Copy)]
struct MeshTriangle {
    vertices: [usize; 3],
    z: f64,
}

#[derive(Debug, Clone)]
struct DeformMesh {
    vertices: Vec<MeshVertex>,
    triangles: Vec<MeshTriangle>,
}

fn is_core_bone(name: &str) -> bool {
    name.split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .any(|token| {
            matches!(
                token.to_ascii_lowercase().as_str(),
                "body" | "torso" | "pelvis" | "spine" | "chest" | "hip" | "segment"
            )
        })
}

/// Derive smooth, normalized bone weights from the bind-pose capsules. The
/// four strongest influences are retained so the result is stable and cheap
/// to evaluate. Core bones receive a small bias so torso/spine transforms
/// deform the silhouette instead of being visually swallowed by nearby limbs.
fn vertex_influences(
    x: f64,
    y: f64,
    rig: &Rig,
    positions: &HashMap<String, (f64, f64)>,
) -> Vec<(usize, f64)> {
    let mut weights: Vec<(usize, f64)> = rig
        .bones
        .iter()
        .enumerate()
        .map(|(index, bone)| {
            let start = positions
                .get(&bone.start_point)
                .copied()
                .unwrap_or((0.0, 0.0));
            let end = positions
                .get(&bone.end_point)
                .copied()
                .unwrap_or((0.0, 0.0));
            let normalized = point_segment_distance(x, y, start.0, start.1, end.0, end.1)
                / bone.radius.max(0.5);
            let core_bias = if is_core_bone(&bone.name) { 1.35 } else { 1.0 };
            // Inverse-square falloff remains well-defined outside every
            // capsule, which guarantees complete coverage of a flat sprite.
            (index, core_bias / (0.35 + normalized).powi(2))
        })
        .collect();
    weights.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    weights.truncate(4);
    let total: f64 = weights.iter().map(|(_, weight)| *weight).sum();
    if total <= f64::EPSILON {
        return vec![(0, 1.0)];
    }
    for (_, weight) in &mut weights {
        *weight /= total;
    }
    weights
}

/// Build a deterministic adaptive grid over the opaque silhouette. This is a
/// deliberately compact first-stage mesh: cells that contain no source alpha
/// are omitted, while exact image boundaries are retained. No random sampling
/// means identical inputs always produce identical topology and pixels.
fn build_deform_mesh(
    master: &RgbaImage,
    rig: &Rig,
    positions: &HashMap<String, (f64, f64)>,
) -> DeformMesh {
    let width = master.width() as usize;
    let height = master.height() as usize;
    let longest = width.max(height);
    let spacing = if longest <= 64 {
        2
    } else if longest <= 128 {
        4
    } else if longest <= 256 {
        6
    } else {
        8
    };
    let axis = |length: usize| {
        let mut values: Vec<usize> = (0..length).step_by(spacing).collect();
        if values.last().copied() != Some(length) {
            values.push(length);
        }
        values
    };
    let xs = axis(width);
    let ys = axis(height);
    let mut vertices = Vec::with_capacity(xs.len() * ys.len());
    for &y in &ys {
        for &x in &xs {
            vertices.push(MeshVertex {
                source: (x as f64, y as f64),
                influences: vertex_influences(x as f64, y as f64, rig, positions),
            });
        }
    }
    let row_width = xs.len();
    let mut triangles = Vec::new();
    for row in 0..ys.len().saturating_sub(1) {
        for column in 0..xs.len().saturating_sub(1) {
            let has_alpha = (ys[row]..ys[row + 1]).any(|y| {
                (xs[column]..xs[column + 1])
                    .any(|x| master.get_pixel(x as u32, y as u32)[3] > 0)
            });
            if !has_alpha {
                continue;
            }
            let top_left = row * row_width + column;
            let top_right = top_left + 1;
            let bottom_left = top_left + row_width;
            let bottom_right = bottom_left + 1;
            for indices in [
                [top_left, top_right, bottom_right],
                [top_left, bottom_right, bottom_left],
            ] {
                let z = indices
                    .iter()
                    .map(|vertex| {
                        vertices[*vertex]
                            .influences
                            .iter()
                            .map(|(bone, weight)| rig.bones[*bone].z as f64 * weight)
                            .sum::<f64>()
                    })
                    .sum::<f64>()
                    / 3.0;
                triangles.push(MeshTriangle { vertices: indices, z });
            }
        }
    }
    triangles.sort_by(|left, right| left.z.total_cmp(&right.z));
    DeformMesh { vertices, triangles }
}

fn edge(a: (f64, f64), b: (f64, f64), point: (f64, f64)) -> f64 {
    (point.0 - a.0) * (b.1 - a.1) - (point.1 - a.1) * (b.0 - a.0)
}

fn render_deform_mesh(
    master: &RgbaImage,
    mesh: &DeformMesh,
    worlds: &[Affine],
    root: Affine,
) -> RgbaImage {
    let width = master.width();
    let height = master.height();
    let deformed: Vec<(f64, f64)> = mesh
        .vertices
        .iter()
        .map(|vertex| {
            let mut x = 0.0;
            let mut y = 0.0;
            for (bone, weight) in &vertex.influences {
                let transformed = worlds[*bone].apply(vertex.source.0, vertex.source.1);
                x += transformed.0 * weight;
                y += transformed.1 * weight;
            }
            root.apply(x, y)
        })
        .collect();
    let mut canvas = RgbaImage::new(width, height);
    for triangle in &mesh.triangles {
        let [i0, i1, i2] = triangle.vertices;
        let destination = [deformed[i0], deformed[i1], deformed[i2]];
        let source = [
            mesh.vertices[i0].source,
            mesh.vertices[i1].source,
            mesh.vertices[i2].source,
        ];
        let area = edge(destination[0], destination[1], destination[2]);
        if area.abs() < 1e-6 {
            continue;
        }
        let min_x = destination
            .iter()
            .map(|point| point.0)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0) as i64;
        let min_y = destination
            .iter()
            .map(|point| point.1)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0) as i64;
        let max_x = destination
            .iter()
            .map(|point| point.0)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .min(width.saturating_sub(1) as f64) as i64;
        let max_y = destination
            .iter()
            .map(|point| point.1)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .min(height.saturating_sub(1) as f64) as i64;
        if min_x > max_x || min_y > max_y {
            continue;
        }
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let sample = (x as f64 + 0.5, y as f64 + 0.5);
                let w0 = edge(destination[1], destination[2], sample) / area;
                let w1 = edge(destination[2], destination[0], sample) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-7 || w1 < -1e-7 || w2 < -1e-7 {
                    continue;
                }
                let source_x = (source[0].0 * w0 + source[1].0 * w1 + source[2].0 * w2)
                    .floor()
                    .clamp(0.0, width.saturating_sub(1) as f64) as u32;
                let source_y = (source[0].1 * w0 + source[1].1 * w1 + source[2].1 * w2)
                    .floor()
                    .clamp(0.0, height.saturating_sub(1) as f64) as u32;
                let pixel = *master.get_pixel(source_x, source_y);
                if pixel[3] > 0 {
                    canvas.put_pixel(x as u32, y as u32, pixel);
                }
            }
        }
    }
    canvas
}

/// Per-bone bounding box of owned source pixels, used as the render sweep
/// region so residual pixels outside the capsule are never dropped.
pub(crate) fn owned_bounds(
    ownership: &[i16],
    bone_count: usize,
    width: usize,
) -> OwnedBounds {
    let mut bounds = vec![None; bone_count];
    for (index, owner) in ownership.iter().enumerate() {
        if *owner < 0 {
            continue;
        }
        let owner = *owner as usize;
        if owner >= bounds.len() {
            continue;
        }
        let x = (index % width) as i64;
        let y = (index / width) as i64;
        let entry = bounds[owner].get_or_insert(((i64::MAX, i64::MAX), (i64::MIN, i64::MIN)));
        entry.0 .0 = entry.0 .0.min(x);
        entry.0 .1 = entry.0 .1.min(y);
        entry.1 .0 = entry.1 .0.max(x);
        entry.1 .1 = entry.1 .1.max(y);
    }
    bounds
}

fn render_frame_with_mesh(
    master: &RgbaImage,
    rig: &Rig,
    frame: &RigFrame,
    ownership: &[i16],
    positions: &HashMap<String, (f64, f64)>,
    mesh: Option<&DeformMesh>,
) -> RgbaImage {
    let width = master.width();
    let height = master.height();
    let mut canvas = RgbaImage::new(width, height);
    if rig.bones.is_empty() {
        return master.clone();
    }
    let mut transforms: HashMap<String, RigTransform> = HashMap::new();
    for transform in &frame.transforms {
        if rig.bones.iter().any(|bone| bone.name == transform.bone) {
            transforms.insert(transform.bone.clone(), transform.clone());
        }
    }
    for contact in &frame.contacts {
        let Some(index) = rig.bones.iter().position(|bone| bone.name == contact.bone) else {
            continue;
        };
        let target = (contact.x - frame.root_dx, contact.y - frame.root_dy);
        let (parent_delta, child_delta) =
            solve_contact_ik(&rig.bones, index, positions, target, contact.bend);
        if let Some(parent_name) = rig.bones[index].parent.clone() {
            transforms
                .entry(parent_name)
                .or_insert_with_key(|name| identity_transform(name))
                .rotate += parent_delta;
        }
        transforms
            .entry(rig.bones[index].name.clone())
            .or_insert_with_key(|name| identity_transform(name))
            .rotate += child_delta;
    }
    let mut order: Vec<usize> = (0..rig.bones.len()).collect();
    order.sort_by_key(|index| (rig.bones[*index].z, *index));
    let bounds = owned_bounds(ownership, rig.bones.len(), master.width() as usize);
    let mut cache = HashMap::new();
    let root = Affine::translation(frame.root_dx, frame.root_dy);
    if let Some(mesh) = mesh.filter(|mesh| !mesh.triangles.is_empty()) {
        let worlds: Vec<Affine> = (0..rig.bones.len())
            .map(|index| {
                bone_world_affine(index, &rig.bones, positions, &transforms, &mut cache, 0)
            })
            .collect();
        return render_deform_mesh(master, mesh, &worlds, root);
    }
    for index in order {
        let Some(((min_ox, min_oy), (max_ox, max_oy))) = bounds[index] else {
            continue;
        };
        let world = bone_world_affine(index, &rig.bones, positions, &transforms, &mut cache, 0)
            .chain(&root);
        // Pad by one pixel: pixel centers extend half a pixel past the integer
        // bounds, and rotation can spread them onto neighboring rows/columns.
        let corners = [
            world.apply(min_ox as f64 - 1.0, min_oy as f64 - 1.0),
            world.apply(max_ox as f64 + 1.0, min_oy as f64 - 1.0),
            world.apply(min_ox as f64 - 1.0, max_oy as f64 + 1.0),
            world.apply(max_ox as f64 + 1.0, max_oy as f64 + 1.0),
        ];
        let bound_min_x = corners.iter().map(|(x, _)| *x).fold(f64::INFINITY, f64::min);
        let bound_min_y = corners.iter().map(|(_, y)| *y).fold(f64::INFINITY, f64::min);
        let bound_max_x = corners.iter().map(|(x, _)| *x).fold(f64::NEG_INFINITY, f64::max);
        let bound_max_y = corners.iter().map(|(_, y)| *y).fold(f64::NEG_INFINITY, f64::max);
        let inverse = world.inverse();
        let x_start = (bound_min_x.max(0.0).floor()) as i64;
        let y_start = (bound_min_y.max(0.0).floor()) as i64;
        let x_end = bound_max_x.min(width as f64 - 1.0).ceil() as i64;
        let y_end = bound_max_y.min(height as f64 - 1.0).ceil() as i64;
        for dy in y_start..=y_end {
            for dx in x_start..=x_end {
                if dx < 0 || dy < 0 || dx >= width as i64 || dy >= height as i64 {
                    continue;
                }
                let (source_x, source_y) = inverse.apply(dx as f64 + 0.5, dy as f64 + 0.5);
                let source_x = source_x.floor() as i64;
                let source_y = source_y.floor() as i64;
                if source_x < 0
                    || source_y < 0
                    || source_x >= width as i64
                    || source_y >= height as i64
                {
                    continue;
                }
                let owner = ownership[source_y as usize * width as usize + source_x as usize];
                if owner != index as i16 {
                    continue;
                }
                let pixel = *master.get_pixel(source_x as u32, source_y as u32);
                canvas.put_pixel(dx as u32, dy as u32, pixel);
            }
        }
    }
    canvas
}

#[cfg(test)]
pub(crate) fn render_frame(
    master: &RgbaImage,
    rig: &Rig,
    frame: &RigFrame,
    ownership: &[i16],
    positions: &HashMap<String, (f64, f64)>,
) -> RgbaImage {
    let mesh = (rig.bones.len() > 1).then(|| build_deform_mesh(master, rig, positions));
    render_frame_with_mesh(master, rig, frame, ownership, positions, mesh.as_ref())
}

pub(crate) fn render_frames(master: &RgbaImage, rig: &Rig) -> Vec<RgbaImage> {
    let positions = point_positions(rig, master.width(), master.height());
    let ownership = build_ownership(master, rig, &positions);
    let mesh = (rig.bones.len() > 1).then(|| build_deform_mesh(master, rig, &positions));
    let mut frames = Vec::new();
    for frame in &rig.frames {
        if frame.hold && !frames.is_empty() {
            frames.push(frames.last().cloned().unwrap_or_else(|| master.clone()));
            continue;
        }
        frames.push(render_frame_with_mesh(
            master,
            rig,
            frame,
            &ownership,
            &positions,
            mesh.as_ref(),
        ));
    }
    if frames.is_empty() {
        frames.push(master.clone());
    }
    frames
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

pub fn validate_rig(rig: &Rig, width: u32, height: u32) -> CommandResult<Vec<String>> {
    let mut warnings = Vec::new();
    let mut names = std::collections::HashSet::new();
    for point in &rig.points {
        if !names.insert(point.name.clone()) {
            warnings.push(format!("Point `{}` is defined more than once", point.name));
        }
        if point.x < 0.0 || point.y < 0.0 || point.x >= width as f64 || point.y >= height as f64 {
            warnings.push(format!(
                "Point `{}` sits outside the {}×{} canvas",
                point.name, width, height
            ));
        }
    }
    let mut seen_bones = Vec::new();
    for bone in &rig.bones {
        if seen_bones.contains(&bone.name) {
            if !warnings
                .iter()
                .any(|warning| warning.contains(&format!("Bone `{}` is defined", bone.name)))
            {
                warnings.push(format!("Bone `{}` is defined more than once", bone.name));
            }
        } else {
            seen_bones.push(bone.name.clone());
        }
        if !names.contains(&bone.start_point) || !names.contains(&bone.end_point) {
            warnings.push(format!(
                "Bone `{}` references a point that does not exist",
                bone.name
            ));
        }
        if !(0.5..=64.0).contains(&bone.radius) {
            warnings.push(format!(
                "Bone `{}` radius {} is outside 0.5–64 px",
                bone.name, bone.radius
            ));
        }
        if let Some(parent) = bone.parent.as_deref() {
            if parent == bone.name {
                warnings.push(format!("Bone `{}` parents itself", bone.name));
            } else if !seen_bones.iter().any(|name| name == parent) {
                warnings.push(format!(
                    "Bone `{}` parents missing bone `{}`",
                    bone.name, parent
                ));
            }
        }
    }
    // Cycle detection over the parent graph.
    for bone in &rig.bones {
        let mut visited = std::collections::HashSet::new();
        let mut current = Some(bone.name.clone());
        while let Some(name) = current {
            if !visited.insert(name.clone()) {
                warnings.push(format!("Bone `{}` is part of a parent cycle", bone.name));
                break;
            }
            current = rig
                .bones
                .iter()
                .find(|candidate| candidate.name == name)
                .and_then(|candidate| candidate.parent.clone());
        }
    }
    for frame in &rig.frames {
        for transform in &frame.transforms {
            if !seen_bones.contains(&transform.bone) {
                warnings.push(format!(
                    "A frame transform targets missing bone `{}`",
                    transform.bone
                ));
            }
        }
        for contact in &frame.contacts {
            if !seen_bones.contains(&contact.bone) {
                warnings.push(format!(
                    "A frame contact targets missing bone `{}`",
                    contact.bone
                ));
            }
        }
    }
    if rig.bones.is_empty() {
        warnings.push("The rig has no bones yet — add at least one before rendering".into());
    }
    Ok(warnings)
}

fn perceptibly_animated_bones(rig: &Rig) -> std::collections::HashSet<String> {
    let mut samples: HashMap<String, Vec<&RigTransform>> = HashMap::new();
    for frame in &rig.frames {
        for transform in &frame.transforms {
            samples.entry(transform.bone.clone()).or_default().push(transform);
        }
    }
    let mut meaningful: std::collections::HashSet<String> = samples
        .into_iter()
        .filter_map(|(bone, values)| {
            let span = |read: fn(&RigTransform) -> f64| {
                let mut minimum = 0.0_f64;
                let mut maximum = 0.0_f64;
                for value in &values {
                    let sample = read(value);
                    minimum = minimum.min(sample);
                    maximum = maximum.max(sample);
                }
                maximum - minimum
            };
            let translation = span(|value| value.dx).hypot(span(|value| value.dy));
            let rotation = span(|value| value.rotate);
            let scale = span(|value| value.scale_x - 1.0).max(span(|value| value.scale_y - 1.0));
            (translation >= 1.5 || rotation >= 8.0 || scale >= 0.05).then_some(bone)
        })
        .collect();
    let mut contact_samples: HashMap<String, Vec<(f64, f64)>> = HashMap::new();
    for frame in &rig.frames {
        for contact in &frame.contacts {
            contact_samples
                .entry(contact.bone.clone())
                .or_default()
                .push((contact.x, contact.y));
        }
    }
    for (bone, values) in contact_samples {
        if values.len() < 2 {
            continue;
        }
        let min_x = values.iter().map(|value| value.0).fold(f64::INFINITY, f64::min);
        let max_x = values.iter().map(|value| value.0).fold(f64::NEG_INFINITY, f64::max);
        let min_y = values.iter().map(|value| value.1).fold(f64::INFINITY, f64::min);
        let max_y = values.iter().map(|value| value.1).fold(f64::NEG_INFINITY, f64::max);
        if (max_x - min_x).hypot(max_y - min_y) >= 1.5 {
            meaningful.insert(bone);
        }
    }
    meaningful
}

fn validate_perceptible_rig_motion(rig: &Rig) -> CommandResult<()> {
    if rig.frames.len() < 2 {
        return Err(CommandError::new(
            "static_rig",
            "Add at least two different pose frames before rendering an animation",
        ));
    }
    let meaningful = perceptibly_animated_bones(rig);
    let required = if matches!(rig.morphology.as_str(), "biped" | "quadruped" | "winged") {
        2
    } else {
        1
    };
    if meaningful.len() < required {
        return Err(CommandError::new(
            "imperceptible_rig_motion",
            format!(
                "The pose frames are effectively static at sprite resolution. Animate at least {required} bone{} across 8°, 1.5 px, or 5% scale, then preview again.",
                if required == 1 { "" } else { "s" }
            ),
        ));
    }
    let body_tokens: &[&str] = match rig.morphology.as_str() {
        "biped" => &["body", "torso", "pelvis", "spine", "chest", "hip"],
        "quadruped" => &["body", "torso", "pelvis", "spine", "chest"],
        "winged" => &["body", "torso", "spine", "chest"],
        "serpentine" => &["body", "segment", "spine", "head", "tail"],
        _ => &[],
    };
    if !body_tokens.is_empty() {
        let core_bones: std::collections::HashSet<&str> = rig
            .bones
            .iter()
            .filter(|bone| {
                let tokens: std::collections::HashSet<&str> = bone
                    .name
                    .split(|character: char| !character.is_ascii_alphanumeric())
                    .filter(|token| !token.is_empty())
                    .collect();
                body_tokens.iter().any(|token| tokens.contains(token))
            })
            .map(|bone| bone.name.as_str())
            .collect();
        let required_core = if rig.morphology == "serpentine" { 2 } else { 1 };
        let animated_core = meaningful
            .iter()
            .filter(|bone| core_bones.contains(bone.as_str()))
            .count();
        if core_bones.len() < required_core || animated_core < required_core {
            return Err(CommandError::new(
                "missing_body_motion",
                format!(
                    "The limbs move, but the {} body stays rigid. Add and animate {} torso, pelvis, spine, or body bone{} with visible compression, rotation, or counter-motion.",
                    rig.morphology,
                    required_core,
                    if required_core == 1 { "" } else { "s" }
                ),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Heuristic point suggestion
// ---------------------------------------------------------------------------

/// Two-pass 3-4 chamfer distance transform of the alpha mask. Values are in
/// thirds of a pixel from the nearest transparent pixel, so local maxima sit
/// on the medial axis of limbs and joints.
pub(crate) fn distance_transform(alpha: &[bool], width: usize, height: usize) -> Vec<f64> {
    let large = (width.max(height) as f64) * 4.0;
    // Transparent pixels are the source (0); opaque pixels accumulate their
    // distance to the nearest transparent pixel through the chamfer passes.
    let mut distances = vec![0.0; width * height];
    for (index, opaque) in alpha.iter().enumerate() {
        if *opaque {
            distances[index] = large;
        }
    }
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let mut value = distances[index];
            if x > 0 {
                value = value.min(distances[index - 1] + 3.0);
            }
            if y > 0 {
                value = value.min(distances[index - width] + 3.0);
                if x > 0 {
                    value = value.min(distances[index - width - 1] + 4.0);
                }
                if x + 1 < width {
                    value = value.min(distances[index - width + 1] + 4.0);
                }
            }
            distances[index] = value;
        }
    }
    for y in (0..height).rev() {
        for x in (0..width).rev() {
            let index = y * width + x;
            let mut value = distances[index];
            if x + 1 < width {
                value = value.min(distances[index + 1] + 3.0);
            }
            if y + 1 < height {
                value = value.min(distances[index + width] + 3.0);
                if x + 1 < width {
                    value = value.min(distances[index + width + 1] + 4.0);
                }
                if x > 0 {
                    value = value.min(distances[index + width - 1] + 4.0);
                }
            }
            distances[index] = value;
        }
    }
    distances
}

struct TemplatePoint {
    name: &'static str,
    kind: &'static str,
    nx: f64,
    ny: f64,
}

struct TemplateBone {
    name: &'static str,
    start: &'static str,
    end: &'static str,
    radius_factor: f64,
    parent: Option<&'static str>,
    z: i64,
}

struct Template {
    points: &'static [TemplatePoint],
    bones: &'static [TemplateBone],
}

static BIPED_POINTS: &[TemplatePoint] = &[
    TemplatePoint { name: "head_top", kind: "anchor", nx: 0.5, ny: 0.04 },
    TemplatePoint { name: "neck", kind: "joint", nx: 0.5, ny: 0.18 },
    TemplatePoint { name: "shoulder_l", kind: "joint", nx: 0.36, ny: 0.23 },
    TemplatePoint { name: "shoulder_r", kind: "joint", nx: 0.64, ny: 0.23 },
    TemplatePoint { name: "elbow_l", kind: "joint", nx: 0.31, ny: 0.38 },
    TemplatePoint { name: "elbow_r", kind: "joint", nx: 0.69, ny: 0.38 },
    TemplatePoint { name: "hand_l", kind: "anchor", nx: 0.29, ny: 0.52 },
    TemplatePoint { name: "hand_r", kind: "anchor", nx: 0.71, ny: 0.52 },
    TemplatePoint { name: "hip", kind: "joint", nx: 0.5, ny: 0.52 },
    TemplatePoint { name: "hip_l", kind: "joint", nx: 0.43, ny: 0.54 },
    TemplatePoint { name: "hip_r", kind: "joint", nx: 0.57, ny: 0.54 },
    TemplatePoint { name: "knee_l", kind: "joint", nx: 0.44, ny: 0.72 },
    TemplatePoint { name: "knee_r", kind: "joint", nx: 0.56, ny: 0.72 },
    TemplatePoint { name: "foot_l", kind: "contact", nx: 0.42, ny: 0.95 },
    TemplatePoint { name: "foot_r", kind: "contact", nx: 0.58, ny: 0.95 },
];

static BIPED_BONES: &[TemplateBone] = &[
    TemplateBone { name: "torso", start: "neck", end: "hip", radius_factor: 1.4, parent: None, z: 5 },
    TemplateBone { name: "head", start: "neck", end: "head_top", radius_factor: 1.2, parent: Some("torso"), z: 7 },
    TemplateBone { name: "upper_arm_l", start: "shoulder_l", end: "elbow_l", radius_factor: 0.7, parent: Some("torso"), z: 2 },
    TemplateBone { name: "lower_arm_l", start: "elbow_l", end: "hand_l", radius_factor: 0.6, parent: Some("upper_arm_l"), z: 2 },
    TemplateBone { name: "upper_arm_r", start: "shoulder_r", end: "elbow_r", radius_factor: 0.7, parent: Some("torso"), z: 9 },
    TemplateBone { name: "lower_arm_r", start: "elbow_r", end: "hand_r", radius_factor: 0.6, parent: Some("upper_arm_r"), z: 9 },
    TemplateBone { name: "thigh_l", start: "hip_l", end: "knee_l", radius_factor: 0.8, parent: Some("torso"), z: 2 },
    TemplateBone { name: "shin_l", start: "knee_l", end: "foot_l", radius_factor: 0.6, parent: Some("thigh_l"), z: 2 },
    TemplateBone { name: "thigh_r", start: "hip_r", end: "knee_r", radius_factor: 0.8, parent: Some("torso"), z: 9 },
    TemplateBone { name: "shin_r", start: "knee_r", end: "foot_r", radius_factor: 0.6, parent: Some("thigh_r"), z: 9 },
];

static QUADRUPED_POINTS: &[TemplatePoint] = &[
    TemplatePoint { name: "nose", kind: "anchor", nx: 0.05, ny: 0.40 },
    TemplatePoint { name: "head", kind: "joint", nx: 0.14, ny: 0.42 },
    TemplatePoint { name: "neck_base", kind: "joint", nx: 0.27, ny: 0.38 },
    TemplatePoint { name: "shoulder", kind: "joint", nx: 0.33, ny: 0.58 },
    TemplatePoint { name: "front_knee", kind: "joint", nx: 0.31, ny: 0.72 },
    TemplatePoint { name: "front_paw", kind: "contact", nx: 0.30, ny: 0.94 },
    TemplatePoint { name: "pelvis", kind: "joint", nx: 0.66, ny: 0.48 },
    TemplatePoint { name: "hip", kind: "joint", nx: 0.68, ny: 0.58 },
    TemplatePoint { name: "hind_knee", kind: "joint", nx: 0.71, ny: 0.68 },
    TemplatePoint { name: "hind_paw", kind: "contact", nx: 0.69, ny: 0.94 },
    TemplatePoint { name: "tail_base", kind: "joint", nx: 0.84, ny: 0.42 },
    TemplatePoint { name: "tail_tip", kind: "anchor", nx: 0.96, ny: 0.34 },
];

static QUADRUPED_BONES: &[TemplateBone] = &[
    TemplateBone { name: "body", start: "neck_base", end: "pelvis", radius_factor: 1.5, parent: None, z: 5 },
    TemplateBone { name: "head", start: "neck_base", end: "head", radius_factor: 1.1, parent: Some("body"), z: 7 },
    TemplateBone { name: "muzzle", start: "head", end: "nose", radius_factor: 0.7, parent: Some("head"), z: 7 },
    TemplateBone { name: "front_leg", start: "shoulder", end: "front_knee", radius_factor: 0.8, parent: Some("body"), z: 9 },
    TemplateBone { name: "front_pastern", start: "front_knee", end: "front_paw", radius_factor: 0.6, parent: Some("front_leg"), z: 9 },
    TemplateBone { name: "hind_leg", start: "hip", end: "hind_knee", radius_factor: 0.8, parent: Some("body"), z: 9 },
    TemplateBone { name: "hind_pastern", start: "hind_knee", end: "hind_paw", radius_factor: 0.6, parent: Some("hind_leg"), z: 9 },
    TemplateBone { name: "tail", start: "tail_base", end: "tail_tip", radius_factor: 0.6, parent: Some("body"), z: 3 },
];

static WINGED_POINTS: &[TemplatePoint] = &[
    TemplatePoint { name: "head_top", kind: "anchor", nx: 0.5, ny: 0.06 },
    TemplatePoint { name: "neck", kind: "joint", nx: 0.5, ny: 0.18 },
    TemplatePoint { name: "wing_root_l", kind: "joint", nx: 0.38, ny: 0.28 },
    TemplatePoint { name: "wing_tip_l", kind: "anchor", nx: 0.12, ny: 0.20 },
    TemplatePoint { name: "wing_root_r", kind: "joint", nx: 0.62, ny: 0.28 },
    TemplatePoint { name: "wing_tip_r", kind: "anchor", nx: 0.88, ny: 0.20 },
    TemplatePoint { name: "tail_base", kind: "joint", nx: 0.5, ny: 0.60 },
    TemplatePoint { name: "tail_tip", kind: "anchor", nx: 0.5, ny: 0.86 },
    TemplatePoint { name: "foot_l", kind: "contact", nx: 0.42, ny: 0.72 },
    TemplatePoint { name: "foot_r", kind: "contact", nx: 0.58, ny: 0.72 },
];

static WINGED_BONES: &[TemplateBone] = &[
    TemplateBone { name: "body", start: "neck", end: "tail_base", radius_factor: 1.4, parent: None, z: 5 },
    TemplateBone { name: "head", start: "neck", end: "head_top", radius_factor: 1.1, parent: Some("body"), z: 7 },
    TemplateBone { name: "wing_l", start: "wing_root_l", end: "wing_tip_l", radius_factor: 0.9, parent: Some("body"), z: 2 },
    TemplateBone { name: "wing_r", start: "wing_root_r", end: "wing_tip_r", radius_factor: 0.9, parent: Some("body"), z: 9 },
    TemplateBone { name: "tail", start: "tail_base", end: "tail_tip", radius_factor: 0.7, parent: Some("body"), z: 3 },
];

static SERPENTINE_POINTS: &[TemplatePoint] = &[
    TemplatePoint { name: "snout", kind: "anchor", nx: 0.06, ny: 0.46 },
    TemplatePoint { name: "head", kind: "joint", nx: 0.16, ny: 0.48 },
    TemplatePoint { name: "neck", kind: "joint", nx: 0.28, ny: 0.42 },
    TemplatePoint { name: "spine_a", kind: "joint", nx: 0.42, ny: 0.50 },
    TemplatePoint { name: "spine_b", kind: "joint", nx: 0.56, ny: 0.56 },
    TemplatePoint { name: "spine_c", kind: "joint", nx: 0.70, ny: 0.54 },
    TemplatePoint { name: "spine_d", kind: "joint", nx: 0.82, ny: 0.50 },
    TemplatePoint { name: "tail_tip", kind: "anchor", nx: 0.95, ny: 0.48 },
];

static SERPENTINE_BONES: &[TemplateBone] = &[
    TemplateBone { name: "head", start: "snout", end: "neck", radius_factor: 1.0, parent: None, z: 6 },
    TemplateBone { name: "segment_a", start: "neck", end: "spine_b", radius_factor: 1.1, parent: Some("head"), z: 5 },
    TemplateBone { name: "segment_b", start: "spine_b", end: "spine_d", radius_factor: 1.0, parent: Some("segment_a"), z: 5 },
    TemplateBone { name: "tail", start: "spine_d", end: "tail_tip", radius_factor: 0.6, parent: Some("segment_b"), z: 5 },
];

static OBJECT_POINTS: &[TemplatePoint] = &[
    TemplatePoint { name: "top_left", kind: "anchor", nx: 0.2, ny: 0.2 },
    TemplatePoint { name: "top_right", kind: "anchor", nx: 0.8, ny: 0.2 },
    TemplatePoint { name: "bottom_left", kind: "anchor", nx: 0.2, ny: 0.8 },
    TemplatePoint { name: "bottom_right", kind: "anchor", nx: 0.8, ny: 0.8 },
    TemplatePoint { name: "center", kind: "pivot", nx: 0.5, ny: 0.5 },
];

static OBJECT_BONES: &[TemplateBone] = &[
    TemplateBone { name: "body", start: "top_left", end: "bottom_right", radius_factor: 2.0, parent: None, z: 1 },
];

static AMPHORPHOUS_POINTS: &[TemplatePoint] = &[
    TemplatePoint { name: "core", kind: "pivot", nx: 0.5, ny: 0.5 },
    TemplatePoint { name: "top", kind: "anchor", nx: 0.5, ny: 0.12 },
    TemplatePoint { name: "left", kind: "anchor", nx: 0.12, ny: 0.5 },
    TemplatePoint { name: "right", kind: "anchor", nx: 0.88, ny: 0.5 },
    TemplatePoint { name: "bottom", kind: "anchor", nx: 0.5, ny: 0.88 },
];

static AMPHORPHOUS_BONES: &[TemplateBone] = &[
    TemplateBone { name: "blob", start: "top", end: "bottom", radius_factor: 2.2, parent: None, z: 1 },
];

fn template_for(morphology: &str) -> Template {
    match morphology {
        "quadruped" => Template { points: QUADRUPED_POINTS, bones: QUADRUPED_BONES },
        "winged" => Template { points: WINGED_POINTS, bones: WINGED_BONES },
        "serpentine" => Template { points: SERPENTINE_POINTS, bones: SERPENTINE_BONES },
        "object" => Template { points: OBJECT_POINTS, bones: OBJECT_BONES },
        "amorphous" => Template { points: AMPHORPHOUS_POINTS, bones: AMPHORPHOUS_BONES },
        _ => Template { points: BIPED_POINTS, bones: BIPED_BONES },
    }
}

pub(crate) fn suggest_points(master: &RgbaImage, morphology: &str) -> CommandResult<RigSuggestion> {
    let width = master.width() as usize;
    let height = master.height() as usize;
    if width == 0 || height == 0 {
        return Err(CommandError::new(
            "invalid_master",
            "The source sprite has no pixels to rig",
        ));
    }
    let alpha: Vec<bool> = master.pixels().map(|pixel| pixel[3] > 0).collect();
    if !alpha.iter().any(|value| *value) {
        return Err(CommandError::new(
            "empty_alpha",
            "The source sprite is fully transparent",
        ));
    }
    let distances = distance_transform(&alpha, width, height);
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    for y in 0..height {
        for x in 0..width {
            if alpha[y * width + x] {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    let box_width = (max_x - min_x + 1) as f64;
    let box_height = (max_y - min_y + 1) as f64;
    let template = template_for(morphology);
    let search_radius = ((box_width.min(box_height) * 0.08).round() as i64).max(2);
    let mut points = Vec::new();
    for (index, entry) in template.points.iter().enumerate() {
        let center_x = min_x as f64 + entry.nx * (box_width - 1.0);
        let center_y = min_y as f64 + entry.ny * (box_height - 1.0);
        let mut best: Option<(f64, f64, f64)> = None; // (score, x, y)
        let window = search_radius;
        let y_lo = (center_y as i64 - window).max(0) as usize;
        let y_hi = (center_y as i64 + window).min(height as i64 - 1) as usize;
        let x_lo = (center_x as i64 - window).max(0) as usize;
        let x_hi = (center_x as i64 + window).min(width as i64 - 1) as usize;
        for y in y_lo..=y_hi {
            for x in x_lo..=x_hi {
                let index = y * width + x;
                if !alpha[index] {
                    continue;
                }
                // Prefer thick, medial pixels close to the template position;
                // thickness weighs double so limb points snap to limb centers
                // rather than the near edge of a thin limb.
                let offset =
                    ((x as f64 - center_x).powi(2) + (y as f64 - center_y).powi(2)).sqrt();
                let score = (distances[index] / 3.0) * 2.0 - offset;
                if best.is_none() || score > best.unwrap().0 {
                    best = Some((score, x as f64, y as f64));
                }
            }
        }
        let (x, y, confidence) = match best {
            Some((_, x, y)) => {
                let thickness = distances[y as usize * width + x as usize] / 3.0;
                let confidence =
                    (0.45 + 0.5 * (thickness / search_radius as f64)).clamp(0.4, 0.97);
                (x, y, confidence)
            }
            None => (center_x, center_y, 0.3),
        };
        points.push(RigPoint {
            id: format!("p{}", index + 1),
            name: entry.name.to_string(),
            kind: entry.kind.to_string(),
            x,
            y,
            confidence: (confidence * 100.0).round() / 100.0,
            source: "auto".to_string(),
            note: None,
        });
    }
    let positions: HashMap<String, (f64, f64)> = points
        .iter()
        .map(|point| (point.name.clone(), (point.x, point.y)))
        .collect();
    let scale_reference = box_width.min(box_height);
    let mut bones = Vec::new();
    for (index, entry) in template.bones.iter().enumerate() {
        let Some(&start) = positions.get(entry.start) else { continue };
        let Some(&end) = positions.get(entry.end) else { continue };
        let midpoint_thickness = {
            let mx = ((start.0 + end.0) / 2.0)
                .round()
                .clamp(0.0, (width - 1) as f64) as usize;
            let my = ((start.1 + end.1) / 2.0)
                .round()
                .clamp(0.0, (height - 1) as f64) as usize;
            distances[my * width + mx] / 3.0
        };
        let radius = (midpoint_thickness * entry.radius_factor)
            .clamp(1.0, (scale_reference / 2.0).max(3.0));
        bones.push(RigBone {
            id: format!("b{}", index + 1),
            name: entry.name.to_string(),
            start_point: entry.start.to_string(),
            end_point: entry.end.to_string(),
            radius: (radius * 10.0).round() / 10.0,
            parent: entry.parent.map(str::to_string),
            z: entry.z,
        });
    }
    let reasoning = format!(
        "Placed {} points and {} bones from the {morphology} anatomy template, snapped onto the medial axis of the {}×{} silhouette.",
        points.len(),
        bones.len(),
        master.width(),
        master.height()
    );
    Ok(RigSuggestion {
        morphology: morphology.to_string(),
        points,
        bones,
        frames: Vec::new(),
        reasoning,
        source: "auto".to_string(),
    })
}

// ---------------------------------------------------------------------------
// AI rig-suggestion contract
// ---------------------------------------------------------------------------

/// Extracts a ```rig-suggestion JSON block (or a compatible plain JSON object)
/// from agent prose and normalizes it into a safe suggestion.
pub fn parse_rig_suggestion_text(text: &str, width: u32, height: u32) -> Option<RigSuggestion> {
    let mut candidates = extract_fenced_blocks(text, "rig-suggestion");
    if candidates.is_empty() {
        candidates = extract_fenced_json_with_points(text);
    }
    for json_text in candidates.iter().rev() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_text) {
            if let Some(suggestion) = normalize_suggestion(value, width, height, "ai") {
                return Some(suggestion);
            }
        }
    }
    None
}

fn extract_fenced_blocks(text: &str, tag: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut remaining = text;
    while let Some(start) = remaining.find("```") {
        let after = &remaining[start + 3..];
        let Some(first_newline) = after.find('\n') else { break };
        let info = after[..first_newline].trim();
        let body = &after[first_newline + 1..];
        let Some(end) = body.find("```") else { break };
        if info == tag || info.starts_with(&format!("{tag} ")) {
            blocks.push(body[..end].trim().to_string());
        }
        remaining = &body[end + 3..];
    }
    blocks
}

fn extract_fenced_json_with_points(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut remaining = text;
    while let Some(start) = remaining.find("```") {
        let after = &remaining[start + 3..];
        let Some(first_newline) = after.find('\n') else { break };
        let info = after[..first_newline].trim().to_ascii_lowercase();
        let body = &after[first_newline + 1..];
        let Some(end) = body.find("```") else { break };
        if info == "json" || info.is_empty() {
            let candidate = body[..end].trim();
            if candidate.contains("\"points\"") && candidate.contains("\"bones\"") {
                blocks.push(candidate.to_string());
            }
        }
        remaining = &body[end + 3..];
    }
    blocks
}

fn json_f64(value: &serde_json::Value, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(number) = value.get(*key).and_then(|entry| entry.as_f64()) {
            if number.is_finite() {
                return Some(number);
            }
        }
    }
    None
}

fn json_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(text) = value.get(*key).and_then(|entry| entry.as_str()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Tolerant conversion of agent JSON into a rig suggestion: coordinates are
/// clamped to the canvas, unknown references are dropped, and cycles are cut.
pub(crate) fn normalize_suggestion(
    value: serde_json::Value,
    width: u32,
    height: u32,
    source: &str,
) -> Option<RigSuggestion> {
    let morphology =
        normalize_morphology(json_string(&value, &["morphology", "profile"]).as_deref());
    let entries = value.get("points")?.as_array()?;
    if entries.is_empty() {
        return None;
    }
    let max_x = (width.saturating_sub(1)) as f64;
    let max_y = (height.saturating_sub(1)) as f64;
    let mut points = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let Some(x) = json_f64(entry, &["x"]) else { continue };
        let Some(y) = json_f64(entry, &["y"]) else { continue };
        let name =
            json_string(entry, &["name", "id"]).unwrap_or_else(|| format!("point_{}", index + 1));
        points.push(RigPoint {
            id: format!("p{}", index + 1),
            name,
            kind: json_string(entry, &["kind", "type"]).unwrap_or_else(|| "joint".into()),
            x: x.clamp(0.0, max_x),
            y: y.clamp(0.0, max_y),
            confidence: json_f64(entry, &["confidence"]).unwrap_or(0.8).clamp(0.0, 1.0),
            source: source.to_string(),
            note: json_string(entry, &["note", "notes"]),
        });
    }
    if points.is_empty() {
        return None;
    }
    let point_exists = |name: &str| points.iter().any(|point| point.name == name);
    let mut bones = Vec::new();
    if let Some(bone_entries) = value.get("bones").and_then(|entry| entry.as_array()) {
        for (index, entry) in bone_entries.iter().enumerate() {
            let Some(start) = json_string(entry, &["start", "startPoint", "from"]) else {
                continue;
            };
            let Some(end) = json_string(entry, &["end", "endPoint", "to"]) else {
                continue;
            };
            if !point_exists(&start) || !point_exists(&end) {
                continue;
            }
            let name = json_string(entry, &["name", "id"])
                .unwrap_or_else(|| format!("bone_{}", index + 1));
            bones.push(RigBone {
                id: format!("b{}", index + 1),
                name,
                start_point: start,
                end_point: end,
                radius: json_f64(entry, &["radius"]).unwrap_or(3.0).clamp(0.5, 64.0),
                parent: json_string(entry, &["parent"]).filter(|parent| !parent.is_empty()),
                z: json_f64(entry, &["z", "zIndex", "depth"])
                    .unwrap_or(5.0)
                    .round() as i64,
            });
        }
    }
    let bone_exists = |bones: &[RigBone], name: &str| bones.iter().any(|bone| bone.name == name);
    // Cut parent cycles by walking each chain once.
    for bone_index in 0..bones.len() {
        let Some(parent) = bones[bone_index].parent.clone() else {
            continue;
        };
        if parent == bones[bone_index].name || !bone_exists(&bones, &parent) {
            bones[bone_index].parent = None;
            continue;
        }
        let mut visited = std::collections::HashSet::new();
        visited.insert(bones[bone_index].name.clone());
        let mut current = Some(parent);
        let mut cyclic = false;
        while let Some(name) = current {
            if !visited.insert(name.clone()) {
                cyclic = true;
                break;
            }
            current = bones
                .iter()
                .find(|candidate| candidate.name == name)
                .and_then(|candidate| candidate.parent.clone());
        }
        if cyclic {
            bones[bone_index].parent = None;
        }
    }
    let mut frames = Vec::new();
    if let Some(frame_entries) = value.get("frames").and_then(|entry| entry.as_array()) {
        for entry in frame_entries {
            let mut transforms = Vec::new();
            if let Some(transform_entries) = entry.get("transforms").and_then(|v| v.as_array()) {
                for transform in transform_entries {
                    let Some(bone) = json_string(transform, &["bone", "name"]) else {
                        continue;
                    };
                    if !bone_exists(&bones, &bone) {
                        continue;
                    }
                    transforms.push(RigTransform {
                        bone,
                        dx: json_f64(transform, &["dx", "x"]).unwrap_or(0.0),
                        dy: json_f64(transform, &["dy", "y"]).unwrap_or(0.0),
                        rotate: json_f64(transform, &["rotate", "angle", "rotation"])
                            .unwrap_or(0.0),
                        scale_x: json_f64(transform, &["scaleX", "scale_x", "scale"])
                            .unwrap_or(1.0),
                        scale_y: json_f64(transform, &["scaleY", "scale_y", "scale"])
                            .unwrap_or(1.0),
                    });
                }
            }
            let mut contacts = Vec::new();
            if let Some(contact_entries) = entry.get("contacts").and_then(|v| v.as_array()) {
                for contact in contact_entries {
                    let Some(bone) = json_string(contact, &["bone", "name"]) else {
                        continue;
                    };
                    if !bone_exists(&bones, &bone) {
                        continue;
                    }
                    contacts.push(RigContact {
                        bone,
                        x: json_f64(contact, &["x"]).unwrap_or(0.0).clamp(0.0, max_x),
                        y: json_f64(contact, &["y"]).unwrap_or(0.0).clamp(0.0, max_y),
                        bend: json_f64(contact, &["bend"]).unwrap_or(1.0),
                    });
                }
            }
            frames.push(RigFrame {
                phase: json_string(entry, &["phase", "name"]),
                hold: entry
                    .get("hold")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false),
                root_dx: json_f64(entry, &["rootDx", "root_dx"]).unwrap_or(0.0),
                root_dy: json_f64(entry, &["rootDy", "root_dy"]).unwrap_or(0.0),
                transforms,
                contacts,
            });
        }
    }
    let reasoning = json_string(&value, &["reasoning", "explanation", "notes"])
        .unwrap_or_else(|| "AI-suggested rig points".to_string());
    Some(RigSuggestion {
        morphology,
        points,
        bones,
        frames,
        reasoning,
        source: source.to_string(),
    })
}

/// Captures a rig suggestion embedded in a completed chat response and stores
/// it as a rig the user can open in the Rig editor.
pub(crate) fn capture_chat_suggestion(
    state: &AppState,
    workspace_id: &str,
    worktree_id: Option<&str>,
    response: &str,
) -> CommandResult<Option<String>> {
    let Some(suggestion) = parse_rig_suggestion_text(response, u32::MAX, u32::MAX) else {
        return Ok(None);
    };
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    let name = format!("AI rig — {}", suggestion.morphology);
    let spec = RigSpec {
        points: suggestion.points.clone(),
        bones: suggestion.bones.clone(),
        frames: suggestion.frames.clone(),
    };
    let spec_json = serde_json::to_string(&spec)
        .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        r#"INSERT INTO rigs(id, workspace_id, worktree_id, asset_id, name, morphology, fps, looping, spec_json, created_at, updated_at)
           VALUES (?1, ?2, ?3, NULL, ?4, ?5, 8, 1, ?6, ?7, ?7)"#,
        params![id, workspace_id, worktree_id, name, suggestion.morphology, spec_json, now],
    )?;
    Ok(Some(name))
}

// ---------------------------------------------------------------------------
// Morphology detection and rig-fit checking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MorphologyScore {
    pub morphology: String,
    pub confidence: f64,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RigFitReport {
    pub detections: Vec<MorphologyScore>,
    pub recommended: RigSuggestion,
    pub capsule_fit: f64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Default)]
struct BandShape {
    runs: usize,
    covered: f64,
    max_run: f64,
}

#[derive(Debug)]
struct ShapeFeatures {
    aspect: f64,
    fill: f64,
    top: BandShape,
    mid: BandShape,
    lower: BandShape,
    /// Two lower-band runs on opposite sides of the silhouette's center.
    lower_straddle: bool,
    /// At least two separated runs somewhere in the lower half.
    lower_separated: bool,
    /// Per-column thickness stays roughly constant (snake-like mass).
    uniform_thickness: bool,
}

fn band_shape(
    alpha: &[bool],
    width: usize,
    min_x: usize,
    max_x: usize,
    y_start: usize,
    y_end: usize,
    box_width: f64,
) -> BandShape {
    let span = (max_x - min_x + 1).max(1);
    let mut occupied = vec![false; span];
    for y in y_start..=y_end.min(alpha.len() / width - 1) {
        let row = y * width;
        for index in 0..span {
            if alpha[row + min_x + index] {
                occupied[index] = true;
            }
        }
    }
    let mut runs = Vec::new();
    let mut start: Option<usize> = None;
    for (index, is_occupied) in occupied.iter().copied().enumerate().take(span) {
        if is_occupied && start.is_none() {
            start = Some(index);
        }
        if start.is_some() && (!is_occupied || index + 1 == span) {
            let begin = start.take().expect("run start");
            let end = if is_occupied { index } else { index - 1 };
            if end - begin + 1 >= 2 {
                runs.push((begin, end));
            }
        }
    }
    let covered = runs
        .iter()
        .map(|run| (run.1 - run.0 + 1) as f64)
        .sum::<f64>()
        / span as f64;
    let max_run = runs
        .iter()
        .map(|run| (run.1 - run.0 + 1) as f64)
        .fold(0.0_f64, f64::max)
        / box_width.max(1.0);
    BandShape {
        runs: runs.len(),
        covered,
        max_run,
    }
}

fn shape_features(master: &RgbaImage) -> Option<ShapeFeatures> {
    let width = master.width() as usize;
    let height = master.height() as usize;
    let alpha: Vec<bool> = master.pixels().map(|pixel| pixel[3] > 8).collect();
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    let mut opaque = 0usize;
    for y in 0..height {
        for x in 0..width {
            if alpha[y * width + x] {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                opaque += 1;
            }
        }
    }
    if opaque == 0 || max_y <= min_y + 2 || max_x <= min_x + 2 {
        return None;
    }
    let box_width = (max_x - min_x + 1) as f64;
    let box_height = (max_y - min_y + 1) as f64;
    let band = (box_height / 5.0).ceil() as usize;
    let top = band_shape(&alpha, width, min_x, max_x, min_y, min_y + band, box_width);
    let mid = band_shape(
        &alpha,
        width,
        min_x,
        max_x,
        min_y + 2 * band,
        min_y + 3 * band,
        box_width,
    );
    let lower_band_start = min_y + ((max_y - min_y) as f64 * 0.66) as usize;
    let lower = band_shape(
        &alpha,
        width,
        min_x,
        max_x,
        lower_band_start,
        max_y,
        box_width,
    );
    // Lower straddle: one run fully left of center, another fully right.
    let center = span_center(min_x, max_x);
    let mut left = false;
    let mut right = false;
    if lower.runs >= 2 {
        let span_alpha: Vec<bool> = (0..(max_x - min_x + 1))
            .map(|index| {
                (lower_band_start..=max_y).any(|y| alpha[y * width + min_x + index])
            })
            .collect();
        let mut runs = Vec::new();
        let mut start: Option<usize> = None;
        for index in 0..span_alpha.len() {
            if span_alpha[index] && start.is_none() {
                start = Some(index);
            }
            if start.is_some() && (!span_alpha[index] || index + 1 == span_alpha.len()) {
                let begin = start.take().expect("run start");
                let end = if span_alpha[index] { index } else { index - 1 };
                if end - begin + 1 >= 2 {
                    runs.push((begin + min_x, end + min_x));
                }
            }
        }
        for (begin, end) in runs {
            if ((end as f64) + 0.5) < center {
                left = true;
            }
            if ((begin as f64) + 0.5) > center {
                right = true;
            }
        }
    }
    // Uniform thickness: per-column vertical extent stays near its mean.
    let mut thickness = Vec::new();
    for x in min_x..=max_x {
        let column: Vec<usize> = (min_y..=max_y)
            .filter(|y| alpha[*y * width + x])
            .collect();
        if !column.is_empty() {
            thickness.push(column.len() as f64);
        }
    }
    let mean = thickness.iter().sum::<f64>() / thickness.len().max(1) as f64;
    let variance = thickness.iter().map(|t| (t - mean).powi(2)).sum::<f64>()
        / thickness.len().max(1) as f64;
    let uniform_thickness = mean > 0.0 && (variance.sqrt() / mean) < 0.55;
    let lower_runs = lower.runs;
    Some(ShapeFeatures {
        aspect: box_width / box_height,
        fill: opaque as f64 / (box_width * box_height),
        top,
        mid,
        lower,
        lower_straddle: left && right,
        lower_separated: lower_runs >= 2,
        uniform_thickness,
    })
}

fn span_center(min_x: usize, max_x: usize) -> f64 {
    (min_x as f64 + max_x as f64 + 1.0) / 2.0
}

fn scored(morphology: &str, mut score: f64, reasons: Vec<String>) -> MorphologyScore {
    let reasoning = if reasons.is_empty() {
        "generic silhouette match".to_string()
    } else {
        reasons.join("; ")
    };
    score = score.clamp(0.05, 0.97);
    MorphologyScore {
        morphology: morphology.to_string(),
        confidence: (score * 100.0).round() / 100.0,
        reasoning,
    }
}

/// Scores every rig profile against the sprite's silhouette so the app can
/// recommend (or reject) a rig before any points are placed.
pub(crate) fn detect_morphology(master: &RgbaImage) -> CommandResult<Vec<MorphologyScore>> {
    let Some(features) = shape_features(master) else {
        return Ok(vec![scored(
            "object",
            0.5,
            vec!["the sprite is too small or thin to profile".into()],
        )]);
    };
    let mut results = Vec::new();

    let mut biped = 0.2;
    let mut biped_reasons = vec!["upright proportions".to_string()];
    if features.aspect < 0.8 {
        biped += 0.35;
        biped_reasons.push("taller than wide".into());
    } else if features.aspect < 0.95 {
        biped += 0.2;
    }
    if features.lower_separated {
        biped += 0.2;
        biped_reasons.push("two separated legs".into());
    }
    if features.lower_straddle {
        biped += 0.1;
        biped_reasons.push("legs straddle the center line".into());
    }
    if features.top.max_run < features.mid.max_run * 0.85 {
        biped += 0.1;
        biped_reasons.push("head narrower than torso".into());
    }
    results.push(scored("biped", biped, biped_reasons));

    let mut quadruped = 0.15;
    let mut quad_reasons = Vec::new();
    if features.aspect > 1.2 {
        quadruped += 0.35;
        quad_reasons.push("longer than tall".into());
    } else if features.aspect > 1.05 {
        quadruped += 0.2;
    }
    if features.lower_straddle {
        quadruped += 0.2;
        quad_reasons.push("support mass on both ends".into());
    }
    if features.lower.runs >= 3 {
        quadruped += 0.15;
        quad_reasons.push("multiple legs visible".into());
    }
    if features.mid.covered > 0.6 {
        quadruped += 0.1;
        quad_reasons.push("continuous horizontal body mass".into());
    }
    results.push(scored("quadruped", quadruped, quad_reasons));

    let mut winged = 0.1;
    let mut wing_reasons = Vec::new();
    if features.aspect > 1.4 {
        winged += 0.35;
        wing_reasons.push("very wide silhouette".into());
    } else if features.aspect > 1.15 {
        winged += 0.15;
    }
    if features.top.max_run > features.mid.max_run * 1.1 {
        winged += 0.25;
        wing_reasons.push("upper mass wider than the torso — wings".into());
    }
    if features.top.covered > 0.5 && features.aspect > 1.2 {
        winged += 0.1;
    }
    results.push(scored("winged", winged, wing_reasons));

    let mut serpentine = 0.15;
    let mut snake_reasons = Vec::new();
    if features.aspect > 1.7 {
        serpentine += 0.4;
        snake_reasons.push("very elongated body".into());
    } else if features.aspect > 1.35 {
        serpentine += 0.2;
    }
    if features.uniform_thickness {
        serpentine += 0.2;
        snake_reasons.push("constant body thickness".into());
    }
    if features.fill < 0.45 {
        serpentine += 0.15;
        snake_reasons.push("thin mass for its bounding box".into());
    }
    if features.lower.runs <= 1 && features.mid.runs <= 1 {
        serpentine += 0.1;
        snake_reasons.push("no separated limbs".into());
    }
    results.push(scored("serpentine", serpentine, snake_reasons));

    let mut amorphous = 0.15;
    let mut amorph_reasons = Vec::new();
    if (0.8..=1.3).contains(&features.aspect) {
        amorphous += 0.3;
        amorph_reasons.push("roughly round silhouette".into());
    }
    if features.fill > 0.6 {
        amorphous += 0.25;
        amorph_reasons.push("solid filled mass".into());
    }
    if !features.lower_separated {
        amorphous += 0.15;
        amorph_reasons.push("no separated legs".into());
    }
    results.push(scored("amorphous", amorphous, amorph_reasons));

    let mut object = 0.2;
    let mut object_reasons = Vec::new();
    if (0.7..=1.45).contains(&features.aspect) {
        object += 0.2;
        object_reasons.push("compact proportions".into());
    }
    if !features.lower_separated {
        object += 0.15;
        object_reasons.push("single rigid body".into());
    }
    if features.fill > 0.45 {
        object += 0.1;
    }
    results.push(scored("object", object, object_reasons));

    results.sort_by(|left, right| {
        right
            .confidence
            .partial_cmp(&left.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(results)
}

/// Fraction of opaque pixels claimed by a capsule proper (not the nearest-bone
/// fallback) — how well the suggested bones tile the silhouette.
pub(crate) fn capsule_coverage(master: &RgbaImage, suggestion: &RigSuggestion) -> f64 {
    let mut positions = HashMap::new();
    for point in &suggestion.points {
        positions.insert(point.name.clone(), (point.x, point.y));
    }
    type Capsule = ((f64, f64), (f64, f64), f64);
    let capsules: Vec<Capsule> = suggestion
        .bones
        .iter()
        .filter_map(|bone| {
            let start = positions.get(&bone.start_point).copied()?;
            let end = positions.get(&bone.end_point).copied()?;
            Some((start, end, bone.radius))
        })
        .collect();
    if capsules.is_empty() {
        return 0.0;
    }
    let mut inside = 0usize;
    let mut total = 0usize;
    for (x, y, pixel) in master.enumerate_pixels() {
        if pixel[3] <= 8 {
            continue;
        }
        total += 1;
        let px = x as f64 + 0.5;
        let py = y as f64 + 0.5;
        if capsules.iter().any(|(start, end, radius)| {
            point_segment_distance(px, py, start.0, start.1, end.0, end.1) <= *radius
        }) {
            inside += 1;
        }
    }
    if total == 0 {
        0.0
    } else {
        inside as f64 / total as f64
    }
}

#[tauri::command]
pub fn analyze_rig_fit(
    asset_id: String,
    state: State<'_, AppState>,
) -> CommandResult<RigFitReport> {
    let (_, master) = load_master_asset(&state, &asset_id)?;
    let detections = detect_morphology(&master)?;
    let best = detections.first().ok_or_else(|| {
        CommandError::new("morphology_failed", "The sprite could not be profiled")
    })?;
    let suggestion = suggest_points(&master, &best.morphology)?;
    let capsule_fit = capsule_coverage(&master, &suggestion);
    let mut warnings = Vec::new();
    if best.confidence < 0.45 {
        warnings.push(format!(
            "No rig profile fits this sprite well (best: {} at {}%). A cleaner master with a readable silhouette and separated limbs will rig and animate better.",
            best.morphology,
            (best.confidence * 100.0).round() as u32
        ));
    }
    let needs_legs = matches!(best.morphology.as_str(), "biped" | "quadruped");
    if needs_legs {
        if let Some(features) = shape_features(&master) {
            if !features.lower_separated {
                warnings.push(
                    "The legs never separate in this silhouette, so leg poses will read poorly. Regenerate the master in a wide stance, or widen the leg capsules after applying."
                        .to_string(),
                );
            }
        }
    }
    if capsule_fit < 0.5 {
        warnings.push(format!(
            "Template capsules claim only {}% of the silhouette directly; fine-tune the bone radii after applying.",
            (capsule_fit * 100.0).round() as u32
        ));
    }
    Ok(RigFitReport {
        detections,
        recommended: suggestion,
        capsule_fit,
        warnings,
    })
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

fn rig_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Rig> {
    let spec_json: String = row.get(8)?;
    let spec: RigSpec = serde_json::from_str(&spec_json).unwrap_or_default();
    Ok(Rig {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        worktree_id: row.get(2)?,
        asset_id: row.get(3)?,
        name: row.get(4)?,
        morphology: row.get(5)?,
        fps: row.get(6)?,
        looping: row.get(7)?,
        points: spec.points,
        bones: spec.bones,
        frames: spec.frames,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

#[tauri::command]
pub fn list_rigs(
    workspace_id: String,
    worktree_id: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Rig>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let select = "SELECT id, workspace_id, worktree_id, asset_id, name, morphology, fps, looping, spec_json, created_at, updated_at FROM rigs";
    let mut statement = if worktree_id.is_some() {
        connection.prepare(&format!(
            "{select} WHERE workspace_id = ?1 AND worktree_id = ?2 ORDER BY updated_at DESC"
        ))?
    } else {
        connection.prepare(&format!(
            "{select} WHERE workspace_id = ?1 ORDER BY updated_at DESC"
        ))?
    };
    let rows = if let Some(value) = worktree_id.as_ref() {
        statement.query_map(params![workspace_id, value], rig_row)?
    } else {
        statement.query_map([&workspace_id], rig_row)?
    };
    Ok(rows.filter_map(Result::ok).collect())
}

fn rig_input_to_rig(input: RigInput, state: &AppState) -> CommandResult<Rig> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(CommandError::new("invalid_name", "Rig name cannot be empty"));
    }
    if !(1.0..=60.0).contains(&input.fps) {
        return Err(CommandError::new(
            "invalid_fps",
            "Rig FPS must be between 1 and 60",
        ));
    }
    let now = Utc::now().to_rfc3339();
    let id = input.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let existing_created_at: Option<String> = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                "SELECT created_at FROM rigs WHERE id = ?1",
                [&id],
                |row| row.get(0),
            )
            .optional()?
    };
    Ok(Rig {
        id,
        workspace_id: input.workspace_id,
        worktree_id: input.worktree_id,
        asset_id: input.asset_id,
        name: name.to_string(),
        morphology: normalize_morphology(Some(&input.morphology)),
        fps: input.fps,
        looping: input.looping,
        points: input.points,
        bones: input.bones,
        frames: input.frames,
        created_at: existing_created_at.unwrap_or_else(|| now.clone()),
        updated_at: now,
    })
}

#[tauri::command]
pub fn save_rig(input: RigInput, state: State<'_, AppState>) -> CommandResult<Rig> {
    save_rig_inner(input, &state)
}

pub(crate) fn save_rig_inner(input: RigInput, state: &AppState) -> CommandResult<Rig> {
    let rig = rig_input_to_rig(input, state)?;
    let spec = RigSpec {
        points: rig.points.clone(),
        bones: rig.bones.clone(),
        frames: rig.frames.clone(),
    };
    let spec_json = serde_json::to_string(&spec)
        .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        r#"INSERT INTO rigs(id, workspace_id, worktree_id, asset_id, name, morphology, fps, looping, spec_json, created_at, updated_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
           ON CONFLICT(id) DO UPDATE SET worktree_id=excluded.worktree_id, asset_id=excluded.asset_id,
             name=excluded.name, morphology=excluded.morphology, fps=excluded.fps, looping=excluded.looping,
             spec_json=excluded.spec_json, updated_at=excluded.updated_at"#,
        params![
            rig.id,
            rig.workspace_id,
            rig.worktree_id,
            rig.asset_id,
            rig.name,
            rig.morphology,
            rig.fps,
            rig.looping,
            spec_json,
            rig.created_at,
            rig.updated_at
        ],
    )?;
    Ok(rig)
}

#[tauri::command]
pub fn validate_rig_spec(input: RigInput, state: State<'_, AppState>) -> CommandResult<Vec<String>> {
    let rig = rig_input_to_rig(input, &state)?;
    let asset_id = rig
        .asset_id
        .clone()
        .ok_or_else(|| CommandError::new("missing_master", "Choose a source sprite first"))?;
    let asset = assets::get_asset(&state, &asset_id)?;
    validate_rig(&rig, asset.width, asset.height)
}

#[tauri::command]
pub fn delete_rig(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute("DELETE FROM rigs WHERE id = ?1", [id])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Suggestions and rendering commands
// ---------------------------------------------------------------------------

fn load_master_asset(
    state: &AppState,
    asset_id: &str,
) -> CommandResult<(crate::models::Asset, RgbaImage)> {
    let asset = assets::get_asset(state, asset_id)?;
    let image = image::open(&asset.path)?.to_rgba8();
    if image.width() < 8 || image.height() < 8 || image.width() > 512 || image.height() > 512 {
        return Err(CommandError::new(
            "invalid_master",
            "Rig masters must be between 8×8 and 512×512 pixels",
        ));
    }
    Ok((asset, image))
}

#[tauri::command]
pub fn suggest_rig_points(
    asset_id: String,
    morphology: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<RigSuggestion> {
    let (_, master) = load_master_asset(&state, &asset_id)?;
    suggest_points(&master, &normalize_morphology(morphology.as_deref()))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRigSuggestionInput {
    pub asset_id: String,
    pub morphology: Option<String>,
    pub motion: Option<String>,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
}

#[tauri::command]
pub async fn ai_suggest_rig_points(
    input: AiRigSuggestionInput,
    state: State<'_, AppState>,
) -> CommandResult<RigSuggestion> {
    let (asset, master) = load_master_asset(&state, &input.asset_id)?;
    let workspace = workspace_path(&state, &asset.workspace_id)?;
    let provider = input.provider_id.unwrap_or_else(|| "codex".to_string());
    let morphology = normalize_morphology(input.morphology.as_deref());
    let motion_intent = input.motion.as_deref().unwrap_or("").trim().to_string();
    let mut prompt = crate::sprite_harness::rig_suggestion_prompt(
        &motion_intent,
        &morphology,
        master.width(),
        master.height(),
    );
    let mut image_paths: Vec<String> = Vec::new();
    if provider == "codex" {
        image_paths.push(asset.path.clone());
    } else {
        // Non-Codex CLIs read images from the workspace by path.
        let relative = Path::new(&asset.path)
            .strip_prefix(&workspace)
            .unwrap_or(Path::new(&asset.path));
        prompt.push_str(&format!(
            "\n\nATTACHED REFERENCE FILE\nInspect this local file before answering:\n- `{}`",
            relative.to_string_lossy()
        ));
    }
    let response = crate::providers::run_agent_text_request(
        &provider,
        input.model.as_deref(),
        input.reasoning_effort.as_deref(),
        &workspace,
        &prompt,
        &image_paths,
    )
    .await?;
    parse_rig_suggestion_text(&response, master.width(), master.height()).ok_or_else(|| {
        CommandError::new(
            "rig_suggestion_missing",
            "The provider replied without a rig-suggestion JSON block. Retry, or use Auto-suggest and drag the points into place.",
        )
    })
}

fn render_rig_frames_blocking(master_path: String, rig: Rig) -> CommandResult<Vec<RgbaImage>> {
    let master = image::open(&master_path)?.to_rgba8();
    Ok(render_frames(&master, &rig))
}

#[tauri::command]
pub async fn render_rig_preview(
    input: RigInput,
    state: State<'_, AppState>,
) -> CommandResult<Vec<String>> {
    let rig = rig_input_to_rig(input, &state)?;
    let asset_id = rig
        .asset_id
        .clone()
        .ok_or_else(|| CommandError::new("missing_master", "Choose a source sprite before previewing"))?;
    let asset = assets::get_asset(&state, &asset_id)?;
    let workspace = workspace_path(&state, &rig.workspace_id)?;
    if rig.bones.is_empty() {
        return Err(CommandError::new(
            "empty_rig",
            "Add at least one bone before previewing",
        ));
    }
    let master_path = asset.path.clone();
    let asset_id_for_directory = asset.id.clone();
    let frames = tauri::async_runtime::spawn_blocking(move || {
        render_rig_frames_blocking(master_path, rig)
    })
    .await
    .map_err(|error| CommandError::new("render_failed", error.to_string()))??;
    let directory = workspace
        .join(".sprite-studio")
        .join("rig-previews")
        .join(&asset_id_for_directory);
    std::fs::create_dir_all(&directory)?;
    let mut paths = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        let path = directory.join(format!("frame_{:02}.png", index + 1));
        frame.save(&path)?;
        paths.push(path.to_string_lossy().into_owned());
    }
    Ok(paths)
}

fn existing_asset_id(state: &AppState, path: &str) -> CommandResult<Option<String>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    Ok(connection
        .query_row(
            "SELECT id FROM assets WHERE path = ?1",
            [path],
            |row| row.get(0),
        )
        .optional()?)
}

pub(crate) fn render_rig_animation_inner(
    frames: &[RgbaImage],
    rig: &Rig,
    asset: &crate::models::Asset,
    workspace: &std::path::Path,
    state: &AppState,
) -> CommandResult<RigRenderResult> {
    let render_name = rig.name.clone();
    let category = if asset.category.is_empty() {
        "props".to_string()
    } else {
        asset.category.clone()
    };
    let output_directory = workspace.join("assets").join(&category);
    std::fs::create_dir_all(&output_directory)?;
    let slug: String = render_name
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() {
                value.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() {
        "rig-animation".to_string()
    } else {
        slug.to_string()
    };
    let mut frame_paths = Vec::new();
    let mut asset_ids = Vec::new();
    let mut animation_frames = Vec::new();
    let now = Utc::now().to_rfc3339();
    for (index, frame) in frames.iter().enumerate() {
        let path = output_directory.join(format!("{}_{:02}.png", slug, index + 1));
        frame.save(&path)?;
        let path_string = path.to_string_lossy().into_owned();
        let registered = assets::inspect(
            &asset.workspace_id,
            workspace,
            &path,
            existing_asset_id(state, &path_string)?,
        )?;
        assets::upsert(state, &registered, "rig-render")?;
        let relative = path.strip_prefix(workspace).unwrap_or(&path);
        frame_paths.push(relative.to_string_lossy().into_owned());
        asset_ids.push(registered.id.clone());
        animation_frames.push(AnimationFrame {
            asset_id: registered.id,
            duration_ms: None,
        });
    }
    let manifest = GenerationManifest {
        kind: Some("sprite".to_string()),
        name: render_name.clone(),
        category: category.clone(),
        fps: rig.fps,
        files: frame_paths.clone(),
        generated_at: now.clone(),
    };
    std::fs::write(
        workspace.join(".sprite-studio").join("last-generation.json"),
        serde_json::to_vec_pretty(&manifest)
            .map_err(|error| CommandError::new("serialization_error", error.to_string()))?,
    )?;
    let animation = save_animation_inner(
        crate::models::AnimationInput {
            id: None,
            workspace_id: asset.workspace_id.clone(),
            worktree_id: rig.worktree_id.clone(),
            name: render_name,
            fps: rig.fps,
            looping: rig.looping,
            frames: animation_frames,
            motion_plan: None,
        },
        state,
    )?;
    Ok(RigRenderResult {
        animation,
        frame_paths,
        asset_ids,
    })
}

#[tauri::command]
pub async fn render_rig_animation(
    input: RigInput,
    state: State<'_, AppState>,
) -> CommandResult<RigRenderResult> {
    let rig = rig_input_to_rig(input, &state)?;
    let asset_id = rig
        .asset_id
        .clone()
        .ok_or_else(|| CommandError::new("missing_master", "Choose a source sprite before rendering"))?;
    let asset = assets::get_asset(&state, &asset_id)?;
    let workspace = workspace_path(&state, &rig.workspace_id)?;
    if rig.bones.is_empty() {
        return Err(CommandError::new(
            "empty_rig",
            "Add at least one bone before rendering",
        ));
    }
    validate_perceptible_rig_motion(&rig)?;
    let master_path = asset.path.clone();
    let render_rig = rig.clone();
    let frames = tauri::async_runtime::spawn_blocking(move || {
        render_rig_frames_blocking(master_path, render_rig)
    })
    .await
    .map_err(|error| CommandError::new("render_failed", error.to_string()))??;
    render_rig_animation_inner(&frames, &rig, &asset, &workspace, &state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_rig() -> Rig {
        Rig {
            id: "rig-1".into(),
            workspace_id: "ws".into(),
            worktree_id: None,
            asset_id: None,
            name: "test".into(),
            morphology: "biped".into(),
            fps: 8.0,
            looping: true,
            points: Vec::new(),
            bones: Vec::new(),
            frames: Vec::new(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

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

    fn solid_square(size: u32) -> RgbaImage {
        let mut image = RgbaImage::new(size, size);
        for y in 0..size {
            for x in 0..size {
                image.put_pixel(x, y, image::Rgba([200, 100, 50, 255]));
            }
        }
        image
    }

    fn rect(image: &mut RgbaImage, x0: usize, y0: usize, x1: usize, y1: usize) {
        for y in y0..y1 {
            for x in x0..x1 {
                image.put_pixel(x as u32, y as u32, image::Rgba([180, 180, 180, 255]));
            }
        }
    }

    fn biped_master() -> RgbaImage {
        let mut image = RgbaImage::new(16, 24);
        rect(&mut image, 6, 1, 10, 6); // head
        rect(&mut image, 5, 6, 11, 14); // torso
        rect(&mut image, 5, 14, 8, 23); // left leg
        rect(&mut image, 9, 14, 12, 23); // right leg
        image
    }

    fn quadruped_master() -> RgbaImage {
        let mut image = RgbaImage::new(48, 24);
        rect(&mut image, 4, 6, 44, 14); // body ends before the leg band
        rect(&mut image, 6, 13, 9, 22);
        rect(&mut image, 14, 13, 17, 22);
        rect(&mut image, 30, 13, 33, 22);
        rect(&mut image, 38, 13, 41, 22);
        image
    }

    fn snake_master() -> RgbaImage {
        let mut image = RgbaImage::new(64, 14);
        for x in 2..61 {
            let wave = (x / 8) % 2;
            rect(&mut image, x, 5 + wave, x + 1, 9 + wave);
        }
        image
    }

    fn blob_master() -> RgbaImage {
        let mut image = RgbaImage::new(32, 32);
        let center = 15.5_f64;
        for y in 0..32usize {
            for x in 0..32usize {
                let distance = ((x as f64 - center).powi(2) + (y as f64 - center).powi(2)).sqrt();
                if distance <= 10.0 {
                    image.put_pixel(x as u32, y as u32, image::Rgba([180, 180, 180, 255]));
                }
            }
        }
        image
    }

    #[test]
    fn detects_biped_from_a_tall_two_legged_silhouette() {
        let detections = detect_morphology(&biped_master()).expect("detection should run");
        assert_eq!(detections[0].morphology, "biped", "ranking: {detections:?}");
        assert!(
            detections[0].confidence >= 0.6,
            "confidence: {detections:?}"
        );
        assert!(detections[0].reasoning.contains("two separated legs"));
    }

    #[test]
    fn detects_quadruped_from_a_long_four_legged_silhouette() {
        let detections = detect_morphology(&quadruped_master()).expect("detection should run");
        assert_eq!(
            detections[0].morphology, "quadruped",
            "ranking: {detections:?}"
        );
        assert!(detections[0].confidence >= 0.6);
    }

    #[test]
    fn detects_serpentine_from_a_long_thin_body() {
        let detections = detect_morphology(&snake_master()).expect("detection should run");
        assert_eq!(
            detections[0].morphology, "serpentine",
            "ranking: {detections:?}"
        );
    }

    #[test]
    fn round_blobs_are_never_profiled_as_bipeds() {
        let detections = detect_morphology(&blob_master()).expect("detection should run");
        assert!(
            matches!(detections[0].morphology.as_str(), "amorphous" | "object"),
            "ranking: {detections:?}"
        );
    }

    #[test]
    fn template_capsules_tile_a_matching_biped_well() {
        let master = biped_master();
        let suggestion = suggest_points(&master, "biped").expect("suggestion should build");
        let coverage = capsule_coverage(&master, &suggestion);
        assert!(
            coverage > 0.55,
            "capsules should claim most of the silhouette, got {coverage}"
        );
    }

    #[test]
    fn affine_inverse_round_trips() {
        let matrix = Affine::identity()
            .chain(&Affine::translation(5.0, -3.0))
            .chain(&Affine::rotation_degrees(37.0))
            .chain(&Affine::scaling(1.5, 0.75));
        let inverse = matrix.inverse();
        for (x, y) in [(0.0, 0.0), (12.0, 4.0), (-7.0, 33.5)] {
            let (tx, ty) = matrix.apply(x, y);
            let (rx, ry) = inverse.apply(tx, ty);
            assert!((rx - x).abs() < 1e-9, "x should round trip");
            assert!((ry - y).abs() < 1e-9, "y should round trip");
        }
    }

    #[test]
    fn ownership_covers_every_opaque_pixel_once() {
        let mut rig = blank_rig();
        rig.points = vec![point("a", 8.0, 0.0), point("b", 8.0, 16.0)];
        rig.bones = vec![RigBone {
            id: "b1".into(),
            name: "limb".into(),
            start_point: "a".into(),
            end_point: "b".into(),
            radius: 8.0,
            parent: None,
            z: 1,
        }];
        let master = solid_square(16);
        let positions = point_positions(&rig, 16, 16);
        let ownership = build_ownership(&master, &rig, &positions);
        for value in ownership {
            assert_eq!(value, 0, "every opaque pixel should belong to the only bone");
        }
    }

    #[test]
    fn deform_mesh_is_deterministic_and_weights_are_normalized() {
        let mut rig = blank_rig();
        rig.points = vec![
            point("chest", 8.0, 2.0),
            point("hip", 8.0, 9.0),
            point("foot", 8.0, 15.0),
        ];
        rig.bones = vec![
            RigBone {
                id: "body".into(),
                name: "torso".into(),
                start_point: "chest".into(),
                end_point: "hip".into(),
                radius: 5.0,
                parent: None,
                z: 0,
            },
            RigBone {
                id: "leg".into(),
                name: "leg".into(),
                start_point: "hip".into(),
                end_point: "foot".into(),
                radius: 3.0,
                parent: Some("torso".into()),
                z: 1,
            },
        ];
        let master = solid_square(16);
        let positions = point_positions(&rig, 16, 16);
        let first = build_deform_mesh(&master, &rig, &positions);
        let second = build_deform_mesh(&master, &rig, &positions);
        assert_eq!(first.vertices.len(), second.vertices.len());
        assert_eq!(first.triangles.len(), second.triangles.len());
        for (left, right) in first.vertices.iter().zip(&second.vertices) {
            assert_eq!(left.source, right.source);
            assert_eq!(left.influences, right.influences);
            let total: f64 = left.influences.iter().map(|(_, weight)| *weight).sum();
            assert!((total - 1.0).abs() < 1e-9, "weights must sum to one");
            assert!(left.influences.len() <= 4, "only four influences are retained");
        }
    }

    #[test]
    fn torso_transform_deforms_a_multi_bone_sprite() {
        let mut rig = blank_rig();
        rig.points = vec![
            point("chest", 8.0, 3.0),
            point("hip", 8.0, 10.0),
            point("foot", 8.0, 15.0),
        ];
        rig.bones = vec![
            RigBone {
                id: "body".into(),
                name: "torso".into(),
                start_point: "chest".into(),
                end_point: "hip".into(),
                radius: 6.0,
                parent: None,
                z: 0,
            },
            RigBone {
                id: "leg".into(),
                name: "leg".into(),
                start_point: "hip".into(),
                end_point: "foot".into(),
                radius: 3.0,
                parent: Some("torso".into()),
                z: 1,
            },
        ];
        let identity = RigFrame {
            phase: None,
            hold: false,
            root_dx: 0.0,
            root_dy: 0.0,
            transforms: vec![],
            contacts: vec![],
        };
        let moving = RigFrame {
            transforms: vec![RigTransform {
                bone: "torso".into(),
                dx: 2.0,
                dy: 0.0,
                rotate: 15.0,
                scale_x: 1.08,
                scale_y: 0.92,
            }],
            ..identity.clone()
        };
        let master = solid_square(16);
        let positions = point_positions(&rig, 16, 16);
        let ownership = build_ownership(&master, &rig, &positions);
        let still = render_frame(&master, &rig, &identity, &ownership, &positions);
        let animated = render_frame(&master, &rig, &moving, &ownership, &positions);
        assert_eq!(
            still.as_raw(),
            master.as_raw(),
            "an identity pose must preserve the source sprite exactly"
        );
        assert_ne!(
            still.as_raw(),
            animated.as_raw(),
            "a visible torso transform must deform the rendered body"
        );
        assert!(
            animated.pixels().any(|pixel| pixel[3] > 0),
            "smooth deformation must keep visible source pixels"
        );
    }

    #[test]
    fn rotation_render_preserves_pixel_count() {
        let mut rig = blank_rig();
        rig.points = vec![point("a", 8.0, 8.0), point("b", 8.0, 1.0)];
        rig.bones = vec![RigBone {
            id: "b1".into(),
            name: "arm".into(),
            start_point: "a".into(),
            end_point: "b".into(),
            radius: 3.0,
            parent: None,
            z: 1,
        }];
        rig.frames = vec![RigFrame {
            phase: None,
            hold: false,
            root_dx: 0.0,
            root_dy: 0.0,
            transforms: vec![RigTransform {
                bone: "arm".into(),
                dx: 0.0,
                dy: 0.0,
                rotate: 90.0,
                scale_x: 1.0,
                scale_y: 1.0,
            }],
            contacts: vec![],
        }];
        let master = solid_square(16);
        let frames = render_frames(&master, &rig);
        assert_eq!(frames.len(), 1);
        let opaque_source = master.pixels().filter(|pixel| pixel[3] > 0).count();
        let opaque_rendered = frames[0].pixels().filter(|pixel| pixel[3] > 0).count();
        assert_eq!(
            opaque_source, opaque_rendered,
            "a 90° rotation should not drop pixels"
        );
    }

    #[test]
    fn render_is_deterministic() {
        let mut rig = blank_rig();
        rig.points = vec![point("a", 8.0, 2.0), point("b", 8.0, 14.0)];
        rig.bones = vec![RigBone {
            id: "b1".into(),
            name: "limb".into(),
            start_point: "a".into(),
            end_point: "b".into(),
            radius: 4.0,
            parent: None,
            z: 1,
        }];
        rig.frames = vec![
            RigFrame {
                phase: None,
                hold: false,
                root_dx: 0.0,
                root_dy: 0.0,
                transforms: vec![RigTransform {
                    bone: "limb".into(),
                    dx: 1.0,
                    dy: 0.0,
                    rotate: 12.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                }],
                contacts: vec![],
            },
            RigFrame {
                phase: None,
                hold: false,
                root_dx: 0.0,
                root_dy: 1.0,
                transforms: vec![RigTransform {
                    bone: "limb".into(),
                    dx: -1.0,
                    dy: 0.0,
                    rotate: -12.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                }],
                contacts: vec![],
            },
        ];
        let master = solid_square(16);
        let first = render_frames(&master, &rig);
        let second = render_frames(&master, &rig);
        for (a, b) in first.iter().zip(second.iter()) {
            assert_eq!(
                a.as_raw(),
                b.as_raw(),
                "identical rigs must render identical bytes"
            );
        }
    }

    #[test]
    fn final_render_rejects_numerical_but_imperceptible_pose_changes() {
        let mut rig = blank_rig();
        rig.morphology = "biped".into();
        rig.bones = vec![
            RigBone { id: "torso".into(), name: "torso".into(), start_point: "chest".into(), end_point: "hip".into(), radius: 3.0, parent: None, z: 0 },
            RigBone { id: "left".into(), name: "left_leg".into(), start_point: "hip".into(), end_point: "left_foot".into(), radius: 2.0, parent: None, z: 1 },
            RigBone { id: "right".into(), name: "right_leg".into(), start_point: "hip".into(), end_point: "right_foot".into(), radius: 2.0, parent: None, z: 2 },
        ];
        let pose = |angle: f64, body_angle: f64| RigFrame {
            phase: None,
            hold: false,
            root_dx: 0.0,
            root_dy: 0.0,
            transforms: vec![
                RigTransform { bone: "torso".into(), dx: 0.0, dy: 0.0, rotate: body_angle, scale_x: 1.0, scale_y: 1.0 },
                RigTransform { bone: "left_leg".into(), dx: 0.0, dy: 0.0, rotate: angle, scale_x: 1.0, scale_y: 1.0 },
                RigTransform { bone: "right_leg".into(), dx: 0.0, dy: 0.0, rotate: -angle, scale_x: 1.0, scale_y: 1.0 },
            ],
            contacts: vec![],
        };
        rig.frames = vec![pose(-2.0, 0.0), pose(2.0, 0.0)];
        let error = validate_perceptible_rig_motion(&rig).expect_err("tiny transforms must fail");
        assert_eq!(error.code, "imperceptible_rig_motion");

        rig.frames = vec![pose(-12.0, 0.0), pose(12.0, 0.0)];
        let error = validate_perceptible_rig_motion(&rig).expect_err("rigid body must fail");
        assert_eq!(error.code, "missing_body_motion");

        rig.frames = vec![pose(-12.0, -6.0), pose(12.0, 6.0)];
        validate_perceptible_rig_motion(&rig).expect("readable opposing poses should pass");
    }

    #[test]
    fn parent_rotation_carries_child_pixels() {
        let mut rig = blank_rig();
        rig.points = vec![point("root", 8.0, 8.0), point("mid", 8.0, 4.0), point("tip", 8.0, 0.0)];
        rig.bones = vec![
            RigBone {
                id: "b1".into(),
                name: "parent".into(),
                start_point: "root".into(),
                end_point: "mid".into(),
                radius: 2.0,
                parent: None,
                z: 1,
            },
            RigBone {
                id: "b2".into(),
                name: "child".into(),
                start_point: "mid".into(),
                end_point: "tip".into(),
                radius: 2.0,
                parent: Some("parent".into()),
                z: 2,
            },
        ];
        rig.frames = vec![RigFrame {
            phase: None,
            hold: false,
            root_dx: 0.0,
            root_dy: 0.0,
            transforms: vec![RigTransform {
                bone: "parent".into(),
                dx: 0.0,
                dy: 0.0,
                rotate: -90.0,
                scale_x: 1.0,
                scale_y: 1.0,
            }],
            contacts: vec![],
        }];
        let master = solid_square(16);
        let positions = point_positions(&rig, 16, 16);
        let ownership = build_ownership(&master, &rig, &positions);
        let rendered = render_frame(&master, &rig, &rig.frames[0], &ownership, &positions);
        // The child tip (8,0) rotates around the parent start (8,8) by -90°
        // (counter-clockwise on screen) to land at (0,8).
        let child_owner = ownership[8];
        assert_eq!(child_owner, 1, "the tip pixel belongs to the child bone");
        let expected = master.get_pixel(8, 0);
        let landed = rendered.get_pixel(0, 8);
        assert_eq!(
            expected, landed,
            "the child tip should follow the parent rotation"
        );
    }

    #[test]
    fn contact_ik_reaches_target() {
        let mut rig = blank_rig();
        rig.points = vec![point("hip", 8.0, 4.0), point("knee", 8.0, 9.0), point("foot", 8.0, 14.0)];
        rig.bones = vec![
            RigBone {
                id: "b1".into(),
                name: "thigh".into(),
                start_point: "hip".into(),
                end_point: "knee".into(),
                radius: 2.0,
                parent: None,
                z: 1,
            },
            RigBone {
                id: "b2".into(),
                name: "shin".into(),
                start_point: "knee".into(),
                end_point: "foot".into(),
                radius: 2.0,
                parent: Some("thigh".into()),
                z: 1,
            },
        ];
        // Plant the foot one pixel to the side of its bind position.
        rig.frames = vec![RigFrame {
            phase: None,
            hold: false,
            root_dx: 0.0,
            root_dy: 0.0,
            transforms: vec![],
            contacts: vec![RigContact {
                bone: "shin".into(),
                x: 10.0,
                y: 14.0,
                bend: 1.0,
            }],
        }];
        let master = solid_square(16);
        let positions = point_positions(&rig, 16, 16);
        let ownership = build_ownership(&master, &rig, &positions);
        let frame = &rig.frames[0];
        // Derive the effective transforms exactly like render_frame does.
        let mut transforms: HashMap<String, RigTransform> = HashMap::new();
        for transform in &frame.transforms {
            transforms.insert(transform.bone.clone(), transform.clone());
        }
        let index = rig.bones.iter().position(|bone| bone.name == "shin").unwrap();
        let (parent_delta, child_delta) = solve_contact_ik(&rig.bones, index, &positions, (10.0, 14.0), 1.0);
        transforms.insert(
            "thigh".into(),
            RigTransform { bone: "thigh".into(), dx: 0.0, dy: 0.0, rotate: parent_delta, scale_x: 1.0, scale_y: 1.0 },
        );
        transforms.insert(
            "shin".into(),
            RigTransform { bone: "shin".into(), dx: 0.0, dy: 0.0, rotate: child_delta, scale_x: 1.0, scale_y: 1.0 },
        );
        let mut cache = HashMap::new();
        let thigh = bone_world_affine(0, &rig.bones, &positions, &transforms, &mut cache, 0);
        let shin = bone_world_affine(1, &rig.bones, &positions, &transforms, &mut cache, 0);
        let (fx, fy) = shin.apply(8.0, 14.0);
        let distance = ((fx - 10.5).powi(2) + (fy - 14.5).powi(2)).sqrt();
        assert!(
            distance < 1.2,
            "IK should plant the foot near the target, got distance {distance}"
        );
        let rendered = render_frame(&master, &rig, frame, &ownership, &positions);
        let opaque = rendered.pixels().filter(|pixel| pixel[3] > 0).count();
        assert!(opaque > 0, "the IK frame should still render pixels");
        let _ = thigh;
    }

    #[test]
    fn suggest_points_snaps_biped_template_to_silhouette() {
        // A 16x24 figure: head, torso, and two legs.
        let mut master = RgbaImage::new(16, 24);
        for y in 1..6usize {
            for x in 6..10usize {
                master.put_pixel(x as u32, y as u32, image::Rgba([255, 0, 0, 255]));
            }
        }
        for y in 6..14usize {
            for x in 5..11usize {
                master.put_pixel(x as u32, y as u32, image::Rgba([0, 255, 0, 255]));
            }
        }
        for y in 14..23usize {
            for x in 5..8usize {
                master.put_pixel(x as u32, y as u32, image::Rgba([0, 0, 255, 255]));
            }
            for x in 9..12usize {
                master.put_pixel(x as u32, y as u32, image::Rgba([0, 0, 255, 255]));
            }
        }
        let suggestion = suggest_points(&master, "biped").expect("suggestion should succeed");
        assert_eq!(suggestion.source, "auto");
        assert!(
            suggestion.points.len() >= 10,
            "biped template should propose a full joint set"
        );
        assert!(!suggestion.bones.is_empty());
        let head_top = suggestion
            .points
            .iter()
            .find(|p| p.name == "head_top")
            .unwrap();
        let hip = suggestion.points.iter().find(|p| p.name == "hip").unwrap();
        assert!(head_top.y < 6.0, "head_top should sit inside the head region");
        assert!(
            hip.y > 6.0 && hip.y < 14.0,
            "hip should sit inside the torso region"
        );
        for point in &suggestion.points {
            assert!((0.0..16.0).contains(&point.x) && (0.0..24.0).contains(&point.y));
        }
    }

    #[test]
    fn parse_rig_suggestion_extracts_and_normalizes() {
        let text = "Here is my analysis.\n\n```rig-suggestion\n{\"morphology\":\"quadruped\",\"points\":[{\"name\":\"head\",\"x\":4,\"y\":10},{\"name\":\"tail\",\"x\":900,\"y\":12},{\"name\":\"hip\",\"x\":20,\"y\":14}],\"bones\":[{\"name\":\"body\",\"start\":\"head\",\"end\":\"hip\",\"radius\":4,\"z\":2},{\"name\":\"ghost\",\"start\":\"missing\",\"end\":\"hip\",\"radius\":2}],\"reasoning\":\"quadruped anatomy\"}\n```\nDone.";
        let suggestion = parse_rig_suggestion_text(text, 32, 32).expect("block should parse");
        assert_eq!(suggestion.morphology, "quadruped");
        assert_eq!(suggestion.points.len(), 3);
        let tail = suggestion
            .points
            .iter()
            .find(|p| p.name == "tail")
            .unwrap();
        assert_eq!(tail.x, 31.0, "out-of-canvas x clamps to the last pixel");
        assert_eq!(suggestion.bones.len(), 1, "bones with unknown points are dropped");
        assert_eq!(suggestion.bones[0].name, "body");
        assert!(suggestion.reasoning.contains("quadruped"));
    }

    #[test]
    fn parse_rig_suggestion_accepts_plain_json_block() {
        let text = "```json\n{\"morphology\":\"biped\",\"points\":[{\"name\":\"a\",\"x\":1,\"y\":1},{\"name\":\"b\",\"x\":9,\"y\":9}],\"bones\":[{\"name\":\"bone\",\"start\":\"a\",\"end\":\"b\",\"radius\":3}],\"frames\":[{\"phase\":\"contact\",\"transforms\":[{\"bone\":\"bone\",\"rotate\":15}]}]}\n```";
        let suggestion = parse_rig_suggestion_text(text, 16, 16).expect("json block should parse");
        assert_eq!(suggestion.frames.len(), 1);
        assert_eq!(suggestion.frames[0].transforms[0].rotate, 15.0);
    }

    #[test]
    fn validate_rig_reports_cycle_and_missing_references() {
        let mut rig = blank_rig();
        rig.points = vec![point("a", 1.0, 1.0)];
        rig.bones = vec![
            RigBone {
                id: "b1".into(),
                name: "one".into(),
                start_point: "a".into(),
                end_point: "missing".into(),
                radius: 2.0,
                parent: Some("two".into()),
                z: 1,
            },
            RigBone {
                id: "b2".into(),
                name: "two".into(),
                start_point: "a".into(),
                end_point: "a".into(),
                radius: 2.0,
                parent: Some("one".into()),
                z: 1,
            },
        ];
        let warnings = validate_rig(&rig, 16, 16).expect("validation should succeed");
        assert!(
            warnings.iter().any(|w| w.contains("cycle")),
            "warnings: {warnings:?}"
        );
        assert!(warnings.iter().any(|w| w.contains("does not exist")));
    }

    #[test]
    fn rig_persists_renders_into_assets_and_creates_an_animation() {
        let root =
            std::env::temp_dir().join(format!("sprite-studio-rig-pipeline-{}", Uuid::new_v4()));
        // Match the scaffold workspace_path() guarantees for initialized projects.
        for folder in ["assets/props", "animations", ".sprite-studio"] {
            std::fs::create_dir_all(root.join(folder)).expect("fixture folders");
        }
        let connection = crate::database::open(&root.join("app.sqlite3")).expect("database");
        let state = AppState {
            db: std::sync::Mutex::new(connection),
            cancellers: std::sync::Mutex::new(HashMap::new()),
        };
        state
            .db
            .lock()
            .expect("database lock")
            .execute(
                "INSERT INTO projects(id,name,path,created_at,last_opened_at) VALUES ('w1','Test',?1,'now','now')",
                [root.to_string_lossy().into_owned()],
            )
            .expect("project row");
        let master_path = root.join("assets/props/master.png");
        solid_square(16).save(&master_path).expect("master png");
        let asset = assets::inspect("w1", &root, &master_path, None).expect("asset inspect");
        assets::upsert(&state, &asset, "import").expect("asset upsert");
        let input = RigInput {
            id: None,
            workspace_id: "w1".into(),
            worktree_id: None,
            asset_id: Some(asset.id.clone()),
            name: "rig walk".into(),
            morphology: "biped".into(),
            fps: 8.0,
            looping: true,
            points: vec![point("a", 8.0, 2.0), point("b", 8.0, 14.0)],
            bones: vec![RigBone {
                id: "b1".into(),
                name: "limb".into(),
                start_point: "a".into(),
                end_point: "b".into(),
                radius: 4.0,
                parent: None,
                z: 1,
            }],
            frames: vec![
                RigFrame {
                    phase: Some("swing".into()),
                    hold: false,
                    root_dx: 0.0,
                    root_dy: 0.0,
                    transforms: vec![RigTransform { bone: "limb".into(), dx: 0.0, dy: 0.0, rotate: 10.0, scale_x: 1.0, scale_y: 1.0 }],
                    contacts: vec![],
                },
                RigFrame {
                    phase: Some("swing back".into()),
                    hold: false,
                    root_dx: 0.0,
                    root_dy: 0.0,
                    transforms: vec![RigTransform { bone: "limb".into(), dx: 0.0, dy: 0.0, rotate: -10.0, scale_x: 1.0, scale_y: 1.0 }],
                    contacts: vec![],
                },
            ],
        };
        let saved = save_rig_inner(input, &state).expect("rig save");
        let rig_rows: i64 = state
            .db
            .lock()
            .expect("database lock")
            .query_row("SELECT COUNT(*) FROM rigs", [], |row| row.get(0))
            .expect("rig count");
        assert_eq!(rig_rows, 1, "the rig should persist with its spec");

        let master = image::open(&master_path).expect("master reload").to_rgba8();
        let frames = render_frames(&master, &saved);
        assert_eq!(frames.len(), 2);
        let result =
            render_rig_animation_inner(&frames, &saved, &asset, &root, &state).expect("render");
        assert_eq!(result.animation.frames.len(), 2);
        assert_eq!(result.animation.fps, 8.0);
        assert!(root.join("assets/props/rig-walk_01.png").exists());
        assert!(root.join("assets/props/rig-walk_02.png").exists());
        let manifest_text = std::fs::read_to_string(root.join(".sprite-studio/last-generation.json"))
            .expect("manifest file");
        let manifest: GenerationManifest =
            serde_json::from_str(&manifest_text).expect("manifest json");
        assert_eq!(manifest.files.len(), 2);
        assert_eq!(manifest.category, "props");
        let asset_rows: i64 = state
            .db
            .lock()
            .expect("database lock")
            .query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))
            .expect("asset count");
        assert_eq!(asset_rows, 3, "master plus two rendered frames");
        let animation_rows: i64 = state
            .db
            .lock()
            .expect("database lock")
            .query_row("SELECT COUNT(*) FROM animations", [], |row| row.get(0))
            .expect("animation count");
        assert_eq!(animation_rows, 1, "rendered rigs become animations");
        drop(state);
        std::fs::remove_dir_all(root).expect("temporary fixture should be removable");
    }
}
