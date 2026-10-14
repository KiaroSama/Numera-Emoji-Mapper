//! Pinned deletion snapshots prove missing, not successful HTTP answers, drives completion.
use _native::{
    coin_rebuild_delete::{DeleteIo, delete},
    telegram::SetState,
};
use serde_json::{Value, json};
use std::time::Duration;
struct Observed<'a> {
    input: &'a Value,
    events: Vec<Value>,
    output: String,
}
impl DeleteIo for Observed<'_> {
    fn delete(&mut self, name: &str) -> Result<(), String> {
        self.events.push(json!(["delete", name]));
        if self.input["errors"]
            .as_array()
            .is_some_and(|v| v.iter().any(|v| v == name))
        {
            Err("controlled delete refusal".into())
        } else {
            Ok(())
        }
    }
    fn probe(&mut self, name: &str) -> SetState {
        self.events.push(json!(["probe", name]));
        match self.input["statuses"][name].as_str().unwrap() {
            "exists" => SetState::Exists,
            "missing" => SetState::Missing,
            _ => SetState::Unknown,
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
    fn wait(&mut self, duration: Duration) {
        self.events.push(json!(["wait", duration.as_secs_f64()]));
    }
}
#[test]
fn old_pack_deletion_stops_at_unknown_and_resumes_only_unconfirmed_names() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-rebuild-delete.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "17771c6245dedf36d6c66deba99bb6405a624505962f74251f7f5a7f7e4eb54f"
    );
    let names = vec!["old1".to_owned(), "old2".to_owned()];
    for case in fixture["cases"].as_array().unwrap() {
        let mut state = case["state"].clone();
        let mut io = Observed {
            input: &case["input"],
            events: Vec::new(),
            output: String::new(),
        };
        let result = delete(
            if case["input"]["missing"] == true {
                None
            } else {
                Some(&names)
            },
            &mut state,
            &mut io,
        );
        match result {
            Ok(done) => {
                assert_eq!(json!(done), case["expected"]["done"]);
                assert!(case["expected"]["error"].is_null());
            }
            Err(error) => assert_eq!(json!(error), case["expected"]["error"]),
        }
        assert_eq!(state, case["expected"]["state"]);
        assert_eq!(json!(io.events), case["expected"]["events"]);
        assert_eq!(json!(io.output), case["expected"]["output"]);
    }
}
