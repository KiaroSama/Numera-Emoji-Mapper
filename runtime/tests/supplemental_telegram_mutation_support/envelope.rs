//! Malformed Bot API outcomes cannot prove absence or discard uncertain mutations.
use super::http;
use numera_runtime::{
    atomic, coin_provider_recover,
    coin_provider_stage::Stage,
    telegram::{ApiError, SetState},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

const SET: &str = "fixture1_by_bot";

fn malformed_outcomes() -> Vec<Value> {
    let mut bodies = vec![json!({"description":"Bad Request: STICKERSET_INVALID"})];
    bodies.extend(
        [Value::Null, json!("false"), json!(0), json!([]), json!({})]
            .into_iter()
            .map(|ok| json!({"ok":ok,"description":"Bad Request: STICKERSET_INVALID"})),
    );
    bodies
}

#[test]
fn a_missing_or_nonboolean_outcome_cannot_prove_a_set_missing() {
    for body in malformed_outcomes() {
        let bot = http::Bot::new(move |request| {
            assert_eq!(request.method, "getStickerSet");
            assert_eq!(request.form["name"], SET);
            http::payload(body.clone())
        });
        assert_eq!(bot.telegram().probe(SET), (SetState::Unknown, None));
        assert_eq!(bot.methods(), ["getStickerSet"]);
    }
}

#[test]
fn malformed_mutation_outcomes_use_live_proof_without_resending() {
    for body in malformed_outcomes() {
        let bot = http::Bot::new(move |request| {
            assert_eq!(request.method, "addStickerToSet");
            http::payload(body.clone())
        });
        let mut probes = 0;
        let mut waits = Vec::new();
        let result = bot.telegram().call_with_wait(
            "addStickerToSet",
            &BTreeMap::new(),
            &[],
            5,
            Some(|| {
                probes += 1;
                Ok(None)
            }),
            |delay| waits.push(delay),
        );
        assert!(matches!(result, Err(ApiError::Ambiguous(_))), "{result:?}");
        assert_eq!(probes, 1);
        assert_eq!(waits, [Duration::from_secs(2)]);
        assert_eq!(bot.methods(), ["addStickerToSet"]);
    }
}

#[test]
fn malformed_missing_evidence_preserves_provider_intent_and_source() {
    for body in malformed_outcomes() {
        let root = tempfile::tempdir().unwrap();
        let published = root.path().join("logos/emoji");
        let mut stage = Stage::new(&published).unwrap();
        let source = stage.source("aaa").unwrap();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([200, 30, 30, 255]))
            .save(&source)
            .unwrap();
        stage.clean(Some(&source)).unwrap();
        let source_before = std::fs::read(&source).unwrap();
        let files = coin_provider_recover::Files {
            root: root.path().into(),
            state: root.path().join("state.json"),
            mapping: root.path().join("map.json"),
            published,
            base: "fixture".into(),
        };
        let state = json!({"sets":[{"index":1,"name":SET,"live":2}],
            "provider_added":[],"provider_in_flight":{"key":"aaa","operation":"add",
                "set_name":SET,"set_index":1,"expected_before":2,"source_dhash":0,
                "source_path":source}});
        atomic::write_json(&files.state, &state).unwrap();
        atomic::write_json(&files.mapping, &json!({"seed":"existing-id"})).unwrap();
        let state_before = std::fs::read(&files.state).unwrap();
        let mapping_before = std::fs::read(&files.mapping).unwrap();
        let bot = http::Bot::new(move |request| {
            assert_eq!(request.method, "getStickerSet");
            assert_eq!(request.form["name"], SET);
            http::payload(body.clone())
        });
        let result = coin_provider_recover::run(&files, &bot.telegram());
        assert!(
            result.is_err(),
            "uncertain recovery was cleared: {result:?}"
        );
        assert_eq!(std::fs::read(&files.state).unwrap(), state_before);
        assert_eq!(std::fs::read(&files.mapping).unwrap(), mapping_before);
        assert_eq!(std::fs::read(&source).unwrap(), source_before);
        assert_eq!(bot.methods(), ["getStickerSet"]);
    }
}

#[test]
fn valid_explicit_outcomes_retain_the_existing_probe_contract() {
    for (body, expected) in [
        (
            json!({"ok":false,"error_code":400,"description":"Bad Request: STICKERSET_INVALID"}),
            (SetState::Missing, None),
        ),
        (
            json!({"ok":false,"error_code":401,"description":"Unauthorized"}),
            (SetState::Unknown, None),
        ),
        (
            json!({"ok":true,"result":{"stickers":[]}}),
            (SetState::Exists, Some(json!({"stickers":[]}))),
        ),
    ] {
        let bot = http::Bot::new(move |request| {
            assert_eq!(request.method, "getStickerSet");
            http::payload(body.clone())
        });
        assert_eq!(bot.telegram().probe(SET), expected);
        assert_eq!(bot.methods(), ["getStickerSet"]);
    }
}
