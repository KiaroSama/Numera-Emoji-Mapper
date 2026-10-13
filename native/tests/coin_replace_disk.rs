//! Actual loopback recovery promotes recorded art and repoints all aliases without mutation.
use _native::coin_replace_disk::{Files, run};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};
#[test]
fn durable_recovery_keeps_target_binding_and_clears_only_after_proven_map_promotion() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let scratch = root
        .join("logs/tmp")
        .join(format!("coin replacement disk {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let images = scratch.join("logos/emoji");
    std::fs::create_dir_all(&images).unwrap();
    let stage = scratch.join("logos/.incoming/fixture");
    std::fs::create_dir_all(stage.join("png")).unwrap();
    let bytes =
        std::fs::read(root.join("tests/fixtures/native/static-codec/alpha-edge.png")).unwrap();
    let prepared = stage.join("aaa.png");
    let original = stage.join("png/aaa.png");
    std::fs::write(&prepared, &bytes).unwrap();
    std::fs::write(&original, &bytes).unwrap();
    let want = _native::codec_image::source_dhash(&prepared).unwrap();
    let files = Files {
        root: scratch.clone(),
        map: scratch.join("map.json"),
        state: scratch.join("state.json"),
        images: images.clone(),
        intent: scratch.join("intent.json"),
        base: "cryptoemoji".into(),
    };
    _native::atomic::write_json(
        &files.map,
        &json!({"aaa":"old","alias":"old","other":"keep"}),
    )
    .unwrap();
    _native::atomic::write_json(&files.state, &json!({"sets":[]})).unwrap();
    let target = |path: &std::path::Path| {
        let text = path.canonicalize().unwrap().to_string_lossy().into_owned();
        if cfg!(windows) {
            text.to_lowercase()
        } else {
            text
        }
    };
    let mut intent = json!({"key":"aaa","set_name":"s1","set_index":1,"old_cid":"old","before":["a","old"],"source_dhash":want,"source":prepared,"stage":stage,
        "map_target":"wrong","state_target":target(&files.state),"promote":[[original,scratch.join("logos/png/aaa.png")],[prepared,images.join("aaa.png")]]});
    _native::atomic::write_json(&files.intent, &intent).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut methods = Vec::new();
        for index in 0..3 {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::yield_now()
                    }
                    Err(e) => panic!("replacement fixture: {e}"),
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
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
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
            let body=match index{0=>json!({"ok":true,"result":{"stickers":[{"custom_emoji_id":"a","file_id":"unchanged"},{"custom_emoji_id":"new","file_id":"f"}]}}).to_string().into_bytes(),1=>json!({"ok":true,"result":{"file_path":"fixture.png"}}).to_string().into_bytes(),_=>bytes.clone()};
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
    let tg = _native::telegram::Telegram::new("synthetic-replacement-token".into(), base).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert!(!run(&files, &tg).unwrap());
        assert!(files.intent.exists() && stage.exists());
        intent["map_target"] = json!(target(&files.map));
        _native::atomic::write_json(&files.intent, &intent).unwrap();
        assert!(run(&files, &tg).unwrap());
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(&files.map).unwrap()).unwrap(),
            json!({"aaa":"new","alias":"new","other":"keep"})
        );
        assert_eq!(
            std::fs::read(images.join("aaa.png")).unwrap(),
            std::fs::read(root.join("tests/fixtures/native/static-codec/alpha-edge.png")).unwrap()
        );
        assert!(!files.intent.exists() && !stage.exists());
        assert!(run(&files, &tg).unwrap());
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
            .any(|method| method.contains("replaceSticker") || method.contains("addSticker"))
    );
    std::fs::remove_dir_all(scratch).unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
