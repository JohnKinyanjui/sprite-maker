use image::{Rgba, RgbaImage};
use rayon::prelude::*;
use std::collections::HashMap;

use super::ik::{
    bone_world_affine, build_ownership, identity_transform, owned_bounds, point_positions,
    solve_contact_ik, Affine,
};
use super::mesh::{build_deform_mesh, DeformMesh};
use super::types::{Rig, RigFrame, RigTransform};

fn edge(a: (f64, f64), b: (f64, f64), point: (f64, f64)) -> f64 {
    (point.0 - a.0) * (b.1 - a.1) - (point.1 - a.1) * (b.0 - a.0)
}

pub(super) fn blend_rgba(existing: Rgba<u8>, incoming: Rgba<u8>) -> Rgba<u8> {
    let source_alpha = incoming[3] as f32 / 255.0;
    if source_alpha <= 0.0 {
        return existing;
    }
    let destination_alpha = existing[3] as f32 / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    if output_alpha <= 0.0 {
        return Rgba([0, 0, 0, 0]);
    }
    let blend_channel = |source: u8, destination: u8| {
        let source_value = source as f32 / 255.0;
        let destination_value = destination as f32 / 255.0;
        ((source_value * source_alpha + destination_value * destination_alpha * (1.0 - source_alpha))
            / output_alpha
            * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Rgba([
        blend_channel(incoming[0], existing[0]),
        blend_channel(incoming[1], existing[1]),
        blend_channel(incoming[2], existing[2]),
        (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8,
    ])
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
                    .clamp(0.0, width.saturating_sub(1) as f64)
                    as u32;
                let source_y = (source[0].1 * w0 + source[1].1 * w1 + source[2].1 * w2)
                    .floor()
                    .clamp(0.0, height.saturating_sub(1) as f64)
                    as u32;
                let pixel = *master.get_pixel(source_x, source_y);
                if pixel[3] > 0 {
                    let destination = *canvas.get_pixel(x as u32, y as u32);
                    canvas.put_pixel(x as u32, y as u32, blend_rgba(destination, pixel));
                }
            }
        }
    }
    canvas
}

/// Per-bone transforms for one frame, with contact IK folded into rotations.
pub(super) fn resolve_frame_transforms(
    rig: &Rig,
    frame: &RigFrame,
    positions: &HashMap<String, (f64, f64)>,
) -> HashMap<String, RigTransform> {
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
    transforms
}

pub(super) fn render_frame_with_mesh(
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
    let transforms = resolve_frame_transforms(rig, frame, positions);
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
        let bound_min_x = corners
            .iter()
            .map(|(x, _)| *x)
            .fold(f64::INFINITY, f64::min);
        let bound_min_y = corners
            .iter()
            .map(|(_, y)| *y)
            .fold(f64::INFINITY, f64::min);
        let bound_max_x = corners
            .iter()
            .map(|(x, _)| *x)
            .fold(f64::NEG_INFINITY, f64::max);
        let bound_max_y = corners
            .iter()
            .map(|(_, y)| *y)
            .fold(f64::NEG_INFINITY, f64::max);
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

pub(crate) fn render_frames(master: &RgbaImage, rig: &Rig) -> Vec<RgbaImage> {
    let positions = point_positions(rig, master.width(), master.height());
    let ownership = build_ownership(master, rig, &positions);
    let mesh = (rig.bones.len() > 1).then(|| build_deform_mesh(master, rig, &positions));
    if rig.frames.is_empty() {
        return vec![master.clone()];
    }
    let mut rendered: Vec<Option<RgbaImage>> = vec![None; rig.frames.len()];
    for (index, image) in (0..rig.frames.len())
        .into_par_iter()
        .filter_map(|index| {
            if rig.frames[index].hold {
                return None;
            }
            Some((
                index,
                render_frame_with_mesh(
                    master,
                    rig,
                    &rig.frames[index],
                    &ownership,
                    &positions,
                    mesh.as_ref(),
                ),
            ))
        })
        .collect::<Vec<_>>()
    {
        rendered[index] = Some(image);
    }
    let mut frames = Vec::with_capacity(rig.frames.len());
    for slot in &mut rendered {
        if let Some(image) = slot.take() {
            frames.push(image);
        } else {
            frames.push(frames.last().cloned().unwrap_or_else(|| master.clone()));
        }
    }
    frames
}
