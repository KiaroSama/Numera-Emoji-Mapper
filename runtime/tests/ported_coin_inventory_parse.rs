// Ported from native/tests/coin_inventory_parse.rs at a39a049c02f8dcc90d18fe8771869f82dd655221.
// Original source SHA256: 9cb2b0d67fe276051e9ec83e622c3592497120aa8654ca7c4db351c80c6fbe6c.
// Original test identities and behavioral assertions retained against the production crate.
extern crate numera_runtime as _native;

// Exact pinned source inventory block observations; no real map or inventory changes.
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[test]
fn missing_inventory_blocks_match_source_regex() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/inventory-parse.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "46985e570f4d9e7ee8c9ae0d326f0517033a3a3822f3b9d594ca07a30a079f47"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 24);
    for (index, case) in cases.iter().enumerate() {
        let have: BTreeSet<String> = serde_json::from_value(case["have"].clone()).unwrap();
        assert_eq!(
            json!(_native::coin_inventory_parse::missing(
                case["text"].as_str().unwrap(),
                &have
            )),
            case["expected"],
            "{index}"
        );
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
