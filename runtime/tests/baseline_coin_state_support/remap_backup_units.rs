//! Actual remap backup sees the concurrent file only after the real canonical acquisition.
use crate as rt;
use crate::baseline_coin_state_support::*;
use serde_json::json;
// tests.test_coin_ticker_map.CanonicalMapWritersCannotLoseAnUpdate.test_remap_apply_backs_up_and_replaces_under_the_lock
#[test]
fn test_remap_apply_backs_up_and_replaces_under_the_lock() {
    let root = Root::new();
    let images = root.0.join("emoji");
    std::fs::create_dir(&images).unwrap();
    let original = noise("btc-original-cli");
    std::fs::write(images.join("btc.png"), &original).unwrap();
    let output = root.0.join("ticker_to_id.json");
    write(&output, &json!({"btc":"STALE"}));
    let state = root.0.join("state.json");
    write(&state, &json!({"sets":[{"index":1,"name":"s1"}]}));
    let files = rt::coin_remap::Files {
        root: root.0.clone(),
        state,
        cache: root.0.join("cache.json"),
        images,
        output: output.clone(),
        candidates: root.0.join("ticker_to_id.candidate.json"),
        base: "cryptoemoji".into(),
    };
    let mut list = reply(
        "getStickerSet",
        ok(
            json!({"stickers":[{"custom_emoji_id":"live-btc","file_id":"s1#0","file_unique_id":"s1#0"}]}),
        ),
    );
    list.action = Some(Box::new(|r| assert_eq!(r.form["name"], "s1")));
    let mut script = vec![list];
    download(&mut script, "s1", 0, &original);
    let server = Server::new(script);
    let mut acquisitions = 0;
    let code = super::run_with_effects(
        &files,
        &server.telegram(),
        rt::coin_remap_policy::Options {
            apply: true,
            max_distance: Some(100.0),
            min_margin: None,
            incomplete_sets: 0,
            failed_images: 0,
        },
        &root.log(),
        rt::coin_remap_math::nearest,
        |path| {
            assert_eq!(path, rt::locks::canonical_map_path(&root.0));
            assert!(
                doc(&files.cache)["sigs"]["live-btc"].is_string(),
                "actual signature work must precede this boundary"
            );
            assert!(
                rt::locks::Lease::acquire(
                    &rt::locks::pack_family_path(&root.0, &files.base),
                    "actual concurrent provider"
                )
                .is_err()
            );
            let lease = rt::locks::Lease::acquire(path, "actual canonical race acquisition")?;
            let mut current = doc(&output);
            current["zzz"] = json!("written-by-the-other-tool");
            rt::atomic::write_json(&output, &current)?;
            acquisitions += 1;
            Ok(lease)
        },
    )
    .unwrap();
    assert_eq!(code, 0);
    assert_eq!(acquisitions, 1);
    assert_eq!(doc(&output), json!({"btc":"live-btc"}));
    assert_eq!(
        doc(&output.with_extension("prebroken.json")),
        json!({"btc":"STALE","zzz":"written-by-the-other-tool"})
    );
    use base64::Engine;
    let cached = doc(&files.cache);
    let signature = base64::engine::general_purpose::STANDARD
        .decode(cached["sigs"]["live-btc"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        signature,
        rt::codec_image::remap_signature(&original).unwrap()
    );
    let requests = server.finish();
    assert_eq!(requests.iter().filter(|r| r.method == "getFile").count(), 1);
    no_mutations(&requests);
    drop(
        rt::locks::Lease::acquire(
            &rt::locks::canonical_map_path(&root.0),
            "canonical release control",
        )
        .unwrap(),
    );
    drop(
        rt::locks::Lease::acquire(
            &rt::locks::pack_family_path(&root.0, &files.base),
            "family release control",
        )
        .unwrap(),
    );
}
