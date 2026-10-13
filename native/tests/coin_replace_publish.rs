//! Pinned replacement transaction traces preserve intent-before-effect and content-before-map.
use _native::coin_replace_publish::{Mutation, PublishIo, replace};
use _native::coin_replace_recovery::RecoveryIo;
use serde_json::{Map, Value, json};
struct Fixture {
    case: Value,
    actions: Vec<String>,
    intents: Vec<Value>,
    mapping: Map<String, Value>,
}
impl RecoveryIo for Fixture {
    fn live(&mut self, _name: &str) -> Result<Vec<Value>, String> {
        self.actions.push("read".into());
        if self.case["read_error"] == true {
            return Err("unknown".into());
        }
        let rows = self
            .case
            .get("after")
            .cloned()
            .unwrap_or(json!(["a", "new"]));
        Ok(rows
            .as_array()
            .unwrap()
            .iter()
            .map(|cid| json!({"custom_emoji_id":cid}))
            .collect())
    }
    fn proof(&mut self, _sticker: &Value, _hash: u64) -> Result<bool, String> {
        self.actions.push("proof".into());
        Ok(self.case["ours"] != false)
    }
    fn promote(&mut self, _intent: &Value) -> Result<(), String> {
        self.actions.push("promote".into());
        Ok(())
    }
    fn repoint(&mut self, old: &str, new: &str) -> Result<(), String> {
        self.actions.push("repoint".into());
        for value in self.mapping.values_mut() {
            if value.as_str() == Some(old) {
                *value = json!(new);
            }
        }
        Ok(())
    }
    fn clear(&mut self) -> Result<(), String> {
        self.actions.push("clear".into());
        Ok(())
    }
    fn clean(&mut self, _intent: &Value) -> Result<(), String> {
        self.actions.push("cleanup".into());
        Ok(())
    }
}
impl PublishIo for Fixture {
    fn save_intent(&mut self, intent: &Value) -> Result<(), String> {
        self.actions.push("intent".into());
        self.intents.push(intent.clone());
        Ok(())
    }
    fn mutate(&mut self, _intent: &Value) -> Mutation {
        self.actions.push("mutate".into());
        match self.case["mutation"].as_str() {
            Some("rejected") => Mutation::Rejected,
            Some("ambiguous") => Mutation::Unknown,
            _ => Mutation::Answered,
        }
    }
}
#[test]
fn source_replace_transaction_maps_only_content_proven_new_id_and_retains_unknown() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-replace-publish.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "86f17862ebe5019e05965fa7aa30380a20226b3632f192f051525290ae7f7031"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let mut io = Fixture {
            case: case["input"].clone(),
            actions: vec![],
            intents: vec![],
            mapping: serde_json::from_value(json!({"aaa":"old","alias":"old","other":"keep"}))
                .unwrap(),
        };
        let intent = case["expected"]["intents"].as_array().unwrap().first();
        assert_eq!(
            replace(intent, &mut io).unwrap(),
            case["expected"]["result"].as_bool().unwrap()
        );
        assert_eq!(json!(io.actions), case["expected"]["actions"]);
        assert_eq!(json!(io.intents), case["expected"]["intents"]);
        assert_eq!(json!(io.mapping), case["expected"]["mapping"]);
    }
}
