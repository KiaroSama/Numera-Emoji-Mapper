//! Actual native provider recovery reads live loopback bytes and persists without a second add.
use _native::{
    coin_provider_recover::{Files, run},
    coin_provider_stage::Stage,
    telegram::Telegram,
};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[test]
fn interrupted_provider_recovers_original_art_map_and_tally_without_mutation() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let scratch = root
        .join("logs/tmp")
        .join(format!("provider restart {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let published = scratch.join("logos/emoji");
    let stage = Stage::new(&published).unwrap();
    let source = stage.source("aaa").unwrap();
    let bytes =
        std::fs::read(root.join("tests/fixtures/native/static-codec/alpha-edge.png")).unwrap();
    std::fs::write(&source, &bytes).unwrap();
    let want = _native::codec_image::source_dhash(&source).unwrap();
    let files = Files {
        root: scratch.clone(),
        state: scratch.join("state.json"),
        mapping: scratch.join("map.json"),
        published: published.clone(),
        base: "cryptoemoji".into(),
    };
    let name = "cryptoemoji1_by_YourEmojiBot";
    _native::atomic::write_json(
        &files.state,
        &json!({"sets":[{"index":1,"name":name,"live":2}],
        "provider_in_flight":{"key":"aaa","operation":"add","set_name":name,"set_index":1,
            "expected_before":2,"source_dhash":want,"source_path":source},"provider_added":[]}),
    )
    .unwrap();
    _native::atomic::write_json(&files.mapping, &json!({"other":"preserved"})).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut methods = Vec::new();
        for index in 0..3 {
            let mut stream = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::yield_now()
                    }
                    Err(e) => panic!("provider recovery fixture: {e}"),
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
            let body = match index {
                0=>json!({"ok":true,"result":{"stickers":[{"custom_emoji_id":"old1"},{"custom_emoji_id":"old2"},{"file_id":"f","custom_emoji_id":"new"}]}}).to_string().into_bytes(),
                1=>json!({"ok":true,"result":{"file_path":"fixture.png"}}).to_string().into_bytes(),
                _=>bytes.clone(),
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
        assert_eq!(run(&files, &tg).unwrap(), Some("aaa".into()));
        let mapping: Value =
            serde_json::from_slice(&std::fs::read(&files.mapping).unwrap()).unwrap();
        assert_eq!(mapping, json!({"other":"preserved","aaa":"new"}));
        let state: Value = serde_json::from_slice(&std::fs::read(&files.state).unwrap()).unwrap();
        assert!(state["provider_in_flight"].is_null());
        assert_eq!(state["provider_added"], json!(["aaa"]));
        assert_eq!(run(&files, &tg).unwrap(), None);
        assert!(!source.exists());
        assert!(published.join("aaa.png").is_file());
    }));
    let methods = server.join().unwrap();
    assert!(
        methods[0].contains("/getStickerSet ")
            && methods[1].contains("/getFile ")
            && methods[2].contains("/fixture.png ")
    );
    assert!(
        !methods
            .iter()
            .any(|m| m.contains("addSticker") || m.contains("createNew"))
    );
    std::fs::remove_dir_all(&scratch).unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
