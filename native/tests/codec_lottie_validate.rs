//! Loaded-document policy only; fixture gzip/load seams do not claim decoder coverage.
use _native::codec_lottie_validate::validate_document;
use serde_json::Value;

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
        let actual = validate_document(
            &case["document"],
            case["filename"].as_str().unwrap(),
            case["compressed_size"].as_u64().unwrap(),
        );
        if case["expected"].is_null() {
            assert_eq!(actual, Ok(()), "{case}");
        } else {
            assert_eq!(
                actual.unwrap_err(),
                case["expected"].as_str().unwrap(),
                "{case}"
            );
        }
    }
}
