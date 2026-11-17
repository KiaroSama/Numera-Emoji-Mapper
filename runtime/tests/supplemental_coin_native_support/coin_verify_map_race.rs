//! Original map-repoint refusal and literal-family exclusion at the actual repair command.
use super::{Harness, Reply, Server, document, hash, released};
use _native::locks::Lease;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

pub(super) fn map_writer_repoint_before_replacement_is_not_overwritten(harness: &Harness<'_>) {
    let root = harness.root;
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    let art = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "alpha-edge")
        .unwrap();
    let bytes = std::fs::read(project.join(art["input"].as_str().unwrap())).unwrap();
    assert_eq!(hash(&bytes), art["input_sha256"]);
    let directory = root.join("verify-map-writer-before-replacement");
    let images = directory.join("emoji");
    std::fs::create_dir_all(&images).unwrap();
    let map = directory.join("map.json");
    let state = directory.join("state.json");
    let published = images.join("btc.png");
    std::fs::write(&published, b"the previously published oracle").unwrap();
    _native::atomic::write_json(&map, &json!({"btc":"cid-old"})).unwrap();
    let set = "cryptoemoji1_by_YourEmojiBot";
    _native::atomic::write_json(&state, &json!({"sets":[{"name":set,"index":1}]})).unwrap();
    let before = std::fs::read(&state).unwrap();
    let initial_map = std::fs::read(&map).unwrap();
    let mut image = Reply::new("GET /provider/fixture", &[], Value::Null);
    image.download = Some(bytes);
    let (image, ready, release) = image.hold();
    let mut server = Server::new(vec![
        Reply::new(
            "GET /api/v3/coins/markets",
            &[(
                "query",
                "vs_currency=usd&order=market_cap_desc&per_page=250&page=1&sparkline=false",
            )],
            json!([{"symbol":"btc","name":"Bitcoin","image":"https://fixture.invalid/provider/fixture"}]),
        ),
        image,
        Reply::new(
            "getStickerSet",
            &[("name", set)],
            json!({"ok":true,"result":{"stickers":[
                {"custom_emoji_id":"a","file_id":"fid-a"},
                {"custom_emoji_id":"cid-old","file_id":"fid-cid-old"},
                {"custom_emoji_id":"c","file_id":"fid-c"}
            ]}}),
        ),
    ]);
    let family = root.join(".locks/pack_cryptoemoji.lock");
    let canonical = root.join(".locks/canonical_map.lock");
    let unrelated = Lease::acquire(
        &root.join(".locks/pack_othercoins.lock"),
        "unrelated family",
    )
    .unwrap();
    let (result, observed) = std::thread::scope(|scope| {
        let observer = scope.spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                tokio::time::timeout(Duration::from_secs(5), ready)
                    .await
                    .unwrap()
                    .unwrap()
            });
            // Actual image delivery precedes the map lease. No internal acquisition callback is injected.
            super::coin_verify_commands::busy(&family);
            let writer =
                Lease::acquire(&canonical, "map-only writer before repair selection").unwrap();
            assert_eq!(std::fs::read(&map).unwrap(), initial_map);
            _native::atomic::write_json(&map, &json!({"btc":"cid-written-by-the-other-tool"}))
                .unwrap();
            drop(writer);
            release.send(()).unwrap();
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
                    "btc".into(),
                ],
                &[],
            )
        }));
        (result, observer.join())
    });
    let requests = server.finish();
    observed.unwrap();
    let result = result.unwrap_or_else(|error| std::panic::resume_unwind(error));
    let requests = requests.unwrap();
    assert_eq!(result.code, 4, "{} {}", result.stdout, result.stderr);
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests
            .iter()
            .map(|r| r.method.as_str())
            .collect::<Vec<_>>(),
        [
            "GET /api/v3/coins/markets",
            "GET /provider/fixture",
            "getStickerSet"
        ]
    );
    assert_eq!(
        document(&map),
        json!({"btc":"cid-written-by-the-other-tool"})
    );
    assert_eq!(std::fs::read(&state).unwrap(), before);
    assert_eq!(
        std::fs::read(&published).unwrap(),
        b"the previously published oracle"
    );
    assert!(!directory.join(".incoming").exists());
    assert!(!root.join("coins/verify_logos_intent.json").exists());
    assert!(
        !result.stdout.contains(super::bot_api::TOKEN)
            && !result.stderr.contains(super::bot_api::TOKEN)
    );
    drop(unrelated);
    released(root);
}

pub(super) fn repair_is_excluded_by_literal_pack_family_not_script_name(harness: &Harness<'_>) {
    let root = harness.root;
    let directory = root.join("verify-literal-family-held");
    let images = directory.join("emoji");
    std::fs::create_dir_all(&images).unwrap();
    let map = directory.join("map.json");
    let state = directory.join("state.json");
    let map_bytes = b"{\"btc\":\"cid-old\"}\n";
    let state_bytes = b"{\"sets\":[{\"name\":\"cryptoemoji1_by_YourEmojiBot\",\"index\":1}]}\n";
    std::fs::write(&map, map_bytes).unwrap();
    std::fs::write(&state, state_bytes).unwrap();
    let path = root.join(".locks/pack_cryptoemoji.lock");
    let lease = Lease::acquire(&path, "actual pack-family competitor").unwrap();
    let mut server = Server::new(vec![]);
    let result = harness.capture(
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
            "btc".into(),
        ],
        &[],
    );
    let requests = server.finish().unwrap();
    assert_eq!(result.code, 4, "{} {}", result.stdout, result.stderr);
    assert!(
        result
            .stderr
            .contains("logo repair family is busy or unavailable")
    );
    assert!(requests.is_empty());
    assert_eq!(std::fs::read(&map).unwrap(), map_bytes);
    assert_eq!(std::fs::read(&state).unwrap(), state_bytes);
    assert!(std::fs::read_dir(&images).unwrap().next().is_none());
    assert!(!root.join("coins/verify_logos_intent.json").exists());
    super::coin_verify_commands::busy(&path);
    drop(lease);
    released(root);
}
