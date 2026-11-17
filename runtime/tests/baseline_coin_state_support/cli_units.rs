//! A fatal final-upload fault traverses the actual build CLI and its saved-cursor exit gate.
use crate as rt;
use crate::baseline_coin_state_support::*;
use serde_json::json;
use std::sync::{Arc, Mutex};
// tests.test_rebuild_dedup_plan.BuildFinishesWhenThePlanIsWalked.test_a_crash_on_the_last_upload_is_partial_and_keeps_its_marker
#[test]
fn test_a_crash_on_the_last_upload_is_partial_and_keeps_its_marker() {
    let f = Rebuild::new(&["seed", "aaa"]);
    f.state(json!({"sets":[{"index":1,"name":"s1","title":"T 1"}],"order":["seed"],"cursor":1}));
    let server = Server::new(vec![me(), listing("s1", 1), listing("s1", 1)]);
    let config = crate::baseline_coin_provider_support::config(
        &f.root.0,
        &[
            ("COIN_PACK_TITLE", ORIGINAL_TITLE),
            ("TELEGRAM_BOT_TOKEN", TOKEN),
            ("PACK_OWNER_USER_ID", "42"),
            ("BRAND_LOGO_BOTS", ""),
            ("NUMERA_TEST_API_BASE", &server.base),
        ],
    );
    let attempts = Arc::new(Mutex::new(vec![]));
    let seen = attempts.clone();
    let state = f.files.directory.join("rebuild_dedup_state.json");
    let expected_base = server.base.clone();
    let log = f.root.log();
    let code = super::run_with_telegram_factory(
        &config,
        &ids(&[
            "build",
            "--coin-dir",
            f.files.directory.to_str().unwrap(),
            "--emoji-dir",
            f.files.images.to_str().unwrap(),
        ]),
        &log,
        move |token, base| {
            assert_eq!(token, TOKEN);
            assert_eq!(base, expected_base);
            Ok(
                rt::telegram::Telegram::new(token, base)?.with_call_fault(Arc::new(
                    move |method, data, uploads| {
                        if method != "addStickerToSet" {
                            return None;
                        }
                        let checkpoint = doc(&state);
                        assert_eq!(checkpoint["cursor"], 2);
                        assert_eq!(checkpoint["in_flight"]["key"], "aaa");
                        assert_eq!(data["name"], "s1");
                        assert_eq!(data["user_id"], "42");
                        assert_eq!(uploads.len(), 1);
                        assert_eq!(uploads[0].bytes, noise("aaa"));
                        seen.lock().unwrap().push("aaa");
                        Some(rt::telegram::ApiError::Invalid(
                            "the process died mid-request".into(),
                        ))
                    },
                )),
            )
        },
    )
    .unwrap();
    assert_eq!(code, 3);
    assert_eq!(*attempts.lock().unwrap(), vec!["aaa"]);
    assert_eq!(f.saved()["cursor"], 2);
    assert_eq!(f.saved()["in_flight"]["key"], "aaa");
    assert_eq!(f.saved()["order"], json!(["seed"]));
    let requests = server.finish();
    assert!(
        requests
            .iter()
            .all(|r| r.method != "createNewStickerSet" && r.method != "sendMessage")
    );
    assert!(
        std::fs::read_to_string(log.path.unwrap())
            .unwrap()
            .contains("the process died mid-request")
    );
    f.released();
}

fn rejection(permanent: bool) -> (Rebuild, u8, Vec<String>) {
    let f = Rebuild::new(&["seed", "aaa", "bbb"]);
    f.state(json!({"sets":[{"index":1,"name":"s1","title":"T 1"}],"order":["seed"],"cursor":1}));
    let mut script = vec![me(), listing("s1", 1), listing("s1", 1)];
    if permanent {
        script.extend([listing("s1", 1), notify()]);
    }
    let server = Server::new(script);
    let config = crate::baseline_coin_provider_support::config(
        &f.root.0,
        &[
            ("COIN_PACK_TITLE", ORIGINAL_TITLE),
            ("TELEGRAM_BOT_TOKEN", TOKEN),
            ("PACK_OWNER_USER_ID", "42"),
            ("BRAND_LOGO_BOTS", ""),
            ("NUMERA_TEST_API_BASE", &server.base),
        ],
    );
    let attempts = Arc::new(Mutex::new(vec![]));
    let seen = attempts.clone();
    let state = f.files.directory.join("rebuild_dedup_state.json");
    let code = super::run_with_telegram_factory(
        &config,
        &ids(&[
            "build",
            "--coin-dir",
            f.files.directory.to_str().unwrap(),
            "--emoji-dir",
            f.files.images.to_str().unwrap(),
        ]),
        &f.root.log(),
        move |token, base| {
            Ok(
                rt::telegram::Telegram::new(token, base)?.with_call_fault(Arc::new(
                    move |method, _, uploads| {
                        if method != "addStickerToSet" {
                            return None;
                        }
                        let checkpoint = doc(&state);
                        let key = checkpoint["in_flight"]["key"].as_str().unwrap();
                        assert_eq!(checkpoint["in_flight"]["expected_before"], 1);
                        assert_eq!(uploads.len(), 1);
                        assert_eq!(uploads[0].bytes, noise(key));
                        seen.lock().unwrap().push(key.to_owned());
                        Some(rt::telegram::ApiError::Rejected(
                            if permanent {
                                "addStickerToSet failed: Bad Request: STICKER_PNG_NOPNG"
                            } else {
                                "addStickerToSet failed after 5 attempts"
                            }
                            .into(),
                        ))
                    },
                )),
            )
        },
    )
    .unwrap();
    let requests = server.finish();
    assert!(
        requests
            .iter()
            .all(|r| r.method != "addStickerToSet" && r.method != "createNewStickerSet")
    );
    let attempts = attempts.lock().unwrap().clone();
    f.released();
    (f, code, attempts)
}
// tests.test_rebuild_dedup_plan.RetryableFailureKeepsThePlanPosition.test_a_transport_failure_leaves_the_cursor_on_the_item
#[test]
fn test_a_transport_failure_leaves_the_cursor_on_the_item() {
    let (f, code, attempts) = rejection(false);
    assert_eq!(
        f.saved()["cursor"],
        1,
        "not-applied failure must retain the plan position"
    );
    assert_eq!(f.saved()["order"], json!(["seed"]));
    assert_eq!(attempts, vec!["aaa"]);
    assert_eq!(code, 3);
}
// tests.test_rebuild_dedup_plan.RetryableFailureKeepsThePlanPosition.test_a_media_rejection_is_still_skipped_for_good
#[test]
fn test_a_media_rejection_is_still_skipped_for_good() {
    let (f, code, attempts) = rejection(true);
    assert_eq!(f.saved()["cursor"], 3);
    assert_eq!(f.saved()["order"], json!(["seed"]));
    assert_eq!(attempts, vec!["aaa", "bbb"]);
    assert_eq!(
        code, 0,
        "normal return after permanent skips is a completed CLI walk"
    );
}

fn ambiguous(creating: bool) -> (Rebuild, u8, Vec<String>) {
    let f = Rebuild::new(&["seed", "aaa", "bbb"]);
    if !creating {
        f.state(
            json!({"sets":[{"index":1,"name":"s1","title":"T 1"}],"order":["seed"],"cursor":1}),
        );
    }
    let script = if creating {
        vec![me()]
    } else {
        vec![me(), listing("s1", 1), listing("s1", 1)]
    };
    let server = Server::new(script);
    let config = crate::baseline_coin_provider_support::config(
        &f.root.0,
        &[
            ("COIN_PACK_TITLE", ORIGINAL_TITLE),
            ("TELEGRAM_BOT_TOKEN", TOKEN),
            ("PACK_OWNER_USER_ID", "42"),
            ("BRAND_LOGO_BOTS", ""),
            ("NUMERA_TEST_API_BASE", &server.base),
        ],
    );
    let attempts = Arc::new(Mutex::new(vec![]));
    let seen = attempts.clone();
    let state = f.files.directory.join("rebuild_dedup_state.json");
    let code = super::run_with_telegram_factory(
        &config,
        &ids(&[
            "build",
            "--coin-dir",
            f.files.directory.to_str().unwrap(),
            "--emoji-dir",
            f.files.images.to_str().unwrap(),
        ]),
        &f.root.log(),
        move |token, base| {
            Ok(
                rt::telegram::Telegram::new(token, base)?.with_call_fault(Arc::new(
                    move |method, _, uploads| {
                        if method
                            != if creating {
                                "createNewStickerSet"
                            } else {
                                "addStickerToSet"
                            }
                        {
                            return None;
                        }
                        let checkpoint = doc(&state);
                        let key = checkpoint["in_flight"]["key"].as_str().unwrap();
                        assert_eq!(uploads.len(), 1);
                        assert_eq!(uploads[0].bytes, noise(key));
                        seen.lock().unwrap().push(key.to_owned());
                        Some(rt::telegram::ApiError::Ambiguous(format!(
                            "{method}: unknown"
                        )))
                    },
                )),
            )
        },
    )
    .unwrap();
    assert!(server.finish().iter().all(|r| r.method != "sendMessage"));
    let attempts = attempts.lock().unwrap().clone();
    f.released();
    (f, code, attempts)
}
// tests.test_rebuild_dedup_resume.AmbiguousUploadStopsTheRun.test_the_second_entry_is_never_uploaded
#[test]
fn test_the_second_entry_is_never_uploaded() {
    let (_, code, attempts) = ambiguous(false);
    assert_eq!(attempts, vec!["aaa"]);
    assert_eq!(code, 3);
}
// tests.test_rebuild_dedup_resume.AmbiguousUploadStopsTheRun.test_the_unresolved_mutation_keeps_its_identity
#[test]
fn test_the_unresolved_mutation_keeps_its_identity() {
    let (f, _, _) = ambiguous(false);
    let marker = f.saved()["in_flight"].clone();
    assert_eq!(marker["key"], "aaa");
    assert_eq!(marker["operation"], "add");
    assert_eq!(marker["set_name"], "s1");
    assert_eq!(marker["set_index"], 1);
    assert_eq!(marker["expected_before"], 1);
    assert_eq!(marker["phase"], "upload");
}
// tests.test_rebuild_dedup_resume.AmbiguousUploadStopsTheRun.test_an_ambiguous_create_records_the_set_it_may_have_made
#[test]
fn test_an_ambiguous_create_records_the_set_it_may_have_made() {
    let (f, code, attempts) = ambiguous(true);
    assert_eq!(attempts.len(), 1);
    assert_eq!(code, 3);
    let marker = f.saved()["in_flight"].clone();
    assert_eq!(marker["operation"], "create");
    assert_eq!(marker["set_name"], CREATED);
    assert_eq!(marker["set_index"], 1);
    assert_eq!(f.saved()["sets"], json!([]));
}
// tests.test_rebuild_dedup_locks.ConcurrentRunsAreLockedOut.test_the_lock_is_released_after_a_stop
#[test]
fn test_the_lock_is_released_after_a_stop() {
    let f = Rebuild::new(&["aaa"]);
    let server = Server::new(vec![me()]);
    let family = rt::locks::pack_family_path(&f.root.0, &f.files.base);
    let held = Arc::new(Mutex::new(vec![]));
    let seen = held.clone();
    let config = crate::baseline_coin_provider_support::config(
        &f.root.0,
        &[
            ("COIN_PACK_TITLE", ORIGINAL_TITLE),
            ("TELEGRAM_BOT_TOKEN", TOKEN),
            ("PACK_OWNER_USER_ID", "42"),
            ("BRAND_LOGO_BOTS", ""),
            ("NUMERA_TEST_API_BASE", &server.base),
        ],
    );
    let code = super::run_with_telegram_factory(
        &config,
        &ids(&[
            "build",
            "--coin-dir",
            f.files.directory.to_str().unwrap(),
            "--emoji-dir",
            f.files.images.to_str().unwrap(),
        ]),
        &f.root.log(),
        move |token, base| {
            Ok(
                rt::telegram::Telegram::new(token, base)?.with_call_fault(Arc::new(
                    move |method, _, uploads| {
                        if method != "createNewStickerSet" {
                            return None;
                        }
                        seen.lock().unwrap().push(
                            rt::locks::Lease::acquire(
                                &family,
                                "actual competitor at stopped mutation",
                            )
                            .is_err(),
                        );
                        assert_eq!(uploads[0].bytes, noise("aaa"));
                        Some(rt::telegram::ApiError::Ambiguous(
                            "createNewStickerSet: unknown".into(),
                        ))
                    },
                )),
            )
        },
    )
    .unwrap();
    assert_eq!(code, 3);
    assert_eq!(*held.lock().unwrap(), vec![true]);
    server.finish();
    f.released();
}
