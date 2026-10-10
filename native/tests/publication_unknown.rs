//! Missing recorded and unknown live sets retain intent and cannot reach mutation calls.
use _native::{
    catalog::Catalog, media_adapter::MediaAdapter, publication::Publisher, publication_recovery,
    publish_collection::Options, telegram::Telegram,
};
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[test]
fn unknown_or_missing_recorded_set_retains_original_upload_intent() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    for missing in [false, true] {
        let root = project.join("logs").join(format!(
            "publication-unknown-{}-{missing}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let (mut socket, _) = loop {
                match listener.accept() {
                    Ok(value) => break value,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "probe did not arrive");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 1024];
            while !bytes.windows(4).any(|part| part == b"\r\n\r\n") {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                assert!(bytes.len() < 16384);
            }
            let request = String::from_utf8(bytes).unwrap();
            assert!(request.lines().next().unwrap().contains("/getStickerSet"));
            let body = if missing {
                json!({"ok":false,"description":"STICKERSET_INVALID"})
            } else {
                json!([])
            }
            .to_string();
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut catalog = Catalog::open(&root.join("catalog.db"), "fixture", &project).unwrap();
            catalog.insert(&json!({"content_key":"s:fixture","fmt":"static","file_path":root.join("unused.png")}),"fixture").unwrap();
            let tg = Telegram::new("test-only-token".into(), base).unwrap();
            let media = MediaAdapter {
                python: "must-not-run".into(),
                root: project.clone(),
                scratch: root.join("codec"),
            };
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let options = Options {
                base: "fixture".into(),
                title: "Fixture".into(),
                bot: "YourEmojiBot".into(),
                user_id: 111111111,
                fmt: "mixed".into(),
                per_set: 200,
                new_set: false,
                into_pack: None,
                repaint: false,
                default_emoji: "😀".into(),
                logo: None,
            };
            let mut state = json!({"base":"fixture","per_set":200,"sets":[{"name":"fixture1_by_YourEmojiBot","index":1,"fmt":"mixed","keys":[],"live":0,"logo":false}],
                "in_flight":{"key":"s:fixture","set_name":"fixture1_by_YourEmojiBot","set_index":1,"operation":"add","expected_before":0}});
            let path = root.join("state.json");
            _native::atomic::write_json(&path, &state).unwrap();
            let original = std::fs::read(&path).unwrap();
            let mut publisher = Publisher {
                tg: &tg,
                cat: &mut catalog,
                media: &media,
                data: root.clone(),
                runtime: &runtime,
                config: None,
            };
            assert!(
                publication_recovery::recover(&mut publisher, &options, &mut state, &path).is_err()
            );
            assert_eq!(std::fs::read(path).unwrap(), original);
            assert_eq!(
                publisher.cat.get("s:fixture").unwrap().unwrap()["uploaded"],
                false
            );
            assert!(!root.join("codec").exists());
        }));
        server.join().unwrap();
        std::fs::remove_dir_all(root).unwrap();
        result.unwrap();
    }
}
