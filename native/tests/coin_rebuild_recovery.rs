//! Pinned interrupted-create/add recovery refuses unknown, foreign and duplicate evidence.
use _native::{
    coin_rebuild_recovery::{RecoveryIo, recover},
    telegram::SetState,
};
use serde_json::{Value, json};
struct Observed<'a> {
    input: &'a Value,
    events: Vec<Value>,
    output: String,
}
impl RecoveryIo for Observed<'_> {
    fn source_exists(&mut self, _key: &str) -> bool {
        self.input["missing"] != true
    }
    fn probe(&mut self, name: &str) -> Result<(SetState, Vec<Value>), String> {
        self.events.push(json!(["probe", name]));
        let status = match self.input["status"].as_str().unwrap() {
            "exists" => SetState::Exists,
            "missing" => SetState::Missing,
            _ => SetState::Unknown,
        };
        Ok((status, self.input["stickers"].as_array().unwrap().clone()))
    }
    fn compare(&mut self, sticker: &Value, _key: &str) -> Result<Option<bool>, String> {
        self.events.push(json!(["compare", sticker["file_id"]]));
        Ok(self.input["same"].as_bool())
    }
    fn source_hash(&mut self, _key: &str) -> Result<u64, String> {
        Ok(0)
    }
    fn live_hash(&mut self, sticker: &Value) -> Result<u64, String> {
        self.events.push(json!(["download", sticker["file_id"]]));
        if sticker["error"] == true {
            Err("controlled decode failure".into())
        } else {
            Ok(sticker["hash"].as_u64().unwrap())
        }
    }
    fn save(&mut self, state: &Value) -> Result<(), String> {
        self.events.push(json!(["save", state]));
        Ok(())
    }
    fn report(&mut self, text: &str) {
        self.output.push_str(text);
        self.output.push('\n');
    }
}
#[test]
fn interrupted_rebuild_is_resolved_by_source_identity_before_any_state_adoption() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-rebuild-recovery.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "17771c6245dedf36d6c66deba99bb6405a624505962f74251f7f5a7f7e4eb54f"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let mut state = case["state"].clone();
        let mut io = Observed {
            input: &case["input"],
            events: Vec::new(),
            output: String::new(),
        };
        let result = recover(
            &mut state,
            case["cum"].as_u64().unwrap(),
            "Fixture",
            &mut io,
        );
        match result {
            Ok(total) => {
                assert_eq!(json!(total), case["expected"]["total"], "{}", case["name"]);
                assert!(case["expected"]["error"].is_null());
            }
            Err(error) => assert_eq!(json!(error), case["expected"]["error"], "{}", case["name"]),
        }
        assert_eq!(state, case["expected"]["state"], "{}", case["name"]);
        assert_eq!(
            json!(io.events),
            case["expected"]["events"],
            "{}",
            case["name"]
        );
        assert_eq!(
            json!(io.output),
            case["expected"]["output"],
            "{}",
            case["name"]
        );
    }
}
