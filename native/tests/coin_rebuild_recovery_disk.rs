//! Real create recovery proves source pixels, persists adoption and does not mutate Telegram.
use _native::{coin_rebuild_map_disk::Files, coin_rebuild_recovery_disk::run};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};
#[test]
fn proven_create_is_adopted_once_and_malformed_readback_keeps_the_intent() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let scratch = root
        .join("logs/tmp")
        .join(format!("coin rebuild recovery {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let images = scratch.join("logos/emoji");
    std::fs::create_dir_all(&images).unwrap();
    let bytes =
        std::fs::read(root.join("tests/fixtures/native/static-codec/alpha-edge.png")).unwrap();
    std::fs::write(images.join("aaa.png"), &bytes).unwrap();
    let files = Files {
        root: scratch.clone(),
        directory: scratch.clone(),
        images,
        base: "cryptoemoji".into(),
    };
    let state_file = scratch.join("rebuild_dedup_state.json");
    let plan_file = scratch.join("rebuild_dedup_plan.json");
    _native::atomic::write_json(
        &plan_file,
        &json!([{"rep":"aaa","tickers":["aaa"],"kw":"A"}]),
    )
    .unwrap();
    let original = json!({"sets":[],"order":[],"cursor":1,"in_flight":{"key":"aaa","operation":"create","set_name":"cryptoemoji1_by_YourEmojiBot","set_index":1,"expected_before":0,"phase":"upload"}});
    _native::atomic::write_json(&state_file, &original).unwrap();
    let plan_before = std::fs::read(&plan_file).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut methods = Vec::new();
        for index in 0..5 {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::yield_now()
                    }
                    Err(e) => panic!("rebuild recovery fixture: {e}"),
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
            while !request.windows(4).any(|v| v == b"\r\n\r\n") {
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
            let body = match index {
                0 | 3 => json!({"ok":true,"result":{"stickers":[{"custom_emoji_id":"111111111","file_id":"f1"}]}}).to_string().into_bytes(),
                1 => json!({"ok":true,"result":{"file_path":"fixture.png"}}).to_string().into_bytes(),
                2 => bytes.clone(),
                _ => json!({"ok":true,"result":{"title":"malformed missing sticker list"}}).to_string().into_bytes(),
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
    let tg =
        _native::telegram::Telegram::new("synthetic-rebuild-recovery-token".into(), base).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let (state, total) = run(&files, "Fixture", &tg).unwrap();
        assert_eq!(total, 1);
        assert_eq!(state["order"], json!(["aaa"]));
        assert!(state["in_flight"].is_null());
        assert_eq!(
            state["sets"],
            json!([{"index":1,"name":"cryptoemoji1_by_YourEmojiBot","title":"Fixture 1","live":1}])
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(&state_file).unwrap()).unwrap(),
            state
        );
        let (again, total) = run(&files, "Fixture", &tg).unwrap();
        assert_eq!(again, state);
        assert_eq!(total, 1);
        _native::atomic::write_json(&state_file, &original).unwrap();
        let before = std::fs::read(&state_file).unwrap();
        assert!(run(&files, "Fixture", &tg).unwrap_err().contains("unknown"));
        assert_eq!(std::fs::read(&state_file).unwrap(), before);
        assert_eq!(std::fs::read(&plan_file).unwrap(), plan_before);
        assert!(
            std::fs::read_dir(scratch.join("logs/tmp"))
                .unwrap()
                .next()
                .is_none()
        );
    }));
    let joined = server.join();
    let cleaned = std::fs::remove_dir_all(&scratch);
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    cleaned.unwrap();
    let methods = joined.unwrap();
    assert_eq!(
        methods
            .iter()
            .filter(|m| m.contains("/getStickerSet "))
            .count(),
        3
    );
    assert!(methods.iter().all(|m| !m.contains("addSticker")
        && !m.contains("createNew")
        && !m.contains("deleteSticker")));
}
