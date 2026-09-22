use super::region_regen::{build_region_mask, composite_with_mask};
use crate::models::{BrushStamp, RegionMaskRect};
use image::RgbaImage;

#[test]
fn build_region_mask_marks_white_rectangles() {
    let mask = build_region_mask(
        8,
        8,
        &[RegionMaskRect {
            x: 2,
            y: 2,
            width: 2,
            height: 2,
        }],
        &[],
    )
    .expect("mask");
    assert_eq!(mask.get_pixel(2, 2)[0], 255);
    assert_eq!(mask.get_pixel(0, 0)[0], 0);
}

#[test]
fn build_region_mask_paints_brush_stamps() {
    let mask = build_region_mask(
        16,
        16,
        &[],
        &[BrushStamp {
            x: 8,
            y: 8,
            radius: 2,
        }],
    )
    .expect("mask");
    assert_eq!(mask.get_pixel(8, 8)[0], 255);
    assert_eq!(mask.get_pixel(0, 0)[0], 0);
}

#[test]
fn build_region_mask_rejects_empty_inputs() {
    let error = build_region_mask(8, 8, &[], &[]).expect_err("empty mask");
    assert_eq!(error.code, "empty_mask");
}

#[test]
fn composite_with_mask_keeps_unmasked_pixels() {
    let mut base = RgbaImage::new(4, 4);
    for pixel in base.pixels_mut() {
        *pixel = image::Rgba([10, 20, 30, 255]);
    }
    let mut patch = RgbaImage::new(4, 4);
    for pixel in patch.pixels_mut() {
        *pixel = image::Rgba([200, 100, 50, 255]);
    }
    let mut mask = RgbaImage::new(4, 4);
    for pixel in mask.pixels_mut() {
        *pixel = image::Rgba([0, 0, 0, 255]);
    }
    mask.put_pixel(1, 1, image::Rgba([255, 255, 255, 255]));

    let composite = composite_with_mask(&base, &patch, &mask).expect("composite");
    assert_eq!(composite.get_pixel(0, 0)[0], 10);
    assert_eq!(composite.get_pixel(1, 1)[0], 200);
}
