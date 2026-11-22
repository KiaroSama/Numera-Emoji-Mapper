use super::*;
// tests.test_media.SubtractMasksAreRefusedBeforeTelegramRefusesThem.test_a_subtract_mask_is_rejected
#[test]
fn test_a_subtract_mask_is_rejected() {
    let t = Scratch::new();
    let path = tgs(&t.join("x.tgs"), &masked("s", false));
    let error = rt::codec_lottie_validate::validate_file_typed(&path).unwrap_err();
    assert_eq!(error.kind, "MediaError");
    assert!(error.message.contains("SUBTRACT"));
}
// tests.test_media.SubtractMasksAreRefusedBeforeTelegramRefusesThem.test_a_subtract_mask_INSIDE_A_PRECOMP_is_rejected
#[test]
fn test_a_subtract_mask_inside_a_precomp_is_rejected() {
    let t = Scratch::new();
    let path = tgs(&t.join("x.tgs"), &masked("s", true));
    let error = rt::codec_lottie_validate::validate_file_typed(&path).unwrap_err();
    assert_eq!(error.kind, "MediaError");
    assert!(error.message.contains("SUBTRACT"));
}
// tests.test_media.SubtractMasksAreRefusedBeforeTelegramRefusesThem.test_an_ADD_mask_is_still_allowed
#[test]
fn test_an_add_mask_is_still_allowed() {
    let t = Scratch::new();
    for precomp in [true, false] {
        let path = tgs(&t.join("x.tgs"), &masked("a", precomp));
        rt::codec_lottie_validate::validate_file_typed(&path).unwrap();
    }
}
