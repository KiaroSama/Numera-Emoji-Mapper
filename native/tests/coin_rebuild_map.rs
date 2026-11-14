//! Independent pinned rebuild-map snapshots preserve aliases and refusal destinations.
use _native::coin_rebuild_map::{MapIo, run};
use serde_json::{Map, Value, json};
struct Observed<'a> {
    input: &'a Value,
    events: Vec<Value>,
    output: String,
}
impl MapIo for Observed<'_> {
    fn list(&mut self, name: &str) -> Result<Vec<Value>, String> {
        self.events.push(json!(["list", name]));
        Ok(self.input["live"][name].as_array().unwrap().clone())
    }
    fn identity(
        &mut self,
        live: &[(String, String)],
        order: &[String],
    ) -> Result<Map<String, Value>, String> {
        self.events.push(json!(["identity", live, order]));
        if let Some(error) = self.input["identity_error"].as_str() {
            Err(error.into())
        } else {
            Ok(self.input["identity"].as_object().unwrap().clone())
        }
    }
    fn current(&mut self) -> Result<Map<String, Value>, String> {
        let current = self.input["current"].as_object().unwrap().clone();
        if !current.is_empty() {
            self.events.push(json!(["current"]));
        }
        Ok(current)
    }
    fn save(&mut self, candidate: bool, value: &Value) -> Result<(), String> {
        self.events.push(json!([
            "write",
            if candidate {
                "ticker_to_id.candidate.json"
            } else {
                "ticker_to_id.json"
            },
            value
        ]));
        Ok(())
    }
    fn fill(&mut self, mapping: &Map<String, Value>) -> Result<(), String> {
        self.events.push(json!(["fill", mapping]));
        Ok(())
    }
    fn report(&mut self, text: &str) {
        self.output.push_str(text);
        self.output.push('\n');
    }
}
#[test]
fn rebuild_map_preserves_source_admission_alias_and_candidate_effects() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-rebuild-map.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "5fc72119e143b9d20af1f9280921d2503327f6c574e7b34e9d93a7ba1e27310e"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let approved = input["approved"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let mut io = Observed {
            input,
            events: Vec::new(),
            output: String::new(),
        };
        let result = run(
            input["plan"].as_array().unwrap(),
            &input["state"],
            &approved,
            &mut io,
        );
        assert_eq!(
            json!(result.err()),
            case["expected"]["error"],
            "{}",
            case["name"]
        );
        assert_eq!(
            json!(io.events),
            case["expected"]["events"],
            "{}",
            case["name"]
        );
        assert_eq!(
            io.output,
            case["expected"]["output"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
}
