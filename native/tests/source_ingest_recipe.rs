//! Original ingest generator inputs are pinned separately from source-observed outputs and calls.
use _native::source_git_inventory::Source;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn fixture() -> Value {
    let bytes = include_bytes!("../../tests/fixtures/backend/ingest.json");
    let hash = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        hash,
        "2ec808a2a1210be99d3d161f3a14df5c8a3ab2b0b6895571cd44ec8fec646818"
    );
    let fixture: Value = serde_json::from_slice(bytes).unwrap();
    _native::source_fixture::cases(&fixture, "d173116:ingest.catalog_identity", 8).unwrap();
    fixture
}

#[test]
fn ingest_recipe_excludes_only_source_results_errors_and_observed_call_order() {
    let original = fixture();
    _native::source_ingest_oracle::fixture_identity(&original).unwrap();
    let mut reordered = original.clone();
    reordered.sort_all_objects();
    _native::source_ingest_oracle::fixture_identity(&reordered).unwrap();

    for observation in ["expected", "error", "calls"] {
        let mut changed = original.clone();
        match observation {
            "expected" => changed["cases"][0]["expected"] = json!(["not-a-source-result", false]),
            "error" => changed["cases"][4]["error"] = json!("not-the-source-refusal"),
            "calls" => changed["cases"][4]["calls"] = json!(["s:different", "s:abc"]),
            _ => unreachable!(),
        }
        _native::source_ingest_oracle::fixture_identity(&changed).unwrap();
    }
}

#[test]
fn ingest_recipe_refuses_coordinated_inputs_outputs_callbacks_and_case_identity_drift() {
    let original = fixture();
    for mutation in [
        "comparison-result",
        "collision-result",
        "threshold",
        "incoming",
        "items",
        "item-order",
        "name",
        "case-order",
        "extra",
    ] {
        let mut changed = original.clone();
        match mutation {
            "comparison-result" => {
                changed["cases"][1]["comparison"]["s:abc"] = json!(true);
                changed["cases"][1]["expected"] = json!(["s:abc", false]);
            }
            "collision-result" => {
                changed["cases"][1]["collision"] = json!("s:changed:abc");
                changed["cases"][1]["expected"] = json!(["s:changed:abc", true]);
            }
            "threshold" => changed["cases"][7]["threshold"] = json!(2),
            "incoming" => changed["cases"][0]["incoming"]["file_path"] = json!("different"),
            "items" => changed["cases"][3]["items"][0]["phash"] = json!(255),
            "item-order" => changed["cases"][4]["items"]
                .as_array_mut()
                .unwrap()
                .swap(0, 1),
            "name" => changed["cases"][0]["name"] = json!("uniquely-renamed"),
            "case-order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
            "extra" => changed["unverified-metadata"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            _native::source_ingest_oracle::fixture_identity(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
}

#[test]
fn ingest_recipe_keeps_nonempty_unique_and_exact_result_or_error_corpus_admission() {
    let original = fixture();
    for mutation in [
        "empty",
        "count",
        "duplicate",
        "both",
        "neither",
        "empty-error",
        "source",
    ] {
        let mut changed = original.clone();
        match mutation {
            "empty" => changed["cases"] = json!([]),
            "count" => {
                changed["cases"].as_array_mut().unwrap().pop();
            }
            "duplicate" => changed["cases"][1]["name"] = changed["cases"][0]["name"].clone(),
            "both" => changed["cases"][0]["error"] = json!("refused"),
            "neither" => {
                changed["cases"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("expected");
            }
            "empty-error" => changed["cases"][4]["error"] = json!(""),
            "source" => changed["source"] = json!("unverified:ingest"),
            _ => unreachable!(),
        }
        assert!(
            _native::source_ingest_oracle::fixture_identity(&changed).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn ingest_source_identity_and_recipe_fail_before_any_interpreter_or_source_call() {
    let original = fixture();
    let ingest = Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: "tests/oracles/ingest.py".into(),
        sha256: "d2e7740602bd6993e51ab6fd0c0244764ca71e47b9dbafa2c6bdd5895050d0b6".into(),
        text: "raise AssertionError('unverified source must never execute')".into(),
    };
    let near = Source {
        baseline_revision: ingest.baseline_revision.clone(),
        path: "tests/_similarity_oracle.py".into(),
        sha256: "9be48166b64ab2052798c67e2f7426c2dd0008fc27b4d668a785877fba7ac44e".into(),
        text: "raise AssertionError('unverified near helper must never execute')".into(),
    };
    assert!(_native::source_ingest_oracle::source_identity(&ingest, &near).is_err());
    assert!(
        _native::source_ingest_oracle::check(&ingest, &near, &original)
            .unwrap_err()
            .contains("source identity drift")
    );
}
