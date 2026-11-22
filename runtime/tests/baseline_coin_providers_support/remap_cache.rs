use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
// tests.test_remap_ids.DownloadLiveTest.test_failed_sticker_leaves_set_incomplete_and_is_retried
#[test]
fn test_failed_sticker_leaves_set_incomplete_and_is_retried() {
    let mut io = CacheFixture::new(&["a", "b"]);
    io.fail.insert("b".into());
    let mut cache = rt::coin_remap_cache::load(&json!({})).unwrap();
    assert_eq!(io.run(&mut cache), ids(&["s1"]));
    assert!(cache["sets"].get("s1").is_none());
    assert_eq!(
        cache["sigs"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        ids(&["a"])
    );
    assert!(cache["errors"].get("b").is_some());
    assert!(!cache.to_string().contains("TOKEN123"));
    io.fail.clear();
    io.fetched.clear();
    let mut cache = rt::coin_remap_cache::load(&doc(&io.path())).unwrap();
    assert!(io.run(&mut cache).is_empty());
    assert_eq!(io.fetched, ids(&["b"]));
    let after = rt::coin_remap_cache::load(&doc(&io.path())).unwrap();
    assert!(after["sets"].get("s1").is_some());
    assert_eq!(after["errors"], json!({}));
}
// tests.test_remap_ids.DownloadLiveTest.test_changed_manifest_invalidates_cache_and_prunes_stale_ids
#[test]
fn test_changed_manifest_invalidates_cache_and_prunes_stale_ids() {
    let mut io = CacheFixture::new(&["a", "b"]);
    let mut cache = rt::coin_remap_cache::load(&json!({})).unwrap();
    assert!(io.run(&mut cache).is_empty());
    let before = cache["sets"]["s1"].clone();
    io.members = vec![sticker("a"), sticker("c")];
    io.fetched.clear();
    assert!(io.run(&mut cache).is_empty());
    assert_eq!(io.fetched, ids(&["c"]));
    assert_ne!(cache["sets"]["s1"], before);
    let mut keys = cache["sigs"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    keys.sort();
    assert_eq!(keys, ids(&["a", "c"]));
}
// tests.test_remap_ids.DownloadLiveTest.test_appended_sticker_is_picked_up
#[test]
fn test_appended_sticker_is_picked_up() {
    let mut io = CacheFixture::new(&["a"]);
    let mut cache = rt::coin_remap_cache::load(&json!({})).unwrap();
    io.run(&mut cache);
    io.members.push(sticker("b"));
    io.fetched.clear();
    io.run(&mut cache);
    assert_eq!(io.fetched, ids(&["b"]));
    let mut keys = cache["sigs"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    keys.sort();
    assert_eq!(keys, ids(&["a", "b"]));
}
// tests.test_remap_ids.DownloadLiveTest.test_legacy_cache_keeps_signatures_but_re_verifies_sets
#[test]
fn test_legacy_cache_keeps_signatures_but_re_verifies_sets() {
    let t = Root::new();
    let path = t.0.join("cache.json");
    let sig = STANDARD.encode(vec![0; 1024]);
    write(&path, &json!({"done_sets":[1],"sigs":{"a":sig}}));
    let cache = rt::coin_remap_cache::load(&doc(&path)).unwrap();
    assert_eq!(cache["sets"], json!({}));
    assert_eq!(
        cache["sigs"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["a"]
    );
}
// tests.test_remap_ids.ACacheFromAnOlderSignatureIsDiscarded.test_signatures_of_the_wrong_length_are_dropped
#[test]
fn test_signatures_of_the_wrong_length_are_dropped() {
    let t = Root::new();
    let path = t.0.join("cache.json");
    write(
        &path,
        &json!({"sets":{"s1":"digest"},"sigs":{"old":STANDARD.encode(vec![0;512]),"new":STANDARD.encode(vec![0;1024])},"errors":{}}),
    );
    let cache = rt::coin_remap_cache::load(&doc(&path)).unwrap();
    assert!(
        cache["sigs"].get("old").is_none(),
        "stale-format signature survived"
    );
    assert!(cache["sigs"].get("new").is_some());
    assert_eq!(
        cache["sets"],
        json!({}),
        "completeness marks must be cleared"
    );
}
