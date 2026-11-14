//! Recovery attributes downloaded content without repeating a mutation; unknown keeps durable intent.
use _native::{
    catalog::Catalog, media_adapter::MediaAdapter, media_native_adapter::NativeMediaAdapter,
    publication::Publisher, publication_recovery::recover, publish_collection::Options,
    telegram::Telegram,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[path = "support/read_only_api.rs"]
mod read_only_api;
use read_only_api::{Fixture, Reply, TOKEN};

fn sticker(file: &str, fuid: &str, cid: &str) -> Value {
    json!({"file_id":file,"file_unique_id":fuid,"custom_emoji_id":cid})
}
fn pack(name: &str, sticker: &Value) -> Value {
    json!({"name":name,"stickers":[sticker]})
}

#[test]
fn applied_add_recovers_by_known_identity_without_second_mutation() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = _native::tooling_environment::physical_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
    )
    .unwrap();
    let parent = _native::tooling_environment::confined_directory(&root, "logs/tmp", true).unwrap();
    let directory = parent.join(format!("recovery-test-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    let source = root.join(fixture["cases"][3]["input"].as_str().unwrap());
    let before = std::fs::read(&source).unwrap();
    assert_eq!(
        _native::tooling_manifest::digest(&source).unwrap(),
        fixture["cases"][3]["input_sha256"]
    );
    let known = sticker("unused-download", "known-new-file", "111111111");
    let actual = sticker("downloaded", "downloaded-fuid", "222222222");
    let corrupt = sticker("corrupt", "must-not-bind", "333333333");
    let known_name = "fixture1_by_YourEmojiBot";
    let actual_name = "readback1_by_YourEmojiBot";
    let unknown_name = "unknown1_by_YourEmojiBot";
    let mut server = Fixture::new(vec![
        Reply::api(
            "getStickerSet",
            "name",
            known_name,
            pack(known_name, &known),
        ),
        Reply::api(
            "getStickerSet",
            "name",
            known_name,
            pack(known_name, &known),
        ),
        Reply::api(
            "getStickerSet",
            "name",
            actual_name,
            pack(actual_name, &actual),
        ),
        Reply::api(
            "getFile",
            "file_id",
            "downloaded",
            json!({"file_path":"fixture.png"}),
        ),
        Reply::file("fixture.png", &before),
        Reply::api(
            "getStickerSet",
            "name",
            actual_name,
            pack(actual_name, &actual),
        ),
        Reply::api(
            "getFile",
            "file_id",
            "downloaded",
            json!({"file_path":"fixture.png"}),
        ),
        Reply::file("fixture.png", &before),
        Reply::api(
            "getStickerSet",
            "name",
            unknown_name,
            pack(unknown_name, &corrupt),
        ),
        Reply::api(
            "getFile",
            "file_id",
            "corrupt",
            json!({"file_path":"corrupt.png"}),
        ),
        Reply::file("corrupt.png", b"not an image"),
        Reply::api(
            "getStickerSet",
            "name",
            "ticker1_by_YourEmojiBot",
            pack("ticker1_by_YourEmojiBot", &actual),
        ),
        Reply::api(
            "getFile",
            "file_id",
            "downloaded",
            json!({"file_path":"fixture.png"}),
        ),
        Reply::file("fixture.png", &before),
        Reply::api(
            "getStickerSet",
            "name",
            "ticker1_by_YourEmojiBot",
            pack("ticker1_by_YourEmojiBot", &actual),
        ),
        Reply::api(
            "getStickerSet",
            "name",
            "ticker1_by_YourEmojiBot",
            pack("ticker1_by_YourEmojiBot", &corrupt),
        ),
        Reply::api(
            "getFile",
            "file_id",
            "corrupt",
            json!({"file_path":"corrupt.png"}),
        ),
        Reply::file("corrupt.png", b"not an image"),
        Reply::api(
            "getStickerSet",
            "name",
            "ticker1_by_YourEmojiBot",
            json!({"name":"ticker1_by_YourEmojiBot","stickers":[]}),
        ),
    ]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut catalog =
            Catalog::open(&directory.join("catalog.db"), "fixture UTC", &root).unwrap();
        catalog.insert(&json!({"content_key":"s:fixture","fmt":"static","file_path":directory.join("unused.png")}),"fixture UTC").unwrap();
        catalog.record_seen("known-new-file", "s:fixture").unwrap();
        let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
        let media = MediaAdapter {
            python: PathBuf::from("must-not-start"),
            root: root.clone(),
            scratch: directory.join("codec"),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut publisher = Publisher {
            tg: &tg,
            cat: &mut catalog,
            media: &media,
            data: directory.clone(),
            runtime: &runtime,
            config: None,
        };
        let mut options = Options {
            base: "fixture".into(),
            title: "Fixture".into(),
            bot: "YourEmojiBot".into(),
            user_id: 111111111,
            fmt: "mixed".into(),
            per_set: 200,
            new_set: false,
            into_pack: Some(1),
            repaint: false,
            default_emoji: "😀".into(),
            logo: None,
        };
        let mut state = json!({"base":"fixture","sets":[{"name":known_name,"fmt":"mixed","index":1,
            "keys":[],"live":0,"logo":false}],"sent":[],"sent_full":[],"skipped":[],
            "in_flight":{"key":"s:fixture","operation":"add","set_name":known_name,"set_index":1}});
        let path = directory.join("publish_fixture.json");
        let original = state.clone();
        let mut foreign = state.clone();
        foreign["in_flight"]["set_name"] = json!("other1_by_YourEmojiBot");
        assert!(
            recover(&mut publisher, &options, &mut foreign, &path)
                .unwrap_err()
                .contains("another pack family")
        );
        assert_eq!(foreign["sets"], original["sets"]);
        assert!(!path.exists(), "invalid intent wrote durable state");
        recover(&mut publisher, &options, &mut state, &path).unwrap();
        assert_eq!(state["sets"][0]["keys"], json!(["s:fixture"]));
        assert_eq!(state["sets"][0]["live"], 1);
        assert!(state["in_flight"].is_null());
        assert_eq!(
            publisher
                .cat
                .dispatch(
                    json!({"operation":"custom_id","base":"fixture","content_key":"s:fixture"})
                )
                .unwrap(),
            "111111111"
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap(),
            state
        );
        assert!(
            !directory.join("codec").exists(),
            "known identity needlessly invoked media runtime"
        );
        let key = fixture["cases"][3]["expected"]["content_key"]
            .as_str()
            .unwrap();
        assert!(
            catalog
                .insert(
                    &json!({"content_key":key,"fmt":"static","file_path":source,
            "phash":fixture["cases"][3]["expected"]["phash"]}),
                    "fixture UTC"
                )
                .unwrap()
        );
        let native = NativeMediaAdapter {
            executable: PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji")),
            root: root.clone(),
            scratch: directory.join("native-codecs"),
        };
        let mut publisher = Publisher {
            tg: &tg,
            cat: &mut catalog,
            media: &native,
            data: directory.clone(),
            runtime: &runtime,
            config: None,
        };
        options.into_pack = None;
        for (base, name, succeeds) in [
            ("readback", actual_name, true),
            ("unknown", unknown_name, false),
        ] {
            options.base = base.into();
            let mut state = json!({"base":base,"sets":[],"sent":[],"sent_full":[],"skipped":[],
                "in_flight":{"key":key,"operation":"create","set_name":name,"set_index":1,"title":"Fixture 1"}});
            let path = directory.join(format!("publish_{base}.json"));
            _native::atomic::write_json(&path, &state).unwrap();
            let state_before = state.clone();
            let bytes_before = std::fs::read(&path).unwrap();
            let rows_before = publisher.cat.all(None).unwrap();
            let recovered = recover(&mut publisher, &options, &mut state, &path);
            if succeeds {
                recovered.unwrap();
                assert!(state["in_flight"].is_null());
                assert_eq!(state["sets"][0]["keys"], json!([key]));
                assert_eq!(state["sets"][0]["live"], 1);
                assert_eq!(
                    publisher
                        .cat
                        .dispatch(json!({"operation":"custom_id","base":base,"content_key":key}))
                        .unwrap(),
                    "222222222"
                );
                assert_eq!(
                    publisher
                        .cat
                        .dispatch(json!({"operation":"seen","file_unique_id":"downloaded-fuid"}))
                        .unwrap(),
                    key
                );
            } else {
                assert_eq!(
                    recovered.unwrap_err(),
                    "created set's first sticker is not proven to be the interrupted item; intent retained"
                );
                assert_eq!(state, state_before);
                assert_eq!(std::fs::read(&path).unwrap(), bytes_before);
                assert_eq!(publisher.cat.all(None).unwrap(), rows_before);
                assert!(
                    publisher
                        .cat
                        .dispatch(json!({"operation":"seen","file_unique_id":"must-not-bind"}))
                        .unwrap()
                        .is_null()
                );
                assert_eq!(
                    publisher
                        .cat
                        .dispatch(json!({"operation":"published","base":base,"content_key":key}))
                        .unwrap(),
                    false
                );
            }
            assert_eq!(
                serde_json::from_slice::<Value>(&std::fs::read(path).unwrap()).unwrap(),
                state
            );
            assert!(
                !native.scratch.exists(),
                "readback ignored invocation-owned codec storage"
            );
            assert_eq!(std::fs::read_dir(directory.join("tmp")).unwrap().count(), 0);
            assert_eq!(std::fs::read(&source).unwrap(), before);
        }

        let effects = _native::upload_effect::UploadEffect {
            tg: &tg,
            media: &native,
            data: &directory,
            runtime: &runtime,
        };
        let sources = [source.clone()];
        let ticker_key = source.file_stem().unwrap().to_str().unwrap();
        let ticker_path = directory.join("ticker-state.json");
        let mut ticker = _native::ticker_state::initial("ticker", 200);
        ticker["in_flight"] = json!({"key":ticker_key,"operation":"create",
            "set_name":"ticker1_by_YourEmojiBot","set_index":1,
            "expected_before":0,"title":"Fixture 1"});
        _native::atomic::write_json(&ticker_path, &ticker).unwrap();
        _native::ticker_recovery::recover(
            &effects,
            &mut ticker,
            &sources,
            &ticker_path,
            "Fixture",
            "ticker",
            None,
        )
        .unwrap();
        assert!(ticker["in_flight"].is_null());
        assert_eq!(ticker["done"], json!([ticker_key]));
        assert_eq!(
            ticker["sets"],
            json!([{"name":"ticker1_by_YourEmojiBot",
            "index":1,"count":1,"title":"Fixture 1"}])
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(&ticker_path).unwrap()).unwrap(),
            ticker
        );
        _native::ticker_recovery::verify_counts(&effects, &ticker).unwrap();
        ticker["in_flight"] = json!({"key":ticker_key,"operation":"add",
            "set_name":"ticker1_by_YourEmojiBot","set_index":1,"expected_before":1});
        _native::atomic::write_json(&ticker_path, &ticker).unwrap();
        let ticker_before = ticker.clone();
        let ticker_bytes = std::fs::read(&ticker_path).unwrap();
        let error = _native::ticker_recovery::recover(
            &effects,
            &mut ticker,
            &sources,
            &ticker_path,
            "Fixture",
            "ticker",
            None,
        )
        .unwrap_err();
        assert_eq!(error, "interrupted source comparison remains unknown");
        assert_eq!(ticker, ticker_before);
        assert_eq!(std::fs::read(&ticker_path).unwrap(), ticker_bytes);
        let error = _native::ticker_recovery::verify_counts(&effects, &ticker).unwrap_err();
        assert_eq!(
            error,
            "live set and recorded count disagree with no unresolved intent"
        );
        assert_eq!(ticker, ticker_before);
        assert_eq!(std::fs::read(&ticker_path).unwrap(), ticker_bytes);
        assert!(
            !native.scratch.exists(),
            "readback ignored invocation-owned codec storage"
        );
        assert_eq!(std::fs::read_dir(directory.join("tmp")).unwrap().count(), 0);
        assert_eq!(std::fs::read(&source).unwrap(), before);
    }));
    let requests = server.finish();
    result.unwrap();
    assert_eq!(requests.unwrap().len(), 19);
    std::fs::remove_dir_all(&directory).unwrap();
}
