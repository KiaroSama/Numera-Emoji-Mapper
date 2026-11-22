// Ported behavioral subset of native/tests/state_artifacts.rs at 45a96f9de70da8429b83c047e0626b6ed6694b20.
// Original source SHA256: 66e713173c8d83ab1af5dba34c3acc409a17198d894f52c8c0f77d75bf53f34e.
// Excluded compatibility-only cases remain in the original suite and migration inventory.
extern crate numera_runtime as _native;

// Structured migration references match source without rewriting free text.
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[path = "support/curation_migration_cases.rs"]
mod curation_migration_cases;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

#[test]
fn original_curation_plan_migrates_repeats_and_restores_without_overwriting_later_edits() {
    let root = tooling_fixture::Root::new("curation migration lifecycle");
    curation_migration_cases::check(&root.0);
    root.clean().unwrap();
}

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
    for field in ["moves", "held"] {
        for record in [
            serde_json::json!(null),
            serde_json::json!(1),
            serde_json::json!("v:old"),
            serde_json::json!([]),
            serde_json::json!({}),
            serde_json::json!({"key":null}),
            serde_json::json!({"key":1}),
        ] {
            let mut invalid = serde_json::json!({"version":1,"moves":[],"held":[]});
            invalid[field] = serde_json::json!([record]);
            let before = invalid.clone();
            assert!(
                _native::state_artifacts::rewrite("pack_plan.json", &invalid, &BTreeMap::new())
                    .is_err(),
                "{field}: {invalid}"
            );
            assert_eq!(invalid, before);
        }
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
