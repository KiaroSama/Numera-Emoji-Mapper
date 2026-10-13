//! Actual content-only command reads loopback, resumes signatures and replaces only admitted maps.
use _native::process_owner::OwnedProcess;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
#[test]
fn actual_remap_reuses_live_cache_and_applies_exact_content_with_original_backup() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let scratch = root
        .join("logs/tmp")
        .join(format!("coin remap cli {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    std::fs::create_dir(scratch.join("assets")).unwrap();
    std::fs::write(scratch.join("assets/panel.html"), b"fixture marker").unwrap();
    std::fs::write(scratch.join("pyproject.toml"), b"# fixture marker\n").unwrap();
    let images = scratch.join("images");
    std::fs::create_dir(&images).unwrap();
    let art =
        std::fs::read(root.join("tests/fixtures/native/static-codec/alpha-edge.png")).unwrap();
    std::fs::write(images.join("aaa.png"), &art).unwrap();
    let state = scratch.join("state.json");
    let cache = scratch.join("cache.json");
    let output = scratch.join("map.json");
    let candidate = scratch.join("candidate.json");
    _native::atomic::write_json(
        &state,
        &json!({"sets":[{"index":1,"name":"cryptoemoji1_by_YourEmojiBot"}]}),
    )
    .unwrap();
    let old = b"{\"aaa\":\"wrong\",\"unrelated\":\"backup only\"}\n";
    std::fs::write(&output, old).unwrap();
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
                    Err(e) => panic!("remap fixture: {e}"),
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
            let body=match index{0|3=>json!({"ok":true,"result":{"stickers":[{"custom_emoji_id":"111111111","file_unique_id":"u1","file_id":"f1"}]}}).to_string().into_bytes(),1=>json!({"ok":true,"result":{"file_path":"fixture.png"}}).to_string().into_bytes(),_=>art.clone()};
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
            let stdout = scratch.join(format!("out{iteration}.txt"));
            let stderr = scratch.join(format!("err{iteration}.txt"));
            let mut cmd = Command::new(&executable);
            cmd.arg("coin-remap-ids")
                .arg("--emoji-dir")
                .arg(&images)
                .arg("--state")
                .arg(&state)
                .arg("--cache")
                .arg(&cache)
                .arg("--out")
                .arg(&output)
                .arg("--candidates")
                .arg(&candidate)
                .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
                .env("NUMERA_TEST_API_BASE", &base)
                .env("COIN_PACK_BASE", "cryptoemoji")
                .env("TELEGRAM_BOT_TOKEN", "synthetic-remap-token")
                .stdin(Stdio::null())
                .stdout(Stdio::from(std::fs::File::create(&stdout).unwrap()))
                .stderr(Stdio::from(std::fs::File::create(&stderr).unwrap()));
            if iteration == 1 {
                cmd.args(["--max-distance", "1", "--apply"]);
            }
            let mut owner = OwnedProcess::spawn(&mut cmd, work).unwrap();
            let status = runtime
                .block_on(owner.wait(Duration::from_secs(10), Duration::from_secs(5)))
                .unwrap();
            owner.finish().unwrap();
            assert_eq!(
                status.code(),
                Some(0),
                "{}",
                std::fs::read_to_string(&stderr).unwrap()
            );
            let data: Value = serde_json::from_slice(&std::fs::read(&cache).unwrap()).unwrap();
            assert_eq!(
                STANDARD
                    .decode(data["sigs"]["111111111"].as_str().unwrap())
                    .unwrap()
                    .len(),
                1024
            );
            assert!(data["sets"]["cryptoemoji1_by_YourEmojiBot"].is_string());
            if iteration == 0 {
                assert_eq!(std::fs::read(&output).unwrap(), old);
                assert!(!output.with_extension("prebroken.json").exists());
            } else {
                assert_eq!(
                    serde_json::from_slice::<Value>(&std::fs::read(&output).unwrap()).unwrap(),
                    json!({"aaa":"111111111"})
                );
                assert_eq!(
                    std::fs::read(output.with_extension("prebroken.json")).unwrap(),
                    old
                );
            }
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
