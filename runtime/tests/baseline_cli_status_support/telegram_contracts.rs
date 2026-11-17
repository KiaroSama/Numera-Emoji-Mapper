use crate::{
    identity::art,
    support::{
        http::{Reply, Server},
        *,
    },
};
use numera_runtime::{
    self as native,
    telegram::{ApiError, SetState},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Arc, Mutex},
};
fn request(source: &Path, create: bool, before: Option<u64>) -> Value {
    json!({"name":"fixture","source":source,"format":"static","create":create,"user_id":1,"emojis":["😀"],"keywords":["public"],"title":"Fixture","expected_before":before})
}
fn compare(
    tg: &native::telegram::Telegram,
    sticker: &Value,
    source: &str,
    _fmt: &str,
) -> native::telegram::Result<Option<bool>> {
    let bytes = tg.download_bytes(sticker["file_id"].as_str().unwrap(), 1)?;
    let image = image::load_from_memory(&bytes)
        .map_err(|e| ApiError::Invalid(e.to_string()))?
        .to_rgba8();
    let ours = native::codec_image::decoded(Path::new(source))
        .map_err(|e| ApiError::Invalid(e.message))?
        .rgba;
    Ok(Some(
        native::native_raster::same_rgba(&image, &ours)
            .map_err(|error| ApiError::Invalid(error.to_string()))?,
    ))
}
// tests.test_native_telegram_compat.NativeCoinTelegramContract.test_foreign_unknown_and_wrong_create_shape_do_not_attribute_by_count
#[test]
fn foreign_unknown_and_wrong_create_shape_never_attribute_by_count() {
    for mode in ["foreign", "unknown", "wrong-create"] {
        let f = Fixture::new();
        let source = art(&f, "numera-emoji-mapper-logo.png", "source.png");
        let other = art(&f, "numera-emoji-mapper-logo-circle.png", "foreign.png");
        let payloads = BTreeMap::from([
            ("ours".to_owned(), std::fs::read(&source).unwrap()),
            ("foreign".into(), std::fs::read(other).unwrap()),
        ]);
        let mutations = Arc::new(Mutex::new(Vec::new()));
        let sent = mutations.clone();
        let mut live =
            vec![json!({"file_id":"foreign","file_unique_id":"old","custom_emoji_id":"111111111"})];
        let bot = Server::new(move |r| {
            if r.verb == "GET" {
                return Reply::Bytes(200, payloads[r.method()].clone());
            }
            if r.method() == "getStickerSet" {
                return if mode == "unknown" && !sent.lock().unwrap().is_empty() {
                    Reply::json(json!([]))
                } else {
                    Reply::ok(json!({"stickers":live}))
                };
            }
            if r.method() == "getFile" {
                return Reply::ok(json!({"file_path":r.form()["file_id"]}));
            }
            let mut count = sent.lock().unwrap();
            count.push(r.method().to_owned());
            match mode {
                "foreign" => {
                    let first = count.len() == 1;
                    live.push(json!({"file_id":if first{"foreign"}else{"ours"},"file_unique_id":if first{"foreign-new"}else{"ours-new"},"custom_emoji_id":if first{"222222222"}else{"333333333"}}));
                    if !first {
                        return Reply::ok(json!(true));
                    }
                }
                "wrong-create" => {
                    live = vec![
                        json!({"file_id":"ours","file_unique_id":"1","custom_emoji_id":"1"}),
                        json!({"file_id":"ours","file_unique_id":"2","custom_emoji_id":"2"}),
                    ]
                }
                _ => {}
            }
            Reply::Disconnect
        });
        let result = native::telegram_mutation::mutate_with_wait(
            &bot.telegram(),
            &request(
                &source,
                mode == "wrong-create",
                if mode == "wrong-create" {
                    None
                } else {
                    Some(1)
                },
            ),
            compare,
            |_| {},
        );
        if mode == "foreign" {
            assert_eq!(result.unwrap(), 3);
            assert_eq!(
                *mutations.lock().unwrap(),
                ["addStickerToSet", "addStickerToSet"]
            );
        } else {
            assert!(
                matches!(result, Err(ApiError::Ambiguous(_))),
                "{mode}: {result:?}"
            );
            assert_eq!(mutations.lock().unwrap().len(), 1);
        }
    }
}
// tests.test_native_telegram_compat.NativeCoinTelegramContract.test_native_mutation_proof_return_classes_callback_and_bytes
#[test]
fn native_mutation_proofs_return_classes_callback_and_bytes() {
    let f = Fixture::new();
    let source = art(&f, "numera-emoji-mapper-logo.png", "source.png");
    let payload = std::fs::read(&source).unwrap();
    let served = payload.clone();
    let mut live =
        vec![json!({"file_id":"old","file_unique_id":"old-unique","custom_emoji_id":"111111111"})];
    let mut lost = false;
    let bot = Server::new(move |r| {
        if r.verb == "GET" {
            return Reply::Bytes(200, served.clone());
        }
        if r.headers
            .get("content-type")
            .is_some_and(|s| s.contains("multipart/form-data"))
        {
            assert_eq!(r.field("file0"), served);
            assert_eq!(r.field("name"), b"fixture");
        }
        match r.method() {
            "getStickerSet" => match r.form()["name"].as_str() {
                "missing" => Reply::json(json!({"ok":false,"description":"STICKERSET_INVALID"})),
                "unknown" => Reply::json(json!([])),
                _ => Reply::ok(json!({"name":"fixture","stickers":live})),
            },
            "getFile" => Reply::ok(json!({"file_path":"source.png"})),
            "getMe" => Reply::ok(json!({"username":"YourEmojiBot"})),
            "addStickerToSet" => {
                live.push(json!({"file_id":"new","file_unique_id":"new-unique","custom_emoji_id":"222222222"}));
                if !lost {
                    lost = true;
                    Reply::Disconnect
                } else {
                    Reply::ok(json!(true))
                }
            }
            "createNewStickerSet" => {
                live = vec![
                    json!({"file_id":"created","file_unique_id":"created-unique","custom_emoji_id":"333333333"}),
                ];
                Reply::ok(json!(true))
            }
            "deleteStickerSet" => {
                Reply::json(json!({"ok":false,"description":"definite rejection"}))
            }
            "sendMessage" => Reply::json(
                json!({"ok":false,"description":"retry after 301","parameters":{"retry_after":301}}),
            ),
            "replaceStickerInSet" => Reply::Disconnect,
            _ => Reply::ok(json!(true)),
        }
    });
    let mut tg = bot.telegram();
    assert_eq!(
        tg.read_call("getMe", &BTreeMap::new()).unwrap()["username"],
        "YourEmojiBot"
    );
    assert_eq!(tg.probe("missing"), (SetState::Missing, None));
    assert_eq!(tg.probe("unknown"), (SetState::Unknown, None));
    let (missing, result) = tg.probe("missing");
    assert_eq!(missing, SetState::Missing);
    assert!(
        result.is_none(),
        "typed absence carries no fabricated live count"
    );
    let (unknown, result) = tg.probe("unknown");
    assert_eq!(unknown, SetState::Unknown);
    assert!(result.is_none());
    let live = tg.probe("fixture").1.unwrap()["stickers"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(
        native::telegram::usable_fuids(&live).unwrap(),
        BTreeSet::from(["old-unique".into()])
    );
    assert_eq!(tg.download_bytes("old", 1).unwrap(), payload);
    assert_eq!(
        native::telegram_mutation::mutate_with_wait(
            &tg,
            &request(&source, false, Some(1)),
            compare,
            |_| {}
        )
        .unwrap(),
        2
    );
    assert_eq!(
        bot.methods()
            .iter()
            .filter(|m| *m == "addStickerToSet")
            .count(),
        1
    );
    assert!(
        native::telegram_mutation::mutate_with_wait(
            &tg,
            &request(&source, false, Some(2)),
            compare,
            |_| {}
        )
        .unwrap()
        .is_null()
    );
    assert_eq!(
        bot.methods()
            .iter()
            .filter(|m| *m == "addStickerToSet")
            .count(),
        2
    );
    assert!(
        native::telegram_mutation::mutate_with_wait(
            &tg,
            &request(&source, true, None),
            compare,
            |_| {}
        )
        .unwrap()
        .is_null()
    );
    assert_eq!(
        bot.methods()
            .iter()
            .filter(|m| *m == "createNewStickerSet")
            .count(),
        1
    );
    assert!(matches!(
        tg.call(
            "deleteStickerSet",
            &BTreeMap::from([("name".into(), "fixture".into())]),
            &[],
            1,
            None::<fn() -> native::telegram::Result<Option<bool>>>
        ),
        Err(ApiError::Rejected(_))
    ));
    tg.max_flood_wait = Some(300);
    assert!(
        matches!(tg.call("sendMessage",&BTreeMap::from([("chat_id".into(),"1".into()),("text".into(),"fixture".into())]),&[],1,None::<fn()->native::telegram::Result<Option<bool>>>),Err(ApiError::FloodWait{method,seconds:301})if method=="sendMessage")
    );
    assert!(matches!(
        tg.call_with_wait(
            "replaceStickerInSet",
            &BTreeMap::new(),
            &[],
            1,
            Some(|| Ok(None)),
            |_| {}
        ),
        Err(ApiError::Ambiguous(_))
    ));
    let calls = std::cell::Cell::new(0);
    let failure = tg
        .call_with_wait(
            "replaceStickerInSet",
            &BTreeMap::new(),
            &[],
            1,
            Some(|| {
                calls.set(calls.get() + 1);
                Err(ApiError::Invalid("caller callback failure".into()))
            }),
            |_| {},
        )
        .unwrap_err();
    assert_eq!(calls.get(), 1);
    assert!(matches!(failure,ApiError::Invalid(ref message)if message=="caller callback failure"));
    assert!(
        !tg.safe("url/bottest-only-token")
            .contains("test-only-token")
    );
}
