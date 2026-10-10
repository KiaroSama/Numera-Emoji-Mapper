//! Reconciliation preserves earlier identity-proven commits when a later duplicate refuses.
use _native::{
    catalog::Catalog, media_adapter::MediaAdapter, publication::Publisher, telegram::Telegram,
};
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[test]
fn duplicate_tail_refuses_without_overwriting_the_first_verified_id() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let root = project
        .join("logs")
        .join(format!("reconcile-commit-{}", std::process::id()));
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
                    assert!(Instant::now() < deadline);
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
        }
        let body = json!({"ok":true,"result":{"stickers":[
            {"file_unique_id":"known-one","custom_emoji_id":"111111111","file_id":"unused"},
            {"file_unique_id":"known-one","custom_emoji_id":"222222222","file_id":"unused"}]}})
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
        catalog.record_seen("known-one", "s:fixture").unwrap();
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
        let mut publisher = Publisher {
            tg: &tg,
            cat: &mut catalog,
            media: &media,
            data: root.clone(),
            runtime: &runtime,
            config: None,
        };
        let mut set = json!({"name":"fixture1_by_YourEmojiBot","keys":[],"live":0,"logo":false});
        let original = set.clone();
        assert!(publisher.reconcile(&mut set, "fixture").is_err());
        assert_eq!(set, original);
        assert_eq!(
            publisher.cat.get("s:fixture").unwrap().unwrap()["uploaded"],
            true,
            "earlier identity-proven progress must survive a later refusal"
        );
        assert_eq!(
            publisher
                .cat
                .dispatch(
                    json!({"operation":"custom_id","base":"fixture","content_key":"s:fixture"})
                )
                .unwrap(),
            json!("111111111")
        );
    }));
    server.join().unwrap();
    std::fs::remove_dir_all(root).unwrap();
    result.unwrap();
}
