//! Actual isolated command writes a live loopback report without any Bot API mutation.
use _native::process_owner::OwnedProcess;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn actual_manifest_command_reads_live_ids_and_redacts_custom_token() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let scratch = root
        .join("logs/tmp")
        .join(format!("coin manifest cli {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let state = scratch.join("state.json");
    let mapping = scratch.join("map.json");
    let name = "cryptoemoji1_by_YourEmojiBot";
    let state_doc = json!({"sets":[{"name":name,"index":1,"title":"Fixture","live":199}]});
    let map_doc = json!({"btc":"111111111","btctrc20":"111111111"});
    _native::atomic::write_json(&state, &state_doc).unwrap();
    _native::atomic::write_json(&mapping, &map_doc).unwrap();
    let before_state = std::fs::read(&state).unwrap();
    let before_map = std::fs::read(&mapping).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let token = "synthetic-manifest-token";
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        for attempt in 0..2 {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::yield_now()
                    }
                    Err(e) => panic!("manifest fixture: {e}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            let header_end = loop {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0 && request.len() + n < 32768);
                request.extend_from_slice(&buffer[..n]);
                if let Some(end) = request.windows(4).position(|b| b == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let head = String::from_utf8(request[..header_end].to_vec()).unwrap();
            let length = head
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    if key.eq_ignore_ascii_case("content-length") {
                        Some(value.trim().parse::<usize>().unwrap())
                    } else {
                        None
                    }
                })
                .unwrap();
            while request.len() < header_end + length {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0 && request.len() + n < 32768);
                request.extend_from_slice(&buffer[..n]);
            }
            assert!(head.lines().next().unwrap().contains("/getStickerSet "));
            assert_eq!(&request[header_end..], format!("name={name}").as_bytes());
            let body = if attempt == 0 {
            json!({"ok":true,"result":{"stickers":[{"custom_emoji_id":"111111111"},{"custom_emoji_id":"222222222"}]}})
        } else {
            json!({"ok":false,"error_code":400,"description":format!("controlled refusal {token}")})
        }.to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let output = scratch.join("stdout.txt");
    let errors = scratch.join("stderr.txt");
    let work = scratch.join("owner");
    std::fs::create_dir(&work).unwrap();
    // The executable discovers this physical fixture root, so its leases/logs
    // cannot collide with another CLI test or touch the checkout's real locks.
    std::fs::create_dir(scratch.join("assets")).unwrap();
    std::fs::write(scratch.join("assets/panel.html"), b"fixture root marker").unwrap();
    std::fs::write(scratch.join("pyproject.toml"), b"# fixture root marker\n").unwrap();
    let executable = scratch.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let mut command = Command::new(executable);
    command
        .arg("coin-write-manifests")
        .arg("--out-dir")
        .arg(&scratch)
        .arg("--state")
        .arg(&state)
        .arg("--map")
        .arg(&mapping)
        .arg("--token-env")
        .arg("FIXTURE_MANIFEST_AUTH")
        .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
        .env("NUMERA_TEST_API_BASE", base)
        .env("COIN_PACK_BASE", "cryptoemoji")
        .env("FIXTURE_MANIFEST_AUTH", token)
        .env_remove("TELEGRAM_BOT_TOKEN")
        .stdin(Stdio::null())
        .stdout(Stdio::from(std::fs::File::create(&output).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&errors).unwrap()));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut owner = OwnedProcess::spawn(&mut command, work).unwrap();
        let status = runtime
            .block_on(owner.wait(Duration::from_secs(10), Duration::from_secs(5)))
            .unwrap();
        owner.finish().unwrap();
        assert_eq!(
            status.code(),
            Some(0),
            "{}",
            std::fs::read_to_string(&errors).unwrap()
        );
        let manifest =
            std::fs::read_to_string(scratch.join(format!("manifests/{name}.md"))).unwrap();
        assert!(manifest.contains("| 1 | btc, btctrc20 | 111111111 |"));
        assert!(manifest.contains("| 2 | (unmapped) | 222222222 |"));
        assert!(manifest.contains("2 stickers"));
        assert!(!manifest.contains("199 stickers"));
        assert_eq!(std::fs::read(&state).unwrap(), before_state);
        assert_eq!(std::fs::read(&mapping).unwrap(), before_map);
        assert!(!std::fs::read_to_string(&output).unwrap().contains(token));
        assert!(!std::fs::read_to_string(&errors).unwrap().contains(token));
        let before_report = std::fs::read(scratch.join(format!("manifests/{name}.md"))).unwrap();
        let before_index = std::fs::read(scratch.join("manifests/INDEX.md")).unwrap();
        let work = scratch.join("refusal owner");
        std::fs::create_dir(&work).unwrap();
        command
            .stdout(Stdio::from(std::fs::File::create(&output).unwrap()))
            .stderr(Stdio::from(std::fs::File::create(&errors).unwrap()));
        let mut owner = OwnedProcess::spawn(&mut command, work).unwrap();
        let status = runtime
            .block_on(owner.wait(Duration::from_secs(10), Duration::from_secs(5)))
            .unwrap();
        owner.finish().unwrap();
        assert_eq!(status.code(), Some(2));
        let error = std::fs::read_to_string(&errors).unwrap();
        assert!(error.contains("controlled refusal") && error.contains("[REDACTED]"));
        assert!(!error.contains(token));
        let logs = std::fs::read_dir(scratch.join("logs"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(logs.len(), 2);
        for path in logs {
            let text = std::fs::read_to_string(path).unwrap();
            assert!(!text.contains(token));
            assert!(text.contains("[runtime]") && text.contains("exit="));
        }
        assert_eq!(
            std::fs::read(scratch.join(format!("manifests/{name}.md"))).unwrap(),
            before_report
        );
        assert_eq!(
            std::fs::read(scratch.join("manifests/INDEX.md")).unwrap(),
            before_index
        );
    }));
    server.join().unwrap();
    std::fs::remove_dir_all(scratch).unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
