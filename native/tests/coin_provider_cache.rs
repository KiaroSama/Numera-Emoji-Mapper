//! Durable search uses immutable source traces, fresh readback, and shared OS/atomic ownership.
use _native::coin_provider_resolve::{Search, resolve_file};
use serde_json::{Value, json};
use std::path::PathBuf;

#[test]
fn cache_restart_quota_unknown_and_failed_save_preserve_answered_progress() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/paprika-resolve.json"
    ))
    .unwrap();
    let scratch = root
        .join("logs/tmp")
        .join(format!("provider cache {}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let path = scratch.join(format!("cache-{index}.json"));
        _native::atomic::write_json(&path, &case["initial"]).unwrap();
        let missing: Vec<(String, String)> =
            serde_json::from_value(case["missing"].clone()).unwrap();
        let mut replies = case["responses"].as_array().unwrap().iter();
        let mut calls = Vec::new();
        let (result, cache) = resolve_file(
            &path,
            &missing,
            case["retry_unmatched"].as_bool().unwrap(),
            |_, ticker| {
                calls.push(ticker.to_owned());
                let value = replies.next().unwrap();
                Ok(match value.as_str() {
                    Some("quota") => Search::Quota,
                    Some("transient") => Search::Transient,
                    _ => Search::Match {
                        id: value[0].as_str().map(str::to_owned),
                        confidence: value[1].as_str().unwrap().into(),
                    },
                })
            },
        )
        .unwrap();
        assert_eq!(json!(cache), case["expected"]["cache"]);
        assert_eq!(json!(calls), case["expected"]["calls"]);
        assert_eq!(json!(result.quota), case["expected"]["quota"]);
        assert_eq!(json!(result.unknown), case["expected"]["unknown"]);
        let readback: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(readback, case["expected"]["cache"]);
        // No future request may happen while a competing writer owns the cache.
        let lock_path = path.with_extension("json.lock");
        let saved_bytes = std::fs::read(&path).unwrap();
        let held = _native::locks::Lease::acquire(&lock_path, "fixture").unwrap();
        assert!(
            resolve_file(&path, &missing, true, |_, _| panic!(
                "search before cache ownership"
            ))
            .is_err()
        );
        drop(held);
        assert_eq!(std::fs::read(&path).unwrap(), saved_bytes);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(lock_path).unwrap();
    }
    let path = scratch.join("resume.json");
    let missing = vec![("One".into(), "one".into()), ("Two".into(), "two".into())];
    assert!(
        resolve_file(&path, &missing, false, |_, ticker| {
            if ticker == "two" {
                Err("controlled search failure".into())
            } else {
                Ok(Search::Match {
                    id: Some("1".into()),
                    confidence: "symbol".into(),
                })
            }
        })
        .is_err()
    );
    let snapshot = std::fs::read(&path).unwrap();
    let (_, cache) = resolve_file(&path, &missing[..1], false, |_, _| {
        panic!("resolved coin searched again")
    })
    .unwrap();
    assert_eq!(cache["one"]["id"], "1");
    assert_eq!(std::fs::read(&path).unwrap(), snapshot);
    std::fs::write(&path, b"{unfinished").unwrap();
    assert!(resolve_file(&path, &[], false, |_, _| panic!("malformed cache searched")).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"{unfinished");
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_file(path.with_extension("json.lock")).unwrap();
    let blocked = scratch.join("blocked.json");
    assert!(
        resolve_file(&blocked, &missing[..1], false, |_, _| {
            std::fs::create_dir(&blocked).unwrap();
            Ok(Search::Match {
                id: Some("1".into()),
                confidence: "symbol".into(),
            })
        })
        .is_err()
    );
    assert!(blocked.is_dir());
    std::fs::remove_dir(&blocked).unwrap();
    std::fs::remove_file(blocked.with_extension("json.lock")).unwrap();
    let absent = scratch.join("unknown.json");
    let (result, cache) =
        resolve_file(&absent, &missing, false, |_, _| Ok(Search::Transient)).unwrap();
    assert_eq!(result.unknown, 2);
    assert!(cache.is_empty() && !absent.exists());
    std::fs::remove_file(absent.with_extension("json.lock")).unwrap();
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 0);
    std::fs::remove_dir(scratch).unwrap();
}
