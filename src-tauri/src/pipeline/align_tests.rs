use super::align::blit_with_offset;
use image::{Rgba, RgbaImage};

#[test]
fn blit_with_offset_supports_negative_shifts() {
    let mut canvas = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 0]));
    let mut marker = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 0]));
    marker.put_pixel(1, 1, Rgba([255, 0, 0, 255]));
    blit_with_offset(&mut canvas, &marker, -1, 0);
    assert_eq!(canvas.get_pixel(0, 1)[0], 255);
}
