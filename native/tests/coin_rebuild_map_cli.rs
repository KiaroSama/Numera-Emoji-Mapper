//! Actual rebuild-map command proves reordered content under both locks and preserves refused maps.
use _native::process_owner::OwnedProcess;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn reordered_images_restore_aliases_and_unreadable_live_content_preserves_the_map() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let scratch = root
        .join("logs/tmp")
        .join(format!("coin rebuild map cli {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    std::fs::create_dir(scratch.join("assets")).unwrap();
    std::fs::write(scratch.join("assets/panel.html"), b"fixture marker").unwrap();
    std::fs::write(scratch.join("pyproject.toml"), b"# fixture marker\n").unwrap();
    let coins = scratch.join("coins");
    let images = coins.join("logos/emoji");
    std::fs::create_dir_all(&images).unwrap();
    let a = std::fs::read(root.join("tests/fixtures/native/static-codec/public-logo.png")).unwrap();
    let b = std::fs::read(root.join("tests/fixtures/native/static-codec/alpha-edge.png")).unwrap();
    std::fs::write(images.join("aaa.png"), &a).unwrap();
    std::fs::write(images.join("bbb.png"), &b).unwrap();
    let plan = coins.join("rebuild_dedup_plan.json");
    let state = coins.join("rebuild_dedup_state.json");
    let map = coins.join("ticker_to_id.json");
    let candidate = coins.join("ticker_to_id.candidate.json");
    _native::atomic::write_json(
        &plan,
        &json!([
            {"rep":"aaa","tickers":["aaa"],"kw":"A"},
            {"rep":"bbb","tickers":["bbb"],"kw":"B"}
        ]),
    )
    .unwrap();
    _native::atomic::write_json(&state, &json!({"sets":[{"index":1,"name":"cryptoemoji1_by_YourEmojiBot"}],"order":["aaa","bbb"],"cursor":2,"in_flight":null})).unwrap();
    _native::atomic::write_json(&map, &json!({"aaa":"old-a","aaaerc":"old-a","bbb":"old-b"}))
        .unwrap();
    std::fs::write(coins.join("currency-emoji-inventory.md"), b"## A - A\nticker: aaa\npremium-id:\n## Alias - A\nticker: aaaerc\npremium-id:\n## B - B\nticker: bbb\npremium-id:\n").unwrap();
    let state_before = std::fs::read(&state).unwrap();
    let plan_before = std::fs::read(&plan).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server_root = scratch.clone();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut methods = Vec::new();
        for index in 0..8 {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::yield_now()
                    }
                    Err(e) => panic!("rebuild map fixture: {e}"),
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
            while !request.windows(4).any(|b| b == b"\r\n\r\n") {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0 && request.len() + n < 32768);
                request.extend_from_slice(&buffer[..n]);
            }
            methods.push(
                String::from_utf8_lossy(&request)
                    .lines()
                    .next()
                    .unwrap()
                    .to_owned(),
            );
            if index == 0 {
                for lock in [
                    _native::locks::pack_family_path(&server_root, "cryptoemoji"),
                    server_root.join(".locks/canonical_map.lock"),
                ] {
                    assert!(
                        _native::locks::Lease::acquire(&lock, "fixture competing writer").is_err(),
                        "snapshot read must own {}",
                        lock.display()
                    );
                }
            }
            let body = match index {
                0 | 5 => json!({"ok":true,"result":{"stickers":[{"custom_emoji_id":"222222222","file_id":"fb"},{"custom_emoji_id":"111111111","file_id":"fa"}]}}).to_string().into_bytes(),
                1 | 6 => json!({"ok":true,"result":{"file_path":"b.png"}}).to_string().into_bytes(),
                2 => b.clone(),
                3 => json!({"ok":true,"result":{"file_path":"a.png"}}).to_string().into_bytes(),
                4 => a.clone(),
                _ => b"controlled unreadable live image".to_vec(),
            };
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
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut accepted_map = Vec::new();
        for iteration in 0..2 {
            let work = scratch.join(format!("owner{iteration}"));
            std::fs::create_dir(&work).unwrap();
            let stdout = scratch.join(format!("out{iteration}.txt"));
            let stderr = scratch.join(format!("err{iteration}.txt"));
            let mut cmd = Command::new(&executable);
            cmd.arg("coin-rebuild-map")
                .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
                .env("NUMERA_TEST_API_BASE", &base)
                .env("COIN_PACK_BASE", "cryptoemoji")
                .env("TELEGRAM_BOT_TOKEN", "synthetic-rebuild-token")
                .stdin(Stdio::null())
                .stdout(Stdio::from(std::fs::File::create(&stdout).unwrap()))
                .stderr(Stdio::from(std::fs::File::create(&stderr).unwrap()));
            let mut owner = OwnedProcess::spawn(&mut cmd, work).unwrap();
            let status = runtime
                .block_on(owner.wait(Duration::from_secs(10), Duration::from_secs(5)))
                .unwrap();
            owner.finish().unwrap();
            assert_eq!(
                status.code(),
                Some(if iteration == 0 { 0 } else { 2 }),
                "{}",
                std::fs::read_to_string(&stderr).unwrap()
            );
            if iteration == 0 {
                accepted_map = std::fs::read(&map).unwrap();
                assert_eq!(
                    serde_json::from_slice::<Value>(&accepted_map).unwrap(),
                    json!({"bbb":"222222222","aaa":"111111111","aaaerc":"111111111"})
                );
                let filled =
                    std::fs::read_to_string(coins.join("currency-emoji-inventory.filled.md"))
                        .unwrap();
                assert_eq!(filled.matches("premium-id: 111111111").count(), 2);
                assert_eq!(filled.matches("premium-id: 222222222").count(), 1);
                assert!(!candidate.exists());
            } else {
                assert_eq!(std::fs::read(&map).unwrap(), accepted_map);
                let refused: Value =
                    serde_json::from_slice(&std::fs::read(&candidate).unwrap()).unwrap();
                assert!(
                    refused["error"]
                        .as_str()
                        .unwrap()
                        .contains("222222222 could not be read")
                );
                assert_eq!(refused["recorded_uploads"], 2);
            }
            assert_eq!(std::fs::read(&state).unwrap(), state_before);
            assert_eq!(std::fs::read(&plan).unwrap(), plan_before);
        }
    }));
    let joined = server.join();
    let cleaned = std::fs::remove_dir_all(&scratch);
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    let methods = joined.unwrap();
    cleaned.unwrap();
    assert_eq!(
        methods
            .iter()
            .filter(|m| m.contains("/getStickerSet "))
            .count(),
        2
    );
    assert!(methods.iter().all(|m| !m.contains("addSticker")
        && !m.contains("createNew")
        && !m.contains("replaceSticker")));
}
