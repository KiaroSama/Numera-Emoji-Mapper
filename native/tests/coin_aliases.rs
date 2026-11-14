//! Pinned source results at the public pure alias boundary; no production files or network.
use _native::coin_aliases::{apply_json, normalize_name};
use serde_json::Value;

#[test]
fn source_normalization_and_alias_groups_preserve_order_and_ambiguity() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-aliases.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["baseline_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(fixture["normalization"].as_array().unwrap().len(), 12);
    assert_eq!(fixture["groups"].as_array().unwrap().len(), 8);
    assert_eq!(fixture["errors"].as_array().unwrap().len(), 6);
    for case in fixture["normalization"].as_array().unwrap() {
        assert_eq!(
            normalize_name(case["input"].as_str().unwrap()),
            case["expected"].as_str().unwrap(),
            "{case}"
        );
    }
    for case in fixture["groups"].as_array().unwrap() {
        let actual = apply_json(&case["request"]).unwrap();
        assert_eq!(actual, case["expected"], "{case}");
        let keys: Vec<_> = actual["mapping"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(
            serde_json::json!(keys),
            case["mapping_keys"],
            "map order: {case}"
        );
    }
    for case in fixture["errors"].as_array().unwrap() {
        let error = apply_json(&case["request"]).unwrap_err();
        assert_eq!(error.kind, case["kind"].as_str().unwrap(), "{case}");
        assert_eq!(error.message, case["message"].as_str().unwrap(), "{case}");
    }
}
