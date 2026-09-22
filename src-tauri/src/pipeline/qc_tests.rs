use super::qc::{
    detect_profile_grid_ink, detect_strip_grid_ink, perceptual_hash, perceptual_hash_distance,
    profile_horizontal_segments, score_strip_image, vertical_alpha_profile,
};
use image::{Rgba, RgbaImage};

#[test]
fn detects_vertical_grid_ink_between_cells() {
    let mut image = RgbaImage::new(16, 8);
    for y in 0..8 {
        image.put_pixel(8, y, Rgba([0, 0, 0, 255]));
    }
    assert!(detect_strip_grid_ink(&image, 2));
}

#[test]
fn detects_profile_grid_ink_on_gutter_columns() {
    let mut image = RgbaImage::new(20, 8);
    for y in 0..8 {
        image.put_pixel(10, y, Rgba([0, 0, 0, 255]));
    }
    assert!(detect_profile_grid_ink(&image, &[0, 10, 20]));
}

#[test]
fn score_strip_recommends_profile_for_alpha_gutters() {
    let mut image = RgbaImage::new(20, 6);
    for x in 1..9 {
        image.put_pixel(x, 4, Rgba([255, 0, 0, 255]));
    }
    for x in 12..18 {
        image.put_pixel(x, 4, Rgba([0, 255, 0, 255]));
    }
    let report = score_strip_image(&image, None, Some("auto"));
    assert_eq!(report.suggested_frame_count, 2);
    assert_eq!(report.layout_recommended, "profile");
    assert!(!report.grid_ink_detected);
}

#[test]
fn score_strip_uses_equal_width_for_horizontal_layout() {
    let mut image = RgbaImage::new(20, 6);
    for x in 1..9 {
        image.put_pixel(x, 4, Rgba([255, 0, 0, 255]));
    }
    for x in 12..18 {
        image.put_pixel(x, 4, Rgba([0, 255, 0, 255]));
    }
    let report = score_strip_image(&image, Some(2), Some("horizontal"));
    assert_eq!(report.segment_widths, vec![10, 10]);
}

#[test]
fn perceptual_hash_distance_counts_bit_differences() {
    let mut left = RgbaImage::new(8, 8);
    let mut right = RgbaImage::new(8, 8);
    for x in 0..8 {
        left.put_pixel(x, 0, Rgba([255, 255, 255, 255]));
        right.put_pixel(x, 7, Rgba([255, 255, 255, 255]));
    }
    let left_hash = perceptual_hash(&left);
    let right_hash = perceptual_hash(&right);
    assert!(perceptual_hash_distance(left_hash, right_hash) > 0);
    assert_eq!(perceptual_hash_distance(left_hash, left_hash), 0);
}

#[test]
fn profile_segments_cover_full_width() {
    let mut image = RgbaImage::new(20, 6);
    for x in 1..9 {
        image.put_pixel(x, 4, Rgba([255, 0, 0, 255]));
    }
    for x in 12..18 {
        image.put_pixel(x, 4, Rgba([0, 255, 0, 255]));
    }
    let profile = vertical_alpha_profile(&image);
    let segments = profile_horizontal_segments(20, &profile, 2);
    let total = segments.iter().map(|(_, width)| *width).sum::<u32>();
    assert_eq!(segments.len(), 2);
    assert_eq!(total, 20);
}
