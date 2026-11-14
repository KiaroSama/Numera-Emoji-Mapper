//! A real map writer lands after the command's first map read, before publisher admission.
use _native::{locks::Lease, logging::RunLog, tooling_process};
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

use crate::bot_api;
use bot_api::{Reply, Server};

const SET: &str = "cryptoemoji1_by_YourEmojiBot";

fn document(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn busy(path: &Path) {
    let error = Lease::acquire(path, "map race competitor")
        .err()
        .expect("competing writer acquired a publication lease");
    assert!(error.to_string().starts_with("OS lock acquisition failed:"));
    #[cfg(windows)]
    {
        let expected = std::io::Error::from_raw_os_error(
            windows_sys::Win32::Foundation::ERROR_LOCK_VIOLATION as i32,
        );
        assert_eq!(
            error.to_string(),
            format!("OS lock acquisition failed: {expected}")
        );
    }
    #[cfg(not(windows))]
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock, "{error}");
}
fn listing(count: usize) -> Reply {
    let stickers = (1..=count).map(|n| json!({"custom_emoji_id":format!("c{n}"),"file_id":format!("f{n}"),"file_unique_id":format!("u{n}")})).collect::<Vec<_>>();
    Reply::new(
        "getStickerSet",
        &[("name", SET)],
        json!({"ok":true,"result":{"stickers":stickers}}),
    )
}

pub fn canonical_map_updates_between_download_and_publication_are_preserved(
    root: &Path,
    executable: &Path,
    runtime: &tokio::runtime::Runtime,
    bytes: &[u8],
) {
    let pinned: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/provider-image.json"
    ))
    .unwrap();
    let fixture = std::env::var_os("NUMERA_PROVIDER_IMAGE_ORACLE")
        .map(|path| document(Path::new(&path)))
        .unwrap_or(pinned);
    let (static_fixture, _) = _native::source_static_oracle::fixtures().unwrap();
    _native::source_static_oracle::fixture_identity(&static_fixture, &fixture).unwrap();
    let art = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "visible-16-alpha-128")
        .unwrap();
    assert_eq!(super::digest(bytes), art["input_sha256"]);
    let input = root.join("map-race-eof");
    std::fs::write(&input, b"").unwrap();
    let log = RunLog::open(
        root,
        "coin-provider-map-race-owner",
        vec![bot_api::TOKEN.into()],
    );
    for scenario in ["other-writer", "already-mapped", "rest-of-batch"] {
        let directory = root.join(format!("provider-map-race-{scenario}"));
        std::fs::create_dir(&directory).unwrap();
        let map = directory.join("ticker_to_id.json");
        let state = directory.join("rebuild_dedup_state.json");
        let images = directory.join("logos/emoji");
        std::fs::create_dir_all(&images).unwrap();
        std::fs::write(images.join("aaa.png"), b"the already published oracle").unwrap();
        _native::atomic::write_json(&map, &json!({"btc":"c-btc"})).unwrap();
        _native::atomic::write_json(&state, &json!({"sets":[{"index":1,"name":SET,"title":"Title 1","live":2}],"provider_added":[]})).unwrap();
        let state_before = std::fs::read(&state).unwrap();
        let original_map = std::fs::read(&map).unwrap();
        let batch = scenario == "rest-of-batch";
        let skip = scenario == "already-mapped";
        let ticker = if batch { "bbb" } else { "aaa" };
        let mut cache = json!({"aaa":{"id":"alpha-coin","conf":"symbol","name":"Alpha Coin"}});
        if batch {
            cache["bbb"] = json!({"id":"alpha-coin","conf":"symbol","name":"Beta Coin"});
        }
        _native::atomic::write_json(&directory.join("paprika_matches.json"), &cache).unwrap();
        let inventory = if batch {
            "## AAA - Alpha Coin\nticker: aaa\npremium-id:\n## BBB - Beta Coin\nticker: bbb\npremium-id:\n"
        } else {
            "## AAA - Alpha Coin\nticker: aaa\npremium-id:\n"
        };
        std::fs::write(directory.join("currency-emoji-inventory.md"), inventory).unwrap();
        let mut concurrent = json!({"btc":"c-btc","zzz":"written-by-the-other-tool"});
        if scenario != "other-writer" {
            concurrent["aaa"] = json!("c-from-the-other-provider");
        }
        let mut image = Reply::new("GET /coin/alpha-coin/logo.png", &[], Value::Null);
        image.download = Some(bytes.to_vec());
        let (image, image_ready, image_release) = image.hold();
        let (me, me_ready, me_release) = Reply::new(
            "getMe",
            &[],
            json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
        )
        .hold();
        let mut replies = vec![image];
        if batch {
            let mut image = Reply::new("GET /coin/alpha-coin/logo.png", &[], Value::Null);
            image.download = Some(bytes.to_vec());
            replies.push(image);
        }
        replies.push(me);
        let mut mutation_signal = None;
        let mut proof_signal = None;
        if !skip {
            replies.extend([listing(2), listing(2)]);
            let sticker = json!({"sticker":"attach://file0","format":"static","emoji_list":["🪙"],"keywords":[ticker]}).to_string();
            let mut add = Reply::new(
                "addStickerToSet",
                &[
                    ("name", SET),
                    ("user_id", "111111111"),
                    ("sticker", &sticker),
                ],
                json!({"ok":true,"result":true}),
            );
            add.upload_rgba
                .insert("file0".into(), art["expected"].clone());
            add.observation = Some((
                state.clone(),
                "/provider_in_flight/key".into(),
                json!(ticker),
            ));
            let (add, ready, release) = add.hold();
            mutation_signal = Some((ready, release));
            replies.extend([
                add,
                listing(3),
                Reply::new(
                    "getFile",
                    &[("file_id", "f3")],
                    json!({"ok":true,"result":{"file_path":"race.png"}}),
                ),
            ]);
            let mut download = Reply::new("getFile", &[], Value::Null);
            download.method = "GET race.png".into();
            download.download_upload = Some("file0".into());
            let (download, ready, release) = download.hold();
            proof_signal = Some((ready, release));
            replies.push(download);
        }
        let mut server = Server::new(replies);
        let mut environment =
            _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
        for (key, value) in [
            ("NUMERA_TEST_API_BASE", server.base.as_str()),
            ("NUMERA_TEST_PROVIDER_BASE", server.base.as_str()),
            ("TELEGRAM_BOT_TOKEN", bot_api::TOKEN),
            ("COIN_PACK_BASE", "cryptoemoji"),
            ("COIN_PACK_TITLE", "Title"),
            ("PACK_OWNER_USER_ID", "111111111"),
        ] {
            environment.insert(key.into(), value.into());
        }
        let argv = vec![
            executable.as_os_str().to_owned(),
            "coin-fetch-paprika".into(),
            "--coin-dir".into(),
            directory.as_os_str().to_owned(),
        ];
        // Literal coordinates are independent of the function used by the publisher.
        let family = root.join(".locks/pack_cryptoemoji.lock");
        let canonical = root.join(".locks/canonical_map.lock");
        let unrelated = Lease::acquire(
            &root.join(".locks/pack_othercoins.lock"),
            "unrelated family stays live",
        )
        .unwrap();
        let (result, observed) = std::thread::scope(|scope| {
            let observer = scope.spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                let wait = |ready: tokio::sync::oneshot::Receiver<()>| {
                    runtime.block_on(async {
                        tokio::time::timeout(Duration::from_secs(5), ready)
                            .await
                            .unwrap()
                            .unwrap()
                    })
                };
                wait(image_ready);
                assert_eq!(std::fs::read(&map).unwrap(), original_map);
                // The download seam follows the stale read, not the internal lock-acquisition call.
                drop(Lease::acquire(&family, "publisher has not entered yet").unwrap());
                let writer = Lease::acquire(&canonical, "independent map writer").unwrap();
                _native::atomic::write_json(&map, &concurrent).unwrap();
                let landed = std::fs::read(&map).unwrap();
                drop(writer);
                image_release.send(()).unwrap();
                wait(me_ready);
                busy(&canonical);
                busy(&family);
                assert_eq!(std::fs::read(&map).unwrap(), landed);
                me_release.send(()).unwrap();
                for (ready, release) in [mutation_signal, proof_signal].into_iter().flatten() {
                    wait(ready);
                    busy(&canonical);
                    busy(&family);
                    assert_eq!(std::fs::read(&map).unwrap(), landed);
                    assert_eq!(document(&state)["provider_in_flight"]["key"], ticker);
                    assert_eq!(
                        std::fs::read(images.join("aaa.png")).unwrap(),
                        b"the already published oracle"
                    );
                    release.send(()).unwrap();
                }
            });
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.block_on(tooling_process::capture_with_input(
                    root,
                    &argv,
                    &environment,
                    &input,
                    Duration::from_secs(10),
                    Duration::from_secs(5),
                    &log,
                ))
            }));
            (result, observer.join())
        });
        let requests = server.finish();
        observed.unwrap();
        let result = result
            .unwrap_or_else(|error| std::panic::resume_unwind(error))
            .unwrap();
        let requests = requests.unwrap();
        assert_eq!(
            result.code, 0,
            "{scenario}: {} {}",
            result.stdout, result.stderr
        );
        assert!(!result.stdout.contains(bot_api::TOKEN) && !result.stderr.contains(bot_api::TOKEN));
        let adds = requests
            .iter()
            .filter(|r| r.method == "addStickerToSet")
            .collect::<Vec<_>>();
        assert_eq!(adds.len(), usize::from(!skip));
        assert_eq!(
            requests.len(),
            if skip {
                2
            } else if batch {
                9
            } else {
                8
            }
        );
        assert!(!requests.iter().any(|r| r.method == "createNewStickerSet"));
        let mut expected = concurrent;
        if !skip {
            expected[ticker] = json!("c3");
        }
        assert_eq!(document(&map), expected);
        let filled =
            std::fs::read_to_string(directory.join("currency-emoji-inventory.filled.md")).unwrap();
        assert!(filled.contains(if scenario == "other-writer" {
            "premium-id: c3"
        } else {
            "premium-id: c-from-the-other-provider"
        }));
        if batch {
            assert!(filled.contains("premium-id: c3"));
        }
        assert_eq!(
            std::fs::read(directory.join("currency-emoji-inventory.md")).unwrap(),
            inventory.as_bytes()
        );
        assert_eq!(document(&directory.join("paprika_matches.json")), cache);
        assert!(document(&state)["provider_in_flight"].is_null());
        assert_eq!(
            document(&state)["provider_added"],
            if skip { json!([]) } else { json!([ticker]) }
        );
        if skip {
            assert_eq!(std::fs::read(&state).unwrap(), state_before);
            assert_eq!(
                std::fs::read(images.join("aaa.png")).unwrap(),
                b"the already published oracle"
            );
        } else {
            bot_api::assert_rgba(
                &std::fs::read(images.join(format!("{ticker}.png"))).unwrap(),
                &art["expected"],
            );
            if batch {
                assert_eq!(
                    std::fs::read(images.join("aaa.png")).unwrap(),
                    b"the already published oracle"
                );
            }
        }
        assert!(
            std::fs::read_dir(directory.join("logos/.incoming"))
                .unwrap()
                .next()
                .is_none()
        );
        drop(unrelated);
        for path in [&family, &canonical] {
            drop(Lease::acquire(path, "publication released").unwrap());
            assert!(path.is_file());
        }
        assert!(!_native::process_owner::cleanup_pending(root));
        assert!(
            std::fs::read_dir(root.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none()
        );
    }
    log.finish(0);
}
