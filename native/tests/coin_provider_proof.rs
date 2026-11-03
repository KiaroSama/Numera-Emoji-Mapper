//! Frozen content proof policy, never live Telegram or positional identity guesses.
use _native::telegram::SetState;
use serde_json::{Value, json};

#[test]
fn provider_unknown_unreadable_duplicate_and_hamming_boundaries_match_source() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/provider-proof.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "4bce8ae96c8ec69e22fa135bb622c26409100b124c578fed94f2b04cb8b114fc"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let state = match case["state"].as_str().unwrap() {
            "exists" => SetState::Exists,
            "missing" => SetState::Missing,
            _ => SetState::Unknown,
        };
        let mut calls = Vec::new();
        let result = _native::coin_provider_proof::landed(
            state,
            case["stickers"].as_array().unwrap(),
            case["before"].as_u64().unwrap() as usize,
            case["want"].as_u64().unwrap(),
            |sticker| {
                let file = sticker["file_id"].as_str().unwrap();
                calls.push(file.to_owned());
                let index: usize = file[1..].parse().unwrap();
                case["hashes"][index]
                    .as_u64()
                    .ok_or_else(|| "controlled unreadable".into())
            },
        );
        let actual = match result {
            Ok(id) => json!({"id":id}),
            Err(_) => json!({"unknown":true}),
        };
        assert_eq!(actual, case["expected"]);
        assert_eq!(json!(calls), case["calls"]);
    }
    let mut state = json!({"provider_added":["a"]});
    _native::coin_provider_proof::record(&mut state, "a").unwrap();
    _native::coin_provider_proof::record(&mut state, "b").unwrap();
    assert_eq!(state, fixture["tally"]);
}
