//! Actual single replacement uploads immutable PNG, proves live art and repoints every alias.
use _native::coin_replace_disk::Files;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};
#[test]
fn native_replacement_uploads_once_and_promotes_only_content_proven_art() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let scratch = root
        .join("logs/tmp")
        .join(format!("replacement effects {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let images = scratch.join("logos/emoji");
    std::fs::create_dir_all(&images).unwrap();
    let bytes =
        std::fs::read(root.join("tests/fixtures/native/static-codec/alpha-edge.png")).unwrap();
    std::fs::write(images.join("aaa.png"), b"prior art").unwrap();
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
    _native::atomic::write_json(&files.state, &json!({"sets":[{"name":"s1","index":1}]})).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let intent_path = files.intent.clone();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut methods = Vec::new();
        let mut uploaded = Vec::new();
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
                    Err(e) => panic!("replacement effects fixture: {e}"),
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
                assert!(n > 0 && request.len() + n < 131072);
                request.extend_from_slice(&buffer[..n]);
                if let Some(index) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let head = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
            let length = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length:").map(str::trim))
                .unwrap_or("0")
                .parse::<usize>()
                .unwrap();
            while request.len() < end + length {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0 && request.len() + n < 131072);
                request.extend_from_slice(&buffer[..n]);
            }
            methods.push(
                String::from_utf8_lossy(&request)
                    .lines()
                    .next()
                    .unwrap()
                    .to_owned(),
            );
            let body=match index{
                0=>json!({"ok":true,"result":{"stickers":[{"custom_emoji_id":"a","file_id":"unchanged"},{"custom_emoji_id":"old","file_id":"old-file"}]}}).to_string().into_bytes(),
                1=>{
                    let intent:Value=serde_json::from_slice(&std::fs::read(&intent_path).unwrap()).unwrap();
                    assert_eq!(intent["old_cid"],"old");assert_eq!(intent["before"],json!(["a","old"]));
                    assert!(head.contains("multipart/form-data"));
                    let start=request.windows(8).position(|bytes|bytes==b"\x89PNG\r\n\x1a\n").unwrap();
                    let tail=request[start..].windows(8).position(|bytes|bytes==b"IEND\xaeB`\x82").unwrap()+start+8;
                    uploaded=request[start..tail].to_vec();
                    assert_eq!(uploaded,std::fs::read(intent["source"].as_str().unwrap()).unwrap());
                    json!({"ok":true,"result":true}).to_string().into_bytes()
                }
                2=>json!({"ok":true,"result":{"stickers":[{"custom_emoji_id":"a","file_id":"unchanged"},{"custom_emoji_id":"new","file_id":"f"}]}}).to_string().into_bytes(),
                3=>json!({"ok":true,"result":{"file_path":"fixture.png"}}).to_string().into_bytes(),
                _=>uploaded.clone(),
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        }
        (methods, uploaded)
    });
    let tg = _native::telegram::Telegram::new("synthetic-replacement-token".into(), base).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert!(
            _native::coin_replace_effects::replace(&files, &tg, "aaa", "AAA", &bytes, 111111111)
                .unwrap()
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(&files.map).unwrap()).unwrap(),
            json!({"aaa":"new","alias":"new","other":"keep"})
        );
        assert_eq!(
            std::fs::read(scratch.join("logos/png/aaa.png")).unwrap(),
            bytes
        );
        assert!(!files.intent.exists());
        assert_eq!(
            std::fs::read_dir(scratch.join("logos/.incoming"))
                .unwrap()
                .count(),
            0
        );
    }));
    let (methods, uploaded) = server.join().unwrap();
    assert_eq!(std::fs::read(images.join("aaa.png")).unwrap(), uploaded);
    assert_eq!(
        methods
            .iter()
            .filter(|method| method.contains("/replaceStickerInSet "))
            .count(),
        1
    );
    assert!(
        !methods
            .iter()
            .any(|method| method.contains("addSticker") || method.contains("createNew"))
    );
    std::fs::remove_dir_all(scratch).unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
