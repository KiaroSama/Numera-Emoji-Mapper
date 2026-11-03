//! Structured migration references match source without rewriting free text.
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
fn historical_recipe_preserves_the_plan_and_generic_remap_boundary() {
    let original: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/backend/state-artifacts.json"
    ))
    .unwrap();
    _native::source_state_artifacts_oracle::fixture_identity(&original).unwrap();
    for mutation in ["input-result", "name", "order", "extra"] {
        let mut changed = original.clone();
        match mutation {
            "input-result" => {
                changed["cases"][0]["mapping"]["v:old"] = serde_json::json!("v:changed");
                changed["cases"][0]["expected"]["moves"][0]["key"] = serde_json::json!("v:changed");
            }
            "name" => changed["cases"][0]["name"] = serde_json::json!("publish_other.json"),
            "order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
            "extra" => changed["unused"] = serde_json::json!(true),
            _ => unreachable!(),
        }
        assert!(
            _native::source_state_artifacts_oracle::fixture_identity(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
    let mut output_only = original.clone();
    output_only["cases"][0]["expected"]["moves"][0]["label"] = serde_json::json!("v:new");
    _native::source_state_artifacts_oracle::fixture_identity(&output_only).unwrap();
    let mut reordered = original;
    reordered.sort_all_objects();
    _native::source_state_artifacts_oracle::fixture_identity(&reordered).unwrap();
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
