//! Real JSON Bot API boundaries against numeric-loopback, never production Telegram.
use super::*;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
const GENERAL: &str = "1111111111:never_issued_general_token_aaaaaaaaaaaa";
const COIN: &str = "2222222222:never_issued_coin_token_bbbbbbbbbbbbbbbb";
const GENERAL_SECRET: &str = "never_issued_general_webhook_cccccccccccccc";
const COIN_SECRET: &str = "never_issued_coin_webhook_ddddddddddddddddd";
struct Reply {
    path: String,
    status: u16,
    response: Value,
    expected: Value,
}
fn response(token: &str, method: &str, result: Value, expected: Value) -> Reply {
    Reply {
        path: format!("/bot{token}/{method}"),
        status: 200,
        response: json!({"ok":true,"result":result}),
        expected,
    }
}
struct Server {
    base: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Result<usize, String>>>,
}
impl Server {
    fn new(script: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let closed = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut pending = std::collections::VecDeque::from(script);
            let mut count = 0;
            let deadline = Instant::now() + Duration::from_secs(15);
            while !closed.load(Ordering::SeqCst) && Instant::now() < deadline {
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(_) => return Err("owned accept failed".into()),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = vec![];
                let (headers, body) = loop {
                    let mut buf = [0; 4096];
                    let n = stream
                        .read(&mut buf)
                        .map_err(|_| "owned HTTP read failed")?;
                    if n == 0 {
                        return Err("truncated owned HTTP request".into());
                    }
                    bytes.extend_from_slice(&buf[..n]);
                    if bytes.len() > 65536 {
                        return Err("owned request exceeded limit".into());
                    }
                    if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                        let h =
                            String::from_utf8(bytes[..end].to_vec()).map_err(|_| "bad header")?;
                        let len = h
                            .lines()
                            .find_map(|line| {
                                line.split_once(':')
                                    .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                                    .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + len {
                            break (h, bytes[end + 4..end + 4 + len].to_vec());
                        }
                    }
                };
                let expected = pending.pop_front().ok_or("unexpected extra API call")?;
                let first = headers
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .collect::<Vec<_>>();
                if first[0] != "POST" || first[1] != expected.path {
                    return Err("wrong API route/order".into());
                }
                if !headers
                    .to_ascii_lowercase()
                    .contains("content-type: application/json")
                {
                    return Err("Bot API body must preserve JSON types".into());
                }
                let actual: Value =
                    serde_json::from_slice(&body).map_err(|_| "request was not JSON")?;
                if actual != expected.expected {
                    return Err("wrong JSON payload".into());
                }
                let body = expected.response.to_string();
                stream.write_all(format!("HTTP/1.1 {} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",expected.status,body.len()).as_bytes()).map_err(|_|"reply failed")?;
                count += 1;
            }
            if !pending.is_empty() {
                return Err("required API calls were not made".into());
            }
            Ok(count)
        });
        Self {
            base,
            stop,
            thread: Some(thread),
        }
    }
    fn finish(mut self) -> usize {
        self.stop.store(true, Ordering::SeqCst);
        self.thread
            .take()
            .unwrap()
            .join()
            .unwrap()
            .expect("actual Bot API transcript")
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("worker")).unwrap();
    std::fs::write(root.path().join("owned.env"),format!("GENERAL_BOT_TOKEN={GENERAL}\nTELEGRAM_BOT_TOKEN={COIN}\nGENERAL_WEBHOOK_SECRET={GENERAL_SECRET}\nCOIN_WEBHOOK_SECRET={COIN_SECRET}\n")).unwrap();
    root
}
fn invoke(
    root: &tempfile::TempDir,
    server: &Server,
    extra: &[&str],
) -> (Result<u8, String>, String) {
    let mut args = vec![
        "webhooks".into(),
        "--env-file".into(),
        root.path().join("owned.env").to_string_lossy().into_owned(),
    ];
    args.extend(extra.iter().map(|s| (*s).into()));
    let action = parse(&args).unwrap();
    let mut settings = Settings::native();
    settings.telegram_base = server.base.clone();
    let mut lines = vec![];
    let result = execute_with(root.path(), action, &settings, &mut |line| {
        lines.push(line.to_owned())
    });
    let output = lines.join("\n");
    for sensitive in [GENERAL, COIN, GENERAL_SECRET, COIN_SECRET] {
        assert!(!output.contains(sensitive));
        if let Err(error) = &result {
            assert!(!error.contains(sensitive));
        }
    }
    (result, output)
}
fn set_payload(role: &str) -> Value {
    json!({"url":format!("https://owned-worker.invalid/tg/{role}"),"secret_token":if role=="general"{GENERAL_SECRET}else{COIN_SECRET},"allowed_updates":["message","edited_message","channel_post"],"drop_pending_updates":false})
}
// Break caught: status accidentally changes the selected bot, or prints an upstream credential echo.
#[test]
fn status_queries_only_selected_bot_and_redacts_server_diagnostics() {
    let root = fixture();
    let server = Server::new(vec![response(
        GENERAL,
        "getWebhookInfo",
        json!({"url":"https://owned-worker.invalid/tg/general","pending_update_count":3,"last_error_message":format!("upstream echoed {GENERAL} and {GENERAL_SECRET}")}),
        json!({}),
    )]);
    let (result, output) = invoke(&root, &server, &["--only", "general", "--status"]);
    assert_eq!(result.unwrap(), 0);
    assert!(output.contains("pending 3"));
    assert!(output.contains("[REDACTED]"));
    assert_eq!(server.finish(), 1);
}
// Break caught: inventing polling detection/Force refusal, wrong role secret, or dropping pending updates.
#[test]
fn explicit_registration_from_polling_queries_info_then_sets_exact_payload() {
    let root = fixture();
    let server = Server::new(vec![
        response(
            GENERAL,
            "getWebhookInfo",
            json!({"url":"","pending_update_count":0}),
            json!({}),
        ),
        response(GENERAL, "setWebhook", json!(true), set_payload("general")),
    ]);
    let (result, output) = invoke(
        &root,
        &server,
        &[
            "--only",
            "general",
            "--base-url",
            "https://owned-worker.invalid///",
        ],
    );
    assert_eq!(result.unwrap(), 0);
    assert!(output.to_lowercase().contains("poll"));
    assert!(output.to_lowercase().contains("no longer"));
    assert_eq!(server.finish(), 2);
}
// Break caught: replacing a genuinely different existing webhook without explicit Force.
#[test]
fn different_nonempty_webhook_is_refused_without_force() {
    let root = fixture();
    let server = Server::new(vec![response(
        GENERAL,
        "getWebhookInfo",
        json!({"url":"https://other-owned.invalid/tg/general"}),
        json!({}),
    )]);
    let (result, output) = invoke(
        &root,
        &server,
        &[
            "--only",
            "general",
            "--base-url",
            "https://owned-worker.invalid",
        ],
    );
    assert_eq!(result.unwrap(), 0);
    assert!(output.contains("--force"));
    assert_eq!(server.finish(), 1);
}
// Break caught: coin administration uses general credentials or Force still does not permit a known replacement.
#[test]
fn force_replaces_known_coin_webhook_with_its_own_secret() {
    let root = fixture();
    let server = Server::new(vec![
        response(
            COIN,
            "getWebhookInfo",
            json!({"url":"https://other-owned.invalid/tg/coin"}),
            json!({}),
        ),
        response(COIN, "setWebhook", json!(true), set_payload("coin")),
    ]);
    assert_eq!(
        invoke(
            &root,
            &server,
            &[
                "--only",
                "COIN",
                "--base-url",
                "https://owned-worker.invalid",
                "--force"
            ]
        )
        .0
        .unwrap(),
        0
    );
    assert_eq!(server.finish(), 2);
}
// Break caught: treating same URL as a refusal and failing to refresh the existing registration's secret/options.
#[test]
fn same_webhook_url_can_be_refreshed_without_force() {
    let root = fixture();
    let server = Server::new(vec![
        response(
            GENERAL,
            "getWebhookInfo",
            json!({"url":"https://owned-worker.invalid/tg/general"}),
            json!({}),
        ),
        response(GENERAL, "setWebhook", json!(true), set_payload("general")),
    ]);
    assert_eq!(
        invoke(
            &root,
            &server,
            &[
                "--only",
                "general",
                "--base-url",
                "https://owned-worker.invalid"
            ]
        )
        .0
        .unwrap(),
        0
    );
    assert_eq!(server.finish(), 2);
}
// Break caught: failed getInfo is mistaken for empty polling state, even when Force is supplied.
#[test]
fn failed_get_info_stops_before_any_mutation() {
    let root = fixture();
    let mut failed = response(GENERAL, "getWebhookInfo", Value::Null, json!({}));
    failed.status = 401;
    failed.response =
        json!({"ok":false,"description":format!("invalid {GENERAL} and {GENERAL_SECRET}")});
    let server = Server::new(vec![failed]);
    let (result, _) = invoke(
        &root,
        &server,
        &[
            "--only",
            "general",
            "--base-url",
            "https://owned-worker.invalid",
            "--force",
        ],
    );
    assert!(result.unwrap_err().contains("getWebhookInfo"));
    assert_eq!(server.finish(), 1);
}
// Break caught: malformed successful-looking metadata creates a false empty webhook decision.
#[test]
fn malformed_get_info_stops_before_any_mutation() {
    for bad in [Value::Null, json!([]), json!({}), json!({"url":23})] {
        let root = fixture();
        let server = Server::new(vec![response(GENERAL, "getWebhookInfo", bad, json!({}))]);
        assert!(
            invoke(&root, &server, &["--only", "general", "--delete"])
                .0
                .is_err()
        );
        assert_eq!(server.finish(), 1);
    }
}
// Break caught: delete bypasses the genuine info query, sends another role's token, or loses updates explicitly.
#[test]
fn delete_queries_info_then_deletes_without_pending_update_drop() {
    let root = fixture();
    let server = Server::new(vec![
        response(
            COIN,
            "getWebhookInfo",
            json!({"url":"https://owned-worker.invalid/tg/coin"}),
            json!({}),
        ),
        response(COIN, "deleteWebhook", json!(true), json!({})),
    ]);
    assert_eq!(
        invoke(&root, &server, &["--only", "coin", "--delete"])
            .0
            .unwrap(),
        0
    );
    assert_eq!(server.finish(), 2);
}
// Break caught: old PS false-success path accepts a failed deletion.
#[test]
fn rejected_delete_is_an_error_with_redacted_reason() {
    let root = fixture();
    let mut failed = response(COIN, "deleteWebhook", Value::Null, json!({}));
    failed.status = 400;
    failed.response = json!({"ok":false,"description":format!("refused {COIN_SECRET}")});
    let server = Server::new(vec![
        response(
            COIN,
            "getWebhookInfo",
            json!({"url":"https://owned-worker.invalid/tg/coin"}),
            json!({}),
        ),
        failed,
    ]);
    assert!(
        invoke(&root, &server, &["--only", "coin", "--delete"])
            .0
            .is_err()
    );
    assert_eq!(server.finish(), 2);
}

#[test]
fn status_malformed_pending_metadata_never_becomes_raw_report_data() {
    let root = fixture();
    let server = Server::new(vec![response(
        GENERAL,
        "getWebhookInfo",
        json!({"url":"", "pending_update_count":{"opaque":"untrusted_nested_count_must_not_be_reported"}}),
        json!({}),
    )]);
    let (result, output) = invoke(&root, &server, &["--only", "general", "--status"]);
    assert_eq!(result.unwrap(), 0);
    assert!(output.contains("pending ?"));
    assert!(!output.contains("untrusted_nested_count_must_not_be_reported"));
    assert_eq!(server.finish(), 1);
}
