//! Original transport policies at the shared post/sleep seam, without network calls or real waiting.
use crate::telegram::{ApiError, Telegram, usable_fuids};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::time::Duration;

#[test]
fn original_token_masking_and_usable_identity_sets_keep_unknown_separate() {
    let token = "1234567890:AAxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    let telegram = Telegram::new(token.into(), "http://127.0.0.1:9".into()).unwrap();
    let safe = telegram.safe(&format!(
        "HTTPSConnectionPool: /bot{token}/addStickerToSet failed"
    ));
    assert!(!safe.contains(token));
    assert!(safe.contains("[REDACTED]"));
    assert_eq!(
        usable_fuids(&[json!({"file_unique_id":"a"}), json!({"file_unique_id":"b"})]),
        Some(["a".into(), "b".into()].into())
    );
    assert_eq!(usable_fuids(&[]), Some(Default::default()));
    for members in [
        vec![json!({"file_unique_id":"a"}), json!({"file_unique_id":""})],
        vec![json!({"i":0}), json!({"i":1})],
        vec![json!({"file_unique_id":"same"}); 2],
    ] {
        assert_eq!(usable_fuids(&members), None);
    }
}

#[test]
fn original_missing_lookup_refuses_at_once_but_create_retries_only_before_the_last_attempt() {
    let telegram = Telegram::new("test-only-token".into(), "http://127.0.0.1:9".into()).unwrap();
    for method in ["getStickerSet", "createNewStickerSet"] {
        let mut posts = 0;
        let mut waits = Vec::new();
        let result = telegram.call_with_effects(
            method,
            3,
            None::<fn() -> crate::telegram::Result<Option<bool>>>,
            || {
                posts += 1;
                Ok(json!({"ok":false,"description":"STICKERSET_INVALID"}))
            },
            |delay| waits.push(delay),
        );
        assert!(matches!(result, Err(ApiError::Rejected(_))));
        if method == "getStickerSet" {
            assert_eq!(posts, 1);
            assert!(waits.is_empty());
        } else {
            assert_eq!(posts, 3);
            assert_eq!(waits, [Duration::from_secs(30), Duration::from_secs(60)]);
            assert!(waits.iter().sum::<Duration>() <= Duration::from_secs(300));
        }
    }
}

#[test]
fn transport_retries_require_explicit_proof_and_never_sleep_after_the_final_read_attempt() {
    let token = "test-only-token";
    let telegram = Telegram::new(token.into(), "http://127.0.0.1:9".into()).unwrap();
    for proof in [Some(true), Some(false), None] {
        let mut posts = 0;
        let mut proofs = 0;
        let mut waits = Vec::new();
        let result = telegram.call_with_effects(
            "addStickerToSet",
            2,
            Some(|| {
                proofs += 1;
                Ok(proof)
            }),
            || {
                posts += 1;
                if posts == 1 {
                    Err(ApiError::Transport("fixture connection reset".into()))
                } else {
                    Ok(json!({"ok":true,"result":true}))
                }
            },
            |delay| waits.push(delay),
        );
        assert_eq!(proofs, 1);
        match proof {
            Some(true) => {
                assert_eq!(result.unwrap(), json!({"verified_applied":true}));
                assert_eq!(posts, 1);
                assert_eq!(waits, [Duration::from_secs(2)]);
            }
            Some(false) => {
                assert_eq!(result.unwrap(), json!(true));
                assert_eq!(posts, 2);
                assert_eq!(waits, [Duration::from_secs(2), Duration::from_secs(3)]);
            }
            None => {
                assert!(matches!(result, Err(ApiError::Ambiguous(_))));
                assert_eq!(posts, 1);
                assert_eq!(waits, [Duration::from_secs(2)]);
            }
        }
    }
    for method in ["getMe", "addStickerToSet", "createNewStickerSet"] {
        let mut posts = 0;
        let mut waits = Vec::new();
        let result = telegram.call_with_effects(
            method,
            2,
            None::<fn() -> crate::telegram::Result<Option<bool>>>,
            || {
                posts += 1;
                Err(ApiError::Transport("fixture connection reset".into()))
            },
            |delay| waits.push(delay),
        );
        if method == "getMe" {
            assert!(matches!(result, Err(ApiError::Transport(_))));
            assert_eq!(posts, 2);
            assert_eq!(waits, [Duration::from_secs(3)]);
        } else {
            assert!(matches!(result, Err(ApiError::Ambiguous(_))));
            assert_eq!(posts, 1);
            assert!(waits.is_empty());
        }
    }
}

#[test]
fn original_waits_and_verified_retries_reach_the_same_redacted_run_file() {
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let parent = crate::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
    let root = parent.join(format!(
        "telegram original diagnostics {}",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    let token = "1234567890:AAxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    // The transport itself must redact even when its token was not registered on this run.
    let log = crate::logging::RunLog::open(&root, "telegram-diagnostics", vec![]);
    let path = log
        .path
        .clone()
        .expect("fixture file logging must initialize");
    let telegram = Telegram::new(token.into(), "http://127.0.0.1:9".into())
        .unwrap()
        .with_log(&log);
    for method in ["getMe", "createNewStickerSet", "addStickerToSet"] {
        let mut posts = 0;
        let mut waits = Vec::new();
        let result = telegram.call_with_effects(
            method,
            2,
            (method == "addStickerToSet").then_some(|| Ok(Some(true))),
            || {
                posts += 1;
                if method == "getMe" {
                    Err(ApiError::Transport(format!(
                        "conn to /bot{token}/getMe reset"
                    )))
                } else if posts == 1 {
                    if method == "createNewStickerSet" {
                        Ok(json!({"ok":false,"description":"STICKERSET_INVALID"}))
                    } else {
                        Err(ApiError::Transport("fixture reset".into()))
                    }
                } else {
                    Ok(json!({"ok":true,"result":true}))
                }
            },
            |delay| waits.push(delay),
        );
        assert_eq!(posts, if method == "addStickerToSet" { 1 } else { 2 });
        if method == "getMe" {
            let error = result.unwrap_err();
            assert!(matches!(error, ApiError::Transport(_)));
            assert_eq!(error.to_string(), "getMe failed after 2 attempts");
        } else {
            assert_eq!(
                result.unwrap(),
                if method == "addStickerToSet" {
                    json!({"verified_applied":true})
                } else {
                    json!(true)
                }
            );
        }
        assert_eq!(
            waits,
            [Duration::from_secs(match method {
                "getMe" => 3,
                "createNewStickerSet" => 30,
                _ => 2,
            })]
        );
    }
    let mut posts = 0;
    let mut waits = Vec::new();
    assert_eq!(telegram.call_with_effects(
        "addStickerToSet", 2, None::<fn() -> crate::telegram::Result<Option<bool>>>,
        || { posts += 1; Ok(if posts == 1 {
            json!({"ok":false,"description":"Too Many Requests: retry after 7","parameters":{"retry_after":7}})
        } else {json!({"ok":true,"result":{"done":true}})}) },
        |delay| waits.push(delay),
    ).unwrap(), json!({"done":true}));
    assert_eq!(posts, 2);
    assert_eq!(waits, [Duration::from_secs(8)]);
    assert_eq!(std::fs::read_dir(root.join("logs")).unwrap().count(), 1);
    drop(telegram);
    log.finish(0);
    drop(log);
    let text = std::fs::read_to_string(path).unwrap();
    assert!(!text.contains(token));
    for message in [
        "net retry 1/2 (getMe)",
        "[REDACTED]",
        "wait 3s",
        "stickerset_invalid; name not released yet, wait 30s (createNewStickerSet)",
        "addStickerToSet: network error but the change is verified live; not re-sending",
        "flood wait 7s (addStickerToSet)",
        "warnings=4 errors=0 critical=0",
    ] {
        assert!(text.contains(message), "missing diagnostic: {message}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn original_flood_ceiling_refuses_long_wait_and_honours_one_short_wait() {
    for (seconds, ceiling, expected_waits, expected_posts) in [
        (900, Some(300), vec![], 1),
        (20, Some(300), vec![Duration::from_secs(21)], 2),
        (20, None, vec![Duration::from_secs(21)], 2),
    ] {
        let mut telegram =
            Telegram::new("test-only-token".into(), "http://127.0.0.1:9".into()).unwrap();
        telegram.max_flood_wait = ceiling;
        let mut answers = VecDeque::from([
            json!({"ok":false,"error_code":429,
                "description":format!("Too Many Requests: retry after {seconds}"),
                "parameters":{"retry_after":seconds}}),
            json!({"ok":true,"result":true}),
        ]);
        let mut posts = 0;
        let mut waits = Vec::new();
        let result = telegram.call_with_effects(
            "deleteStickerFromSet",
            5,
            None::<fn() -> crate::telegram::Result<Option<bool>>>,
            || {
                posts += 1;
                Ok(answers
                    .pop_front()
                    .expect("unexpected retry after fixed source responses"))
            },
            |delay| waits.push(delay),
        );
        if seconds == 900 {
            match result {
                Err(ApiError::FloodWait { method, seconds }) => {
                    assert_eq!(method, "deleteStickerFromSet");
                    assert_eq!(seconds, 900);
                }
                other => panic!("long wait must refuse without sleep: {other:?}"),
            }
            assert_eq!(answers.len(), 1, "refusal must not consume the next reply");
        } else {
            assert_eq!(result.unwrap(), Value::Bool(true));
            assert!(answers.is_empty());
        }
        assert_eq!(posts, expected_posts);
        assert_eq!(waits, expected_waits);
    }
}
