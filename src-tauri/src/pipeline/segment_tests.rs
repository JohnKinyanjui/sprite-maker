use super::segment::{
    dp_horizontal_segments, infer_frame_count, infer_frame_count_from_image, segments_cover_width,
};
use super::qc::vertical_alpha_profile;
use image::{Rgba, RgbaImage};

#[test]
fn infers_frame_count_from_alpha_valleys() {
    let mut image = RgbaImage::new(30, 8);
    for x in 2..8 {
        image.put_pixel(x, 4, Rgba([255, 0, 0, 255]));
    }
    for x in 12..18 {
        image.put_pixel(x, 4, Rgba([0, 255, 0, 255]));
    }
    for x in 22..28 {
        image.put_pixel(x, 4, Rgba([0, 0, 255, 255]));
    }
    let inference = infer_frame_count_from_image(&image);
    assert_eq!(inference.suggested_frame_count, 3);
    assert!(inference.confidence > 0.4);
}

#[test]
fn dp_segments_cover_full_width() {
    let mut image = RgbaImage::new(24, 6);
    for x in 1..7 {
        image.put_pixel(x, 4, Rgba([255, 0, 0, 255]));
    }
    for x in 13..19 {
        image.put_pixel(x, 4, Rgba([0, 255, 0, 255]));
    }
    let profile = vertical_alpha_profile(&image);
    let segments = dp_horizontal_segments(24, &profile, 2);
    assert_eq!(segments.len(), 2);
    assert!(segments_cover_width(&segments, 24));
}

#[test]
fn dp_segments_respect_minimum_width() {
    let image = RgbaImage::new(40, 6);
    let profile = vertical_alpha_profile(&image);
    let segments = dp_horizontal_segments(40, &profile, 8);
    assert_eq!(segments.len(), 8);
    assert!(segments_cover_width(&segments, 40));
    assert!(segments.iter().all(|(_, width)| *width >= 4));
}

#[test]
fn infer_frame_count_handles_empty_strip() {
    let image = RgbaImage::new(4, 4);
    let profile = vertical_alpha_profile(&image);
    let inference = infer_frame_count(4, &profile, 4);
    assert_eq!(inference.suggested_frame_count, 1);
}
