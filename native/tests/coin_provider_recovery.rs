//! Pinned recovery ordering and original recorded image identity; all external seams fake.
use _native::coin_provider_proof::{RecoveryIo, recover};
use serde_json::{Map, Value, json};

struct Evidence<'a> {
    case: &'a Value,
    writes: Vec<Value>,
    proofs: Vec<Value>,
    promotions: Vec<Value>,
    hashes: Vec<Value>,
}
impl RecoveryIo for Evidence<'_> {
    fn fallback_hash(&mut self, _intent: &Value) -> Result<u64, String> {
        self.hashes.push(json!("aaa.png"));
        Ok(23)
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
            .push(json!(["aaa.png", intent["key"], want]));
        Ok(self.case["promote"].as_bool().unwrap())
    }
}

#[test]
fn unknown_keeps_intent_and_confirmed_recovery_saves_map_before_matching_source() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/provider-recovery.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut state = case["initial"].clone();
        let mut mapping = Map::new();
        let mut io = Evidence {
            case,
            writes: Vec::new(),
            proofs: Vec::new(),
            promotions: Vec::new(),
            hashes: Vec::new(),
        };
        let outcome = recover(&mut state, &mut mapping, &mut io);
        let result = match outcome {
            Ok(ticker) => json!({"ticker":ticker}),
            Err(_) => json!({"unknown":true}),
        };
        assert_eq!(
            json!({"result":result,"state":state,"mapping":mapping,"writes":io.writes,
            "proofs":io.proofs,"promotions":io.promotions,"hashes":io.hashes}),
            case["expected"]
        );
    }
}
