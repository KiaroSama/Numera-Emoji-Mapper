//! Source-observed resume/reconcile/report behavior uses virtual transport and time.
use _native::coin_audit::{AuditIo, run};
use serde_json::{Map, Value, json};
use std::time::Duration;

struct Fixture {
    input: Value,
    calls: Vec<String>,
    downloads: Vec<String>,
    writes: Vec<Value>,
    waits: Vec<f64>,
    reports: Vec<String>,
}
impl AuditIo for Fixture {
    fn list(&mut self, name: &str) -> Result<Vec<Value>, String> {
        self.calls.push(name.into());
        let count = self.calls.iter().filter(|called| *called == name).count();
        if count as u64 <= self.input["failures"][name].as_u64().unwrap_or(0) {
            return Err("listing refused".into());
        }
        Ok(self.input["sets"][name]
            .as_array()
            .unwrap()
            .iter()
            .map(|cid| json!({"custom_emoji_id":cid,"file_id":cid}))
            .collect())
    }
    fn analyze(&mut self, sticker: &Value) -> Result<(String, bool), String> {
        let cid = sticker["file_id"].as_str().unwrap();
        self.downloads.push(cid.into());
        let value = &self.input["analysis"][cid];
        if let Some(error) = value.as_str() {
            return Err(error.into());
        }
        Ok((
            value[0].as_str().unwrap().into(),
            value[1].as_bool().unwrap(),
        ))
    }
    fn save(&mut self, value: &Map<String, Value>) -> Result<(), String> {
        self.writes.push(json!(value));
        Ok(())
    }
    fn report(&mut self, text: &str) -> Result<(), String> {
        self.reports.push(text.into());
        Ok(())
    }
    fn wait(&mut self, duration: Duration) {
        self.waits.push(duration.as_secs_f64());
    }
}

#[test]
fn source_audit_retries_failed_art_keeps_unknown_sets_and_drops_deleted_ids() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/coin-audit.json")).unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "f62da85bd939464a2673a758740cab9001ae0920a000df32f7827d42485b5272"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let sets = input["sets"]
            .as_object()
            .unwrap()
            .keys()
            .enumerate()
            .map(|(index, name)| json!({"name":name,"index":index+1}))
            .collect::<Vec<_>>();
        let mut cache = input["cache"].as_object().unwrap().clone();
        let mut io = Fixture {
            input: input.clone(),
            calls: vec![],
            downloads: vec![],
            writes: vec![],
            waits: vec![],
            reports: vec![],
        };
        assert_eq!(
            u64::from(run(&sets, &mut cache, &mut io).unwrap()),
            case["expected"]["code"].as_u64().unwrap()
        );
        assert_eq!(json!(io.calls), case["expected"]["calls"]);
        assert_eq!(json!(io.downloads), case["expected"]["downloads"]);
        assert_eq!(json!(io.writes), case["expected"]["writes"]);
        assert_eq!(json!(io.waits), case["expected"]["waits"]);
        assert_eq!(json!(io.reports), case["expected"]["reports"]);
    }
    let mut live = Map::new();
    live.insert("x".into(), json!({"set":1,"pos":0}));
    assert!(
        _native::coin_audit::report(
            &Map::from_iter([("x".into(), json!({"hash":"h","blank":false}))]),
            &live
        )
        .unwrap()
        .0
        .contains("live stickers: 1")
    );
}
