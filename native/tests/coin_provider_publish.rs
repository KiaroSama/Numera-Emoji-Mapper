//! Pinned provider publication plan/effects trace; all external effects remain virtual.
use _native::coin_provider_proof::RecoveryIo;
use _native::coin_provider_publish::{Mutation, PublishIo, publish};
use serde_json::{Map, Value, json};
struct Fake<'a> {
    case: &'a Value,
    writes: Vec<Value>,
    mutations: Vec<Value>,
    proofs: Vec<Value>,
    promotions: Vec<Value>,
}
impl RecoveryIo for Fake<'_> {
    fn fallback_hash(&mut self, _: &Value) -> Result<u64, String> {
        Ok(17)
    }
    fn landed(&mut self, intent: &Value, want: u64) -> Result<Option<String>, String> {
        self.proofs.push(json!([
            intent["set_name"],
            intent["expected_before"],
            want,
            "aaa.png"
        ]));
        if self.case["outcome"] == "unknown" {
            Err("controlled unknown".into())
        } else {
            Ok(self.case["outcome"].as_str().map(str::to_owned))
        }
    }
    fn save(&mut self, kind: &str, value: &Value) -> Result<(), String> {
        self.writes.push(json!([kind, value]));
        Ok(())
    }
    fn promote(&mut self, intent: &Value, want: u64) -> Result<bool, String> {
        self.promotions
            .push(json!([intent["source_path"], intent["key"], want]));
        Ok(true)
    }
}
impl PublishIo for Fake<'_> {
    fn live_count(&mut self, _: &str) -> Result<usize, String> {
        Ok(self.case["before"].as_u64().unwrap() as usize)
    }
    fn source(&mut self, ticker: &str) -> Result<(String, u64), String> {
        Ok((format!("{ticker}.png"), 17))
    }
    fn mutate(&mut self, intent: &Value, _: &str) -> Mutation {
        self.mutations.push(json!([
            intent["operation"],
            intent["set_name"],
            intent["expected_before"],
            intent["source_path"]
        ]));
        match self.case["mutation"].as_str() {
            Some("rejected") => Mutation::Rejected,
            Some("unknown") => Mutation::Unknown,
            _ => Mutation::Answered,
        }
    }
    fn pause(&mut self) -> Result<(), String> {
        Ok(())
    }
}
#[test]
fn frozen_provider_publication_preserves_unknown_intent_and_verified_mapping_order() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/provider-publish.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut state = case["initial"].clone();
        let mut mapping = case["current"].as_object().unwrap().clone();
        let mut io = Fake {
            case: &case["case"],
            writes: vec![],
            mutations: vec![],
            proofs: vec![],
            promotions: vec![],
        };
        let result = publish(
            &mut state,
            &mut mapping,
            &["aaa".into()],
            "cryptoemoji",
            "Title",
            "bot",
            "fixture UTC",
            &Map::new(),
            &mut io,
        )
        .unwrap();
        let expected = &case["expected"];
        let expected_state = expected["state"].clone();
        // Source fake returns a detached decoded state, while captured writes prove durable state.
        let mut written = expected_state;
        if let Some(row) = expected["writes"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .find(|row| row[0] == "state")
        {
            written = row[1].clone();
        }
        assert_eq!(json!([result.0, result.1]), expected["result"]);
        assert_eq!(state, written);
        assert_eq!(json!(mapping), expected["mapping"]);
        assert_eq!(json!(io.writes), expected["writes"]);
        assert_eq!(json!(io.mutations), expected["mutations"]);
        assert_eq!(json!(io.proofs), expected["proofs"]);
        assert_eq!(json!(io.promotions), expected["promotions"]);
    }
}
