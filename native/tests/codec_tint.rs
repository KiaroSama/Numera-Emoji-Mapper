//! Independent pinned-source JSON results through the public pure Lottie tint boundary.
use _native::codec_tint::{numeric, parse_tint, tint_gradient, tint_lottie};
use serde_json::Value;

#[test]
fn source_tint_keeps_alpha_opacity_keyframes_and_numeric_bool_quirks() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/lottie-tint.json")).unwrap();
    assert_eq!(
        fixture["baseline_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(fixture["parse"].as_array().unwrap().len(), 15);
    assert_eq!(fixture["numeric"].as_array().unwrap().len(), 9);
    assert_eq!(fixture["gradients"].as_array().unwrap().len(), 14);
    assert_eq!(fixture["trees"].as_array().unwrap().len(), 11);
    for case in fixture["parse"].as_array().unwrap() {
        if let Some(expected) = case.get("expected") {
            assert_eq!(
                serde_json::json!(parse_tint(case["input"].as_str().unwrap()).unwrap()),
                *expected
            );
        } else {
            assert_eq!(
                parse_tint(case["input"].as_str().unwrap()).unwrap_err(),
                case["error"].as_str().unwrap()
            );
        }
    }
    for case in fixture["numeric"].as_array().unwrap() {
        assert_eq!(
            numeric(&case["input"]),
            case["expected"].as_bool().unwrap(),
            "{case}"
        );
    }
    let rgb: [f64; 3] = serde_json::from_value(fixture["rgb"].clone()).unwrap();
    for case in fixture["gradients"].as_array().unwrap() {
        let mut actual = case["input"].clone();
        tint_gradient(&mut actual, rgb);
        assert_eq!(actual, case["expected"], "{case}");
    }
    for case in fixture["trees"].as_array().unwrap() {
        let mut actual = case["input"].clone();
        tint_lottie(&mut actual, rgb);
        assert_eq!(actual, case["expected"], "{case}");
    }
}
