//! Large catalogs must not render every animation just because the panel opened.
use _native::{media_adapter::MediaCodec, panel_preview::Previews};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

#[derive(Clone)]
struct Recorder(Arc<Mutex<Vec<Value>>>);
impl MediaCodec for Recorder {
    fn with_scratch(&self, _: PathBuf) -> Self {
        self.clone()
    }
    async fn call(&self, request: Value) -> Result<Value, String> {
        std::fs::write(request["output"].as_str().unwrap(), b"recorded render")
            .map_err(|error| error.to_string())?;
        self.0.lock().unwrap().push(request);
        Ok(Value::Null)
    }
}

#[test]
fn opening_a_thousand_animations_primes_only_stills_and_later_requests_reuse_cache() {
    let root = tooling_fixture::Root::new("preview demand");
    let source = root.0.join("source.tgs");
    std::fs::write(
        &source,
        b"policy fixture; real codec coverage is in preview_warm",
    )
    .unwrap();
    let mut cards = vec![json!({"key":"__logo","isLogo":true})];
    let mut paths = serde_json::Map::new();
    for index in 0..1000 {
        let key = format!("a:{index}");
        cards.push(json!({"key":key,"fmt":"animated","included":true}));
        paths.insert(key, json!(source));
    }
    let view = json!({"view":cards,"by_key":paths});
    let calls = Arc::new(Mutex::new(Vec::new()));
    let previews = Previews::new(Recorder(calls.clone()), &root.0, 2);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        assert_eq!(
            previews.warm_initial(&view, 15).await,
            json!({"rendered":16,"cached":0,"failed":0})
        );
        assert!(
            calls
                .lock()
                .unwrap()
                .iter()
                .all(|call| call["still"] == true)
        );
        assert_eq!(
            previews.warm_initial(&view, 15).await,
            json!({"rendered":0,"cached":16,"failed":0})
        );
        assert_eq!(calls.lock().unwrap().len(), 16);
        // An offscreen item remains available on demand and is then cached.
        for _ in 0..2 {
            assert_eq!(
                previews
                    .bytes("a:999", &source, 15, false, 104)
                    .await
                    .unwrap(),
                b"recorded render"
            );
        }
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 17);
        assert_eq!(calls.last().unwrap()["still"], false);
    });
}
