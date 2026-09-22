use crate::error::{CommandError, CommandResult};
use image::{Rgba, RgbaImage};
use std::collections::HashMap;
use std::path::Path;

const KEY_THRESHOLD_SQ: f64 = 72.0 * 72.0;
const VISIBLE_ALPHA: u8 = 64;
const OPAQUE_ALPHA: u8 = 255;
const BRIGHT_FRINGE_LUMINANCE: f64 = 190.0;
const OPAQUE_BRIGHT_EDGE_LUMINANCE: f64 = 215.0;

pub(crate) fn extract_palette(master: &RgbaImage, max_colors: usize) -> Vec<[u8; 3]> {
    let mut counts: HashMap<[u8; 3], u32> = HashMap::new();
    for pixel in master.pixels() {
        if pixel[3] >= 128 {
            let rgb = [pixel[0], pixel[1], pixel[2]];
            *counts.entry(rgb).or_insert(0) += 1;
        }
    }
    let mut sorted = counts.into_iter().collect::<Vec<_>>();
    sorted.sort_unstable_by_key(|entry| std::cmp::Reverse(entry.1));
    sorted
        .into_iter()
        .take(max_colors)
        .map(|(rgb, _)| rgb)
        .collect()
}

fn corner_pixels(image: &RgbaImage) -> [Rgba<u8>; 4] {
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return [Rgba([0, 0, 0, 0]); 4];
    }
    [
        *image.get_pixel(0, 0),
        *image.get_pixel(width - 1, 0),
        *image.get_pixel(0, height - 1),
        *image.get_pixel(width - 1, height - 1),
    ]
}

fn detect_background(image: &RgbaImage) -> (bool, [u8; 3]) {
    let corners = corner_pixels(image);
    let alpha_sum: u32 = corners.iter().map(|pixel| pixel[3] as u32).sum();
    let has_native_alpha = alpha_sum <= 64;
    let key = [
        (corners.iter().map(|pixel| pixel[0] as u32).sum::<u32>() / 4) as u8,
        (corners.iter().map(|pixel| pixel[1] as u32).sum::<u32>() / 4) as u8,
        (corners.iter().map(|pixel| pixel[2] as u32).sum::<u32>() / 4) as u8,
    ];
    (has_native_alpha, key)
}

fn color_distance_squared(rgb: [u8; 3], key: [u8; 3]) -> f64 {
    let red = rgb[0] as f64 - key[0] as f64;
    let green = rgb[1] as f64 - key[1] as f64;
    let blue = rgb[2] as f64 - key[2] as f64;
    red * red + green * green + blue * blue
}

fn nearest_palette(rgb: [u8; 3], palette: &[[u8; 3]]) -> [u8; 3] {
    palette
        .iter()
        .min_by_key(|color| {
            let red = rgb[0] as i32 - color[0] as i32;
            let green = rgb[1] as i32 - color[1] as i32;
            let blue = rgb[2] as i32 - color[2] as i32;
            red * red + green * green + blue * blue
        })
        .copied()
        .unwrap_or(rgb)
}

#[allow(dead_code)]
pub(crate) fn alpha_coverage(image: &RgbaImage) -> f64 {
    let visible = image.pixels().filter(|pixel| pixel[3] > 8).count();
    visible as f64 / (image.width() as f64 * image.height() as f64).max(1.0)
}

fn neighbors_4(x: u32, y: u32, width: u32, height: u32) -> Vec<(u32, u32)> {
    let mut neighbors = Vec::with_capacity(4);
    if x > 0 {
        neighbors.push((x - 1, y));
    }
    if y > 0 {
        neighbors.push((x, y - 1));
    }
    if x + 1 < width {
        neighbors.push((x + 1, y));
    }
    if y + 1 < height {
        neighbors.push((x, y + 1));
    }
    neighbors
}

fn touches_transparent(image: &RgbaImage, x: u32, y: u32) -> bool {
    let (width, height) = image.dimensions();
    neighbors_4(x, y, width, height)
        .iter()
        .any(|(neighbor_x, neighbor_y)| image.get_pixel(*neighbor_x, *neighbor_y)[3] == 0)
}

fn pixel_luminance(pixel: Rgba<u8>) -> f64 {
    0.299 * pixel[0] as f64 + 0.587 * pixel[1] as f64 + 0.114 * pixel[2] as f64
}

fn is_edge_fringe_pixel(image: &RgbaImage, x: u32, y: u32, pixel: Rgba<u8>) -> bool {
    if pixel[3] == 0 || !touches_transparent(image, x, y) {
        return false;
    }
    let luminance = pixel_luminance(pixel);
    pixel[3] < VISIBLE_ALPHA
        || (luminance > BRIGHT_FRINGE_LUMINANCE && pixel[3] < OPAQUE_ALPHA)
        || luminance > OPAQUE_BRIGHT_EDGE_LUMINANCE
}

fn defringe_pass(image: &mut RgbaImage) {
    let (width, height) = image.dimensions();
    let mut to_clear = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let pixel = *image.get_pixel(x, y);
            if is_edge_fringe_pixel(image, x, y, pixel) {
                to_clear.push((x, y));
            }
        }
    }
    for (x, y) in to_clear {
        image.put_pixel(x, y, Rgba([0, 0, 0, 0]));
    }
}

pub(crate) fn defringe(image: &mut RgbaImage) {
    for _ in 0..2 {
        defringe_pass(image);
    }
}

pub(crate) fn remove_orphan_pixels(image: &mut RgbaImage) {
    let (width, height) = image.dimensions();
    let mut to_clear = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let pixel = *image.get_pixel(x, y);
            if pixel[3] < VISIBLE_ALPHA {
                continue;
            }
            let has_neighbor = neighbors_4(x, y, width, height).iter().any(|(neighbor_x, neighbor_y)| {
                image.get_pixel(*neighbor_x, *neighbor_y)[3] >= VISIBLE_ALPHA
            });
            if has_neighbor {
                continue;
            }
            let luminance = pixel_luminance(pixel);
            if luminance > 160.0 || pixel[3] < OPAQUE_ALPHA {
                to_clear.push((x, y));
            }
        }
    }
    for (x, y) in to_clear {
        image.put_pixel(x, y, Rgba([0, 0, 0, 0]));
    }
}

pub(crate) fn normalize_sprite_alpha(
    image: &RgbaImage,
    master_palette: Option<&[[u8; 3]]>,
) -> RgbaImage {
    let (width, height) = image.dimensions();
    let (has_native_alpha, key) = detect_background(image);
    let palette = master_palette.unwrap_or(&[]);
    let mut output = image.clone();

    for y in 0..height {
        for x in 0..width {
            let pixel = *output.get_pixel(x, y);
            let visible = if has_native_alpha {
                pixel[3] >= VISIBLE_ALPHA
            } else {
                color_distance_squared([pixel[0], pixel[1], pixel[2]], key) > KEY_THRESHOLD_SQ
            };
            if !visible {
                output.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
    }

    defringe(&mut output);

    for y in 0..height {
        for x in 0..width {
            let pixel = *output.get_pixel(x, y);
            if pixel[3] == 0 {
                continue;
            }
            if !palette.is_empty() {
                let mapped = nearest_palette([pixel[0], pixel[1], pixel[2]], palette);
                output.put_pixel(x, y, Rgba([mapped[0], mapped[1], mapped[2], OPAQUE_ALPHA]));
            } else if has_native_alpha && pixel[3] < OPAQUE_ALPHA {
                output.put_pixel(x, y, Rgba([pixel[0], pixel[1], pixel[2], OPAQUE_ALPHA]));
            }
        }
    }

    defringe(&mut output);
    remove_orphan_pixels(&mut output);
    output
}

pub(crate) fn normalize_sprite_file(path: &Path, master_path: Option<&Path>) -> CommandResult<bool> {
    let image = image::open(path)?.to_rgba8();
    let palette = if let Some(master_path) = master_path {
        let master = image::open(master_path)?.to_rgba8();
        extract_palette(&master, 64)
    } else {
        Vec::new()
    };
    let normalized = normalize_sprite_alpha(&image, Some(&palette));
    if normalized.as_raw() != image.as_raw() {
        normalized.save(path).map_err(CommandError::from)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[allow(dead_code)]
    fn solid_image(width: u32, height: u32, color: Rgba<u8>) -> RgbaImage {
        RgbaImage::from_pixel(width, height, color)
    }

    #[test]
    fn removes_opaque_black_background() {
        let mut image = RgbaImage::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                let pixel = if x == 1 && y == 1 {
                    Rgba([40, 80, 120, 255])
                } else {
                    Rgba([0, 0, 0, 255])
                };
                image.put_pixel(x, y, pixel);
            }
        }
        let normalized = normalize_sprite_alpha(&image, None);
        assert_eq!(normalized.get_pixel(0, 0)[3], 0);
        assert_eq!(normalized.get_pixel(1, 1)[3], 255);
        assert!(alpha_coverage(&normalized) < 0.5);
    }

    #[test]
    fn removes_white_fringe_and_orphans() {
        let mut image = RgbaImage::new(5, 5);
        for pixel in image.pixels_mut() {
            *pixel = Rgba([0, 0, 0, 0]);
        }
        image.put_pixel(2, 2, Rgba([40, 80, 120, 255]));
        image.put_pixel(3, 2, Rgba([255, 255, 255, 180]));
        image.put_pixel(0, 0, Rgba([200, 200, 200, 255]));
        let normalized = normalize_sprite_alpha(&image, None);
        assert_eq!(normalized.get_pixel(0, 0)[3], 0);
        assert_eq!(normalized.get_pixel(3, 2)[3], 0);
        assert_eq!(normalized.get_pixel(2, 2)[3], 255);
    }

    #[test]
    fn removes_opaque_white_dots_touching_transparency() {
        let mut image = RgbaImage::new(5, 5);
        for pixel in image.pixels_mut() {
            *pixel = Rgba([0, 0, 0, 0]);
        }
        image.put_pixel(2, 2, Rgba([120, 30, 30, 255]));
        image.put_pixel(3, 2, Rgba([255, 255, 255, 255]));
        image.put_pixel(2, 3, Rgba([255, 255, 255, 255]));
        let normalized = normalize_sprite_alpha(&image, None);
        assert_eq!(normalized.get_pixel(3, 2)[3], 0);
        assert_eq!(normalized.get_pixel(2, 3)[3], 0);
        assert_eq!(normalized.get_pixel(2, 2)[3], 255);
    }
}
