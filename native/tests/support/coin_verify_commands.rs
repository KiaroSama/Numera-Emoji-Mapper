//! Original verification configuration and no-market failure outcomes at the copied command seam.
use super::{Harness, Reply, Server, document, released};
use serde_json::json;

pub(super) fn busy(path: &std::path::Path) {
    let error = _native::locks::Lease::acquire(path, "repair map-only competitor")
        .err()
        .expect("competing writer acquired repair ownership");
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

pub(super) fn check(harness: &Harness<'_>) {
    let root = harness.root;
    for scenario in [
        "missing-owner",
        "invalid-owner",
        "missing-token",
        "unreadable-state",
        "not-repaired",
    ] {
        let directory = root.join(format!("verify-{scenario}"));
        let images = directory.join("emoji");
        std::fs::create_dir_all(&images).unwrap();
        let state = directory.join("state.json");
        let map = directory.join("map.json");
        let state_bytes: &[u8] = if scenario == "unreadable-state" {
            b"{not json"
        } else {
            b"{\"sets\":[{\"name\":\"cryptoemoji1_by_YourEmojiBot\",\"index\":1}]}\n"
        };
        std::fs::write(&state, state_bytes).unwrap();
        _native::atomic::write_json(&map, &json!({"sol":"cid-old"})).unwrap();
        let map_before = std::fs::read(&map).unwrap();
        let mut server = Server::new(if scenario == "not-repaired" {
            vec![Reply::new(
                "GET /api/v3/coins/markets",
                &[(
                    "query",
                    "vs_currency=usd&order=market_cap_desc&per_page=250&page=1&sparkline=false",
                )],
                json!([]),
            )]
        } else {
            vec![]
        });
        let arguments = vec![
            "--emoji-dir".into(),
            images.as_os_str().to_owned(),
            "--map".into(),
            map.as_os_str().to_owned(),
            "--state".into(),
            state.as_os_str().to_owned(),
            "--fix".into(),
            "--only".into(),
            "sol".into(),
        ];
        let overrides: &[(&str, &str)] = match scenario {
            "missing-owner" => &[("PACK_OWNER_USER_ID", "")],
            "invalid-owner" => &[("PACK_OWNER_USER_ID", "not-a-number")],
            "missing-token" => &[("TELEGRAM_BOT_TOKEN", "")],
            _ => &[],
        };
        let result = harness.capture(&server, &arguments, overrides);
        assert_eq!(
            result.code,
            if scenario == "not-repaired" { 4 } else { 2 },
            "{scenario}: {} {}",
            result.stdout,
            result.stderr
        );
        let requests = server.finish().unwrap();
        assert_eq!(requests.len(), usize::from(scenario == "not-repaired"));
        assert!(!result.stderr.contains("Traceback"));
        assert!(!result.stdout.contains(super::bot_api::TOKEN));
        assert!(!result.stderr.contains(super::bot_api::TOKEN));
        assert_eq!(std::fs::read(&map).unwrap(), map_before);
        assert_eq!(document(&map), json!({"sol":"cid-old"}));
        assert_eq!(std::fs::read(&state).unwrap(), state_bytes);
        assert!(std::fs::read_dir(&images).unwrap().next().is_none());
        assert!(!root.join("coins/verify_logos_intent.json").exists());
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 3);
        released(root);
    }
    successful_fix(harness);
}

fn successful_fix(harness: &Harness<'_>) {
    use sha2::{Digest, Sha256};
    use std::path::Path;
    let root = harness.root;
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let (pinned, provider) = _native::source_static_oracle::fixtures().unwrap();
    let fixture = std::env::var_os("NUMERA_STATIC_CONVERT_ORACLE")
        .map(|path| document(Path::new(&path)))
        .unwrap_or(pinned);
    _native::source_static_oracle::fixture_identity(&fixture, &provider).unwrap();
    let art = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "alpha-edge")
        .unwrap();
    let bytes = std::fs::read(project.join(art["input"].as_str().unwrap())).unwrap();
    let hash = |bytes: &[u8]| {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    assert_eq!(hash(&bytes), art["input_sha256"]);
    let directory = root.join("verify-success");
    let images = directory.join("emoji");
    std::fs::create_dir_all(&images).unwrap();
    let map = directory.join("map.json");
    let state = directory.join("state.json");
    let set = "rebuilt_pack_1";
    _native::atomic::write_json(
        &map,
        &json!({"sol":"cid-old","wrapped":"cid-old","eth":"keep"}),
    )
    .unwrap();
    _native::atomic::write_json(&state, &json!({"sets":[{"name":"old_pack_1","index":1}]}))
        .unwrap();
    std::fs::write(images.join("sol.png"), b"previous prepared art").unwrap();
    std::fs::create_dir(directory.join("png")).unwrap();
    std::fs::write(directory.join("png/sol.png"), b"previous raw art").unwrap();
    let state_before = std::fs::read(&state).unwrap();
    let intent = root.join("coins/verify_logos_intent.json");
    assert!(!intent.exists());
    let listing = |cid: &str| {
        Reply::new(
            "getStickerSet",
            &[("name", set)],
            json!({"ok":true,"result":{"stickers":[
                {"custom_emoji_id":"a","file_id":"fid-a"},
                {"custom_emoji_id":cid,"file_id":format!("fid-{cid}")},
                {"custom_emoji_id":"c","file_id":"fid-c"}
            ]}}),
        )
    };
    let mut image = Reply::new("GET /provider/fixture", &[], serde_json::Value::Null);
    image.download = Some(bytes.clone());
    let sticker = json!({"sticker":"attach://file0","format":"static","emoji_list":["🪙"],"keywords":["sol","solana"]}).to_string();
    let mut replacement = Reply::new(
        "replaceStickerInSet",
        &[
            ("user_id", "111111111"),
            ("name", set),
            ("old_sticker", "fid-cid-old"),
            ("sticker", &sticker),
        ],
        json!({"ok":true,"result":true}),
    );
    replacement.upload_sha256.insert(
        "file0".into(),
        art["expected"]["png_sha256"].as_str().unwrap().into(),
    );
    replacement.observation = Some((
        intent.clone(),
        "/before".into(),
        json!(["a", "cid-old", "c"]),
    ));
    let mut download = Reply::new("getFile", &[], serde_json::Value::Null);
    download.method = "GET repaired.png".into();
    download.download_upload = Some("file0".into());
    let (image, image_ready, image_release) = image.hold();
    let (before_listing, listing_ready, listing_release) = listing("cid-old").hold();
    let (replacement, mutation_ready, mutation_release) = replacement.hold();
    let (download, proof_ready, proof_release) = download.hold();
    let mut server = Server::new(vec![
        Reply::new(
            "GET /api/v3/coins/markets",
            &[(
                "query",
                "vs_currency=usd&order=market_cap_desc&per_page=250&page=1&sparkline=false",
            )],
            json!([{"symbol":"sol","name":"Solana","image":"https://fixture.invalid/provider/fixture"}]),
        ),
        image,
        before_listing,
        replacement,
        listing("cid-new"),
        Reply::new(
            "getFile",
            &[("file_id", "fid-cid-new")],
            json!({"ok":true,"result":{"file_path":"repaired.png"}}),
        ),
        download,
    ]);
    let family = root.join(".locks/pack_cryptoemoji.lock");
    let canonical = root.join(".locks/canonical_map.lock");
    let unrelated = _native::locks::Lease::acquire(
        &root.join(".locks/pack_othercoins.lock"),
        "unrelated family may stay live",
    )
    .unwrap();
    let map_before = std::fs::read(&map).unwrap();
    let (result, observed) = std::thread::scope(|scope| {
        let observer = scope.spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                tokio::time::timeout(std::time::Duration::from_secs(5), image_ready)
                    .await
                    .unwrap()
                    .unwrap()
            });
            busy(&family);
            assert_eq!(std::fs::read(&state).unwrap(), state_before);
            // Fixture fault after early validation, not a competing lock-respecting writer.
            _native::atomic::write_json(&state, &json!({"sets":[{"name":set,"index":1}]})).unwrap();
            let rebuilt = std::fs::read(&state).unwrap();
            image_release.send(()).unwrap();
            for (index, (ready, release)) in [
                (listing_ready, listing_release),
                (mutation_ready, mutation_release),
                (proof_ready, proof_release),
            ]
            .into_iter()
            .enumerate()
            {
                runtime.block_on(async {
                    tokio::time::timeout(std::time::Duration::from_secs(5), ready)
                        .await
                        .unwrap()
                        .unwrap()
                });
                busy(&family);
                busy(&canonical);
                assert_eq!(std::fs::read(&state).unwrap(), rebuilt);
                assert_eq!(std::fs::read(&map).unwrap(), map_before);
                assert_eq!(
                    std::fs::read(images.join("sol.png")).unwrap(),
                    b"previous prepared art"
                );
                assert_eq!(
                    std::fs::read(directory.join("png/sol.png")).unwrap(),
                    b"previous raw art"
                );
                if index == 0 {
                    assert!(!intent.exists());
                } else {
                    assert_eq!(document(&intent)["old_cid"], "cid-old");
                    assert_eq!(document(&intent)["before"], json!(["a", "cid-old", "c"]));
                }
                release.send(()).unwrap();
            }
            rebuilt
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            harness.capture(
                &server,
                &[
                    "--emoji-dir".into(),
                    images.as_os_str().to_owned(),
                    "--map".into(),
                    map.as_os_str().to_owned(),
                    "--state".into(),
                    state.as_os_str().to_owned(),
                    "--fix".into(),
                    "--only".into(),
                    "sol".into(),
                ],
                &[],
            )
        }));
        (result, observer.join())
    });
    let requests = server.finish();
    let rebuilt = observed.unwrap();
    let result = result.unwrap_or_else(|error| std::panic::resume_unwind(error));
    let requests = requests.unwrap();
    drop(unrelated);
    assert_eq!(result.code, 0, "{} {}", result.stdout, result.stderr);
    assert_eq!(requests.len(), 7);
    let writes = requests
        .iter()
        .filter(|request| request.method == "replaceStickerInSet")
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 1);
    let uploaded = &writes[0].files["file0"];
    assert_eq!(hash(uploaded), art["expected"]["png_sha256"]);
    assert_eq!(
        document(&map),
        json!({"sol":"cid-new","wrapped":"cid-new","eth":"keep"})
    );
    assert_eq!(std::fs::read(images.join("sol.png")).unwrap(), *uploaded);
    assert_eq!(std::fs::read(directory.join("png/sol.png")).unwrap(), bytes);
    assert_eq!(std::fs::read(&state).unwrap(), rebuilt);
    assert_ne!(rebuilt, state_before);
    assert_eq!(document(&state), json!({"sets":[{"name":set,"index":1}]}));
    assert!(!intent.exists());
    assert!(
        std::fs::read_dir(directory.join(".incoming"))
            .unwrap()
            .next()
            .is_none()
    );
    assert!(!requests.iter().any(|request| matches!(
        request.method.as_str(),
        "addStickerToSet" | "createNewStickerSet"
    )));
    assert!(!result.stderr.contains("Traceback") && !result.stderr.contains(super::bot_api::TOKEN));
    released(root);
}
