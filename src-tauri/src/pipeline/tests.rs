use super::normalize::{
    align_frame_to_baseline, compute_shared_scale, foot_y_on_canvas, shift_canvas_vertical,
};
use super::qc::profile_horizontal_segments;
use super::strip::{foreground_bounds, recover_foreground, split_cells};
use crate::models::AnimationFrame;
use image::{Rgba, RgbaImage};
#[test]
fn recovers_foreground_from_strip_cell() {
    let mut image = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 0]));
    image.put_pixel(3, 6, Rgba([255, 0, 0, 255]));
    image.put_pixel(4, 6, Rgba([255, 0, 0, 255]));
    let recovered = recover_foreground(&image);
    assert_eq!(recovered.dimensions(), (2, 1));
    assert_eq!(foreground_bounds(&recovered), Some((0, 0, 1, 0)));
}

#[test]
fn splits_horizontal_strip_in_order() {
    let mut image = RgbaImage::new(8, 4);
    for x in 0..8 {
        image.put_pixel(x, 1, Rgba([x as u8, 0, 0, 255]));
    }
    let frames = split_cells(&image, "horizontal", 4, None).expect("split");
    assert_eq!(frames.len(), 4);
    assert_eq!(frames[0].get_pixel(0, 1)[0], 0);
    assert_eq!(frames[3].get_pixel(0, 1)[0], 6);
}

#[test]
fn baseline_alignment_places_subject_foot_on_anchor_line() {
    let mut source = RgbaImage::new(8, 8);
    source.put_pixel(3, 5, Rgba([255, 0, 0, 255]));
    source.put_pixel(4, 5, Rgba([255, 0, 0, 255]));
    let aligned = align_frame_to_baseline(
        &source,
        Some((3, 5, 4, 5)),
        Some((3.5, 5.0)),
        8,
        8,
        4.0,
        7,
        1.0,
        0,
    );
    assert_eq!(foot_y_on_canvas(&aligned), Some(7));
}

#[test]
fn lock_first_frame_shift_aligns_foot_positions() {
    let mut low = RgbaImage::new(8, 8);
    low.put_pixel(3, 6, Rgba([255, 0, 0, 255]));
    let mut high = RgbaImage::new(8, 8);
    high.put_pixel(3, 4, Rgba([0, 255, 0, 255]));
    let first = align_frame_to_baseline(
        &low,
        Some((3, 6, 3, 6)),
        Some((3.0, 6.0)),
        8,
        8,
        4.0,
        7,
        1.0,
        0,
    );
    let reference = foot_y_on_canvas(&first).expect("first foot");
    let second = align_frame_to_baseline(
        &high,
        Some((3, 4, 3, 4)),
        Some((3.0, 4.0)),
        8,
        8,
        4.0,
        7,
        1.0,
        0,
    );
    let current = foot_y_on_canvas(&second).expect("second foot");
    let locked = shift_canvas_vertical(&second, reference as i32 - current as i32);
    assert_eq!(foot_y_on_canvas(&locked), Some(reference));
}

#[test]
fn shared_scale_shrinks_oversized_subject_to_fit_canvas() {
    let mut source = RgbaImage::new(20, 12);
    for x in 2..18 {
        source.put_pixel(x, 10, Rgba([255, 0, 0, 255]));
    }
    let decoded = vec![(
        AnimationFrame {
            asset_id: "asset".into(),
            duration_ms: None,
            offset_x: 0,
            offset_y: 0,
        },
        source,
        Some((2, 10, 17, 10)),
        Some((9.5, 10.0)),
    )];
    let scale = compute_shared_scale(&decoded, 8, 7, 0);
    assert!(scale < 1.0);
    let aligned = align_frame_to_baseline(
        &decoded[0].1,
        decoded[0].2,
        decoded[0].3,
        8,
        8,
        4.0,
        7,
        scale,
        0,
    );
    assert_eq!(aligned.dimensions(), (8, 8));
    assert_eq!(foot_y_on_canvas(&aligned), Some(7));
}

#[test]
fn profile_split_finds_gutters_between_touching_poses() {
    let mut image = RgbaImage::new(24, 8);
    for x in 1..7 {
        image.put_pixel(x, 6, Rgba([255, 0, 0, 255]));
    }
    for x in 13..19 {
        image.put_pixel(x, 4, Rgba([0, 255, 0, 255]));
    }
    let frames = split_cells(&image, "profile", 2, None).expect("profile split");
    assert_eq!(frames.len(), 2);
    assert!(frames[0].pixels().any(|pixel| pixel[0] > 200));
    assert!(frames[1].pixels().any(|pixel| pixel[1] > 200));
    let profile = super::qc::vertical_alpha_profile(&image);
    let segments = profile_horizontal_segments(24, &profile, 2);
    assert_eq!(segments.len(), 2);
}

fn inset_padding_keeps_output_canvas_dimensions() {
    let mut source = RgbaImage::new(8, 8);
    source.put_pixel(3, 5, Rgba([255, 0, 0, 255]));
    let aligned = align_frame_to_baseline(
        &source,
        Some((3, 5, 3, 5)),
        Some((3.0, 5.0)),
        8,
        8,
        4.0,
        7,
        1.0,
        2,
    );
    assert_eq!(aligned.dimensions(), (8, 8));
}
