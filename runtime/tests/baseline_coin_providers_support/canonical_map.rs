use super::*;
// tests.test_coin_providers.TheCanonicalMapIsRereadUnderTheLock.test_neither_writers_ids_are_lost
#[test]
fn test_neither_writers_ids_are_lost() {
    let f = Provider::new(2);
    let stale = doc(&f.files.mapping);
    write(
        &f.files.mapping,
        &json!({"btc":"c-btc","zzz":"written-by-the-other-tool"}),
    );
    let mut stage = f.stage(&["aaa"], &f.ours);
    let mut live = f.before.clone();
    live.push(member(3));
    let mut script = vec![
        me(),
        listing(&f.before),
        listing(&f.before),
        f.mutation(false, 2, Body::Json(ok(json!(true)))),
        listing(&live),
    ];
    download(&mut script, "f3", &f.ours);
    let server = Server::new(script);
    let result = f.run(&server, &mut stage, &["aaa"]).unwrap();
    assert_eq!((result.0, result.1), (1, 0));
    let saved = doc(&f.files.mapping);
    assert_eq!(saved["btc"], "c-btc");
    assert_eq!(saved["zzz"], "written-by-the-other-tool");
    assert_eq!(saved["aaa"], "c3");
    assert_eq!(
        result.2["zzz"], "written-by-the-other-tool",
        "owned return map refreshes caller's canonical view"
    );
    assert!(stale.get("zzz").is_none());
    server.finish();
    f.release();
}
// tests.test_coin_providers.TheCanonicalMapIsRereadUnderTheLock.test_a_ticker_mapped_while_we_waited_is_not_uploaded_again
#[test]
fn test_a_ticker_mapped_while_we_waited_is_not_uploaded_again() {
    let f = Provider::new(2);
    let stale = doc(&f.files.mapping);
    write(
        &f.files.mapping,
        &json!({"btc":"c-btc","aaa":"c-from-the-other-provider"}),
    );
    let mut stage = f.stage(&["aaa"], &f.ours);
    let server = Server::new(vec![me()]);
    let result = f.run(&server, &mut stage, &["aaa"]).unwrap();
    assert_eq!((result.0, result.1), (0, 0));
    assert_eq!(
        server.finish().len(),
        1,
        "already live ticker must have no mutation"
    );
    assert_eq!(f.before.len(), 2);
    assert_eq!(doc(&f.files.mapping)["aaa"], "c-from-the-other-provider");
    assert_eq!(result.2["aaa"], "c-from-the-other-provider");
    assert!(stale.get("aaa").is_none());
    f.release();
}
// tests.test_coin_providers.TheCanonicalMapIsRereadUnderTheLock.test_the_rest_of_the_batch_is_still_published
#[test]
fn test_the_rest_of_the_batch_is_still_published() {
    let f = Provider::new(2);
    write(
        &f.files.mapping,
        &json!({"btc":"c-btc","aaa":"c-from-the-other-provider"}),
    );
    let mut stage = f.stage(&["aaa", "bbb"], &f.ours);
    let mut live = f.before.clone();
    live.push(member(3));
    let mut script = vec![
        me(),
        listing(&f.before),
        listing(&f.before),
        f.mutation(false, 2, Body::Json(ok(json!(true)))),
        listing(&live),
    ];
    download(&mut script, "f3", &f.ours);
    let server = Server::new(script);
    let result = f.run(&server, &mut stage, &["aaa", "bbb"]).unwrap();
    assert_eq!((result.0, result.1), (1, 0));
    let requests = server.finish();
    let adds = requests
        .iter()
        .filter(|r| r.method == "addStickerToSet")
        .collect::<Vec<_>>();
    assert_eq!(
        adds.iter()
            .map(
                |r| r.observation.as_ref().unwrap()["provider_in_flight"]["key"]
                    .as_str()
                    .unwrap()
            )
            .collect::<Vec<_>>(),
        vec!["bbb"]
    );
    assert_eq!(result.2["bbb"], "c3");
    f.release();
}
// tests.test_coin_providers.TheCanonicalMapIsRereadUnderTheLock.test_a_map_writer_blocks_the_publisher
#[test]
fn test_a_map_writer_blocks_the_publisher() {
    let f = Provider::new(2);
    let mut stage = f.stage(&["aaa"], &f.ours);
    let server = Server::new(vec![]);
    let hold = rt::locks::Lease::acquire(
        &f.root.0.join(".locks/canonical_map.lock"),
        "real map writer",
    )
    .unwrap();
    assert!(f.run(&server, &mut stage, &["aaa"]).is_err());
    assert!(server.finish().is_empty());
    drop(hold);
    drop(
        rt::locks::Lease::acquire(
            &rt::locks::pack_family_path(&f.root.0, "cryptoemoji"),
            "family released after map refusal",
        )
        .unwrap(),
    );
    f.release();
}
