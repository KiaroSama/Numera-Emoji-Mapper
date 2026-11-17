use super::*;
// tests.test_check_all_packs.CheckAllPacksTest.test_blank_is_the_pipeline_wide_rule_not_a_local_copy
#[test]
fn audit_analysis_calls_the_shared_blank_rule_on_actual_rgba_once() {
    let rgba = RgbaImage::from_pixel(100, 100, image::Rgba([200, 20, 20, 255]));
    let bytes = raster::encode_png(&rgba).unwrap();
    assert!(!raster::is_blank(&rgba));
    let calls = std::cell::Cell::new(0);
    let (_, blank) = audit_image_with_blank(&bytes, |actual| {
        calls.set(calls.get() + 1);
        assert_eq!(actual, &rgba);
        true
    })
    .unwrap();
    assert!(
        blank,
        "the analyzer must return the supplied typed sentinel rather than an inline blank algorithm"
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(audit_image(&bytes).unwrap().1, raster::is_blank(&rgba));
}
