//! Final links exercise the actual isolated CLI; failures never publish a success milestone.
use _native::process_owner::OwnedProcess;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn final_links_resend_explicitly_and_preserve_state_when_send_or_input_fails() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let scratch = root
        .join("logs/tmp")
        .join(format!("coin rebuild links {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::fs::create_dir(scratch.join("assets")).unwrap();
        std::fs::write(scratch.join("assets/panel.html"), b"fixture marker").unwrap();
        std::fs::write(scratch.join("pyproject.toml"), b"# fixture marker\n").unwrap();
        let coins = scratch.join("coins");
        std::fs::create_dir(&coins).unwrap();
        let state = coins.join("rebuild_dedup_state.json");
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/coin-rebuild-links.json"
        ))
        .unwrap();
        let source_case = &fixture["cases"][1];
        let mut original = source_case["input"]["state"].clone();
        original["sets"]
            .as_array_mut()
            .unwrap()
            .sort_by_key(|set| set["index"].as_u64());
        _native::atomic::write_json(&state, &original).unwrap();
        let packs = source_case["expected"]["events"][1][1].as_array().unwrap();
        let mut lines = vec![
            source_case["expected"]["events"][1][2]
                .as_str()
                .unwrap()
                .to_owned(),
            String::new(),
        ];
        for pack in packs {
            lines.push(format!(
                "{}. https://t.me/addemoji/{}",
                pack["title"].as_str().unwrap(),
                pack["name"].as_str().unwrap()
            ));
        }
        let expected_text = lines.join("\n");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server_root = scratch.clone();
        let state_read = state.clone();
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(15);
            let mut methods = Vec::new();
            for index in 0..9 {
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            std::thread::yield_now()
                        }
                        Err(error) => panic!("coin links fixture: {error}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                let end = loop {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0 && request.len() + n < 65536);
                    request.extend_from_slice(&buffer[..n]);
                    if let Some(i) = request.windows(4).position(|v| v == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let header = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                let length = header
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:").map(str::trim))
                    .unwrap_or("0")
                    .parse::<usize>()
                    .unwrap();
                assert!(end + length < 65536);
                while request.len() < end + length {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0 && request.len() + n < 65536);
                    request.extend_from_slice(&buffer[..n]);
                }
                let method = String::from_utf8_lossy(&request[..end])
                    .lines()
                    .next()
                    .unwrap()
                    .to_owned();
                let sending = [1, 3, 5].contains(&index);
                assert!(method.contains(if sending { "/sendMessage " } else { "/getMe " }));
                if sending {
                    let form = reqwest::Url::parse(&format!(
                        "http://fixture/?{}",
                        std::str::from_utf8(&request[end..end + length]).unwrap()
                    ))
                    .unwrap();
                    let fields = form
                        .query_pairs()
                        .collect::<std::collections::BTreeMap<_, _>>();
                    assert_eq!(fields.get("text").unwrap().as_ref(), expected_text);
                    assert_eq!(fields.get("chat_id").unwrap().as_ref(), "111111111");
                    assert_eq!(
                        fields.get("disable_web_page_preview").unwrap().as_ref(),
                        "true"
                    );
                    assert!(
                        _native::locks::Lease::acquire(
                            &_native::locks::pack_family_path(&server_root, "cryptoemoji"),
                            "fixture"
                        )
                        .is_err()
                    );
                    let saved: Value =
                        serde_json::from_slice(&std::fs::read(&state_read).unwrap()).unwrap();
                    if index == 1 {
                        assert!(saved.get("final_sent").is_none());
                    } else {
                        assert_eq!(saved["final_sent"], true);
                    }
                }
                methods.push(method);
                let body = if index == 5 {
                    json!({"ok":false,"description":"controlled link refusal"})
                } else if sending {
                    json!({"ok":true,"result":{"message_id":index}})
                } else {
                    json!({"ok":true,"result":{"username":"YourEmojiBot"}})
                }
                .to_string()
                .into_bytes();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            }
            methods
        });
        let invocation = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let executable = scratch.join(if cfg!(windows) {
                "numera-emoji.exe"
            } else {
                "numera-emoji"
            });
            std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            for iteration in 0..6 {
                if iteration == 3 {
                    _native::atomic::write_json(&state, &json!({"sets":[],"sent":[]})).unwrap();
                }
                if iteration == 4 {
                    std::fs::remove_file(&state).unwrap();
                }
                if iteration == 5 {
                    std::fs::write(&state, b"{truncated").unwrap();
                }
                let before = std::fs::read(&state).ok();
                let work = scratch.join(format!("owner{iteration}"));
                std::fs::create_dir(&work).unwrap();
                let output = scratch.join(format!("out{iteration}.txt"));
                let errors = scratch.join(format!("err{iteration}.txt"));
                let mut command = Command::new(&executable);
                command
                    .args(["coin-rebuild", "links"])
                    .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
                    .env("NUMERA_TEST_API_BASE", &base)
                    .env("COIN_PACK_BASE", "cryptoemoji")
                    .env("COIN_PACK_TITLE", "Fixture")
                    .env("PACK_OWNER_USER_ID", "111111111")
                    .env("TELEGRAM_BOT_TOKEN", "synthetic-link-token")
                    .env_remove("WORKER_PUBLISH_URL")
                    .env_remove("WORKER_PUBLISH_SECRET")
                    .env_remove("PACK_LINKS_CHAT_ID")
                    .stdin(Stdio::null())
                    .stdout(Stdio::from(std::fs::File::create(&output).unwrap()))
                    .stderr(Stdio::from(std::fs::File::create(&errors).unwrap()));
                let mut owner = OwnedProcess::spawn(&mut command, work).unwrap();
                let status = runtime
                    .block_on(owner.wait(Duration::from_secs(10), Duration::from_secs(5)))
                    .unwrap();
                owner.finish().unwrap();
                assert_eq!(
                    status.code(),
                    Some(if iteration == 2 || iteration == 5 {
                        2
                    } else {
                        0
                    }),
                    "{}",
                    std::fs::read_to_string(&errors).unwrap()
                );
                let text = std::fs::read_to_string(&output).unwrap();
                if iteration < 2 {
                    let mut expected = source_case["expected"]["state"].clone();
                    expected["sets"]
                        .as_array_mut()
                        .unwrap()
                        .sort_by_key(|set| set["index"].as_u64());
                    let saved: Value =
                        serde_json::from_slice(&std::fs::read(&state).unwrap()).unwrap();
                    assert_eq!(saved, expected);
                    assert_eq!(text, source_case["expected"]["output"].as_str().unwrap());
                } else {
                    assert_eq!(std::fs::read(&state).ok(), before);
                    assert!(!text.contains("sent final links"));
                    if iteration == 3 || iteration == 4 {
                        assert_eq!(text, "no sets to send.\n");
                    }
                }
            }
        }));
        let joined = server.join();
        if let Err(error) = invocation {
            std::panic::resume_unwind(error);
        }
        let methods = joined.unwrap();
        assert_eq!(
            methods
                .iter()
                .filter(|m| m.contains("/sendMessage "))
                .count(),
            3
        );
        assert!(
            methods
                .iter()
                .all(|m| m.contains("/getMe ") || m.contains("/sendMessage "))
        );
    }));
    if let Err(error) = result {
        // Preserve failure diagnostics and ownership markers until cleanup can be verified.
        std::panic::resume_unwind(error);
    }
    for iteration in 0..6 {
        assert!(!scratch.join(format!("owner{iteration}")).exists());
    }
    let released = _native::locks::Lease::acquire(
        &_native::locks::pack_family_path(&scratch, "cryptoemoji"),
        "fixture release",
    )
    .unwrap();
    drop(released);
    std::fs::remove_dir_all(&scratch).unwrap();
}
