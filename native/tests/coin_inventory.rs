//! Source inventory text policies; no CLI, filesystem locks or missing-header parser claim.
use _native::coin_inventory::{base_ticker, norm, refill};
use serde_json::Value;
use std::collections::BTreeMap;

#[test]
fn pinned_inventory_keeps_alias_safeguards_counts_newlines_and_stale_id_clearing() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-inventory.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["baseline_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(fixture["norm"].as_array().unwrap().len(), 9);
    assert_eq!(fixture["base_ticker"].as_array().unwrap().len(), 14);
    assert_eq!(fixture["refill"].as_array().unwrap().len(), 15);
    for case in fixture["norm"].as_array().unwrap() {
        assert_eq!(
            norm(case["input"].as_str().unwrap()),
            case["expected"].as_str().unwrap()
        );
    }
    for case in fixture["base_ticker"].as_array().unwrap() {
        assert_eq!(
            base_ticker(case["input"].as_str().unwrap()),
            case["expected"].as_str().unwrap()
        );
    }
    for case in fixture["refill"].as_array().unwrap() {
        let mapping: BTreeMap<String, String> =
            serde_json::from_value(case["mapping"].clone()).unwrap();
        let actual = refill(&mapping, case["text"].as_str().unwrap());
        assert_eq!(
            actual.text,
            case["expected"]["text"].as_str().unwrap(),
            "{case}"
        );
        assert_eq!(
            actual.filled,
            case["expected"]["filled"].as_u64().unwrap() as usize
        );
        assert_eq!(
            actual.total,
            case["expected"]["total"].as_u64().unwrap() as usize
        );
    }
}
