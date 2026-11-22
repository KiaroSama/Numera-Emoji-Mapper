use super::*;
// tests.test_media.TestBlankDetection.test_fully_transparent_is_blank
#[test]
fn test_fully_transparent_is_blank() {
    assert!(raster::is_blank(&RgbaImage::new(100, 100)));
}
// tests.test_media.TestBlankDetection.test_a_visible_image_is_not_blank
#[test]
fn test_a_visible_image_is_not_blank() {
    assert!(!raster::is_blank(&RgbaImage::from_pixel(
        100,
        100,
        Rgba([10, 20, 30, 255])
    )));
}
// tests.test_media.TestBlankDetection.test_the_visibility_floor_is_exact
#[test]
fn test_the_visibility_floor_is_exact() {
    assert!(raster::is_blank(&RgbaImage::from_pixel(
        100,
        100,
        Rgba([5, 5, 5, 10])
    )));
    assert!(!raster::is_blank(&RgbaImage::from_pixel(
        100,
        100,
        Rgba([5, 5, 5, 11])
    )));
}
// tests.test_media.TestBlankDetection.test_a_few_visible_pixels_still_count_as_blank
#[test]
fn test_a_few_visible_pixels_still_count_as_blank() {
    let mut image = RgbaImage::new(100, 100);
    for x in 0..8 {
        image[(x, 0)] = Rgba([255, 255, 255, 255]);
    }
    assert!(raster::is_blank(&image));
    image[(8, 0)] = Rgba([255, 255, 255, 255]);
    assert!(!raster::is_blank(&image));
}
