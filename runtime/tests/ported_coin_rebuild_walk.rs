// Ported from native/tests/coin_rebuild_walk.rs at 45a96f9de70da8429b83c047e0626b6ed6694b20.
// Original source SHA256: e183c5d20accd0fd46f8350bb246f8c954525be632ea9666e076915031960a59.
// Original test identities and behavioral assertions retained against the production crate.
extern crate numera_runtime as _native;

// Pinned cursor walk: definitive skips persist, uncertain effects stop before the next intent.
use _native::{
    coin_rebuild_recovery::RecoveryIo,
    coin_rebuild_walk::{BuildIo, Mutation, Options, walk},
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
        true
    }
    fn probe(&mut self, _name: &str) -> Result<(SetState, Vec<Value>), String> {
        panic!("independent recovery seam")
    }
    fn compare(&mut self, _sticker: &Value, _key: &str) -> Result<Option<bool>, String> {
        panic!("independent recovery seam")
    }
    fn source_hash(&mut self, _key: &str) -> Result<u64, String> {
        panic!("independent recovery seam")
    }
    fn live_hash(&mut self, _sticker: &Value) -> Result<u64, String> {
        panic!("independent recovery seam")
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
impl BuildIo for Observed<'_> {
    fn live_count(&mut self, name: &str) -> Result<u64, String> {
        self.events.push(json!(["live", name]));
        if self.input["live"][name] == "unknown" {
            Err("controlled live unknown".into())
        } else {
            Ok(self.input["live"][name].as_u64().unwrap_or(0))
        }
    }
    fn source_status(&mut self, key: &str) -> Result<Option<bool>, String> {
        if self.input["missing"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == key)
        {
            Ok(None)
        } else {
            Ok(Some(
                self.input["blank"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v == key),
            ))
        }
    }
    fn delete_old(&mut self, _state: &mut Value) -> Result<bool, String> {
        self.events.push(json!(["delete"]));
        Ok(self.input["deleted"] == true)
    }
    fn mutate(&mut self, intent: &Value, _keywords: &str) -> Mutation {
        let key = intent["key"].as_str().unwrap();
        let op = intent["operation"].as_str().unwrap();
        self.events
            .push(json!(["mutate", op, intent["set_name"], key]));
        match self.input["errors"]
            .get(format!("{key}:{op}"))
            .or_else(|| self.input["errors"].get(key))
            .and_then(Value::as_str)
        {
            Some("ambiguous") => Mutation::Unknown("controlled ambiguous".into()),
            Some("unknown") => Mutation::Rejected("controlled unknown".into()),
            Some(error) => Mutation::Rejected(error.into()),
            None => Mutation::Answered,
        }
    }
    fn notify(&mut self, _state: &mut Value, name: &str, title: &str) -> Result<(), String> {
        self.events.push(json!(["notify", name, title]));
        Ok(())
    }
    fn wait(&mut self) {
        self.events.push(json!(["wait", 0.1]));
    }
}
#[test]
fn rebuild_walk_matches_source_cursor_intent_and_stop_effects() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-rebuild-walk.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "17771c6245dedf36d6c66deba99bb6405a624505962f74251f7f5a7f7e4eb54f"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let mut state = input["state"].clone();
        let mut io = Observed {
            input,
            events: Vec::new(),
            output: String::new(),
        };
        let result = walk(
            input["plan"].as_array().unwrap(),
            &mut state,
            &Options {
                owner: input["owner"].as_u64().unwrap(),
                base: "cryptoemoji".into(),
                title: "Fixture".into(),
                bot: "YourEmojiBot".into(),
            },
            &mut io,
        );
        // The production build advertises its native replacement command; all
        // surrounding refusal text and every effect still use the frozen oracle.
        let mut expected_error = case["expected"]["error"].clone();
        if let Some(message) = expected_error.as_str() {
            expected_error =
                json!(message.replace("coins/remap_ids.py", "numera-emoji coin-remap-ids"));
        }
        assert_eq!(json!(result.err()), expected_error, "{}", case["name"]);
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

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
