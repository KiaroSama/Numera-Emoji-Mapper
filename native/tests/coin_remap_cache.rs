//! Pinned manifest-key/cache progression preserves retryable failures and live-only candidates.
use _native::coin_remap_cache::{CacheIo, download, load};
use serde_json::{Value, json};
use std::time::Duration;
struct Fixture {
    input: Value,
    downloads: Vec<String>,
    writes: Vec<Value>,
    waits: Vec<f64>,
}
impl CacheIo for Fixture {
    fn list(&mut self, name: &str) -> Result<Vec<Value>, String> {
        Ok(self.input["sets"][name].as_array().unwrap().iter().map(|cid|json!({"custom_emoji_id":cid,"file_unique_id":format!("u-{}",cid.as_str().unwrap()),"file_id":cid})).collect())
    }
    fn signature(&mut self, sticker: &Value) -> Result<Vec<u8>, String> {
        let cid = sticker["file_id"].as_str().unwrap();
        self.downloads.push(cid.into());
        if self.input["fail"]
            .as_array()
            .is_some_and(|values| values.iter().any(|value| value == cid))
        {
            return Err("controlled decode failure".into());
        }
        Ok(vec![0; 1024])
    }
    fn save(&mut self, cache: &Value) -> Result<(), String> {
        self.writes.push(cache.clone());
        Ok(())
    }
    fn wait(&mut self, duration: Duration) {
        self.waits.push(duration.as_secs_f64());
    }
}
#[test]
fn source_remap_cache_uses_live_manifest_retries_errors_and_prunes_deleted_ids() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-remap-cache.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "b66136d42adc3448d44e67c36c9f8c827a4e63d6ce259af5d6b437299b043ed7"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let mut cache = load(&input["cache"]).unwrap();
        assert_eq!(cache, case["expected"]["loaded"]);
        let sets = input["sets"]
            .as_object()
            .unwrap()
            .keys()
            .enumerate()
            .map(|(index, name)| json!({"name":name,"index":index+1}))
            .collect::<Vec<_>>();
        let mut io = Fixture {
            input: input.clone(),
            downloads: vec![],
            writes: vec![],
            waits: vec![],
        };
        assert_eq!(
            json!(download(&sets, &mut cache, &mut io).unwrap()),
            case["expected"]["incomplete"]
        );
        assert_eq!(cache, case["expected"]["cache"]);
        assert_eq!(json!(io.downloads), case["expected"]["downloads"]);
        assert_eq!(json!(io.writes), case["expected"]["writes"]);
        let waits = case["expected"]["waits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_f64().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(io.waits, waits);
    }
    assert!(load(&json!({"sigs":{"x":"%%%%"}})).is_err());
    original_cache_assertions();
}

fn original_cache_assertions() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let old = STANDARD.encode(vec![0; 512]);
    let current = STANDARD.encode(vec![0; 1024]);
    assert_eq!(
        load(&json!({"done_sets":[1],"sigs":{"a":current}})).unwrap(),
        json!({"sets":{},"sigs":{"a":current},"errors":{}})
    );
    assert_eq!(
        load(&json!({"sets":{"s1":"digest"},"sigs":{"old":old,"new":current},"errors":{}}))
            .unwrap(),
        json!({"sets":{},"sigs":{"new":current},"errors":{}})
    );
    for scenario in ["retry", "replace", "append"] {
        let mut io = Fixture {
            input: json!({"sets":{"s1":if scenario == "append" {json!(["a"])} else {json!(["a","b"]) }},
                "fail":if scenario == "retry" {json!(["b"])} else {json!([])}}),
            downloads: vec![],
            writes: vec![],
            waits: vec![],
        };
        let sets = [json!({"index":1,"name":"s1"})];
        let mut cache = load(&json!({})).unwrap();
        assert_eq!(
            download(&sets, &mut cache, &mut io).unwrap(),
            if scenario == "retry" {
                vec!["s1"]
            } else {
                vec![]
            }
        );
        assert_eq!(
            io.downloads,
            if scenario == "append" {
                vec!["a"]
            } else {
                vec!["a", "b"]
            }
        );
        if scenario == "retry" {
            assert!(cache["sets"].get("s1").is_none());
            assert_eq!(
                cache["sigs"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                ["a"]
            );
            assert!(cache["errors"].get("b").is_some());
        }
        let before = cache["sets"]["s1"].clone();
        // Resume the serialized checkpoint, not the first run's in-memory cache.
        cache = load(io.writes.last().unwrap()).unwrap();
        io.input["sets"]["s1"] = if scenario == "replace" {
            json!(["a", "c"])
        } else {
            json!(["a", "b"])
        };
        io.input["fail"] = json!([]);
        io.downloads.clear();
        io.writes.clear();
        io.waits.clear();
        assert!(download(&sets, &mut cache, &mut io).unwrap().is_empty());
        assert_eq!(
            io.downloads,
            if scenario == "replace" {
                vec!["c"]
            } else {
                vec!["b"]
            }
        );
        assert_eq!(io.waits, [0.02]);
        assert!(cache["sets"]["s1"].is_string());
        assert_eq!(cache["errors"], json!({}));
        assert_eq!(
            cache["sigs"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            if scenario == "replace" {
                vec!["a", "c"]
            } else {
                vec!["a", "b"]
            }
        );
        if scenario == "replace" {
            assert_ne!(cache["sets"]["s1"], before);
        }
        assert_eq!(io.writes.last(), Some(&cache));
        assert!(!cache.to_string().contains("TOKEN123"));
    }
}
