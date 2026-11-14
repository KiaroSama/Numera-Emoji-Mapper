//! Independent pinned-source JSON results through the typed Lottie tint boundary.
use _native::codec_tint::{numeric, parse_tint, tint_gradient, tint_lottie};
use pyo3::prelude::*;
use serde_json::Value;

#[test]
fn original_blue_repaint_keeps_solid_keyframe_alpha_and_gradient_opacity() {
    use serde_json::json;
    let cases = [
        (
            json!({"c":{"a":0,"k":[1.0,0.0,0.0,1.0]}}),
            json!({"c":{"a":0,"k":[0.0,0.0,1.0,1.0]}}),
        ),
        (
            json!({"c":{"a":0,"k":[1.0,0.0,0.0,0.25]}}),
            json!({"c":{"a":0,"k":[0.0,0.0,1.0,0.25]}}),
        ),
        (
            json!({"c":{"a":1,"k":[{"t":0,"s":[1.0,0.0,0.0,1.0]},{"t":30,"s":[0.0,1.0,0.0,0.5]}]}}),
            json!({"c":{"a":1,"k":[{"t":0,"s":[0.0,0.0,1.0,1.0]},{"t":30,"s":[0.0,0.0,1.0,0.5]}]}}),
        ),
        (
            json!({"c":{"a":1,"k":[{"t":0,"s":[1.0,0.0,0.0],"e":[0.0,1.0,0.0]}]}}),
            json!({"c":{"a":1,"k":[{"t":0,"s":[0.0,0.0,1.0],"e":[0.0,0.0,1.0]}]}}),
        ),
        (
            json!({"g":{"p":2,"k":{"a":0,"k":[0.0,1.0,0.0,0.0,1.0,0.0,0.0,1.0,0.0,1.0,1.0,0.25]}}}),
            json!({"g":{"p":2,"k":{"a":0,"k":[0.0,0.0,0.0,1.0,1.0,0.0,0.0,1.0,0.0,1.0,1.0,0.25]}}}),
        ),
        (
            json!({"g":{"k":{"a":0,"k":[0.0,1.0,0.0,0.0,1.0,0.0,1.0,0.0]}}}),
            json!({"g":{"k":{"a":0,"k":[0.0,0.0,0.0,1.0,1.0,0.0,0.0,1.0]}}}),
        ),
        (
            json!({"g":{"k":{"a":0,"k":[0.0,1.0,0.0,0.0,1.0,0.5]}}}),
            json!({"g":{"k":{"a":0,"k":[0.0,1.0,0.0,0.0,1.0,0.5]}}}),
        ),
    ];
    Python::initialize();
    Python::attach(|py| {
        let json = py.import("json").unwrap();
        for (input, expected) in cases {
            let node = json.call_method1("loads", (input.to_string(),)).unwrap();
            tint_lottie(&node, [0.0, 0.0, 1.0]).unwrap();
            let actual: Value = serde_json::from_str(
                &json
                    .call_method1("dumps", (&node,))
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(actual, expected);
        }
    });
}

#[test]
fn source_tint_keeps_alpha_opacity_keyframes_and_numeric_bool_quirks() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/lottie-tint.json")).unwrap();
    assert_eq!(
        fixture["baseline_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"],
        "845d1443956499a93413316342f06d819e9ef89a9a8bb789dcaa53638cdbfc59"
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
    // Keep the original CLI spelling assertions alongside the larger source corpus.
    for text in ["#FF8800", "ff8800"] {
        assert_eq!(parse_tint(text).unwrap(), [255, 136, 0]);
    }
    for text in ["white", "#FFF", "#GGGGGG", "#FF88000", ""] {
        assert!(parse_tint(text).is_err(), "{text:?}");
    }
    let rgb: [f64; 3] = serde_json::from_value(fixture["rgb"].clone()).unwrap();
    Python::initialize();
    Python::attach(|py| {
        let json = py.import("json").unwrap();
        for case in fixture["numeric"].as_array().unwrap() {
            let input = json
                .call_method1("loads", (case["input"].to_string(),))
                .unwrap();
            assert_eq!(
                numeric(&input),
                case["expected"].as_bool().unwrap(),
                "{case}"
            );
        }
        for group in ["gradients", "trees"] {
            for case in fixture[group].as_array().unwrap() {
                let input = json
                    .call_method1("loads", (case["input"].to_string(),))
                    .unwrap();
                if group == "gradients" {
                    tint_gradient(&input, rgb).unwrap();
                } else {
                    tint_lottie(&input, rgb).unwrap();
                }
                let expected = json
                    .call_method1("loads", (case["expected"].to_string(),))
                    .unwrap();
                assert!(input.eq(&expected).unwrap(), "{group}: {case}");
            }
        }
    });
}
