//! Original NativeTelegramTransport assertions at the public typed transport seam.
//! Baseline 1a36a968; tests/test_native_telegram.py SHA256
//! 487b2da9cfbd69fbba405aa85f96a896abec579e86344e49f4ecf27cda266786.
use _native::telegram::{SetState, Telegram};
use _native::{logging::RunLog, tooling_environment, tooling_process};
use serde_json::json;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::time::Duration;

const CHILD: &str = "NUMERA_TELEGRAM_TRANSPORT_TEST_CHILD";
const TOKEN: &str = "test-only-token";

#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/mutation_proof.rs"]
mod mutation_proof;
#[path = "support/telegram_binding_cases.rs"]
mod telegram_binding_cases;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

fn isolated(case: &str, test: impl FnOnce()) {
    if let Ok(selected) = std::env::var(CHILD) {
        assert_eq!(selected, case, "wrong isolated transport case selected");
        assert_eq!(
            std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
            Ok("1")
        );
        for key in [
            "TELEGRAM_BOT_TOKEN",
            "GENERAL_BOT_TOKEN",
            "COIN_BOT_TOKEN",
            "PACK_OWNER_USER_ID",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ] {
            assert!(std::env::var_os(key).is_none(), "inherited {key}");
        }
        println!("transport case started: {case}");
        test();
        println!("transport case completed: {case}");
        return;
    }
    let scratch = Root::new("telegram transport");
    let outcome = (|| -> Result<(), String> {
        let mut environment = BTreeMap::new();
        for key in [
            "PATH",
            "SystemRoot",
            "SYSTEMROOT",
            "WINDIR",
            "LD_LIBRARY_PATH",
            "DYLD_LIBRARY_PATH",
            "DYLD_FALLBACK_LIBRARY_PATH",
            "PYTHONHOME",
        ] {
            if let Ok(value) = std::env::var(key) {
                environment.insert(key.into(), value);
            }
        }
        environment.insert(CHILD.into(), case.into());
        environment.insert("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into());
        let environment = tooling_environment::environment(&scratch.0, &environment, false)?;
        let argv = [
            std::env::current_exe()
                .map_err(|e| e.to_string())?
                .into_os_string(),
            OsString::from("--exact"),
            case.into(),
            "--nocapture".into(),
            "--test-threads=1".into(),
        ];
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let log = RunLog::open(&scratch.0, "telegram-transport-test", vec![TOKEN.into()]);
        if log.path.is_none() {
            return Err("Transport test file logging could not initialize".into());
        }
        let captured = runtime.block_on(tooling_process::capture(
            &scratch.0,
            &argv,
            &environment,
            Duration::from_secs(12),
            Duration::from_secs(5),
            &log,
        ));
        log.finish(captured.as_ref().map_or(1, |result| result.code));
        let actual = captured?;
        if actual.code != 0
            || !actual
                .stdout
                .contains(&format!("transport case started: {case}"))
            || !actual
                .stdout
                .contains(&format!("transport case completed: {case}"))
        {
            return Err(format!(
                "Isolated transport case failed (exit={}):\n{}\n{}",
                actual.code, actual.stdout, actual.stderr
            ));
        }
        Ok(())
    })();
    assert!(outcome.is_ok(), "{}", outcome.unwrap_err());
    scratch.clean().unwrap();
}

#[test]
fn test_native_transport_cannot_bypass_suite_network_guard() {
    isolated(
        "test_native_transport_cannot_bypass_suite_network_guard",
        || {
            for base in [
                "https://api.telegram.org",
                "http://localhost",
                "http://example.com",
            ] {
                match Telegram::new(TOKEN.into(), base.into()) {
                    Err(error) => {
                        assert!(error.to_string().contains("numeric loopback"), "{error}")
                    }
                    Ok(_) => panic!("transport accepted a non-numeric-loopback base"),
                }
            }
        },
    );
}

#[test]
fn test_probe_keeps_exists_missing_unknown_and_sends_form() {
    isolated(
        "test_probe_keeps_exists_missing_unknown_and_sends_form",
        || {
            let mut server = bot_api::Server::new(vec![
                bot_api::Reply::new(
                    "getStickerSet",
                    &[("name", "exists")],
                    json!({"ok":true,"result":{"stickers":[],"name":"exists"}}),
                ),
                bot_api::Reply::new(
                    "getStickerSet",
                    &[("name", "missing")],
                    json!({"ok":false,"description":"STICKERSET_INVALID"}),
                ),
                bot_api::Reply::new("getStickerSet", &[("name", "unknown")], json!([])),
            ]);
            let native = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
            for (name, state) in [
                ("exists", SetState::Exists),
                ("missing", SetState::Missing),
                ("unknown", SetState::Unknown),
            ] {
                let (actual, payload) = native.probe(name);
                assert_eq!(actual, state, "probe {name}");
                if name != "exists" {
                    assert!(payload.is_none(), "probe {name} returned a payload");
                }
            }
            let observed = server.finish().unwrap();
            assert_eq!(observed.len(), 3);
            for (request, name) in observed.iter().zip(["exists", "missing", "unknown"]) {
                assert_eq!(request.method, "getStickerSet");
                assert_eq!(request.form, BTreeMap::from([("name".into(), name.into())]));
            }
        },
    );
}

#[test]
fn original_non_object_bodies_are_retried_through_the_actual_wire_parser() {
    isolated(
        "original_non_object_bodies_are_retried_through_the_actual_wire_parser",
        || {
            let replies = [json!([]), json!("ok"), json!(42)]
                .into_iter()
                .flat_map(|body| {
                    (0..2).map(move |_| bot_api::Reply::new("getMe", &[], body.clone()))
                })
                .collect();
            let mut server = bot_api::Server::new(replies);
            let telegram = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
            for body in ["list", "string", "integer"] {
                println!("malformed transport body started: {body}");
                let mut delays = Vec::new();
                let error = telegram
                    .call_with_wait(
                        "getMe",
                        &BTreeMap::new(),
                        &[],
                        2,
                        None::<fn() -> _native::telegram::Result<Option<bool>>>,
                        |delay| delays.push(delay),
                    )
                    .unwrap_err();
                assert_eq!(delays, [Duration::from_secs(3)]);
                assert!(matches!(error, _native::telegram::ApiError::Transport(_)));
                assert_eq!(error.to_string(), "getMe failed after 2 attempts");
                println!("malformed transport body refused: {body}");
            }
            let requests = server.finish().unwrap();
            assert_eq!(requests.len(), 6);
            assert!(
                requests
                    .iter()
                    .all(|r| r.method == "getMe" && r.form.is_empty())
            );
        },
    );
}

#[test]
fn original_non_object_add_response_asks_for_proof_without_resending() {
    isolated(
        "original_non_object_add_response_asks_for_proof_without_resending",
        || {
            let mut server =
                bot_api::Server::new(vec![bot_api::Reply::new("addStickerToSet", &[], json!([]))]);
            let telegram = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
            let mut proofs = 0;
            let result = telegram
                .call(
                    "addStickerToSet",
                    &BTreeMap::new(),
                    &[],
                    2,
                    Some(|| {
                        proofs += 1;
                        Ok(Some(true))
                    }),
                )
                .unwrap();
            assert_eq!(result, json!({"verified_applied":true}));
            assert_eq!(proofs, 1);
            let requests = server.finish().unwrap();
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0].method, "addStickerToSet");
            assert!(requests[0].form.is_empty());
        },
    );
}

#[test]
fn original_added_identity_unknowns_never_certify_or_resend_an_upload() {
    isolated(
        "original_added_identity_unknowns_never_certify_or_resend_an_upload",
        || mutation_proof::run("add-unknown-a"),
    );
}

#[test]
fn original_added_content_proof_and_clean_add_keep_their_distinct_readbacks() {
    isolated(
        "original_added_content_proof_and_clean_add_keep_their_distinct_readbacks",
        || mutation_proof::run("add-unknown-b"),
    );
}

#[test]
fn original_definitely_unapplied_adds_allow_one_retry_without_claiming_success() {
    isolated(
        "original_definitely_unapplied_adds_allow_one_retry_without_claiming_success",
        || mutation_proof::run("add-negative"),
    );
}

#[test]
fn original_create_adoption_requires_one_proven_image() {
    isolated("original_create_adoption_requires_one_proven_image", || {
        mutation_proof::run("create-positive")
    });
}

#[test]
fn original_foreign_or_absent_create_can_retry_but_never_invent_acceptance() {
    isolated(
        "original_foreign_or_absent_create_can_retry_but_never_invent_acceptance",
        || mutation_proof::run("create-negative"),
    );
}

#[test]
fn original_post_add_size_requires_our_content_and_preserves_missing_as_unknown() {
    isolated(
        "original_post_add_size_requires_our_content_and_preserves_missing_as_unknown",
        || mutation_proof::run("size"),
    );
}

#[test]
fn original_live_count_keeps_exists_missing_and_unknown_distinct_at_the_native_binding() {
    use pyo3::prelude::*;
    use serde_json::Value;
    isolated(
        "original_live_count_keeps_exists_missing_and_unknown_distinct_at_the_native_binding",
        || {
            let mut replies = Vec::new();
            for (name, payload) in [
                ("exists", json!({"ok":true,"result":{"stickers":[{},{}]}})),
                (
                    "missing",
                    json!({"ok":false,"description":"STICKERSET_INVALID"}),
                ),
                ("unknown", json!([])),
            ] {
                for _ in 0..2 {
                    replies.push(bot_api::Reply::new(
                        "getStickerSet",
                        &[("name", name)],
                        payload.clone(),
                    ));
                }
            }
            let mut server = bot_api::Server::new(replies);
            Python::initialize();
            Python::attach(|py| -> PyResult<()> {
                let native = py
                    .get_type::<_native::telegram_bindings::NativeTelegram>()
                    .call1((TOKEN, &server.base))?;
                for (name, count) in [("exists", Some(2)), ("missing", Some(0)), ("unknown", None)]
                {
                    let probe: String = native.call_method1("probe", (name,))?.extract()?;
                    let probe: Value = serde_json::from_str(&probe).unwrap();
                    assert_eq!(probe[0], name);
                    if name != "exists" {
                        assert!(probe[1].is_null());
                    }
                    let raw: String = native.call_method1("live_count", (name,))?.extract()?;
                    let envelope: Value = serde_json::from_str(&raw).unwrap();
                    if let Some(count) = count {
                        assert_eq!(envelope, json!({"ok":true,"result":count}));
                    } else {
                        // The binding envelope adapts the retained wrapper's LiveStateUnknown.
                        assert_eq!(envelope["ok"], false);
                        assert_eq!(envelope["kind"], "ambiguous");
                        assert!(envelope["error"].as_str().unwrap().contains("unknown"));
                        assert!(envelope.get("result").is_none());
                    }
                }
                Ok(())
            })
            .unwrap();
            let requests = server.finish().unwrap();
            assert_eq!(requests.len(), 6);
            for (actual, name) in requests.iter().zip([
                "exists", "exists", "missing", "missing", "unknown", "unknown",
            ]) {
                assert_eq!(actual.method, "getStickerSet");
                assert_eq!(actual.form, BTreeMap::from([("name".into(), name.into())]));
            }
        },
    );
}

#[test]
fn registered_foreign_tail_retry_returns_only_the_proven_live_count() {
    isolated(
        "registered_foreign_tail_retry_returns_only_the_proven_live_count",
        || telegram_binding_cases::check("foreign"),
    );
}

#[test]
fn registered_unknown_add_refuses_without_comparison_or_second_mutation() {
    isolated(
        "registered_unknown_add_refuses_without_comparison_or_second_mutation",
        || telegram_binding_cases::check("unknown"),
    );
}

#[test]
fn registered_multi_item_create_refuses_without_comparison_or_second_mutation() {
    isolated(
        "registered_multi_item_create_refuses_without_comparison_or_second_mutation",
        || telegram_binding_cases::check("wrong-create"),
    );
}

#[test]
fn registered_telegram_preserves_bytes_results_flood_fields_and_callback_exception_identity() {
    isolated(
        "registered_telegram_preserves_bytes_results_flood_fields_and_callback_exception_identity",
        || telegram_binding_cases::check("public"),
    );
}

#[test]
fn original_creation_repainting_is_true_when_requested_and_absent_by_default() {
    isolated(
        "original_creation_repainting_is_true_when_requested_and_absent_by_default",
        || mutation_proof::run("repaint"),
    );
}
