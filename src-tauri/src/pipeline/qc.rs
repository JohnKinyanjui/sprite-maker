use crate::models::StripScoreReport;
use image::{imageops::FilterType, GenericImageView, RgbaImage};

use super::segment::{
    dp_horizontal_segments, equal_horizontal_segments, equal_vertical_segment_heights,
    infer_frame_count_from_image, segments_cover_width,
};

/// Vertical sum of opaque pixels per column — used to find low-alpha gutters between strip cells.
pub(crate) fn vertical_alpha_profile(image: &RgbaImage) -> Vec<u32> {
    let (width, height) = image.dimensions();
    let mut profile = vec![0u32; width as usize];
    for x in 0..width {
        for y in 0..height {
            if image.get_pixel(x, y)[3] > 8 {
                profile[x as usize] += 1;
            }
        }
    }
    profile
}

/// Pick horizontal cut boundaries near equal-width targets but biased toward alpha valleys.
pub(crate) fn profile_horizontal_segments(
    width: u32,
    profile: &[u32],
    frame_count: u32,
) -> Vec<(u32, u32)> {
    let frame_count = frame_count.max(1);
    let mut boundaries = vec![0_u32];
    for index in 1..frame_count {
        let ideal = (width * index) / frame_count;
        let margin = (width / (frame_count * 4)).max(2);
        let start = ideal.saturating_sub(margin);
        let end = (ideal + margin).min(width.saturating_sub(1));
        let mut best = ideal;
        let mut best_value = u32::MAX;
        for x in start..=end {
            let value = profile.get(x as usize).copied().unwrap_or(u32::MAX);
            if value < best_value {
                best_value = value;
                best = x;
            }
        }
        boundaries.push(best);
    }
    boundaries.push(width);
    let mut segments = Vec::with_capacity(frame_count as usize);
    for index in 0..frame_count as usize {
        let start = boundaries[index];
        let end = boundaries.get(index + 1).copied().unwrap_or(width);
        if end > start {
            segments.push((start, end - start));
        }
    }
    segments
}

fn column_has_grid_ink(image: &RgbaImage, x: u32) -> bool {
    let (width, height) = image.dimensions();
    if x >= width {
        return false;
    }
    let mut dark_opaque = 0_u32;
    let mut samples = 0_u32;
    for offset in -1i32..=1 {
        let column = x as i32 + offset;
        if column < 0 || column as u32 >= width {
            continue;
        }
        for y in 0..height {
            let pixel = image.get_pixel(column as u32, y);
            if pixel[3] > 32 {
                samples += 1;
                if pixel[0] < 48 && pixel[1] < 48 && pixel[2] < 48 {
                    dark_opaque += 1;
                }
            }
        }
    }
    samples > 0 && dark_opaque * 100 / samples > 70
}

pub(crate) fn detect_strip_grid_ink(image: &RgbaImage, frame_count: u32) -> bool {
    if frame_count < 2 {
        return false;
    }
    let (width, height) = image.dimensions();
    if width < frame_count * 4 || height < 4 {
        return false;
    }
    let cell_width = width / frame_count;
    let mut inked_gutters = 0_u32;
    for index in 1..frame_count {
        let x = index * cell_width;
        if x >= width {
            break;
        }
        if column_has_grid_ink(image, x) {
            inked_gutters += 1;
        }
    }
    inked_gutters >= frame_count / 2
}

pub(crate) fn detect_profile_grid_ink(
    image: &RgbaImage,
    boundaries: &[u32],
) -> bool {
    if boundaries.len() < 3 {
        return false;
    }
    let inked = boundaries
        .iter()
        .skip(1)
        .take(boundaries.len() - 2)
        .filter(|x| column_has_grid_ink(image, **x))
        .count();
    inked >= boundaries.len() / 3
}

pub(crate) fn perceptual_hash_distance(left: u64, right: u64) -> u32 {
    (left ^ right).count_ones()
}

pub(crate) fn perceptual_hash(image: &RgbaImage) -> u64 {
    let gray = image::imageops::grayscale(image);
    let small = image::imageops::resize(&gray, 8, 8, FilterType::Triangle);
    let average = small.pixels().map(|pixel| pixel[0] as u64).sum::<u64>() / 64;
    let mut hash = 0_u64;
    for (index, pixel) in small.pixels().enumerate() {
        if pixel[0] as u64 >= average {
            hash |= 1_u64 << index;
        }
    }
    hash
}

fn preview_segments(
    image: &RgbaImage,
    layout: &str,
    frame_count: u32,
) -> (Vec<(u32, u32)>, String) {
    let (width, height) = image.dimensions();
    let profile = vertical_alpha_profile(image);
    let layout = layout.trim().to_ascii_lowercase();

    if layout == "horizontal" || layout == "strip" {
        return (
            equal_horizontal_segments(width, frame_count),
            layout.clone(),
        );
    }

    if layout == "vertical" {
        let cell_heights = equal_vertical_segment_heights(height, frame_count);
        let segments = cell_heights
            .iter()
            .enumerate()
            .map(|(index, cell_height)| (index as u32 * *cell_height, *cell_height))
            .collect();
        return (segments, "vertical".into());
    }

    if layout == "grid" {
        return (
            equal_horizontal_segments(width, frame_count),
            "grid".into(),
        );
    }

    if layout == "dp" {
        let segments = dp_horizontal_segments(width, &profile, frame_count);
        return (segments, "dp".into());
    }

    let profile_segments = profile_horizontal_segments(width, &profile, frame_count);
    if segments_cover_width(&profile_segments, width) {
        return (profile_segments, "profile".into());
    }

    let dp_segments = dp_horizontal_segments(width, &profile, frame_count);
    if segments_cover_width(&dp_segments, width) {
        return (dp_segments, "dp".into());
    }

    (profile_segments, "profile".into())
}

pub(crate) fn score_strip_image(
    image: &RgbaImage,
    frame_count: Option<u32>,
    layout: Option<&str>,
) -> StripScoreReport {
    let (width, height) = image.dimensions();
    let inference = infer_frame_count_from_image(image);
    let frame_count_used = frame_count
        .filter(|value| *value > 0)
        .unwrap_or(inference.suggested_frame_count)
        .max(1);
    let layout = layout.unwrap_or("auto");
    let (segments, layout_used) = preview_segments(image, layout, frame_count_used);

    let segment_widths = segments.iter().map(|(_, w)| *w).collect::<Vec<_>>();
    let min_cell_width = segment_widths.iter().copied().min().unwrap_or(0);

    let mut boundaries = vec![0_u32];
    let mut cursor = 0_u32;
    for (_, segment_width) in &segments {
        cursor += *segment_width;
        if cursor < width {
            boundaries.push(cursor);
        }
    }
    boundaries.push(width);

    let grid_ink_detected = detect_profile_grid_ink(image, &boundaries)
        || detect_strip_grid_ink(image, frame_count_used);

    let mut mean_motion_delta = 0.0;
    if segments.len() >= 2 {
        let mut hashes = Vec::new();
        for (x, segment_width) in &segments {
            let cell = image.view(*x, 0, *segment_width, height).to_image();
            hashes.push(perceptual_hash(&cell));
        }
        let total = hashes
            .windows(2)
            .map(|pair| perceptual_hash_distance(pair[0], pair[1]) as f64)
            .sum::<f64>();
        mean_motion_delta = total / (hashes.len() - 1) as f64;
    }

    let mut warnings = Vec::new();
    if grid_ink_detected {
        warnings.push(
            "AI grid ink detected on strip gutters — prefer layout \"profile\" or regenerate without visible cell borders".into(),
        );
    }
    if min_cell_width < 8 {
        warnings.push(format!(
            "Smallest detected cell is only {min_cell_width}px wide — verify frame count or regenerate the strip"
        ));
    }
    if frame_count.filter(|value| *value > 0).is_some()
        && frame_count_used != inference.suggested_frame_count
        && inference.confidence >= 0.55
    {
        warnings.push(format!(
            "Inferred {inferred} frames with {:.0}% confidence but {used} were requested",
            inference.confidence * 100.0,
            inferred = inference.suggested_frame_count,
            used = frame_count_used,
        ));
    }
    if mean_motion_delta < 2.0 && segments.len() > 1 {
        warnings.push(
            "Adjacent strip cells look nearly identical — animation may not move enough".into(),
        );
    }
    if !segments_cover_width(&segments, width) {
        warnings.push(
            "Could not derive clean segment boundaries for this strip — try layout \"dp\" or adjust frame count".into(),
        );
    }

    let profile_candidate =
        profile_horizontal_segments(width, &vertical_alpha_profile(image), frame_count_used);
    let layout_recommended = if grid_ink_detected {
        "profile".into()
    } else if segments_cover_width(&profile_candidate, width) {
        "profile".into()
    } else if layout_used == "dp" {
        "dp".into()
    } else if layout_used == "horizontal" || layout_used == "strip" {
        "horizontal".into()
    } else if layout_used == "vertical" {
        "vertical".into()
    } else {
        "auto".into()
    };

    StripScoreReport {
        suggested_frame_count: inference.suggested_frame_count,
        frame_count_used,
        inference_confidence: inference.confidence,
        grid_ink_detected,
        min_cell_width,
        mean_motion_delta,
        segment_widths,
        layout_recommended,
        warnings,
    }
}

