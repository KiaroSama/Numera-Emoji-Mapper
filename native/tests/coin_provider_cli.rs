//! Source assertions through real provider entry points, including durable unknown-upload restart.
use _native::process_owner::OwnedProcess;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[path = "support/coin_cli_args.rs"]
mod coin_cli_args;
#[path = "support/coin_provider_map_race.rs"]
mod coin_provider_map_race;

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn document(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn request(listener: &TcpListener, deadline: Instant) -> (TcpStream, Vec<u8>) {
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                std::thread::yield_now();
            }
            Err(e) => panic!("provider CLI fixture accept: {e}"),
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    let end = loop {
        let n = stream.read(&mut buffer).unwrap();
        assert!(n > 0 && bytes.len() + n <= 131072);
        bytes.extend_from_slice(&buffer[..n]);
        if let Some(i) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let header = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
    let size = header
        .lines()
        .find_map(|line| line.strip_prefix("content-length:").map(str::trim))
        .unwrap_or("0")
        .parse::<usize>()
        .unwrap();
    assert!(end + size <= 131072);
    while bytes.len() < end + size {
        let n = stream.read(&mut buffer).unwrap();
        assert!(n > 0 && bytes.len() + n <= 131072);
        bytes.extend_from_slice(&buffer[..n]);
    }
    (stream, bytes)
}

fn invoke(
    executable: &Path,
    directory: &Path,
    base: &str,
    provider: &str,
    extra: &[&str],
    run: usize,
    runtime: &tokio::runtime::Runtime,
) -> (i32, String, String) {
    let work = directory.join(format!("owner{run}"));
    std::fs::create_dir(&work).unwrap();
    let output = directory.join(format!("out{run}.txt"));
    let errors = directory.join(format!("err{run}.txt"));
    let mut cmd = Command::new(executable);
    cmd.arg(format!("coin-fetch-{provider}"))
        .args(extra)
        .arg("--coin-dir")
        .arg(directory)
        .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
        .env("NUMERA_TEST_PROVIDER_BASE", base)
        .env("NUMERA_TEST_API_BASE", base)
        .env("COIN_PACK_BASE", "cryptoemoji")
        .env("COIN_PACK_TITLE", "Fixture")
        .env("PACK_OWNER_USER_ID", "111111111")
        .stdin(Stdio::null())
        .stdout(Stdio::from(std::fs::File::create(&output).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&errors).unwrap()));
    for (key, _) in std::env::vars() {
        if ["TOKEN", "SECRET", "PASSWORD", "API_KEY"]
            .iter()
            .any(|part| key.contains(part))
        {
            cmd.env_remove(key);
        }
    }
    cmd.env_remove("TELEGRAM_API_BASE")
        .env_remove("PACK_LINKS_CHAT_ID")
        .env_remove("WORKER_PUBLISH_URL");
    if !extra.contains(&"--dry") {
        cmd.env("TELEGRAM_BOT_TOKEN", "synthetic-provider-cli-token");
    }
    if provider == "cmc" {
        cmd.env("CMC_API_KEY", "synthetic-provider-cli-key");
    }
    let mut owner = OwnedProcess::spawn(&mut cmd, work.clone()).unwrap();
    let status = runtime
        .block_on(owner.wait(Duration::from_secs(10), Duration::from_secs(5)))
        .unwrap();
    owner.finish().unwrap();
    assert!(!work.exists());
    let out = std::fs::read_to_string(output).unwrap();
    let err = std::fs::read_to_string(errors).unwrap();
    assert!(
        !out.contains("synthetic-provider-cli-token")
            && !err.contains("synthetic-provider-cli-token")
    );
    assert!(
        !out.contains("synthetic-provider-cli-key") && !err.contains("synthetic-provider-cli-key")
    );
    (status.code().unwrap(), out, err)
}

#[test]
fn actual_provider_commands_cache_publish_refuse_and_restart_without_a_second_upload() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let scratch = root
        .join("logs/tmp")
        .join(format!("coin provider cli {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    std::fs::create_dir(scratch.join("assets")).unwrap();
    std::fs::write(scratch.join("assets/panel.html"), b"fixture marker").unwrap();
    std::fs::write(scratch.join("pyproject.toml"), b"# fixture marker\n").unwrap();
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
    let images: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/provider-image.json"
    ))
    .unwrap();
    let image = images["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "visible-16-alpha-128")
        .unwrap();
    let art = std::fs::read(root.join(image["input"].as_str().unwrap())).unwrap();
    assert_eq!(digest(&art), image["input_sha256"]);
    let blank =
        std::fs::read(root.join("tests/fixtures/native/provider-image/visible-0-alpha-255.png"))
            .unwrap();
    let changed_art =
        std::fs::read(root.join("tests/fixtures/native/provider-image/visible-9-alpha-255.png"))
            .unwrap();
    assert_ne!(digest(&changed_art), digest(&art));
    let publication: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/provider-publish.json"
    ))
    .unwrap();
    assert_eq!(
        publication["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let cases = publication["cases"].as_array().unwrap();
    for scenario in ["paprika", "cmc", "rejected", "blank", "restart", "quota"] {
        let directory = scratch.join(scenario);
        std::fs::create_dir(&directory).unwrap();
        let state = directory.join("rebuild_dedup_state.json");
        let mapping = directory.join("ticker_to_id.json");
        let cache = directory.join("paprika_matches.json");
        let initial = json!({"sets":[{"index":1,"name":"cryptoemoji1_by_YourEmojiBot","title":"Fixture 1","live":2}],"provider_added":[]});
        _native::atomic::write_json(&state, &initial).unwrap();
        _native::atomic::write_json(&mapping, &json!({"other":"preserved"})).unwrap();
        _native::atomic::write_json(
            &cache,
            &if matches!(scenario, "paprika" | "quota") {
                json!({})
            } else {
                json!({"aaa":{"id":"alpha-coin","conf":"symbol","name":"Alpha Coin"}})
            },
        )
        .unwrap();
        std::fs::write(
            directory.join("currency-emoji-inventory.md"),
            b"## AAA - Alpha Coin\nticker: aaa\npremium-id:\n",
        )
        .unwrap();
        let original_state = std::fs::read(&state).unwrap();
        let original_map = std::fs::read(&mapping).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server_directory = directory.clone();
        let server_root = scratch.clone();
        let good = art.clone();
        let bad = blank.clone();
        let fresh = changed_art.clone();
        let png_hash = image["expected"]["png_sha256"].as_str().unwrap().to_owned();
        let sequence: Vec<&str> = match scenario {
            "paprika" => vec![
                "search",
                "logo",
                "getMe",
                "getStickerSet",
                "getStickerSet",
                "addStickerToSet",
                "getStickerSet",
                "getFile",
                "fixture.png",
            ],
            "cmc" => vec![
                "map",
                "info",
                "logo",
                "getMe",
                "getStickerSet",
                "getStickerSet",
                "addStickerToSet",
                "getStickerSet",
                "getFile",
                "fixture.png",
            ],
            "rejected" => vec![
                "logo",
                "getMe",
                "getStickerSet",
                "getStickerSet",
                "addStickerToSet",
            ],
            "blank" => vec!["logo"],
            "restart" => vec![
                "logo",
                "getMe",
                "getStickerSet",
                "getStickerSet",
                "addStickerToSet",
                "getStickerSet",
                "logo",
                "getMe",
                "getStickerSet",
                "getFile",
                "fixture.png",
            ],
            _ => vec!["search"],
        };
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(15);
            let mut calls = Vec::new();
            let mut probes = 0;
            let mut uploaded = Vec::new();
            let mut downloads = 0;
            for method in sequence {
                let (mut stream, bytes) = request(&listener, deadline);
                let first = String::from_utf8_lossy(&bytes)
                    .lines()
                    .next()
                    .unwrap()
                    .to_owned();
                calls.push(first.clone());
                let body = match method {
                    "search" => {
                        assert_eq!(
                            first,
                            "GET /v1/search?c=currencies&limit=10&q=Alpha+Coin HTTP/1.1"
                        );
                        if scenario == "quota" {
                            write!(stream, "HTTP/1.1 402 Quota\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                            continue;
                        }
                        json!({"currencies":[{"id":"alpha-coin","name":"Alpha Coin","symbol":"AAA"}]}).to_string().into_bytes()
                    }
                    "map" => {
                        assert_eq!(first, "GET /v1/cryptocurrency/map?symbol=AAA HTTP/1.1");
                        assert!(
                            String::from_utf8_lossy(&bytes)
                                .to_ascii_lowercase()
                                .contains("x-cmc_pro_api_key: synthetic-provider-cli-key")
                        );
                        json!({"data":[{"id":7,"name":"Alpha Coin","symbol":"AAA"}]})
                            .to_string()
                            .into_bytes()
                    }
                    "info" => {
                        assert_eq!(first, "GET /v2/cryptocurrency/info?id=7 HTTP/1.1");
                        // Even a provider-supplied remote CDN URL must hit this fixture, never DNS.
                        json!({"data":{"7":{"logo":"https://images.example.invalid/64x64/7.png?version=1"}}}).to_string().into_bytes()
                    }
                    "logo" => {
                        assert_eq!(
                            first,
                            if scenario == "cmc" {
                                "GET /128x128/7.png?version=1 HTTP/1.1"
                            } else {
                                "GET /coin/alpha-coin/logo.png HTTP/1.1"
                            }
                        );
                        downloads += 1;
                        if scenario == "blank" {
                            bad.clone()
                        } else if scenario == "restart" && downloads == 2 {
                            fresh.clone()
                        } else {
                            good.clone()
                        }
                    }
                    "getMe" => json!({"ok":true,"result":{"username":"YourEmojiBot"}})
                        .to_string()
                        .into_bytes(),
                    "getStickerSet" => {
                        probes += 1;
                        assert!(
                            _native::locks::Lease::acquire(
                                &_native::locks::pack_family_path(&server_root, "cryptoemoji"),
                                "fixture"
                            )
                            .is_err()
                        );
                        assert!(
                            _native::locks::Lease::acquire(
                                &server_root.join(".locks/canonical_map.lock"),
                                "fixture"
                            )
                            .is_err()
                        );
                        if scenario == "restart" && probes == 3 {
                            json!({"ok":true,"result":[]}).to_string().into_bytes()
                        } else {
                            let mut stickers = vec![
                                json!({"file_id":"old1","file_unique_id":"old-u1","custom_emoji_id":"old-c1"}),
                                json!({"file_id":"old2","file_unique_id":"old-u2","custom_emoji_id":"old-c2"}),
                            ];
                            if probes >= 3 {
                                stickers.push(json!({"file_id":"new","file_unique_id":"new-u","custom_emoji_id":"cid"}));
                            }
                            json!({"ok":true,"result":{"stickers":stickers}})
                                .to_string()
                                .into_bytes()
                        }
                    }
                    "addStickerToSet" => {
                        let intent = document(&server_directory.join("rebuild_dedup_state.json"))["provider_in_flight"].clone();
                        assert_eq!(intent["key"], "aaa");
                        assert_eq!(intent["operation"], "add");
                        assert_eq!(intent["expected_before"], 2);
                        assert_eq!(intent["set_name"], "cryptoemoji1_by_YourEmojiBot");
                        uploaded = std::fs::read(intent["source_path"].as_str().unwrap()).unwrap();
                        assert_eq!(digest(&uploaded), png_hash);
                        assert!(bytes.windows(uploaded.len()).any(|part| part == uploaded));
                        assert_eq!(
                            document(&server_directory.join("ticker_to_id.json")),
                            json!({"other":"preserved"})
                        );
                        if scenario == "rejected" {
                            json!({"ok":false,"description":"STICKER_PNG_NOPNG"})
                        } else if scenario == "restart" {
                            json!({"ok":true})
                        } else {
                            json!({"ok":true,"result":true})
                        }
                        .to_string()
                        .into_bytes()
                    }
                    "getFile" => json!({"ok":true,"result":{"file_path":"fixture.png"}})
                        .to_string()
                        .into_bytes(),
                    _ => uploaded.clone(),
                };
                if !matches!(method, "search" | "map" | "info" | "logo") {
                    assert!(first.contains(&format!("/{method} ")), "{first}");
                }
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            }
            calls
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let provider = if scenario == "cmc" { "cmc" } else { "paprika" };
            if scenario == "paprika" {
                let result = invoke(
                    &executable,
                    &directory,
                    &base,
                    provider,
                    &["--dry"],
                    0,
                    &runtime,
                );
                assert_eq!(result.0, 0, "{}", result.2);
                assert_eq!(std::fs::read(&state).unwrap(), original_state);
                assert_eq!(std::fs::read(&mapping).unwrap(), original_map);
                assert_eq!(document(&cache)["aaa"]["id"], "alpha-coin");
                assert!(!directory.join("logos").exists());
            }
            let first = invoke(&executable, &directory, &base, provider, &[], 1, &runtime);
            let expected = if matches!(scenario, "rejected" | "blank" | "restart") {
                4
            } else {
                0
            };
            assert_eq!(first.0, expected, "scenario={scenario}: {}", first.2);
            if scenario == "quota" || scenario == "blank" {
                assert_eq!(std::fs::read(&state).unwrap(), original_state);
                assert_eq!(std::fs::read(&mapping).unwrap(), original_map);
                if scenario == "quota" {
                    assert_eq!(document(&cache), json!({}));
                }
                return;
            }
            if scenario == "rejected" {
                assert_eq!(document(&mapping), cases[5]["expected"]["mapping"]);
                assert!(document(&state)["provider_in_flight"].is_null());
                assert!(!directory.join("logos/emoji/aaa.png").exists());
                return;
            }
            if scenario == "restart" {
                let intent = document(&state)["provider_in_flight"].clone();
                let retained = PathBuf::from(intent["source_path"].as_str().unwrap());
                assert!(retained.is_file());
                assert_eq!(
                    digest(&std::fs::read(&retained).unwrap()),
                    image["expected"]["png_sha256"]
                );
                assert_eq!(document(&mapping), cases[3]["expected"]["mapping"]);
                assert!(!directory.join("logos/emoji/aaa.png").exists());
                let second = invoke(&executable, &directory, &base, provider, &[], 2, &runtime);
                assert_eq!(second.0, 0, "{}", second.2);
                assert!(!retained.exists());
            } else if scenario == "paprika" {
                let before = std::fs::read(&state).unwrap();
                let second = invoke(&executable, &directory, &base, provider, &[], 2, &runtime);
                assert_eq!(second.0, 0, "{}", second.2);
                assert_eq!(std::fs::read(&state).unwrap(), before);
            }
            assert_eq!(document(&mapping), cases[0]["expected"]["mapping"]);
            let saved = document(&state);
            assert!(saved["provider_in_flight"].is_null());
            assert_eq!(saved["provider_added"], json!(["aaa"]));
            assert_eq!(saved["sets"], initial["sets"]);
            assert_eq!(
                digest(&std::fs::read(directory.join("logos/emoji/aaa.png")).unwrap()),
                image["expected"]["png_sha256"]
            );
            assert!(
                std::fs::read_to_string(directory.join("currency-emoji-inventory.filled.md"))
                    .unwrap()
                    .contains("premium-id: cid")
            );
        }));
        let joined = server.join();
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
        let calls = joined.unwrap();
        assert_eq!(
            calls
                .iter()
                .filter(|line| line.contains("/addStickerToSet "))
                .count(),
            usize::from(matches!(
                scenario,
                "paprika" | "cmc" | "rejected" | "restart"
            ))
        );
        assert!(
            !calls
                .iter()
                .any(|line| line.contains("/createNewStickerSet "))
        );
        if directory.join("logos/.incoming").exists() {
            for entry in std::fs::read_dir(directory.join("logos/.incoming")).unwrap() {
                assert_eq!(std::fs::read_dir(entry.unwrap().path()).unwrap().count(), 0);
            }
        }
    }
    coin_cli_args::check(&scratch, &executable, &runtime);
    coin_provider_map_race::canonical_map_updates_between_download_and_publication_are_preserved(
        &scratch,
        &executable,
        &runtime,
        &art,
    );
    for lock in [
        _native::locks::pack_family_path(&scratch, "cryptoemoji"),
        scratch.join(".locks/canonical_map.lock"),
    ] {
        let lease = _native::locks::Lease::acquire(&lock, "fixture release").unwrap();
        drop(lease);
    }
    std::fs::remove_dir_all(scratch).unwrap();
}
