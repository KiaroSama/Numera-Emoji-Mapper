//! Original complete provider CLI outcomes; only the real HTTP client's retry clock is injected.
use crate as rt;
use crate::baseline_coin_provider_support as support;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use support::*;
fn child(kind: &str) -> bool {
    let Some(root) = std::env::var_os("NUMERA_COIN_PROVIDER_CLI_CHILD") else {
        return false;
    };
    let root = std::path::PathBuf::from(root);
    assert_eq!(
        std::env::var("NUMERA_COIN_PROVIDER_CLI_KIND").unwrap(),
        kind
    );
    let config = config(
        &root,
        &[
            (
                "NUMERA_TEST_API_BASE",
                &std::env::var("NUMERA_TEST_API_BASE").unwrap(),
            ),
            ("TELEGRAM_BOT_TOKEN", TOKEN),
            ("PACK_OWNER_USER_ID", "1"),
        ],
    );
    let waits = Arc::new(Mutex::new(vec![]));
    let observed = waits.clone();
    let code = super::run_with_http_factory(
        &config,
        "paprika",
        &ids(&["--coin-dir", root.to_str().unwrap()]),
        &rt::logging::RunLog::open(&root, "provider-cli-original", vec![TOKEN.into()]),
        move |timeout| {
            Ok(rt::coin_http::ProviderHttp::new(timeout)?
                .with_retry_clock(Arc::new(move |wait| observed.lock().unwrap().push(wait))))
        },
    )
    .unwrap();
    write(
        &root.join("outcome.json"),
        &json!({"code":code,"waits":waits.lock().unwrap().iter().map(Duration::as_secs_f64).collect::<Vec<_>>()}),
    );
    true
}
fn invoke(test: &str, kind: &str, root: &Root, server: &Server) {
    let mut env = std::collections::BTreeMap::new();
    for key in [
        "NUMERA_RUNTIME_TEST_ADMISSION",
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "LD_LIBRARY_PATH",
    ] {
        if let Ok(v) = std::env::var(key) {
            env.insert(key.into(), v);
        }
    }
    env.extend([
        (
            "NUMERA_COIN_PROVIDER_CLI_CHILD".into(),
            root.0.to_string_lossy().into_owned(),
        ),
        ("NUMERA_COIN_PROVIDER_CLI_KIND".into(), kind.into()),
        ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
        ("NUMERA_TEST_API_BASE".into(), server.base.clone()),
        ("NUMERA_TEST_PROVIDER_BASE".into(), server.base.clone()),
    ]);
    let exe = std::env::current_exe().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = runtime
        .block_on(rt::tooling_process::capture(
            &root.0,
            &[
                exe.into_os_string(),
                "--exact".into(),
                format!("coin_provider_cli::baseline_provider_cli_tests::{test}").into(),
                "--nocapture".into(),
            ],
            &env,
            Duration::from_secs(20),
            Duration::from_secs(12),
            &root.log(),
        ))
        .unwrap();
    assert_eq!(result.code, 0, "{}\n{}", result.stdout, result.stderr);
    for text in [&result.stdout, &result.stderr] {
        assert!(!text.contains(TOKEN));
    }
    assert!(!rt::process_owner::cleanup_pending(&root.0));
}
fn files(root: &Root, unmatched: bool) {
    std::fs::write(
        root.0.join("currency-emoji-inventory.md"),
        "## AAA - Alpha Coin\n  ticker: aaa\n  premium-id:\n",
    )
    .unwrap();
    write(&root.0.join("ticker_to_id.json"), &json!({"btc":"c-btc"}));
    write(
        &root.0.join("paprika_matches.json"),
        &if unmatched {
            json!({})
        } else {
            json!({"aaa":{"id":"alpha-coin","conf":"symbol","name":"Alpha Coin"}})
        },
    );
    write(
        &root.0.join("rebuild_dedup_state.json"),
        &json!({"sets":[{"index":1,"name":SET,"title":"T 1"}]}),
    );
}
// tests.test_coin_providers.AnOutageIsNotNoMatch.test_an_unreachable_provider_leaves_the_cache_alone_and_exits_partial
#[test]
fn test_an_unreachable_provider_leaves_the_cache_alone_and_exits_partial() {
    if child("outage") {
        return;
    }
    let root = Root::new();
    files(&root, true);
    let mut script = vec![];
    for query in ["Alpha+Coin", "aaa"] {
        for _ in 0..4 {
            script.push(json_reply(
                &format!("/v1/search?c=currencies&limit=10&q={query}"),
                500,
                Value::Null,
            ));
        }
    }
    let server = Server::new(script);
    invoke(
        "test_an_unreachable_provider_leaves_the_cache_alone_and_exits_partial",
        "outage",
        &root,
        &server,
    );
    assert_eq!(doc(&root.0.join("outcome.json"))["code"], 3);
    assert_eq!(
        doc(&root.0.join("paprika_matches.json")),
        json!({}),
        "outage must not cache no-match"
    );
    assert_eq!(
        doc(&root.0.join("outcome.json"))["waits"],
        json!([4.0, 8.0, 12.0, 4.0, 8.0, 12.0])
    );
    assert_eq!(server.finish().len(), 8);
}
// tests.test_coin_providers.CommandExitCodes.test_a_blank_provider_logo_exits_nonzero
#[test]
fn test_a_blank_provider_logo_exits_nonzero() {
    if child("blank") {
        return;
    }
    let root = Root::new();
    files(&root, false);
    let server = Server::new(vec![provider_reply(
        "/coin/alpha-coin/logo.png",
        200,
        &solid(64, [0, 0, 0, 0]),
    )]);
    invoke(
        "test_a_blank_provider_logo_exits_nonzero",
        "blank",
        &root,
        &server,
    );
    assert_eq!(doc(&root.0.join("outcome.json"))["code"], 4);
    let requests = server.finish();
    assert_eq!(requests.len(), 1, "blank logo must never reach Telegram");
    assert!(requests.iter().all(|r| r.method.starts_with("GET ")));
}
// tests.test_coin_providers.CommandExitCodes.test_a_failed_add_exits_nonzero
#[test]
fn test_a_failed_add_exits_nonzero() {
    if child("failed") {
        return;
    }
    let root = Root::new();
    files(&root, false);
    let before = vec![member(1), member(2)];
    let server = Server::new(vec![
        provider_reply("/coin/alpha-coin/logo.png", 200, &ramp(false)),
        me(),
        listing(&before),
        listing(&before),
        reply(
            "addStickerToSet",
            reject("addStickerToSet failed: STICKER_PNG_NOPNG"),
        ),
    ]);
    invoke("test_a_failed_add_exits_nonzero", "failed", &root, &server);
    assert_eq!(doc(&root.0.join("outcome.json"))["code"], 4);
    assert!(doc(&root.0.join("ticker_to_id.json")).get("aaa").is_none());
    assert_eq!(
        server
            .finish()
            .iter()
            .filter(|r| r.method == "addStickerToSet")
            .count(),
        1
    );
}
// tests.test_coin_providers.CommandExitCodes.test_a_clean_run_exits_zero_and_records_the_id
#[test]
fn test_a_clean_run_exits_zero_and_records_the_id() {
    if child("clean") {
        return;
    }
    let root = Root::new();
    files(&root, false);
    let before = vec![member(1), member(2)];
    let live = vec![member(1), member(2), member(3)];
    let mut script = vec![
        provider_reply("/coin/alpha-coin/logo.png", 200, &ramp(false)),
        me(),
        listing(&before),
        listing(&before),
        reply("addStickerToSet", ok(json!(true))),
        listing(&live),
    ];
    download(&mut script, "f3", &ramp(false));
    let server = Server::new(script);
    invoke(
        "test_a_clean_run_exits_zero_and_records_the_id",
        "clean",
        &root,
        &server,
    );
    assert_eq!(doc(&root.0.join("outcome.json"))["code"], 0);
    assert_eq!(doc(&root.0.join("ticker_to_id.json"))["aaa"], "c3");
    assert_eq!(
        server
            .finish()
            .iter()
            .filter(|r| r.method == "addStickerToSet")
            .count(),
        1
    );
}
