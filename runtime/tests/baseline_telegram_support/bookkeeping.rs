use super::*;
// tests.test_telegram_client.BookkeepingFollowsLiveState.test_a_foreign_sticker_during_a_retry_does_not_drift_the_count
#[test]
fn test_a_foreign_sticker_during_a_retry_does_not_drift_the_count() {
    if child_driver() {
        return;
    }
    let root = Root::new();
    let src = root.0.join("src");
    std::fs::create_dir(&src).unwrap();
    let b = src.join("b.png");
    let c = src.join("c.png");
    image::RgbaImage::from_pixel(100, 100, image::Rgba([200, 0, 0, 255]))
        .save(&b)
        .unwrap();
    image::RgbaImage::from_pixel(100, 100, image::Rgba([0, 200, 0, 255]))
        .save(&c)
        .unwrap();
    let foreign = root.0.join("foreign.png");
    image::RgbaImage::from_pixel(100, 100, image::Rgba([0, 0, 200, 255]))
        .save(&foreign)
        .unwrap();
    let bytes = std::fs::read(&b).unwrap();
    let foreign_bytes = std::fs::read(&foreign).unwrap();
    let state = root.0.join("state.json");
    std::fs::write(&state,json!({"base":"t","per_set":200,"done":["a"],"sent":[],"sets":[{"name":"t1_by_bot","title":"T 1","count":1,"index":1}],"in_flight":null}).to_string()).unwrap();
    // The production codec companion is copied into this owned installed-root fixture.
    let executable = root.0.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), executable).unwrap();
    std::fs::create_dir(root.0.join("assets")).unwrap();
    std::fs::write(root.0.join("assets/panel.html"), b"owned codec-root marker").unwrap();
    let mut script = vec![
        reply("getMe", ok(json!({"username":"bot"}))),
        listing(&["live0"]),
        listing(&["live0"]),
    ];
    let mut first = fault("addStickerToSet");
    first.observe = Some(state.clone());
    script.push(first);
    script.push(listing(&["live0", "foreign"]));
    download(&mut script, "foreign", &foreign_bytes);
    let mut second = reply("addStickerToSet", ok(json!(true)));
    second.observe = Some(state.clone());
    script.push(second);
    script.push(listing(&["live0", "foreign", "ours2"]));
    download(&mut script, "foreign", &foreign_bytes);
    download(&mut script, "ours2", &bytes);
    script.push(listing(&["live0", "foreign", "ours2"]));
    let mut third = reply("addStickerToSet", ok(json!(true)));
    third.observe = Some(state.clone());
    script.push(third);
    script.push(reply("sendMessage", ok(json!({}))));
    let server = Server::new(script);
    run_child(
        "bookkeeping::test_a_foreign_sticker_during_a_retry_does_not_drift_the_count",
        &root,
        &server.base,
        "ticker",
        &ids(&[
            "--base",
            "t",
            "--title",
            "T",
            "--user-id",
            "1",
            "--source-dir",
            src.to_str().unwrap(),
            "--token-env",
            "FAKE_TOKEN",
            "--state",
            state.to_str().unwrap(),
        ]),
    );
    let requests = server.finish();
    let intents = requests
        .iter()
        .filter(|r| r.method == "addStickerToSet")
        .map(|r| r.observation.as_ref().unwrap()["in_flight"].clone())
        .collect::<Vec<_>>();
    let saved: Value = serde_json::from_slice(&std::fs::read(state).unwrap()).unwrap();
    let actual_live = 1 + requests
        .iter()
        .filter(|r| r.method == "addStickerToSet")
        .count();
    assert_eq!(
        saved["sets"][0]["count"],
        json!(actual_live),
        "recorded count must describe actual live0+foreign+oursb+oursc"
    );
    assert_eq!(actual_live, 4);
    let adds = requests
        .iter()
        .filter(|r| r.method == "addStickerToSet")
        .collect::<Vec<_>>();
    assert_eq!(adds[0].files["file0"], bytes);
    assert_eq!(adds[1].files["file0"], bytes);
    assert_eq!(adds[2].files["file0"], std::fs::read(&c).unwrap());
    assert_eq!(
        intents
            .iter()
            .map(|i| i["expected_before"].clone())
            .collect::<Vec<_>>(),
        vec![json!(1), json!(1), json!(3)]
    );
    assert_eq!(
        intents.iter().map(|i| i["key"].clone()).collect::<Vec<_>>(),
        vec![json!("b"), json!("b"), json!("c")]
    );
    let mut done: Vec<String> = serde_json::from_value(saved["done"].clone()).unwrap();
    done.sort();
    assert_eq!(done, ids(&["a", "b", "c"]));
}

// tests.test_telegram_client.BookkeepingFollowsLiveState.test_an_unknown_size_after_a_retried_add_is_not_guessed
#[test]
fn test_an_unknown_size_after_a_retried_add_is_not_guessed() {
    let m = mutation_with_retry(
        false,
        Some(members(&["live0"])),
        Some(listing(&["live0", "foreign"])),
        Some(fault("getStickerSet")),
        matches(&[("foreign", Some(false))]),
        Some(ok(json!(true))),
        false,
    );
    assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
    assert_eq!(mutation_count(&m), 2);
}
// tests.test_telegram_client.BookkeepingFollowsLiveState.test_a_foreign_sticker_alone_does_not_certify_the_size
#[test]
fn test_a_foreign_sticker_alone_does_not_certify_the_size() {
    let m = mutation_with_retry(
        false,
        Some(members(&["live0"])),
        Some(listing(&["live0", "foreign"])),
        Some(listing(&["live0", "foreign"])),
        matches(&[("foreign", Some(false))]),
        Some(ok(json!(true))),
        false,
    );
    assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
    assert_eq!(mutation_count(&m), 2);
}
// tests.test_telegram_client.BookkeepingFollowsLiveState.test_an_undisturbed_add_costs_no_extra_probe
#[test]
fn test_an_undisturbed_add_costs_no_extra_probe() {
    let m = mutation(
        false,
        Some(members(&["live0"])),
        None,
        None,
        matches(&[]),
        false,
        false,
    );
    assert_eq!(
        m.result.unwrap(),
        Value::Null,
        "clean add stays at assumed+1"
    );
    assert_eq!(
        m.requests
            .iter()
            .filter(|r| r.method == "getStickerSet")
            .count(),
        1,
        "only preadd identity snapshot"
    );
}
