use crate::support::*;
use numera_runtime::{
    self as native,
    coin_audit::{self, AuditIo},
};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
struct Audit {
    root: Scratch,
    sets: BTreeMap<String, Vec<Value>>,
    blobs: BTreeMap<String, Vec<u8>>,
    fail: BTreeSet<String>,
    throttle: BTreeMap<String, u32>,
    kill: Option<String>,
    fetched: Vec<String>,
    listings: u32,
    waits: Vec<Duration>,
}
impl Audit {
    fn new() -> Self {
        let mut a = Self {
            root: Scratch::new(),
            sets: BTreeMap::from([("s1".into(), vec![sticker("a"), sticker("b")])]),
            blobs: BTreeMap::new(),
            fail: BTreeSet::new(),
            throttle: BTreeMap::new(),
            kill: None,
            fetched: vec![],
            listings: 0,
            waits: vec![],
        };
        a.blobs.insert("a".into(), image([200, 20, 20, 255]));
        a.blobs.insert("b".into(), image([20, 200, 20, 255]));
        a
    }
    fn cache(&self) -> std::path::PathBuf {
        self.root.0.join("audit.json")
    }
    fn output(&self) -> std::path::PathBuf {
        self.root.0.join("report.txt")
    }
    fn run(&mut self) -> u8 {
        let mut cache = if self.cache().exists() {
            read_json(&self.cache()).as_object().unwrap().clone()
        } else {
            Map::new()
        };
        let sets = self
            .sets
            .keys()
            .enumerate()
            .map(|(i, name)| json!({"name":name,"index":i+1}))
            .collect::<Vec<_>>();
        coin_audit::run(&sets, &mut cache, self).unwrap()
    }
    fn text(&self) -> String {
        std::fs::read_to_string(self.output()).unwrap()
    }
}
fn image(color: [u8; 4]) -> Vec<u8> {
    native::native_raster::encode_png(&image::RgbaImage::from_pixel(100, 100, image::Rgba(color)))
        .unwrap()
}
fn sticker(cid: &str) -> Value {
    json!({"custom_emoji_id":cid,"file_unique_id":format!("fu-{cid}"),"file_id":cid})
}
impl AuditIo for Audit {
    fn list(&mut self, name: &str) -> Result<Vec<Value>, String> {
        self.listings += 1;
        if self.kill.as_deref() == Some(name) {
            panic!("killed mid-run");
        }
        if let Some(left) = self.throttle.get_mut(name)
            && *left > 0
        {
            *left -= 1;
            return Err("429 Too Many Requests for TOKEN123".into());
        }
        Ok(self.sets[name].clone())
    }
    fn analyze(&mut self, sticker: &Value) -> Result<(String, bool), String> {
        let id = sticker["file_id"].as_str().unwrap();
        self.fetched.push(id.into());
        if self.fail.contains(id) {
            Err(format!("download failed for file_id {id}"))
        } else {
            native::codec_image::audit_image(&self.blobs[id])
        }
    }
    fn save(&mut self, cache: &Map<String, Value>) -> Result<(), String> {
        native::atomic::write_json(&self.cache(), &Value::Object(cache.clone()))
            .map_err(|error| error.to_string())
    }
    fn report(&mut self, text: &str) -> Result<(), String> {
        native::atomic::write_bytes(&self.output(), text.as_bytes())
            .map_err(|error| error.to_string())
    }
    fn wait(&mut self, d: Duration) {
        self.waits.push(d);
    }
}
// tests.test_check_all_packs.CheckAllPacksTest.test_deleted_sticker_leaves_totals_and_duplicate_groups
#[test]
fn deleted_cached_sticker_leaves_live_totals_and_duplicate_groups() {
    let mut a = Audit::new();
    document(
        &a.cache(),
        &json!({"a":{"hash":"H1","blank":false},"b":{"hash":"H2","blank":false},"gone":{"hash":"H1","blank":false}}),
    );
    a.blobs.insert("b".into(), a.blobs["a"].clone());
    assert_eq!(a.run(), 0);
    assert!(a.text().contains("live stickers: 2"));
    assert!(a.text().contains("DUPLICATE image groups: 0"));
    assert!(!std::fs::read_to_string(a.cache()).unwrap().contains("gone"));
}
// tests.test_check_all_packs.CheckAllPacksTest.test_analyse_failure_is_an_error_that_is_retried_not_a_blank
#[test]
fn failed_analysis_is_retryable_error_and_never_cached_blank() {
    let mut a = Audit::new();
    a.fail.insert("b".into());
    assert_eq!(a.run(), 3);
    assert!(a.text().contains("FAILED (retried on next run): 1"));
    assert!(a.text().contains("BLANK stickers: 0"));
    assert!(read_json(&a.cache())["b"].get("error").is_some());
    assert!(
        !std::fs::read_to_string(a.cache())
            .unwrap()
            .contains("TOKEN123")
    );
    a.fail.clear();
    a.fetched.clear();
    assert_eq!(a.run(), 0);
    assert_eq!(a.fetched, ["b"]);
    assert!(a.text().contains("FAILED (retried on next run): 0"));
}
// tests.test_check_all_packs.CheckAllPacksTest.test_blank_sticker_is_still_reported_with_live_position
#[test]
fn blank_sticker_is_reported_with_live_zero_based_position() {
    let mut a = Audit::new();
    a.blobs.insert("b".into(), image([0, 0, 0, 0]));
    assert_eq!(a.run(), 0);
    assert!(a.text().contains("blank cid=b set=1 pos=1"));
}
// tests.test_check_all_packs.CheckAllPacksListingRetry.test_a_throttled_listing_is_retried_instead_of_ending_the_run
#[test]
fn throttled_listing_retries_without_exposing_token() {
    let mut a = Audit::new();
    a.sets.get_mut("s1").unwrap().truncate(1);
    a.throttle.insert("s1".into(), 2);
    assert_eq!(a.run(), 0);
    assert_eq!(a.listings, 3);
    assert!(a.text().contains("live stickers: 1"));
    assert!(!a.text().contains("TOKEN123"));
    assert_eq!(
        &a.waits[..2],
        &[Duration::from_secs(2), Duration::from_secs(4)]
    );
    // Capture the actual command's retry diagnostics as well as the injected core clock.
    let f = Fixture::new();
    document(
        &f.root.join("coins/rebuild_dedup_state.json"),
        &json!({"sets":[{"index":1,"name":"s1"}]}),
    );
    let payload = image([200, 20, 20, 255]);
    let mut listings = 0;
    let bot = crate::support::http::Server::new(move |r| {
        use crate::support::http::Reply;
        if r.verb == "GET" {
            return Reply::Bytes(200, payload.clone());
        }
        match r.method() {
            "getStickerSet" => {
                listings += 1;
                if listings < 3 {
                    Reply::json(
                        json!({"ok":false,"description":"Too Many Requests: retry after 0 for TOKEN123","parameters":{"retry_after":0}}),
                    )
                } else {
                    Reply::ok(json!({"stickers":[sticker("a")]}))
                }
            }
            "getFile" => Reply::ok(json!({"file_path":"a.png"})),
            _ => panic!("unexpected {}", r.method()),
        }
    });
    let mut env = bot.environment();
    env.extend([
        ("TELEGRAM_BOT_TOKEN", "TOKEN123".into()),
        ("COIN_PACK_BASE", "s".into()),
    ]);
    let run = f.run(&["coin-check-packs"], &env);
    run.code(0);
    assert_eq!(
        bot.methods()
            .iter()
            .filter(|m| *m == "getStickerSet")
            .count(),
        3
    );
    assert!(!run.out.contains("TOKEN123"));
    assert!(!run.err.contains("TOKEN123"));
    let report = std::fs::read_to_string(f.root.join("coins/pack_audit4_report.txt")).unwrap();
    assert!(report.contains("live stickers: 1"));
    assert!(!report.contains("TOKEN123"));
}
// tests.test_check_all_packs.CheckAllPacksListingRetry.test_work_already_analysed_survives_a_set_that_will_not_list
#[test]
fn checkpoint_survives_unlistable_next_set_and_report_is_refused() {
    let mut a = Audit::new();
    a.sets.insert("s1".into(), vec![sticker("a")]);
    a.sets.insert("s2".into(), vec![sticker("b")]);
    a.throttle.insert("s2".into(), 99);
    assert_eq!(a.run(), 4);
    assert_eq!(
        read_json(&a.cache())
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["a"]
    );
    assert!(!a.output().exists());
}
// tests.test_check_all_packs.CheckAllPacksListingRetry.test_each_set_boundary_checkpoints_the_audit
#[test]
fn each_set_boundary_checkpoints_before_interrupted_listing() {
    let mut a = Audit::new();
    a.sets.insert("s1".into(), vec![sticker("a")]);
    a.sets.insert("s2".into(), vec![sticker("b")]);
    a.kill = Some("s2".into());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| a.run()));
    assert!(result.is_err());
    assert!(a.cache().is_file());
    assert_eq!(
        read_json(&a.cache())
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["a"]
    );
}
