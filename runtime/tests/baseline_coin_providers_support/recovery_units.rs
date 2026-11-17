//! A typed fatal call-boundary fault must retain intent without inventing a proof outage.
use crate as rt;
use crate::baseline_coin_provider_support::*;
use serde_json::json;
use std::sync::{Arc, Mutex};
// tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_an_unexpected_add_failure_keeps_the_intent_and_stops
#[test]
fn test_an_unexpected_add_failure_keeps_the_intent_and_stops() {
    let f = Provider::new(2);
    write(&f.files.mapping, &json!({}));
    let mut stage = f.stage(&["aaa", "bbb"], &f.ours);
    let attempts = Arc::new(Mutex::new(vec![]));
    let seen = attempts.clone();
    let state = f.files.state.clone();
    let ours = f.ours.clone();
    let server = Server::new(vec![
        me(),
        listing(&f.before),
        listing(&f.before),
        listing(&f.before),
    ]);
    let tg = server
        .telegram()
        .with_call_fault(Arc::new(move |method, data, uploads| {
            if method != "addStickerToSet" {
                return None;
            }
            let intent = doc(&state)["provider_in_flight"].clone();
            assert_eq!(intent["key"], "aaa");
            assert_eq!(intent["expected_before"], 2);
            assert_eq!(data["name"], SET);
            assert_eq!(uploads.len(), 1);
            assert_eq!(uploads[0].bytes, ours);
            seen.lock()
                .unwrap()
                .push(intent["key"].as_str().unwrap().to_owned());
            Some(rt::telegram::ApiError::Invalid(
                "controlled fatal owned I/O failure at add call".into(),
            ))
        }));
    let result = rt::coin_provider_effects::run(
        &f.files,
        &tg,
        &mut stage,
        &ids(&["aaa", "bbb"]),
        "T",
        1,
        &serde_json::Map::new(),
    )
    .unwrap();
    assert_eq!((result.0, result.1), (0, 2));
    assert_eq!(*attempts.lock().unwrap(), vec!["aaa"]);
    assert_eq!(doc(&f.files.state)["provider_in_flight"]["key"], "aaa");
    assert_eq!(doc(&f.files.mapping), json!({}));
    let (health, live) = tg.probe(SET);
    assert_eq!(health, rt::telegram::SetState::Exists);
    assert_eq!(live.unwrap()["stickers"].as_array().unwrap().len(), 2);
    let requests = server.finish();
    assert!(
        requests
            .iter()
            .all(|r| r.method != "addStickerToSet" && r.method != "createNewStickerSet")
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method == "getStickerSet")
            .count(),
        3
    );
    f.release();
}
