//! Complete live inventories retire failures belonging to stickers that no longer exist.
extern crate numera_runtime as production;

use production::coin_remap_cache::{self, CacheIo, ReconcileStats};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

#[derive(Default)]
struct Observer {
    members: BTreeMap<String, Vec<Value>>,
    unavailable_sets: BTreeSet<String>,
    unavailable_images: BTreeSet<String>,
    fetched: Vec<String>,
    saved: Vec<Value>,
}

impl CacheIo for Observer {
    fn list(&mut self, name: &str) -> Result<Vec<Value>, String> {
        if self.unavailable_sets.contains(name) {
            return Err("synthetic listing unavailable".into());
        }
        self.members
            .get(name)
            .cloned()
            .ok_or_else(|| "unexpected set".into())
    }
    fn signature(&mut self, sticker: &Value) -> Result<Vec<u8>, String> {
        let cid = sticker["custom_emoji_id"].as_str().unwrap();
        self.fetched.push(cid.into());
        if self.unavailable_images.contains(cid) {
            Err("synthetic image unavailable".into())
        } else {
            Ok(vec![23; 1024])
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

fn sticker(cid: &str) -> Value {
    json!({"custom_emoji_id":cid,"file_unique_id":format!("file-{cid}")})
}

#[test]
fn removed_failed_sticker_no_longer_blocks_a_fully_proven_remap() {
    let mut observer = Observer::default();
    observer.members.insert(
        "fixture1".into(),
        vec![sticker("kept"), sticker("removed"), sticker("stale")],
    );
    observer.unavailable_images.insert("removed".into());
    let sets = [json!({"name":"fixture1"})];
    let mut cache = coin_remap_cache::load(&json!({})).unwrap();
    assert_eq!(
        coin_remap_cache::download(&sets, &mut cache, &mut observer).unwrap(),
        ["fixture1"]
    );
    assert!(cache["errors"].get("removed").is_some());
    assert!(cache["sigs"].get("removed").is_none());
    let kept = cache["sigs"]["kept"].clone();

    observer
        .members
        .insert("fixture1".into(), vec![sticker("kept")]);
    observer.fetched.clear();
    observer.saved.clear();
    let stats =
        coin_remap_cache::reconcile_with_stats(&mut cache, &["fixture1".into()], &mut observer)
            .unwrap();
    assert_eq!(
        stats,
        ReconcileStats {
            retired_sigs: 1,
            retired_errors: 1,
            retained_errors: 0
        }
    );
    assert!(
        observer.fetched.is_empty(),
        "valid cached artwork must remain reusable"
    );
    assert_eq!(cache["sigs"], json!({"kept":kept}));
    assert_eq!(cache["errors"], json!({}));
    assert_eq!(
        observer.saved.last(),
        Some(&cache),
        "failure retirement must be durable"
    );
    let decision = production::coin_remap_policy::decide(
        &[production::coin_remap_policy::Match {
            ticker: "btc".into(),
            cid: "kept".into(),
            distance: 0.0,
            margin: f64::INFINITY,
        }],
        production::coin_remap_policy::Options {
            apply: true,
            max_distance: Some(1.0),
            min_margin: None,
            incomplete_sets: usize::from(cache["sets"]["fixture1"].is_null()),
            failed_images: cache["errors"].as_object().unwrap().len(),
        },
    )
    .unwrap();
    assert_eq!(decision.code, 0);
    assert_eq!(decision.destination, Some("map"));
}

#[test]
fn error_only_cache_is_pruned_after_a_complete_empty_inventory() {
    let mut observer = Observer::default();
    observer.members.insert("fixture1".into(), vec![]);
    let mut cache = coin_remap_cache::load(&json!({"errors":{"removed":"old failure"}})).unwrap();
    let stats =
        coin_remap_cache::reconcile_with_stats(&mut cache, &["fixture1".into()], &mut observer)
            .unwrap();
    assert_eq!(
        stats,
        ReconcileStats {
            retired_sigs: 0,
            retired_errors: 1,
            retained_errors: 0
        }
    );
    assert_eq!(cache["sigs"], json!({}));
    assert_eq!(cache["errors"], json!({}));
    assert_eq!(observer.saved.last(), Some(&cache));
    assert!(observer.fetched.is_empty());
}

#[test]
fn current_failed_sticker_remains_retryable_while_removed_failures_are_retired() {
    let mut observer = Observer::default();
    observer
        .members
        .insert("fixture1".into(), vec![sticker("current")]);
    observer.unavailable_images.insert("current".into());
    let mut cache = coin_remap_cache::load(
        &json!({"errors":{"removed":"old failure","current":"old failure"}}),
    )
    .unwrap();
    let stats =
        coin_remap_cache::reconcile_with_stats(&mut cache, &["fixture1".into()], &mut observer)
            .unwrap();
    assert_eq!(
        stats,
        ReconcileStats {
            retired_sigs: 0,
            retired_errors: 1,
            retained_errors: 1
        }
    );
    assert_eq!(
        cache["errors"],
        json!({"current":"synthetic image unavailable"})
    );
    assert_eq!(cache["sigs"], json!({}));
    assert!(cache["sets"].get("fixture1").is_none());
    assert_eq!(observer.fetched, ["current"]);
    assert_eq!(observer.saved.last(), Some(&cache));

    observer.unavailable_images.clear();
    observer.fetched.clear();
    assert!(
        coin_remap_cache::download(&[json!({"name":"fixture1"})], &mut cache, &mut observer)
            .unwrap()
            .is_empty()
    );
    assert_eq!(observer.fetched, ["current"]);
    assert_eq!(cache["errors"], json!({}));
    assert!(cache["sigs"].get("current").is_some());
    assert!(cache["sets"].get("fixture1").is_some());
}

#[test]
fn later_unavailable_listing_preserves_failures_without_complete_membership_evidence() {
    let mut observer = Observer::default();
    observer.members.insert("fixture1".into(), vec![]);
    observer.unavailable_sets.insert("fixture2".into());
    let mut cache = coin_remap_cache::load(&json!({"errors":{"unknown":"old failure"}})).unwrap();
    let failure = coin_remap_cache::download(
        &[json!({"name":"fixture1"}), json!({"name":"fixture2"})],
        &mut cache,
        &mut observer,
    )
    .unwrap_err();
    assert_eq!(failure, "synthetic listing unavailable");
    assert_eq!(cache["errors"], json!({"unknown":"old failure"}));
    assert_eq!(observer.saved.last().unwrap()["errors"], cache["errors"]);
    assert!(observer.fetched.is_empty());
}

#[path = "support/test_admission.rs"]
mod test_admission;
