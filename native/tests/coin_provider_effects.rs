//! Actual native top-up uploads once, verifies bytes, persists and skips on the next run.
use _native::{coin_provider_recover::Files, coin_provider_stage::Stage, telegram::Telegram};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[test]
fn native_provider_effects_upload_once_and_preserve_existing_mapping() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let scratch = root
        .join("logs/tmp")
        .join(format!("provider effects {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let files = Files {
        root: scratch.clone(),
        state: scratch.join("state.json"),
        mapping: scratch.join("map.json"),
        published: scratch.join("logos/emoji"),
        base: "cryptoemoji".into(),
    };
    let mut stage = Stage::new(&files.published).unwrap();
    let source = stage.source("aaa").unwrap();
    let bytes =
        std::fs::read(root.join("tests/fixtures/native/static-codec/alpha-edge.png")).unwrap();
    std::fs::write(&source, &bytes).unwrap();
    _native::atomic::write_json(&files.state,&json!({"sets":[{"index":1,"name":"cryptoemoji1_by_YourEmojiBot","title":"Title 1","live":0}],"provider_added":[]})).unwrap();
    _native::atomic::write_json(&files.mapping, &json!({"other":"preserved"})).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut methods = Vec::new();
        let live=json!({"ok":true,"result":{"stickers":[{"file_id":"f","file_unique_id":"u","custom_emoji_id":"new"}]}}).to_string().into_bytes();
        for index in 0..8 {
            let mut stream = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::yield_now()
                    }
                    Err(e) => panic!("provider effects fixture: {e}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut buf = [0; 4096];
            let end = loop {
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0 && request.len() + n < 131072);
                request.extend_from_slice(&buf[..n]);
                if let Some(i) = request.windows(4).position(|b| b == b"\r\n\r\n") {
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
            while request.len() < end + length {
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0 && request.len() + n < 131072);
                request.extend_from_slice(&buf[..n]);
            }
            methods.push(
                String::from_utf8_lossy(&request)
                    .lines()
                    .next()
                    .unwrap()
                    .to_owned(),
            );
            let body = match index {
                0 | 7 => json!({"ok":true,"result":{"username":"YourEmojiBot"}})
                    .to_string()
                    .into_bytes(),
                1 | 2 => json!({"ok":true,"result":{"stickers":[]}})
                    .to_string()
                    .into_bytes(),
                3 => {
                    assert!(request.windows(bytes.len()).any(|part| part == bytes));
                    json!({"ok":true,"result":true}).to_string().into_bytes()
                }
                4 => live.clone(),
                5 => json!({"ok":true,"result":{"file_path":"fixture.png"}})
                    .to_string()
                    .into_bytes(),
                _ => bytes.clone(),
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
    let tg = Telegram::new("test-only-token".into(), base).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let (added, failed, mapping) = _native::coin_provider_effects::run(
            &files,
            &tg,
            &mut stage,
            &["aaa".into()],
            "Title",
            1,
            &serde_json::Map::new(),
        )
        .unwrap();
        assert_eq!((added, failed), (1, 0));
        assert_eq!(mapping["other"], "preserved");
        assert_eq!(mapping["aaa"], "new");
        let state: Value = serde_json::from_slice(&std::fs::read(&files.state).unwrap()).unwrap();
        assert!(state["provider_in_flight"].is_null());
        assert_eq!(state["provider_added"], json!(["aaa"]));
        assert!(files.published.join("aaa.png").is_file() && !source.exists());
        let mut next = Stage::new(&files.published).unwrap();
        assert_eq!(
            _native::coin_provider_effects::run(
                &files,
                &tg,
                &mut next,
                &["aaa".into()],
                "Title",
                1,
                &serde_json::Map::new()
            )
            .unwrap()
            .0,
            0
        );
    }));
    let methods = server.join().unwrap();
    assert_eq!(
        methods
            .iter()
            .filter(|m| m.contains("/addStickerToSet "))
            .count(),
        1
    );
    assert!(!methods.iter().any(|m| m.contains("createNewStickerSet")));
    std::fs::remove_dir_all(&scratch).unwrap();
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
