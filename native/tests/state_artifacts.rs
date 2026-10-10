//! Structured migration references match source without rewriting free text.
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
#[test]
fn durable_references_are_rewritten_once_without_touching_labels() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let doc: Value = serde_json::from_slice(
        &std::fs::read(root.join("tests/fixtures/backend/state-artifacts.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(doc["source"], "d173116:state_artifacts.rewrite");
    let cases = doc["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3);
    for case in cases {
        let map: BTreeMap<String, String> =
            serde_json::from_value(case["mapping"].clone()).unwrap();
        assert_eq!(
            _native::state_artifacts::rewrite(
                case["name"].as_str().unwrap(),
                &case["document"],
                &map
            )
            .unwrap(),
            case["expected"]
        );
    }
    assert!(!_native::state_artifacts::state_name(
        "../publish_fixture.json"
    ));
}
