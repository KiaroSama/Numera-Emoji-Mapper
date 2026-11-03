//! Historical plan-step recipe admission is pure; source replay belongs to the bounded worker.
use _native::source_steps_oracle;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[test]
fn fixed_steps_recipe_preserves_inputs_order_and_names_but_replays_outputs() {
    let bytes = include_bytes!("../../tests/fixtures/backend/steps.json");
    let actual = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        actual,
        "63e38f56182a4e98aa3883e9ababd1cba53a7baac3b645f2125be87e35a000c3"
    );
    let original: Value = serde_json::from_slice(bytes).unwrap();
    source_steps_oracle::fixture_identity(&original).unwrap();
    let mut reordered = original.clone();
    reordered.sort_all_objects();
    source_steps_oracle::fixture_identity(&reordered).unwrap();
    let mut output_only = original.clone();
    output_only["cases"][0]["expected"] = json!({"target-derived":"must not pass replay"});
    source_steps_oracle::fixture_identity(&output_only).unwrap();
    output_only = original.clone();
    output_only["cases"][2]["error"] = json!("altered source refusal");
    source_steps_oracle::fixture_identity(&output_only).unwrap();
    for mutation in [
        "consistent-result",
        "name",
        "order",
        "dependency",
        "root-extra",
    ] {
        let mut changed = original.clone();
        match mutation {
            "consistent-result" => {
                changed["cases"][0]["ids"]["a"] = json!("555555555");
                changed["cases"][0]["expected"]["counted"] = json!(3);
            }
            "name" => changed["cases"][0]["name"] = json!("uniquely-renamed"),
            "order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
            "dependency" => changed["cases"][0]["plan"]["targets"][0][1] = json!(3),
            "root-extra" => changed["unchecked-metadata"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            source_steps_oracle::fixture_identity(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
}
