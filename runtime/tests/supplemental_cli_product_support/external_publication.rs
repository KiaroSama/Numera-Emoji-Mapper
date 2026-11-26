use super::*;
use crate::http::{Reply, Server};
const BASE: &str = "fixture";
const NAME: &str = "fixtures1_by_fixturebot";

#[test]
fn external_publication_proves_cold_attribution_and_mutation_readback() {
    for (already_live, matching) in [(true, true), (false, true), (false, false)] {
        let mut app = Installation::new();
        let upload = frozen(STATIC);
        let path = app.seed(
            STATIC_KEY,
            "static",
            "source café.png",
            &upload,
            1096366658209276687,
        );
        let state_file = app.data.join("publish_fixture.json");
        let sets = if already_live {
            json!([{"fmt":"static","index":1,"name":NAME,"title":"Fixture 1","live":0,"logo":false,"keys":[]}])
        } else {
            json!([])
        };
        numera_runtime::atomic::write_json(
            &state_file,
            &json!({"base":BASE,"sets":sets,"sent":[NAME],"sent_full":[],"skipped":[]}),
        )
        .unwrap();
        // The comparison must own only its download and codec workspace.
        let incoming = app.data.join("tmp/download.dl");
        fs::create_dir(incoming.parent().unwrap()).unwrap();
        fs::write(&incoming, b"caller-owned input").unwrap();
        let readback = if matching {
            upload.clone()
        } else {
            frozen("static-codec/opaque.png")
        };
        let mut present = already_live;
        let service = Server::new(move |request| {
            if request.verb == "GET" {
                assert_eq!(
                    request.path,
                    format!("/file/bot{}/readback.png", read_only_api::TOKEN)
                );
                assert!(request.body.is_empty());
                return Reply::Bytes(200, readback.clone());
            }
            assert_eq!(request.verb, "POST");
            let method = request.method();
            assert_eq!(
                request.path,
                format!("/bot{}/{method}", read_only_api::TOKEN)
            );
            match method {
                "getMe" => {
                    assert!(request.form().is_empty());
                    Reply::ok(json!({"username":"fixturebot"}))
                }
                "createNewStickerSet" => {
                    assert!(!present);
                    assert_eq!(request.field("name"), NAME.as_bytes());
                    assert_eq!(request.field("user_id"), b"1");
                    assert_eq!(request.field("title"), b"Fixture 1");
                    assert_eq!(request.field("sticker_type"), b"custom_emoji");
                    assert_eq!(request.field("file0"), upload);
                    present = true;
                    Reply::ok(json!(true))
                }
                "getStickerSet" => {
                    assert!(present);
                    assert_eq!(
                        request.form(),
                        BTreeMap::from([("name".into(), NAME.into())])
                    );
                    Reply::ok(json!({"name":NAME,"stickers":[sticker()]}))
                }
                "getFile" => {
                    assert_eq!(
                        request.form(),
                        BTreeMap::from([("file_id".into(), "remote".into())])
                    );
                    Reply::ok(json!({"file_path":"readback.png"}))
                }
                _ => panic!("unexpected request"),
            }
        });
        app.api(service.base.clone());
        let result = app.run(
            &[
                "build-collection",
                "--base",
                BASE,
                "--title",
                "Fixture",
                "--formats",
                "static",
                "--user-id",
                "1",
                "--no-brand-logo",
            ],
            &app.data,
        );
        let mut expected = vec!["getMe"];
        if !already_live {
            expected.push("createNewStickerSet");
        }
        expected.extend(["getStickerSet", "getFile", "readback.png"]);
        if matching {
            expected.push("getStickerSet");
        }
        assert_eq!(
            service.methods(),
            expected,
            "{} {}",
            result.stdout,
            result.stderr
        );
        assert_eq!(
            result.code,
            if matching { 0 } else { 4 },
            "{} {}",
            result.stdout,
            result.stderr
        );
        let mut cat = app.catalog();
        assert_eq!(
            cat.dispatch(json!({"operation":"custom_id","base":BASE,"content_key":STATIC_KEY}))
                .unwrap(),
            if matching {
                json!(EMOJI_ID)
            } else {
                Value::Null
            }
        );
        assert_eq!(
            cat.dispatch(json!({"operation":"seen","file_unique_id":"unique"}))
                .unwrap(),
            if matching {
                json!(STATIC_KEY)
            } else {
                Value::Null
            }
        );
        let state: Value = serde_json::from_slice(&fs::read(&state_file).unwrap()).unwrap();
        if matching {
            assert!(state["in_flight"].is_null());
            assert_eq!(state["sets"][0]["keys"], json!([STATIC_KEY]));
        } else {
            assert_eq!(state["in_flight"]["key"], STATIC_KEY);
            assert_eq!(state["sets"], json!([]));
        }
        assert_eq!(fs::read(&path).unwrap(), frozen(STATIC));
        assert_eq!(fs::read(&incoming).unwrap(), b"caller-owned input");
        assert_eq!(fs::read_dir(app.data.join("tmp")).unwrap().count(), 1);
        for entry in fs::read_dir(app.base()).unwrap() {
            let entry = entry.unwrap();
            assert_eq!(entry.file_name(), "native-codec");
            empty(&entry.path());
        }
    }
}
