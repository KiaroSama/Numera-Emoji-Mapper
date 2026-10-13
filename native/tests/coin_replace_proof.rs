//! Pinned full-manifest evidence, not positional guessing, proves a replacement CID.
use serde_json::Value;
#[test]
fn source_replacement_id_requires_new_identity_and_unchanged_other_ids() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-replace-proof.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "86f17862ebe5019e05965fa7aa30380a20226b3632f192f051525290ae7f7031"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let rows = |key: &str| {
            case[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            serde_json::json!(_native::coin_replace_proof::new_id(
                &rows("before"),
                &rows("after"),
                case["position"].as_i64().unwrap()
            )),
            case["expected"]
        );
    }
}
