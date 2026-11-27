//! A matching manifest cannot replace proof that every live ID has a cached signature.
extern crate numera_runtime as production;

use production::coin_remap_cache::{self, CacheIo};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

#[derive(Default)]
struct Observer {
    members: BTreeMap<String, Vec<Value>>,
    unavailable: BTreeSet<String>,
    fetched: Vec<String>,
    saved: Vec<Value>,
}

impl CacheIo for Observer {
    fn list(&mut self, name: &str) -> Result<Vec<Value>, String> {
        self.members
            .get(name)
            .cloned()
            .ok_or_else(|| "unexpected fixture pack".into())
    }

    fn signature(&mut self, sticker: &Value) -> Result<Vec<u8>, String> {
        let cid = sticker["custom_emoji_id"].as_str().unwrap();
        self.fetched.push(cid.into());
        if self.unavailable.contains(cid) {
            Err("synthetic image unavailable".into())
        } else {
            Ok(vec![cid.as_bytes()[0]; 1024])
        }
    }

    fn save(&mut self, cache: &Value) -> Result<(), String> {
        self.saved.push(cache.clone());
        Ok(())
    }

    fn wait(&mut self, duration: Duration) {
        assert_eq!(duration, Duration::from_millis(20));
    }
}

impl Observer {
    fn pack(&mut self, name: &str, cids: &[&str]) {
        self.members.insert(
            name.into(),
            cids.iter()
                .map(|cid| json!({"custom_emoji_id":cid,"file_unique_id":format!("file-{cid}")}))
                .collect(),
        );
    }

    fn run(&mut self, cache: &mut Value, names: &[&str]) -> Vec<String> {
        coin_remap_cache::download(
            &names
                .iter()
                .map(|name| json!({"name":name}))
                .collect::<Vec<_>>(),
            cache,
            self,
        )
        .unwrap()
    }

    fn checkpoint(&mut self) -> Value {
        let cache = coin_remap_cache::load(self.saved.last().unwrap()).unwrap();
        self.saved.clear();
        self.fetched.clear();
        cache
    }
}

fn complete_two_packs() -> (Observer, Value) {
    let mut observer = Observer::default();
    observer.pack("fixture1", &["a"]);
    observer.pack("fixture2", &["b"]);
    let mut cache = coin_remap_cache::load(&json!({})).unwrap();
    assert!(
        observer
            .run(&mut cache, &["fixture1", "fixture2"])
            .is_empty()
    );
    assert_eq!(observer.fetched, ["a", "b"]);
    let cache = observer.checkpoint();
    (observer, cache)
}

#[test]
fn restored_pack_refetches_signatures_retired_while_its_manifest_was_absent() {
    let (mut observer, mut cache) = complete_two_packs();
    let restored_digest = cache["sets"]["fixture2"].clone();
    assert!(observer.run(&mut cache, &["fixture1"]).is_empty());
    assert_eq!(cache["sets"]["fixture2"], restored_digest);
    assert!(cache["sigs"].get("b").is_none());
    cache = observer.checkpoint();

    assert!(
        observer
            .run(&mut cache, &["fixture1", "fixture2"])
            .is_empty()
    );
    assert_eq!(observer.fetched, ["b"]);
    assert!(cache["sigs"]["a"].is_string());
    assert!(cache["sigs"]["b"].is_string());
    assert_eq!(cache["errors"], json!({}));
    assert_eq!(observer.saved.last(), Some(&cache));
}

#[test]
fn reverted_live_manifest_recovers_a_signature_pruned_during_an_incomplete_change() {
    let mut observer = Observer::default();
    observer.pack("fixture1", &["a", "b"]);
    let mut cache = coin_remap_cache::load(&json!({})).unwrap();
    assert!(observer.run(&mut cache, &["fixture1"]).is_empty());
    let original_digest = cache["sets"]["fixture1"].clone();
    cache = observer.checkpoint();

    observer.pack("fixture1", &["a", "c"]);
    observer.unavailable.insert("c".into());
    assert_eq!(observer.run(&mut cache, &["fixture1"]), ["fixture1"]);
    assert_eq!(cache["sets"]["fixture1"], original_digest);
    assert!(cache["sigs"].get("b").is_none());
    assert!(cache["errors"].get("c").is_some());
    cache = observer.checkpoint();

    observer.pack("fixture1", &["a", "b"]);
    assert!(observer.run(&mut cache, &["fixture1"]).is_empty());
    assert_eq!(observer.fetched, ["b"]);
    assert!(cache["sigs"]["b"].is_string());
    assert_eq!(cache["errors"], json!({}));
    assert_eq!(observer.saved.last(), Some(&cache));
}

#[test]
fn failed_recovery_is_incomplete_and_blocks_apply_until_the_signature_is_proven() {
    let (mut observer, mut cache) = complete_two_packs();
    assert!(observer.run(&mut cache, &["fixture1"]).is_empty());
    cache = observer.checkpoint();
    observer.unavailable.insert("b".into());

    let incomplete = observer.run(&mut cache, &["fixture1", "fixture2"]);
    assert_eq!(incomplete, ["fixture2"]);
    assert_eq!(observer.fetched, ["b"]);
    assert_eq!(cache["errors"], json!({"b":"synthetic image unavailable"}));
    assert!(cache["sigs"].get("b").is_none());
    let options = production::coin_remap_policy::Options {
        apply: true,
        max_distance: Some(1.0),
        min_margin: None,
        incomplete_sets: incomplete.len(),
        failed_images: cache["errors"].as_object().unwrap().len(),
    };
    let decision = production::coin_remap_policy::decide(&[], options).unwrap();
    assert_eq!(decision.code, 3);
    assert_eq!(decision.destination, Some("candidate"));
    cache = observer.checkpoint();

    observer.unavailable.clear();
    assert!(
        observer
            .run(&mut cache, &["fixture1", "fixture2"])
            .is_empty()
    );
    assert_eq!(observer.fetched, ["b"]);
    assert!(cache["sigs"]["b"].is_string());
    assert_eq!(cache["errors"], json!({}));
    assert_eq!(observer.saved.last(), Some(&cache));
}

#[test]
fn unchanged_complete_manifests_reuse_all_signatures_without_downloads_or_writes() {
    let (mut observer, mut cache) = complete_two_packs();
    let before = cache.clone();
    assert!(
        observer
            .run(&mut cache, &["fixture1", "fixture2"])
            .is_empty()
    );
    assert_eq!(cache, before);
    assert!(observer.fetched.is_empty());
    assert!(observer.saved.is_empty());
}

#[test]
fn overlapping_pack_keeps_shared_signatures_and_restored_pack_fetches_only_missing_members() {
    let mut observer = Observer::default();
    observer.pack("fixture1", &["a", "b"]);
    observer.pack("fixture2", &["b"]);
    let mut cache = coin_remap_cache::load(&json!({})).unwrap();
    assert!(
        observer
            .run(&mut cache, &["fixture1", "fixture2"])
            .is_empty()
    );
    assert_eq!(observer.fetched, ["a", "b"]);
    let shared = cache["sigs"]["b"].clone();
    cache = observer.checkpoint();

    assert!(observer.run(&mut cache, &["fixture2"]).is_empty());
    assert_eq!(cache["sigs"], json!({"b":shared}));
    cache = observer.checkpoint();
    let stats = coin_remap_cache::reconcile_with_stats(
        &mut cache,
        &["fixture1".into(), "fixture2".into()],
        &mut observer,
    )
    .unwrap();
    assert_eq!(stats.retained_errors, 0);
    assert_eq!(observer.fetched, ["a"]);
    assert!(cache["sigs"]["a"].is_string());
    assert_eq!(cache["sigs"]["b"], shared);
    assert_eq!(observer.saved.last(), Some(&cache));
}

#[test]
fn unresolved_current_error_requires_a_successful_signature_retry_despite_matching_digest() {
    let (mut observer, mut cache) = complete_two_packs();
    cache["errors"]["b"] = json!("synthetic unresolved prior error");
    observer.unavailable.insert("b".into());
    assert_eq!(
        observer.run(&mut cache, &["fixture1", "fixture2"]),
        ["fixture2"]
    );
    assert_eq!(observer.fetched, ["b"]);
    assert_eq!(cache["errors"], json!({"b":"synthetic image unavailable"}));
    cache = observer.checkpoint();

    observer.unavailable.clear();
    let stats = coin_remap_cache::reconcile_with_stats(
        &mut cache,
        &["fixture1".into(), "fixture2".into()],
        &mut observer,
    )
    .unwrap();
    assert_eq!(stats.retained_errors, 0);
    assert_eq!(observer.fetched, ["b"]);
    assert_eq!(cache["errors"], json!({}));
    assert_eq!(observer.saved.last(), Some(&cache));
}

#[test]
fn public_reconcile_revalidates_unusable_signature_entries_before_trusting_a_digest() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    for invalid in [
        Value::Null,
        json!("not base64"),
        json!(STANDARD.encode([7; 512])),
    ] {
        let (mut observer, mut cache) = complete_two_packs();
        cache["sigs"]["b"] = invalid;
        coin_remap_cache::reconcile(
            &mut cache,
            &["fixture1".into(), "fixture2".into()],
            &mut observer,
        )
        .unwrap();
        assert_eq!(observer.fetched, ["b"]);
        assert_eq!(
            STANDARD
                .decode(cache["sigs"]["b"].as_str().unwrap())
                .unwrap(),
            vec![b'b'; 1024]
        );
        assert_eq!(cache["errors"], json!({}));
        assert_eq!(observer.saved.last(), Some(&cache));
    }
}

#[path = "support/test_admission.rs"]
mod test_admission;
