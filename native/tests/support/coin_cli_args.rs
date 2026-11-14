//! Original argument refusals and exhausted logo paging through copied commands and bounded HTTP.
use _native::{logging::RunLog, tooling_process};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::bot_api;

pub fn check(root: &Path, executable: &Path, runtime: &tokio::runtime::Runtime) {
    let data = root.join("argument-admission");
    std::fs::create_dir(&data).unwrap();
    for (name, bytes) in [
        ("ticker_to_id.json", b"{\"other\":\"preserved\"}".as_slice()),
        (
            "rebuild_dedup_state.json",
            b"{\"sets\":[],\"provider_added\":[]}".as_slice(),
        ),
        ("paprika_matches.json", b"{}".as_slice()),
        (
            "currency-emoji-inventory.md",
            b"## Other - Other Coin\nticker: other\npremium-id: preserved\n".as_slice(),
        ),
    ] {
        std::fs::write(data.join(name), bytes).unwrap();
    }
    let before = snapshot(&data);
    let input = root.join("argument-eof");
    std::fs::write(&input, b"").unwrap();
    let log = RunLog::open(
        root,
        "coin-argument-owner",
        vec!["synthetic-argument-key".into()],
    );
    for method in [
        "GET",
        "POST",
        "GET-wrong-method",
        "malformed-form",
        "malformed-json",
    ] {
        let mut observer = bot_api::Server::new(vec![]);
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let request = if method == "GET" {
            client.get(format!("{}/unknown/deep/path", observer.base))
        } else if method == "POST" {
            client.post(format!("{}/unknown/deep/path", observer.base))
        } else if method == "GET-wrong-method" {
            client.get(format!("{}/bottest-only-token/getMe", observer.base))
        } else if method == "malformed-form" {
            client
                .post(format!("{}/bottest-only-token/getMe", observer.base))
                .header("content-type", "application/json")
                .body("{}")
        } else {
            client
                .post(format!("{}/publish", observer.base))
                .header("content-type", "application/json")
                .body("{")
        };
        assert!(request.send().unwrap().status().is_client_error());
        let error = observer.finish().unwrap_err();
        assert!(
            if method.starts_with("malformed") {
                error.contains("rejected before observation")
            } else {
                error.contains("unexpected synthetic HTTP")
            },
            "{error}"
        );
    }
    let mut cases = Vec::new();
    for provider in ["paprika", "cmc"] {
        let command = format!("coin-fetch-{provider}");
        for typo in ["--dryy", "--dry-run", "--dr", "-dry", "dry"] {
            cases.push((command.clone(), vec![typo], 2, "Usage:", false));
        }
        cases.push((
            command.clone(),
            vec!["--dry"],
            0,
            "dry run: no downloads/pack changes.",
            false,
        ));
        cases.push((command, vec![], 2, "TELEGRAM_BOT_TOKEN is not set", true));
    }
    cases.push((
        "coin-fetch-logos".into(),
        vec!["--help"],
        0,
        "Default: 40 market pages",
        false,
    ));
    for pages in ["notanumber", "0", "-3"] {
        cases.push((
            "coin-fetch-logos".into(),
            vec![pages],
            2,
            if pages == "-3" {
                "unknown coin-fetch-logos argument"
            } else {
                "pages must be at least 1"
            },
            false,
        ));
    }
    for (command, extra, code, diagnostic, missing_token) in cases {
        let mut observer = bot_api::Server::new(vec![]);
        let inherited = std::env::vars().collect();
        let mut environment = _native::tooling_test_runner::scrubbed_environment(&inherited);
        environment.insert("NUMERA_TEST_PROVIDER_BASE".into(), observer.base.clone());
        environment.insert("NUMERA_TEST_API_BASE".into(), observer.base.clone());
        environment.insert("CMC_API_KEY".into(), "synthetic-argument-key".into());
        assert!(!environment.contains_key("TELEGRAM_BOT_TOKEN"));
        let mut argv: Vec<OsString> =
            vec![executable.as_os_str().to_owned(), command.clone().into()];
        argv.extend(extra.iter().map(|value| OsString::from(*value)));
        argv.extend(["--coin-dir".into(), data.as_os_str().to_owned()]);
        println!("native coin argument admission: {command} {extra:?}");
        let result = runtime
            .block_on(tooling_process::capture_with_input(
                root,
                &argv,
                &environment,
                &input,
                Duration::from_secs(6),
                Duration::from_secs(3),
                &log,
            ))
            .unwrap();
        assert_eq!(result.code, code, "{} {}", result.stdout, result.stderr);
        if missing_token || code != 0 {
            assert!(result.stderr.contains(diagnostic), "{}", result.stderr);
        } else {
            assert!(result.stdout.contains(diagnostic), "{}", result.stdout);
        }
        assert!(!result.stderr.contains("Traceback"));
        assert!(!result.stdout.contains("synthetic-argument-key"));
        assert!(!result.stderr.contains("synthetic-argument-key"));
        assert!(observer.finish().unwrap().is_empty());
        assert_eq!(snapshot(&data), before);
        if command == "coin-fetch-paprika" && extra == ["--dry"] {
            let lease = _native::locks::Lease::acquire(
                &data.join("paprika_matches.json.lock"),
                "released cache ownership",
            )
            .unwrap();
            drop(lease);
        }
        assert!(!_native::process_owner::cleanup_pending(root));
        assert!(
            std::fs::read_dir(root.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none()
        );
    }
    exhausted_logo_paging(root, executable, runtime, &input, &log);
    provider_search(root, executable, runtime, &input, &log);
    log.finish(0);
}

fn exhausted_logo_paging(
    root: &Path,
    executable: &Path,
    runtime: &tokio::runtime::Runtime,
    input: &Path,
    log: &RunLog,
) {
    let data = root.join("exhausted-logo-paging");
    std::fs::create_dir(&data).unwrap();
    let map = b"{\"other\":\"preserved\"}";
    std::fs::write(data.join("ticker_to_id.json"), map).unwrap();
    let replies = (0..6)
        .map(|_| {
            let mut reply = bot_api::Reply::new(
                "GET /api/v3/coins/markets",
                &[
                    ("user-agent", "Mozilla/5.0 (logo-fetcher; local tool)"),
                    (
                        "query",
                        "vs_currency=usd&order=market_cap_desc&per_page=250&page=1&sparkline=false",
                    ),
                ],
                serde_json::Value::Null,
            );
            reply.status = reqwest::StatusCode::SERVICE_UNAVAILABLE;
            reply.download = Some(b"not JSON".to_vec());
            // The real header parser rounds this positive advice to zero nanoseconds.
            reply.headers.insert(
                "retry-after",
                reqwest::header::HeaderValue::from_static("0.0000000001"),
            );
            reply
        })
        .collect();
    let mut observer = bot_api::Server::new(replies);
    let mut environment =
        _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
    environment.insert("NUMERA_TEST_PROVIDER_BASE".into(), observer.base.clone());
    environment.insert("COIN_PAGE_DELAY".into(), "0".into());
    let argv = vec![
        executable.as_os_str().to_owned(),
        "coin-fetch-logos".into(),
        "2".into(),
        "--coin-dir".into(),
        data.as_os_str().to_owned(),
    ];
    let result = runtime
        .block_on(tooling_process::capture_with_input(
            root,
            &argv,
            &environment,
            input,
            Duration::from_secs(6),
            Duration::from_secs(3),
            log,
        ))
        .unwrap();
    let requests = observer.finish().unwrap();
    assert_eq!(result.code, 3, "{} {}", result.stdout, result.stderr);
    assert!(
        result
            .stdout
            .contains("INCOMPLETE: market paging stopped early")
    );
    assert!(!result.stderr.contains("Traceback"));
    assert_eq!(requests.len(), 6);
    assert!(
        requests
            .iter()
            .all(|request| request.method == "GET /api/v3/coins/markets")
    );
    assert_eq!(std::fs::read(data.join("ticker_to_id.json")).unwrap(), map);
    assert_eq!(
        std::fs::read(data.join("keywords.csv")).unwrap(),
        b"ticker,name,format,file,keywords\r\n"
    );
    assert_eq!(
        std::fs::read_dir(data.join("logos/png")).unwrap().count(),
        0
    );
    assert_eq!(std::fs::read_dir(data.join("logos")).unwrap().count(), 1);
    assert_eq!(std::fs::read_dir(&data).unwrap().count(), 3);
    assert!(!_native::process_owner::cleanup_pending(root));
    assert!(
        std::fs::read_dir(root.join("logs/tmp/tooling"))
            .unwrap()
            .next()
            .is_none()
    );
}

fn provider_search(
    root: &Path,
    executable: &Path,
    runtime: &tokio::runtime::Runtime,
    input: &Path,
    log: &RunLog,
) {
    use serde_json::{Value, json};
    for scenario in ["outage", "cached-miss", "retry-miss"] {
        let data = root.join(format!("provider-search-{scenario}"));
        std::fs::create_dir(&data).unwrap();
        let cache = data.join("paprika_matches.json");
        _native::atomic::write_json(
            &cache,
            &if scenario == "outage" {
                json!({})
            } else {
                json!({"aaa":{"id":null,"conf":"no-results","name":"Alpha Coin"}})
            },
        )
        .unwrap();
        std::fs::write(data.join("ticker_to_id.json"), b"{\"btc\":\"c-btc\"}").unwrap();
        std::fs::write(data.join("rebuild_dedup_state.json"), b"{\"sets\":[]}").unwrap();
        std::fs::write(
            data.join("currency-emoji-inventory.md"),
            b"## AAA - Alpha Coin\nticker: aaa\npremium-id:\n",
        )
        .unwrap();
        let before = snapshot(&data);
        let replies = if scenario == "cached-miss" {
            Vec::new()
        } else if scenario == "outage" {
            ["Alpha+Coin", "aaa"]
                .into_iter()
                .flat_map(|query| {
                    (0..4).map(move |_| {
                        let mut reply = bot_api::Reply::new("GET /v1/search", &[], Value::Null);
                        reply
                            .fields
                            .insert("query".into(), format!("c=currencies&limit=10&q={query}"));
                        reply.fields.insert(
                            "user-agent".into(),
                            "Mozilla/5.0 (logo-fetcher; local)".into(),
                        );
                        reply.status = reqwest::StatusCode::SERVICE_UNAVAILABLE;
                        reply.download = Some(b"not JSON".to_vec());
                        reply.headers.insert(
                            "retry-after",
                            reqwest::header::HeaderValue::from_static("0.0000000001"),
                        );
                        reply
                    })
                })
                .collect()
        } else {
            vec![bot_api::Reply::new(
                "GET /v1/search",
                &[
                    ("query", "c=currencies&limit=10&q=Alpha+Coin"),
                    ("user-agent", "Mozilla/5.0 (logo-fetcher; local)"),
                ],
                json!({"currencies":[{"id":"alpha-coin","name":"Different Coin","symbol":"AAA"}]}),
            )]
        };
        let mut observer = bot_api::Server::new(replies);
        let mut environment =
            _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
        environment.insert("NUMERA_TEST_PROVIDER_BASE".into(), observer.base.clone());
        environment.insert("NUMERA_TEST_API_BASE".into(), observer.base.clone());
        let mut argv = vec![
            executable.as_os_str().to_owned(),
            "coin-fetch-paprika".into(),
            "--dry".into(),
            "--coin-dir".into(),
            data.as_os_str().to_owned(),
        ];
        if scenario == "retry-miss" {
            argv.push("--retry-unmatched".into());
        }
        let result = runtime
            .block_on(tooling_process::capture_with_input(
                root,
                &argv,
                &environment,
                input,
                Duration::from_secs(12),
                Duration::from_secs(8),
                log,
            ))
            .unwrap();
        assert_eq!(
            result.code,
            if scenario == "outage" { 3 } else { 0 },
            "{} {}",
            result.stdout,
            result.stderr
        );
        assert!(result.stdout.contains(if scenario == "outage" {
            "unknown=1"
        } else {
            "unknown=0"
        }));
        assert!(!result.stderr.contains("Traceback"));
        let calls = observer.finish().unwrap();
        assert_eq!(
            calls.len(),
            match scenario {
                "outage" => 8,
                "cached-miss" => 0,
                _ => 1,
            }
        );
        assert!(calls.iter().all(|call| call.method == "GET /v1/search"));
        let mut expected = before;
        if scenario == "retry-miss" {
            let actual = std::fs::read(&cache).unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&actual).unwrap(),
                json!({"aaa":{"id":"alpha-coin","conf":"symbol","name":"Alpha Coin"}})
            );
            expected.insert(cache.clone(), actual);
        }
        assert_eq!(snapshot(&data), expected);
        assert!(!data.join("logos").exists());
        assert!(!data.join("currency-emoji-inventory.filled.md").exists());
        drop(
            _native::locks::Lease::acquire(
                &data.join("paprika_matches.json.lock"),
                "search release",
            )
            .unwrap(),
        );
        assert!(!_native::process_owner::cleanup_pending(root));
        assert!(
            std::fs::read_dir(root.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none()
        );
    }
}

fn snapshot(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    std::fs::read_dir(path)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            let name = entry.file_name();
            if name == "paprika_matches.json.lock" {
                return None;
            }
            Some((entry.path(), std::fs::read(entry.path()).unwrap()))
        })
        .collect()
}
