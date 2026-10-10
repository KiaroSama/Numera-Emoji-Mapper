//! Recovery observes live identity and commits an applied add without sending it again.
use _native::{
    catalog::Catalog, media_adapter::MediaAdapter, publication::Publisher,
    publication_recovery::recover, publish_collection::Options, telegram::Telegram,
};
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[test]
fn applied_add_recovers_by_known_identity_without_second_mutation() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let directory = root
        .join("logs")
        .join(format!("recovery-test-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut methods = Vec::new();
        while methods.len() < 2 {
            let (mut stream, _) = match listener.accept() {
                Ok(value) => value,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "recovery API did not finish");
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 1024];
            while !bytes.windows(4).any(|b| b == b"\r\n\r\n") {
                let size = stream.read(&mut buffer).unwrap();
                assert!(size > 0);
                bytes.extend_from_slice(&buffer[..size]);
                assert!(bytes.len() < 16384);
            }
            let text = String::from_utf8(bytes).unwrap();
            methods.push(text.lines().next().unwrap().to_owned());
            let body = json!({"ok":true,"result":{"name":"fixture1_by_YourEmojiBot","stickers":[
                {"file_id":"unused-download","file_unique_id":"known-new-file","custom_emoji_id":"111111111"}]}}).to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        }
        methods
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut catalog =
            Catalog::open(&directory.join("catalog.db"), "fixture UTC", &root).unwrap();
        catalog.insert(&json!({"content_key":"s:fixture","fmt":"static","file_path":directory.join("unused.png")}),"fixture UTC").unwrap();
        catalog.record_seen("known-new-file", "s:fixture").unwrap();
        let tg = Telegram::new("test-only-token".into(), base).unwrap();
        let media = MediaAdapter {
            python: PathBuf::from("must-not-start"),
            root: root.clone(),
            scratch: directory.join("codec"),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut publisher = Publisher {
            tg: &tg,
            cat: &mut catalog,
            media: &media,
            data: directory.clone(),
            runtime: &runtime,
            config: None,
        };
        let options = Options {
            base: "fixture".into(),
            title: "Fixture".into(),
            bot: "YourEmojiBot".into(),
            user_id: 111111111,
            fmt: "mixed".into(),
            per_set: 200,
            new_set: false,
            into_pack: Some(1),
            repaint: false,
            default_emoji: "😀".into(),
            logo: None,
        };
        let mut state = json!({"base":"fixture","sets":[{"name":"fixture1_by_YourEmojiBot","fmt":"mixed","index":1,
            "keys":[],"live":0,"logo":false}],"sent":[],"sent_full":[],"skipped":[],
            "in_flight":{"key":"s:fixture","operation":"add","set_name":"fixture1_by_YourEmojiBot","set_index":1}});
        let path = directory.join("publish_fixture.json");
        let before = state.clone();
        let mut foreign = state.clone();
        foreign["in_flight"]["set_name"] = json!("other1_by_YourEmojiBot");
        assert!(
            recover(&mut publisher, &options, &mut foreign, &path)
                .unwrap_err()
                .contains("another pack family")
        );
        assert_eq!(foreign["sets"], before["sets"]);
        assert!(!path.exists(), "invalid intent wrote durable state");
        recover(&mut publisher, &options, &mut state, &path).unwrap();
        assert_eq!(state["sets"][0]["keys"], json!(["s:fixture"]));
        assert_eq!(state["sets"][0]["live"], 1);
        assert!(state["in_flight"].is_null());
        assert_eq!(
            publisher
                .cat
                .dispatch(
                    json!({"operation":"custom_id","base":"fixture","content_key":"s:fixture"})
                )
                .unwrap(),
            "111111111"
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).unwrap()).unwrap(),
            state
        );
        assert!(
            !directory.join("codec").exists(),
            "known identity needlessly invoked media runtime"
        );
    }));
    let methods = server.join().unwrap();
    assert!(methods.iter().all(|m| m.contains("/getStickerSet ")));
    std::fs::remove_dir_all(&directory).unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
