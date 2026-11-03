//! Loaded-document policy only; fixture gzip/load seams do not claim decoder coverage.
use _native::codec_lottie_validate::{validate_document, validate_document_typed};
use serde_json::{Value, json};

#[test]
fn historical_lottie_recipe_pins_exact_inputs_not_generated_messages_or_classes() {
    let original: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/lottie-validation.json"
    ))
    .unwrap();
    _native::source_lottie_oracle::fixture_identity(&original).unwrap();
    let mut observations = original.clone();
    observations["cases"][0]["expected"] = json!("source must reject changed output");
    observations["cases"][0]["source_error_kind"] = json!("TypeError");
    _native::source_lottie_oracle::fixture_identity(&observations).unwrap();
    for mutation in ["document", "filename", "size", "order", "extra"] {
        let mut changed = original.clone();
        match mutation {
            "document" => changed["cases"][0]["document"]["fr"] = json!(1),
            "filename" => changed["cases"][0]["filename"] = json!("other.tgs"),
            "size" => changed["cases"][0]["compressed_size"] = json!(1),
            "order" => changed["cases"].as_array_mut().unwrap().swap(0, 2),
            "extra" => changed["unchecked"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            _native::source_lottie_oracle::fixture_identity(&changed).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn historical_file_recipe_pins_paths_hashes_and_comparisons_not_validation_results() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/lottie-file.json")).unwrap();
    _native::source_lottie_oracle::file_fixture_identity(&fixture).unwrap();
    let mut result = fixture.clone();
    result["validations"][0]["expected"] = json!(false);
    _native::source_lottie_oracle::file_fixture_identity(&result).unwrap();
    for mutation in ["path", "hash", "comparison", "order", "extra"] {
        let mut changed = fixture.clone();
        match mutation {
            "path" => changed["validations"][0]["input"] = json!("../unverified.tgs"),
            "hash" => changed["validations"][0]["input_sha256"] = json!("0".repeat(64)),
            "comparison" => changed["comparisons"][0]["same_image"] = json!(false),
            "order" => changed["validations"].as_array_mut().unwrap().swap(0, 1),
            "extra" => changed["unchecked"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            _native::source_lottie_oracle::file_fixture_identity(&changed).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn pinned_loaded_lottie_contract_preserves_validation_order_and_nested_masks() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/lottie-validation.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["baseline_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 84);
    for case in fixture["cases"].as_array().unwrap() {
        let filename = case["filename"].as_str().unwrap();
        let size = case["compressed_size"].as_u64().unwrap();
        let actual = validate_document(&case["document"], filename, size);
        let typed = validate_document_typed(&case["document"], filename, size);
        if case["expected"].is_null() {
            assert_eq!(actual, Ok(()), "{case}");
            typed.unwrap();
        } else {
            assert_eq!(
                actual.unwrap_err(),
                case["expected"].as_str().unwrap(),
                "{case}"
            );
            let error = typed.unwrap_err();
            assert_eq!(
                error.kind,
                case["source_error_kind"].as_str().unwrap(),
                "{case}"
            );
            assert_eq!(error.message, case["expected"].as_str().unwrap(), "{case}");
            if case["source_error_kind"] == "MediaError" {
                let reply = _native::codec_error::CodecError::worker_reply(Err(error)).unwrap();
                assert_eq!(reply.exit, 1);
                assert_eq!(reply.envelope, json!({"ok":false,"error":case["expected"]}));
            } else {
                let escaped = _native::codec_error::CodecError::worker_reply(Err(error))
                    .err()
                    .unwrap();
                assert_eq!(escaped.kind, case["source_error_kind"].as_str().unwrap());
            }
        }
    }
}
