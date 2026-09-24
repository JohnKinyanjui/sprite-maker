//! Renders a parts rig: every bone moves its own complete part image, so the
//! far limb keeps its pixels when the near limb swings away and no joint
//! tears. Rotated parts are sampled RotSprite-style from an 8× Scale2x
//! upscale, which keeps pixel-art outlines continuous instead of jagged.

use image::{Rgba, RgbaImage};
use rayon::prelude::*;
use std::collections::HashMap;

use super::ik::{bone_world_affine, point_positions, Affine};
use super::parts::{PartLayer, PartsRig};
use super::skin::{blend_rgba, resolve_frame_transforms};
use super::types::{Rig, RigFrame};

const UPSCALE: u32 = 8;

/// One Scale2x (EPX) pass: doubles the image while keeping hard pixel edges
/// and smoothing diagonal staircases.
fn scale2x(image: &RgbaImage) -> RgbaImage {
    let (width, height) = image.dimensions();
    let transparent = Rgba([0, 0, 0, 0]);
    let at = |x: i64, y: i64| {
        if x < 0 || y < 0 || x >= width as i64 || y >= height as i64 {
            transparent
        } else {
            *image.get_pixel(x as u32, y as u32)
        }
    };
    let mut out = RgbaImage::new(width * 2, height * 2);
    for y in 0..height as i64 {
        for x in 0..width as i64 {
            let e = at(x, y);
            let (b, d, f, h) = (at(x, y - 1), at(x - 1, y), at(x + 1, y), at(x, y + 1));
            let (e0, e1, e2, e3) = if b != h && d != f {
                (
                    if d == b { d } else { e },
                    if b == f { f } else { e },
                    if d == h { d } else { e },
                    if h == f { f } else { e },
                )
            } else {
                (e, e, e, e)
            };
            let (ox, oy) = (x as u32 * 2, y as u32 * 2);
            out.put_pixel(ox, oy, e0);
            out.put_pixel(ox + 1, oy, e1);
            out.put_pixel(ox, oy + 1, e2);
            out.put_pixel(ox + 1, oy + 1, e3);
        }
    }
    out
}

struct PreparedLayer<'a> {
    layer: &'a PartLayer,
    upscaled: RgbaImage,
}

fn is_axis_aligned(world: &Affine) -> bool {
    (world.a - 1.0).abs() < 1e-3
        && world.b.abs() < 1e-3
        && world.c.abs() < 1e-3
        && (world.d - 1.0).abs() < 1e-3
}

fn render_parts_frame(
    size: (u32, u32),
    rig: &Rig,
    frame: &RigFrame,
    layers: &HashMap<&str, PreparedLayer<'_>>,
    positions: &HashMap<String, (f64, f64)>,
) -> RgbaImage {
    let (width, height) = size;
    let mut canvas = RgbaImage::new(width, height);
    let transforms = resolve_frame_transforms(rig, frame, positions);
    let root = Affine::translation(frame.root_dx, frame.root_dy);
    let mut cache = HashMap::new();
    let mut order: Vec<usize> = (0..rig.bones.len()).collect();
    order.sort_by_key(|index| (rig.bones[*index].z, *index));
    for index in order {
        let Some(prepared) = layers.get(rig.bones[index].name.as_str()) else {
            continue;
        };
        let layer = prepared.layer;
        let world = bone_world_affine(index, &rig.bones, positions, &transforms, &mut cache, 0)
            .chain(&root);
        let inverse = world.inverse();
        let (lw, lh) = (layer.image.width() as f64, layer.image.height() as f64);
        let (lx, ly) = (layer.x as f64, layer.y as f64);
        let corners = [
            world.apply(lx - 1.0, ly - 1.0),
            world.apply(lx + lw + 1.0, ly - 1.0),
            world.apply(lx - 1.0, ly + lh + 1.0),
            world.apply(lx + lw + 1.0, ly + lh + 1.0),
        ];
        let min_x = corners.iter().map(|point| point.0).fold(f64::INFINITY, f64::min).floor().max(0.0) as i64;
        let min_y = corners.iter().map(|point| point.1).fold(f64::INFINITY, f64::min).floor().max(0.0) as i64;
        let max_x = corners.iter().map(|point| point.0).fold(f64::NEG_INFINITY, f64::max).ceil().min(width as f64 - 1.0) as i64;
        let max_y = corners.iter().map(|point| point.1).fold(f64::NEG_INFINITY, f64::max).ceil().min(height as f64 - 1.0) as i64;
        let rotated = !is_axis_aligned(&world);
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let (sx, sy) = inverse.apply(x as f64 + 0.5, y as f64 + 0.5);
                let (local_x, local_y) = (sx - lx, sy - ly);
                if local_x < 0.0 || local_y < 0.0 || local_x >= lw || local_y >= lh {
                    continue;
                }
                let pixel = if rotated {
                    let ux = ((local_x * UPSCALE as f64).floor() as u32).min(prepared.upscaled.width() - 1);
                    let uy = ((local_y * UPSCALE as f64).floor() as u32).min(prepared.upscaled.height() - 1);
                    *prepared.upscaled.get_pixel(ux, uy)
                } else {
                    *layer.image.get_pixel(local_x.floor() as u32, local_y.floor() as u32)
                };
                if pixel[3] > 0 {
                    let existing = *canvas.get_pixel(x as u32, y as u32);
                    canvas.put_pixel(x as u32, y as u32, blend_rgba(existing, pixel));
                }
            }
        }
    }
    canvas
}

/// Render every rig frame by moving whole parts. Bones without a part layer
/// draw nothing; hold frames repeat the previous frame.
pub(crate) fn render_parts_frames(size: (u32, u32), rig: &Rig, parts: &PartsRig) -> Vec<RgbaImage> {
    let positions = point_positions(rig, size.0, size.1);
    let layers: HashMap<&str, PreparedLayer<'_>> = parts
        .layers
        .iter()
        .map(|layer| {
            let upscaled = (0..UPSCALE.trailing_zeros()).fold(layer.image.clone(), |image, _| scale2x(&image));
            (layer.bone.as_str(), PreparedLayer { layer, upscaled })
        })
        .collect();
    if rig.frames.is_empty() {
        return vec![render_parts_frame(size, rig, &RigFrame {
            phase: None,
            hold: false,
            root_dx: 0.0,
            root_dy: 0.0,
            transforms: Vec::new(),
            contacts: Vec::new(),
        }, &layers, &positions)];
    }
    let rendered: Vec<Option<RgbaImage>> = rig
        .frames
        .par_iter()
        .map(|frame| (!frame.hold).then(|| render_parts_frame(size, rig, frame, &layers, &positions)))
        .collect();
    let mut frames: Vec<RgbaImage> = Vec::with_capacity(rendered.len());
    for slot in rendered {
        let image = slot
            .or_else(|| frames.last().cloned())
            .unwrap_or_else(|| RgbaImage::new(size.0, size.1));
        frames.push(image);
    }
    frames
}

/// True when the rig was built on the assembled skeleton: every part's bone
/// exists and pivots where the part was placed. A rig planned on its own
/// skeleton (or edited far away from it) keeps the classic renderer.
pub(crate) fn rig_uses_parts(rig: &Rig, parts: &PartsRig) -> bool {
    let position = |points: &[super::types::RigPoint], name: &str| {
        points.iter().find(|point| point.name == name).map(|point| (point.x, point.y))
    };
    parts.bones.iter().all(|part_bone| {
        rig.bones.iter().any(|bone| {
            bone.name == part_bone.name
                && bone.start_point == part_bone.start_point
                && match (position(&rig.points, &bone.start_point), position(&parts.points, &part_bone.start_point)) {
                    (Some(a), Some(b)) => (a.0 - b.0).abs() <= 1.5 && (a.1 - b.1).abs() <= 1.5,
                    _ => false,
                }
        })
    })
}

#[cfg(test)]
pub(crate) fn scale2x_for_tests(image: &RgbaImage) -> RgbaImage {
    scale2x(image)
}

/// Render a rig against its master image, moving whole parts when the master
/// was assembled from a parts sheet and warping the master otherwise.
pub(crate) fn render_master_frames(
    master_path: &std::path::Path,
    master: &RgbaImage,
    rig: &Rig,
) -> Vec<RgbaImage> {
    if let Some(parts) = super::parts::load_for_master(master_path) {
        if rig_uses_parts(rig, &parts) {
            return render_parts_frames(master.dimensions(), rig, &parts);
        }
    }
    super::skin::render_frames(master, rig)
}
