//! Archive recipe identity includes folder titles, not source-observed rows, reports or names.
use _native::source_git_inventory::Source;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn fixture() -> Value {
    let bytes = include_bytes!("../../tests/fixtures/backend/archive.json");
    let digest = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        digest,
        "b09d84a67655825ee17d895decc6b73459dace9d34a6e2a5a4ef4c8fa6a9c8a3"
    );
    let fixture: Value = serde_json::from_slice(bytes).unwrap();
    _native::source_fixture::cases(&fixture, "d173116:pack_archive.names+rows+reports", 1).unwrap();
    fixture
}

#[test]
fn archive_recipe_excludes_only_source_observations_at_their_original_locations() {
    let original = fixture();
    _native::source_archive_oracle::fixture_identity(&original).unwrap();
    let mut reordered = original.clone();
    reordered.sort_all_objects();
    _native::source_archive_oracle::fixture_identity(&reordered).unwrap();
    for observation in [
        "rows",
        "error",
        "history",
        "manifest",
        "folder",
        "archive-name",
    ] {
        let mut changed = original.clone();
        match observation {
            "rows" => changed["cases"][0]["expected"] = json!([]),
            "error" => {
                changed["cases"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("expected");
                changed["cases"][0]["error"] = json!("not-the-source-outcome");
            }
            "history" => changed["cases"][0]["history_markdown"] = json!("changed\n"),
            "manifest" => changed["cases"][0]["manifest_markdown"] = json!("changed\n"),
            "folder" => changed["folders"][0]["folder"] = json!("not-a-source-folder"),
            "archive-name" => changed["archive_name"] = json!("not-a-source-name"),
            _ => unreachable!(),
        }
        _native::source_archive_oracle::fixture_identity(&changed).unwrap();
    }
}

#[test]
fn archive_recipe_pins_coordinated_case_inputs_titles_order_and_unrecognized_metadata() {
    let original = fixture();
    for mutation in [
        "record",
        "live",
        "items",
        "keywords-report",
        "keys",
        "case-name",
        "folder-title",
        "folder-order",
        "folder-extra",
        "root-extra",
        "case-extra",
    ] {
        let mut changed = original.clone();
        match mutation {
            "record" => {
                changed["cases"][0]["record"]["title"] = json!("Changed");
                changed["cases"][0]["manifest_markdown"] = json!("# Changed\n");
            }
            "live" => {
                changed["cases"][0]["live"][1]["custom_emoji_id"] = json!("444444444");
                changed["cases"][0]["expected"][1]["premium_id"] = json!("444444444");
            }
            "items" => {
                changed["cases"][0]["items"]["s:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]["path"] =
                    json!("other.tgs")
            }
            "keywords-report" => {
                changed["cases"][0]["items"]["s:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]["keywords"] =
                    json!(["changed|keyword"]);
                changed["cases"][0]["manifest_markdown"] = json!("changed\\|keyword\n");
            }
            "keys" => changed["cases"][0]["keys"]["222222222"] = json!("unknown"),
            "case-name" => changed["cases"][0]["name"] = json!("uniquely-renamed"),
            "folder-title" => {
                changed["folders"][0]["title"] = json!("Changed");
                changed["folders"][0]["folder"] = json!("Changed");
            }
            "folder-order" => changed["folders"].as_array_mut().unwrap().swap(0, 1),
            "folder-extra" => changed["folders"][0]["unchecked"] = json!(true),
            "root-extra" => changed["unchecked"] = json!(true),
            "case-extra" => changed["cases"][0]["unchecked"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            _native::source_archive_oracle::fixture_identity(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
}

#[test]
fn archive_recipe_refuses_empty_partial_or_wrongly_shaped_corpora() {
    let original = fixture();
    for mutation in [
        "empty-cases",
        "both",
        "neither",
        "source",
        "no-folders",
        "empty-folders",
        "folder-scalar",
        "no-title",
    ] {
        let mut changed = original.clone();
        match mutation {
            "empty-cases" => changed["cases"] = json!([]),
            "both" => changed["cases"][0]["error"] = json!("refused"),
            "neither" => {
                changed["cases"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("expected");
            }
            "source" => changed["source"] = json!("unverified:archive"),
            "no-folders" => {
                changed.as_object_mut().unwrap().remove("folders");
            }
            "empty-folders" => changed["folders"] = json!([]),
            "folder-scalar" => changed["folders"][0] = json!("not-an-object"),
            "no-title" => {
                changed["folders"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("title");
            }
            _ => unreachable!(),
        }
        assert!(
            _native::source_archive_oracle::fixture_identity(&changed).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn archive_oracle_refuses_spoofed_source_before_interpreter_initialization() {
    let archive = Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: "tests/oracles/pack_archive.py".into(),
        sha256: "312e32d30bd0b8cdd905b93654a28b28131ce32f768fefd26e9d12aecd7f1c20".into(),
        text: "raise AssertionError('unverified archive source must never execute')".into(),
    };
    let rows = Source {
        baseline_revision: archive.baseline_revision.clone(),
        path: "tests/oracles/pack_rows.py".into(),
        sha256: "1c0c0fc15ab5b6abcecb182f138f89da55c566c7210102c1eaaa5ad115343121".into(),
        text: "raise AssertionError('unverified rows source must never execute')".into(),
    };
    assert!(_native::source_archive_oracle::source_identity(&archive, &rows).is_err());
    assert!(
        _native::source_archive_oracle::check(&archive, &rows, &fixture())
            .unwrap_err()
            .contains("source identity drift")
    );
}
