use super::*;
fn fixture() -> Provider {
    let f = Provider::new(2);
    write(&f.files.mapping, &json!({}));
    f
}
fn live(f: &Provider) -> Vec<Value> {
    let mut v = f.before.clone();
    v.push(member(3));
    v
}
fn settled(f: &Provider, result: &(usize, usize, Map<String, Value>)) {
    assert_eq!((result.0, result.1), (1, 0));
    assert_eq!(result.2["aaa"], "c3");
    assert_eq!(doc(&f.files.mapping)["aaa"], "c3");
    assert!(doc(&f.files.state)["provider_in_flight"].is_null());
}
// tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_the_unverified_upload_is_recorded_and_not_repeated
#[test]
fn test_the_unverified_upload_is_recorded_and_not_repeated() {
    let f = fixture();
    let (intent, first) = begin_unresolved(&f, &["aaa"]);
    let mut next = f.stage(&["aaa"], &f.ours);
    let (result, last) = resume(&f, &mut next, &live(&f), &f.ours);
    settled(&f, &result);
    assert_eq!(
        first
            .iter()
            .chain(&last)
            .filter(|r| r.method == "addStickerToSet")
            .count(),
        1
    );
    assert_eq!(intent["key"], "aaa");
    assert_eq!(intent["operation"], "add");
    assert_eq!(intent["set_name"], SET);
    assert_eq!(intent["expected_before"], 2);
}
// tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_a_re_downloaded_source_does_not_unmake_the_recovery
#[test]
fn test_a_re_downloaded_source_does_not_unmake_the_recovery() {
    let f = fixture();
    let (_, first) = begin_unresolved(&f, &["aaa"]);
    std::fs::write(f.files.published.join("aaa.png"), &f.foreign).unwrap();
    let mut next = f.stage(&["aaa"], &f.foreign);
    let (result, last) = resume(&f, &mut next, &live(&f), &f.ours);
    settled(&f, &result);
    assert_eq!(
        first
            .iter()
            .chain(&last)
            .filter(|r| r.method == "addStickerToSet")
            .count(),
        1
    );
}
// tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_a_skipped_ticker_does_not_replace_the_published_logo
#[test]
fn test_a_skipped_ticker_does_not_replace_the_published_logo() {
    let f = fixture();
    let published = std::fs::read(f.files.published.join("aaa.png")).unwrap();
    let mut stage = f.stage(&["aaa"], &f.foreign);
    let source = stage.source("aaa").unwrap();
    write(&f.files.mapping, &json!({"aaa":"c-the-other-tools"}));
    let server = Server::new(vec![me()]);
    let result = f.run(&server, &mut stage, &["aaa"]).unwrap();
    server.finish();
    assert_eq!((result.0, result.1), (0, 0));
    assert_eq!(
        std::fs::read(f.files.published.join("aaa.png")).unwrap(),
        published
    );
    assert!(!source.exists());
    write(&f.files.mapping, &json!({}));
    let mut stage = f.stage(&["aaa"], &f.foreign);
    let source = stage.source("aaa").unwrap();
    let mut script = vec![
        me(),
        listing(&f.before),
        listing(&f.before),
        f.mutation(false, 2, Body::Json(ok(json!(true)))),
        listing(&live(&f)),
    ];
    download(&mut script, "f3", &f.foreign);
    let server = Server::new(script);
    let result = f.run(&server, &mut stage, &["aaa"]).unwrap();
    settled(&f, &result);
    server.finish();
    assert_ne!(
        std::fs::read(f.files.published.join("aaa.png")).unwrap(),
        published
    );
    assert!(!source.exists());
    f.release();
}
// tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_another_process_cannot_swap_the_image_mid_upload
#[test]
fn test_another_process_cannot_swap_the_image_mid_upload() {
    let f = fixture();
    let mut stage = f.stage(&["aaa"], &f.ours);
    let source = stage.source("aaa").unwrap();
    let mut mutation = f.mutation(false, 2, Body::Json(ok(json!(true))));
    let published = f.files.published.join("aaa.png");
    let incoming = f
        .files
        .published
        .parent()
        .unwrap()
        .join(".incoming/aaa.png");
    let foreign = f.foreign.clone();
    let ours = f.ours.clone();
    mutation.action = Some(Box::new(move |r| {
        assert_eq!(r.files["file0"], ours);
        std::fs::write(&incoming, &foreign).unwrap();
        std::fs::write(&published, &foreign).unwrap();
    }));
    let mut script = vec![
        me(),
        listing(&f.before),
        listing(&f.before),
        mutation,
        listing(&live(&f)),
    ];
    download(&mut script, "f3", &f.ours);
    let server = Server::new(script);
    let result = f.run(&server, &mut stage, &["aaa"]).unwrap();
    settled(&f, &result);
    let requests = server.finish();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method == "addStickerToSet")
            .count(),
        1
    );
    assert_eq!(
        rt::codec_image::source_dhash(&f.files.published.join("aaa.png")).unwrap(),
        rt::codec_image::provider_dhash(&f.ours).unwrap()
    );
    assert!(!source.exists());
    f.release();
}
// tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_an_unresolved_upload_keeps_its_source_for_the_next_run
#[test]
fn test_an_unresolved_upload_keeps_its_source_for_the_next_run() {
    let f = fixture();
    let before = std::fs::read(f.files.published.join("aaa.png")).unwrap();
    let (intent, first) = begin_unresolved(&f, &["aaa"]);
    let retained = std::path::PathBuf::from(intent["source_path"].as_str().unwrap());
    assert!(retained.is_file());
    assert_eq!(
        std::fs::read(f.files.published.join("aaa.png")).unwrap(),
        before
    );
    let mut next = f.stage(&["aaa"], &f.foreign);
    assert_ne!(next.source("aaa").unwrap(), retained);
    let (result, last) = resume(&f, &mut next, &live(&f), &f.ours);
    settled(&f, &result);
    assert_eq!(
        first
            .iter()
            .chain(&last)
            .filter(|r| r.method == "addStickerToSet")
            .count(),
        1
    );
    assert_eq!(
        rt::codec_image::source_dhash(&f.files.published.join("aaa.png")).unwrap(),
        rt::codec_image::provider_dhash(&f.ours).unwrap()
    );
}
// tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_recovery_never_promotes_an_image_it_cannot_prove
#[test]
fn test_recovery_never_promotes_an_image_it_cannot_prove() {
    let f = fixture();
    let before = std::fs::read(f.files.published.join("aaa.png")).unwrap();
    let (intent, _) = begin_unresolved(&f, &["aaa"]);
    std::fs::remove_file(intent["source_path"].as_str().unwrap()).unwrap();
    let mut next = f.stage(&["aaa"], &f.foreign);
    let log = f.root.log();
    let mut script = vec![me(), listing(&live(&f))];
    download(&mut script, "f3", &f.ours);
    let server = Server::new(script);
    let result = rt::coin_provider_effects::run(
        &f.files,
        &server.telegram().with_log(&log),
        &mut next,
        &ids(&["aaa"]),
        "T",
        1,
        &Map::new(),
    )
    .unwrap();
    server.finish();
    settled(&f, &result);
    assert_eq!(
        std::fs::read(f.files.published.join("aaa.png")).unwrap(),
        before
    );
    assert!(
        std::fs::read_to_string(log.path.unwrap())
            .unwrap()
            .contains("was NOT updated")
    );
    f.release();
}
// tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_an_upload_that_never_landed_is_retried_once
#[test]
fn test_an_upload_that_never_landed_is_retried_once() {
    let f = fixture();
    let (_, first) = begin_unresolved(&f, &["aaa"]);
    let mut stage = f.stage(&["aaa"], &f.ours);
    let mut script = vec![
        me(),
        listing(&f.before),
        listing(&f.before),
        listing(&f.before),
        f.mutation(false, 2, Body::Json(ok(json!(true)))),
        listing(&live(&f)),
    ];
    download(&mut script, "f3", &f.ours);
    let server = Server::new(script);
    let result = f.run(&server, &mut stage, &["aaa"]).unwrap();
    let last = server.finish();
    settled(&f, &result);
    assert_eq!(
        first
            .iter()
            .chain(&last)
            .filter(|r| r.method == "addStickerToSet")
            .count(),
        2
    );
    f.release();
}
// tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_nothing_else_is_mutated_while_an_outcome_is_unresolved
#[test]
fn test_nothing_else_is_mutated_while_an_outcome_is_unresolved() {
    let f = fixture();
    let mut stage = f.stage(&["aaa", "bbb", "ccc"], &f.ours);
    let mut live = live(&f);
    live.push(member(4));
    let mut script = vec![
        me(),
        listing(&f.before),
        listing(&f.before),
        f.mutation(false, 2, Body::Json(ok(json!(true)))),
        listing(&live),
    ];
    download(&mut script, "f3", &f.ours);
    download(&mut script, "f4", &f.ours);
    let server = Server::new(script);
    let result = f.run(&server, &mut stage, &["aaa", "bbb", "ccc"]).unwrap();
    assert_eq!((result.0, result.1), (0, 3));
    let requests = server.finish();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method == "addStickerToSet")
            .count(),
        1
    );
    assert_eq!(doc(&f.files.state)["provider_in_flight"]["key"], "aaa");
    f.release();
}
// tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_a_definitive_rejection_still_clears_the_intent
#[test]
fn test_a_definitive_rejection_still_clears_the_intent() {
    let f = fixture();
    let mut stage = f.stage(&["aaa"], &f.ours);
    let server = Server::new(vec![
        me(),
        listing(&f.before),
        listing(&f.before),
        f.mutation(
            false,
            2,
            Body::Json(reject("addStickerToSet failed: STICKER_PNG_NOPNG")),
        ),
    ]);
    let result = f.run(&server, &mut stage, &["aaa"]).unwrap();
    let requests = server.finish();
    assert_eq!((result.0, result.1), (0, 1));
    assert!(doc(&f.files.state)["provider_in_flight"].is_null());
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method == "addStickerToSet")
            .count(),
        1
    );
}
