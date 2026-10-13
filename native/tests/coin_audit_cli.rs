//! Actual audit command resumes cached IDs and uses shared pixel decoding on loopback bytes.
use _native::process_owner::OwnedProcess;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn actual_audit_downloads_only_missing_art_and_reports_current_live_positions() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let scratch = root
        .join("logs/tmp")
        .join(format!("coin audit cli {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    std::fs::create_dir(scratch.join("assets")).unwrap();
    std::fs::write(scratch.join("assets/panel.html"), b"fixture root marker").unwrap();
    std::fs::write(scratch.join("pyproject.toml"), b"# fixture root marker\n").unwrap();
    let coins = scratch.join("coins");
    std::fs::create_dir(&coins).unwrap();
    let state = coins.join("rebuild_dedup_state.json");
    _native::atomic::write_json(
        &state,
        &json!({"sets":[{"index":1,"name":"cryptoemoji1_by_YourEmojiBot"}]}),
    )
    .unwrap();
    let before_state = std::fs::read(&state).unwrap();
    _native::atomic::write_json(&coins.join("pack_audit4.json"), &json!({"a":{"hash":"old","blank":false},"gone":{"hash":"old","blank":false},"b":{"error":"retry this image"}})).unwrap();
    let bytes =
        std::fs::read(root.join("tests/fixtures/native/provider-image/visible-0-alpha-255.png"))
            .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut methods = Vec::new();
        for index in 0..4 {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::yield_now()
                    }
                    Err(e) => panic!("audit fixture: {e}"),
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
            let body=match index {
                0|3=>json!({"ok":true,"result":{"stickers":[{"custom_emoji_id":"a","file_id":"cached"},{"custom_emoji_id":"b","file_id":"download"}]}}).to_string().into_bytes(),
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
        for iteration in 0..2 {
            let work = scratch.join(format!("owner{iteration}"));
            std::fs::create_dir(&work).unwrap();
            let output = scratch.join(format!("out{iteration}.txt"));
            let errors = scratch.join(format!("err{iteration}.txt"));
            let mut command = Command::new(&executable);
            command
                .arg("coin-check-packs")
                .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
                .env("NUMERA_TEST_API_BASE", &base)
                .env("COIN_PACK_BASE", "cryptoemoji")
                .env("TELEGRAM_BOT_TOKEN", "synthetic-audit-token")
                .stdin(Stdio::null())
                .stdout(Stdio::from(std::fs::File::create(&output).unwrap()))
                .stderr(Stdio::from(std::fs::File::create(&errors).unwrap()));
            let mut owner = OwnedProcess::spawn(&mut command, work).unwrap();
            let code = runtime
                .block_on(owner.wait(Duration::from_secs(10), Duration::from_secs(5)))
                .unwrap()
                .code();
            owner.finish().unwrap();
            assert_eq!(
                code,
                Some(0),
                "{}",
                std::fs::read_to_string(&errors).unwrap()
            );
            let report = std::fs::read_to_string(coins.join("pack_audit4_report.txt")).unwrap();
            assert!(
                report.contains("live stickers: 2") && report.contains("blank cid=b set=1 pos=1")
            );
            assert!(report.contains("DUPLICATE image groups: 0"));
            let cache: Value =
                serde_json::from_slice(&std::fs::read(coins.join("pack_audit4.json")).unwrap())
                    .unwrap();
            assert!(cache.get("gone").is_none() && cache["b"].get("error").is_none());
            assert_eq!(cache["a"], json!({"hash":"old","blank":false}));
            assert_eq!(std::fs::read(&state).unwrap(), before_state);
        }
    }));
    let methods = server.join().unwrap();
    assert!(
        methods[0].contains("/getStickerSet ")
            && methods[1].contains("/getFile ")
            && methods[2].contains("/fixture.png ")
            && methods[3].contains("/getStickerSet ")
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
