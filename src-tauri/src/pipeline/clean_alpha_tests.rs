use super::clean_alpha::clean_rgba_image;
use image::{Rgba, RgbaImage};

#[test]
fn clean_rgba_image_clears_semi_transparent_fringe() {
    let mut image = RgbaImage::new(4, 4);
    for pixel in image.pixels_mut() {
        *pixel = Rgba([0, 0, 0, 0]);
    }
    image.put_pixel(2, 2, Rgba([40, 80, 120, 255]));
    image.put_pixel(3, 2, Rgba([255, 255, 255, 120]));
    let cleaned = clean_rgba_image(&image);
    assert_eq!(cleaned.get_pixel(3, 2)[3], 0);
    assert_eq!(cleaned.get_pixel(2, 2)[3], 255);
}
