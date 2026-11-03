//! Provider recovery across run namespaces and competing artwork, through the actual effects seam.
use super::{SET, document, download, files, listing, me, member, mutation, released};
use crate::bot_api::{Reply, Server};
use _native::{coin_provider_effects, coin_provider_stage::Stage, telegram::Telegram};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(super) fn check(root: &Path, ours: &[u8], changed: &[u8]) {
    unresolved_then_resume(root, ours, changed);
    competing_artwork(root, ours, changed);
    unexpected_reply(root, ours);
}

fn unresolved_then_resume(root: &Path, ours: &[u8], changed: &[u8]) {
    for applied in [true, false] {
        let files = files(
            root.join(if applied {
                "landed-restart"
            } else {
                "not-landed-restart"
            }),
            2,
        );
        std::fs::create_dir_all(&files.published).unwrap();
        let destination = files.published.join("aaa.png");
        std::fs::write(&destination, ours).unwrap();
        let before = std::fs::read(&files.mapping).unwrap();
        let old = (1..=2).map(member).collect::<Vec<_>>();
        let mut stage = Stage::new(&files.published).unwrap();
        let source = stage.source("aaa").unwrap();
        std::fs::write(&source, ours).unwrap();
        std::fs::write(stage.source("bbb").unwrap(), ours).unwrap();
        let mut server = Server::new(vec![
            me(),
            listing(SET, &old),
            listing(SET, &old),
            mutation(&files, SET, 2, "aaa", ours, json!({"ok":true})),
            Reply::new(
                "getStickerSet",
                &[("name", SET)],
                json!({"ok":false,"description":"controlled unavailable"}),
            ),
        ]);
        let tg = Telegram::new(crate::bot_api::TOKEN.into(), server.base.clone()).unwrap();
        let outcome = coin_provider_effects::run(
            &files,
            &tg,
            &mut stage,
            &["aaa".into(), "bbb".into()],
            "Title",
            1,
            &serde_json::Map::new(),
        )
        .unwrap();
        assert_eq!((outcome.0, outcome.1), (0, 2));
        assert_eq!(document(&files.mapping), Value::Object(outcome.2));
        assert_eq!(std::fs::read(&files.mapping).unwrap(), before);
        let intent = document(&files.state)["provider_in_flight"].clone();
        assert_eq!(intent["key"], "aaa");
        assert_eq!(intent["operation"], "add");
        assert_eq!(intent["set_name"], SET);
        assert_eq!(intent["expected_before"], 2);
        assert_eq!(intent["source_dhash"], 0);
        assert_eq!(
            PathBuf::from(intent["source_path"].as_str().unwrap()),
            source
        );
        assert_eq!(std::fs::read(&source).unwrap(), ours);
        assert_eq!(std::fs::read(&destination).unwrap(), ours);
        assert_eq!(std::fs::read_dir(stage.directory()).unwrap().count(), 1);
        assert!(!stage.source("bbb").unwrap().exists());
        let first = server.finish().unwrap();
        assert_eq!(first.len(), 5);
        assert_eq!(
            first
                .iter()
                .filter(|r| r.method == "addStickerToSet")
                .count(),
            1
        );
        drop(tg);
        drop(stage);
        released(&files);

        // A new per-run namespace has different art. Durable intent still describes the first send.
        let mut next = Stage::new(&files.published).unwrap();
        assert_ne!(next.directory(), source.parent().unwrap());
        std::fs::write(next.source("aaa").unwrap(), changed).unwrap();
        let mut live = old.clone();
        live.push(member(3));
        let mut replies = vec![me()];
        if applied {
            replies.push(listing(SET, &live));
            download(&mut replies, 3, ours);
        } else {
            replies.extend([
                listing(SET, &old),
                listing(SET, &old),
                listing(SET, &old),
                mutation(
                    &files,
                    SET,
                    2,
                    "aaa",
                    changed,
                    json!({"ok":true,"result":true}),
                ),
                listing(SET, &live),
            ]);
            download(&mut replies, 3, changed);
        }
        let mut server = Server::new(replies);
        let tg = Telegram::new(crate::bot_api::TOKEN.into(), server.base.clone()).unwrap();
        let outcome = coin_provider_effects::run(
            &files,
            &tg,
            &mut next,
            &["aaa".into()],
            "Title",
            1,
            &serde_json::Map::new(),
        )
        .unwrap();
        assert_eq!((outcome.0, outcome.1), (1, 0));
        assert_eq!(outcome.2["aaa"], "c3");
        assert_eq!(outcome.2["btc"], "c-btc");
        assert_eq!(outcome.2["other"], "preserved");
        assert_eq!(document(&files.mapping), Value::Object(outcome.2));
        let state = document(&files.state);
        assert!(state["provider_in_flight"].is_null());
        assert_eq!(state["provider_added"], json!(["aaa"]));
        assert_eq!(state["sets"].as_array().unwrap().len(), 1);
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            if applied { ours } else { changed }
        );
        assert!(!next.directory().exists());
        assert_eq!(source.exists(), !applied);
        if !applied {
            assert_eq!(std::fs::read(&source).unwrap(), ours);
        }
        let second = server.finish().unwrap();
        assert_eq!(second.len(), if applied { 4 } else { 8 });
        let adds = second
            .iter()
            .filter(|r| r.method == "addStickerToSet")
            .collect::<Vec<_>>();
        assert_eq!(adds.len(), usize::from(!applied));
        if let Some(add) = adds.first() {
            assert_eq!(
                add.observation.as_ref().unwrap()["provider_in_flight"]["source_dhash"],
                u64::MAX
            );
        }
        assert!(!second.iter().any(|r| r.method == "createNewStickerSet"));
        released(&files);
    }
}

fn competing_artwork(root: &Path, ours: &[u8], changed: &[u8]) {
    let files = files(root.join("mid-upload-artwork"), 2);
    std::fs::create_dir_all(&files.published).unwrap();
    let mut stage = Stage::new(&files.published).unwrap();
    let source = stage.source("aaa").unwrap();
    std::fs::write(&source, ours).unwrap();
    let old = (1..=2).map(member).collect::<Vec<_>>();
    let mut live = old.clone();
    live.push(member(3));
    let (held, mut reached, release) = mutation(
        &files,
        SET,
        2,
        "aaa",
        ours,
        json!({"ok":true,"result":true}),
    )
    .hold();
    let mut replies = vec![
        me(),
        listing(SET, &old),
        listing(SET, &old),
        held,
        listing(SET, &live),
    ];
    download(&mut replies, 3, ours);
    let mut server = Server::new(replies);
    let tg = Telegram::new(crate::bot_api::TOKEN.into(), server.base.clone()).unwrap();
    let (result, rival) = std::thread::scope(|scope| {
        let observer = scope.spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime
                .block_on(async {
                    tokio::time::timeout(std::time::Duration::from_secs(4), &mut reached).await
                })
                .unwrap()
                .unwrap();
            // HTTP has received our exact multipart bytes; another run now writes different art.
            let rival = Stage::new(&files.published).unwrap();
            assert_ne!(rival.directory(), source.parent().unwrap());
            std::fs::write(rival.source("aaa").unwrap(), changed).unwrap();
            std::fs::write(
                files.published.parent().unwrap().join(".incoming/aaa.png"),
                changed,
            )
            .unwrap();
            std::fs::write(files.published.join("aaa.png"), changed).unwrap();
            assert_eq!(std::fs::read(&source).unwrap(), ours);
            release.send(()).unwrap();
            runtime.shutdown_timeout(std::time::Duration::from_secs(1));
            rival
        });
        let result = coin_provider_effects::run(
            &files,
            &tg,
            &mut stage,
            &["aaa".into()],
            "Title",
            1,
            &serde_json::Map::new(),
        );
        (result, observer.join().unwrap())
    });
    let outcome = result.unwrap();
    assert_eq!((outcome.0, outcome.1), (1, 0));
    assert_eq!(outcome.2["aaa"], "c3");
    assert_eq!(document(&files.mapping), Value::Object(outcome.2));
    assert!(document(&files.state)["provider_in_flight"].is_null());
    assert_eq!(document(&files.state)["provider_added"], json!(["aaa"]));
    assert_eq!(
        std::fs::read(files.published.join("aaa.png")).unwrap(),
        ours
    );
    assert_eq!(
        std::fs::read(rival.source("aaa").unwrap()).unwrap(),
        changed
    );
    let shared = files.published.parent().unwrap().join(".incoming/aaa.png");
    assert_eq!(std::fs::read(&shared).unwrap(), changed);
    assert!(!stage.directory().exists());
    let requests = server.finish().unwrap();
    assert_eq!(requests.len(), 7);
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method == "addStickerToSet")
            .count(),
        1
    );
    // Only the rival's owner may discard it; publisher cleanup must leave both competing files alone.
    let mut rival = rival;
    rival.clean(None).unwrap();
    std::fs::remove_file(shared).unwrap();
    released(&files);
}

fn unexpected_reply(root: &Path, ours: &[u8]) {
    let files = files(root.join("unexpected-add-response"), 2);
    let map_before = std::fs::read(&files.mapping).unwrap();
    let mut stage = Stage::new(&files.published).unwrap();
    let source = stage.source("aaa").unwrap();
    for ticker in ["aaa", "bbb"] {
        std::fs::write(stage.source(ticker).unwrap(), ours).unwrap();
    }
    let old = (1..=2).map(member).collect::<Vec<_>>();
    let unknown = || {
        Reply::new(
            "getStickerSet",
            &[("name", SET)],
            json!({"ok":false,"description":"controlled unavailable"}),
        )
    };
    let mut server = Server::new(vec![
        me(),
        listing(SET, &old),
        listing(SET, &old),
        mutation(&files, SET, 2, "aaa", ours, json!([])),
        unknown(),
        unknown(),
    ]);
    let tg = Telegram::new(crate::bot_api::TOKEN.into(), server.base.clone()).unwrap();
    let outcome = coin_provider_effects::run(
        &files,
        &tg,
        &mut stage,
        &["aaa".into(), "bbb".into()],
        "Title",
        1,
        &serde_json::Map::new(),
    )
    .unwrap();
    assert_eq!((outcome.0, outcome.1), (0, 2));
    assert_eq!(document(&files.mapping), Value::Object(outcome.2));
    assert_eq!(std::fs::read(&files.mapping).unwrap(), map_before);
    let state = document(&files.state);
    assert_eq!(state["provider_in_flight"]["key"], "aaa");
    assert_eq!(state["provider_in_flight"]["source_dhash"], 0);
    assert_eq!(
        PathBuf::from(state["provider_in_flight"]["source_path"].as_str().unwrap()),
        source
    );
    assert_eq!(state["provider_added"], json!([]));
    assert_eq!(std::fs::read(&source).unwrap(), ours);
    assert_eq!(std::fs::read_dir(stage.directory()).unwrap().count(), 1);
    assert!(!stage.source("bbb").unwrap().exists());
    assert!(!files.published.join("aaa.png").exists());
    let requests = server.finish().unwrap();
    assert_eq!(requests.len(), 6);
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method == "addStickerToSet")
            .count(),
        1
    );
    assert!(!requests.iter().any(|r| r.method == "createNewStickerSet"));
    released(&files);
}
