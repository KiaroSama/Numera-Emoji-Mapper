//! Unresolved ticker mutations stop at the real command boundary without losing their intent.
use super::{Commands, SET, add, download, listing, member};
use crate::bot_api::{Reply, Server};
use _native::atomic;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

const SECOND: &str = "t2_by_YourEmojiBot";

fn create(path: &Path, name: &str, index: u64, key: &str, bytes: &[u8], answer: Value) -> Reply {
    let mut reply = Reply::new("createNewStickerSet", &[], answer);
    reply.fields = BTreeMap::from([
        ("user_id".into(), "111111111".into()),
        ("name".into(), name.into()),
        ("title".into(), format!("T {index}")),
        ("sticker_type".into(), "custom_emoji".into()),
        ("stickers".into(), json!([{"sticker":"attach://file0","format":"static","emoji_list":["🪙"],"keywords":[key]}]).to_string()),
    ]);
    reply.files.insert("file0".into(), bytes.to_vec());
    reply.observation = Some((path.into(), "/in_flight/key".into(), json!(key)));
    reply
}

fn set(name: &str, members: &[Value]) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", name)],
        json!({"ok":true,"result":{"stickers":members}}),
    )
}

fn missing(name: &str) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", name)],
        json!({"ok":false,"description":"Bad Request: STICKERSET_INVALID"}),
    )
}

fn unknown(name: &str) -> Reply {
    Reply::new("getStickerSet", &[("name", name)], json!([]))
}

fn me() -> Reply {
    Reply::new(
        "getMe",
        &[],
        json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
    )
}

fn checked_call(
    commands: &Commands<'_>,
    images: &Path,
    path: &Path,
    replies: Vec<Reply>,
    extra: &[&str],
    code: i32,
) -> Vec<crate::bot_api::Request> {
    let expected = replies
        .iter()
        .map(|reply| (reply.method.clone(), reply.fields.clone()))
        .collect::<Vec<_>>();
    let mut server = Server::new(replies);
    let result = commands.call(images, path, &server.base, extra);
    assert_eq!(
        result.code,
        code,
        "{}: {} {}",
        path.display(),
        result.stdout,
        result.stderr
    );
    let requests = server.finish().unwrap();
    assert_eq!(requests.len(), expected.len());
    for (request, (method, form)) in requests.iter().zip(expected) {
        assert_eq!(request.method, method);
        assert_eq!(request.form, form);
    }
    assert!(
        !requests
            .iter()
            .any(|request| request.method == "sendMessage")
    );
    requests
}

pub(super) fn run(commands: &Commands<'_>, inputs: &[Vec<u8>; 3]) {
    for variant in [
        "ours",
        "foreign",
        "unreadable",
        "unknown",
        "missing",
        "extra",
        "add",
    ] {
        let directory = commands.root.join(format!("unresolved-{variant}"));
        let images = directory.join("src");
        std::fs::create_dir_all(&images).unwrap();
        let paths = [
            images.join("a.png"),
            images.join("b.png"),
            images.join("c.png"),
        ];
        // The third input is deliberately blank foreign content, never a queued upload.
        let payloads = [&inputs[0], &inputs[1], &inputs[0]];
        for (path, bytes) in paths.iter().zip(payloads) {
            std::fs::write(path, bytes).unwrap();
        }
        let path = directory.join("state.json");
        // Only the positive control suppresses final notification; refusal cases must never reach it.
        let state = json!({"base":"t","per_set":200,"done":[],"sent":if variant == "ours" {json!([SET])} else {json!([])},"sets":[],"in_flight":null});
        atomic::write_json(&path, &state).unwrap();
        let ours = member("created-a");
        let mut replies = vec![
            me(),
            create(
                &path,
                SET,
                1,
                "a",
                &inputs[0],
                if variant == "add" {
                    json!({"ok":true,"result":true})
                } else {
                    json!({"ok":true})
                },
            ),
        ];
        match variant {
            "unknown" => replies.push(unknown(SET)),
            "missing" => replies.push(missing(SET)),
            "extra" => replies.push(listing(&[ours.clone(), member("stranger")])),
            _ => {
                replies.push(listing(std::slice::from_ref(&ours)));
                if variant == "unreadable" {
                    replies.push(Reply::new(
                        "getFile",
                        &[("file_id", "created-a")],
                        json!({"ok":true,"result":{}}),
                    ));
                } else {
                    download(
                        &mut replies,
                        "created-a",
                        &inputs[usize::from(variant == "foreign")],
                    );
                }
            }
        }
        if matches!(variant, "ours" | "add") {
            replies.push(listing(std::slice::from_ref(&ours)));
            replies.push(add(
                &path,
                &inputs[1],
                "b",
                1,
                if variant == "add" {
                    json!({"ok":true})
                } else {
                    json!({"ok":true,"result":true})
                },
            ));
            if variant == "ours" {
                replies.push(listing(&[ours.clone(), member("added-b")]));
                replies.push(add(
                    &path,
                    &inputs[0],
                    "c",
                    2,
                    json!({"ok":true,"result":true}),
                ));
            }
        }
        println!("unresolved ticker command: {variant}");
        let requests = checked_call(
            commands,
            &images,
            &path,
            replies,
            &[],
            if variant == "ours" { 0 } else { 3 },
        );
        let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["sent"], state["sent"]);
        if variant == "ours" {
            assert_eq!(saved["done"], json!(["a", "b", "c"]));
            assert_eq!(
                saved["sets"],
                json!([{"name":SET,"title":"T 1","index":1,"count":3}])
            );
            assert!(saved["in_flight"].is_null());
        } else if variant == "add" {
            assert_eq!(saved["done"], json!(["a"]));
            assert_eq!(saved["sets"][0]["count"], 1);
            assert_eq!(saved["in_flight"]["key"], "b");
            assert_eq!(saved["in_flight"]["operation"], "add");
            assert_eq!(saved["in_flight"]["expected_before"], 1);
        } else {
            assert_eq!(saved["done"], json!([]));
            assert_eq!(saved["sets"], json!([]));
            assert_eq!(saved["in_flight"]["key"], "a");
            assert_eq!(saved["in_flight"]["operation"], "create");
            assert_eq!(saved["in_flight"]["set_name"], SET);
            assert_eq!(saved["in_flight"]["expected_before"], 0);
        }
        let intents = requests
            .iter()
            .filter_map(|request| request.observation.as_ref())
            .map(|state| &state["in_flight"])
            .collect::<Vec<_>>();
        assert_eq!(
            intents
                .iter()
                .map(|intent| intent["key"].as_str().unwrap())
                .collect::<Vec<_>>(),
            if variant == "ours" {
                vec!["a", "b", "c"]
            } else if variant == "add" {
                vec!["a", "b"]
            } else {
                vec!["a"]
            }
        );
        for intent in intents {
            assert_eq!(intent["set_name"], SET);
            assert_eq!(intent["set_index"], 1);
            assert_eq!(intent["format"], "static");
            assert!(intent["started_utc"].as_str().unwrap().ends_with('Z'));
        }
        for (source, bytes) in paths.iter().zip(payloads) {
            assert_eq!(&std::fs::read(source).unwrap(), bytes);
        }
    }
    later_sets(commands, inputs);
    recorded_sets(commands, &inputs[0]);
}

fn later_sets(commands: &Commands<'_>, inputs: &[Vec<u8>; 3]) {
    for variant in [
        "landed",
        "missing",
        "foreign",
        "extra",
        "unreadable",
        "unknown",
    ] {
        let directory = commands.root.join(format!("later-create-{variant}"));
        let images = directory.join("src");
        std::fs::create_dir_all(&images).unwrap();
        for (key, bytes) in ["a", "b"].iter().zip(inputs) {
            std::fs::write(images.join(format!("{key}.png")), bytes).unwrap();
        }
        let path = directory.join("state.json");
        let state = json!({"base":"t","per_set":1,"done":["a"],"sent":[SET,SECOND],"sets":[{"name":SET,"title":"T 1","count":1,"index":1}],"in_flight":{"key":"b","operation":"create","set_name":SECOND,"set_index":2,"expected_before":0,"title":"T 2"}});
        atomic::write_json(&path, &state).unwrap();
        let before = std::fs::read(&path).unwrap();
        let ours = member("created-b");
        let mut replies = vec![me()];
        match variant {
            "unknown" => replies.push(unknown(SECOND)),
            "missing" => replies.push(missing(SECOND)),
            "extra" => replies.push(set(SECOND, &[ours.clone(), member("extra")])),
            _ => {
                replies.push(set(SECOND, std::slice::from_ref(&ours)));
                if variant == "unreadable" {
                    replies.push(Reply::new(
                        "getFile",
                        &[("file_id", "created-b")],
                        json!({"ok":true,"result":{}}),
                    ));
                } else {
                    download(
                        &mut replies,
                        "created-b",
                        &inputs[if variant == "foreign" { 2 } else { 1 }],
                    );
                }
            }
        }
        if matches!(variant, "landed" | "missing") {
            replies.push(listing(&[member("recorded-a")]));
            if variant == "landed" {
                replies.push(set(SECOND, std::slice::from_ref(&ours)));
            } else {
                replies.push(create(
                    &path,
                    SECOND,
                    2,
                    "b",
                    &inputs[1],
                    json!({"ok":true,"result":true}),
                ));
                replies.push(set(SECOND, std::slice::from_ref(&ours)));
                download(&mut replies, "created-b", &inputs[1]);
            }
        }
        println!("later ticker create command: {variant}");
        let success = matches!(variant, "landed" | "missing");
        let requests = checked_call(
            commands,
            &images,
            &path,
            replies,
            &["--per-set", "1"],
            if success {
                0
            } else if matches!(variant, "foreign" | "extra") {
                4
            } else {
                3
            },
        );
        if success {
            let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(saved["done"], json!(["a", "b"]));
            assert_eq!(
                saved["sets"],
                json!([{"name":SET,"title":"T 1","count":1,"index":1},{"name":SECOND,"title":"T 2","count":1,"index":2}])
            );
            assert_eq!(saved["sent"], state["sent"]);
            assert!(saved["in_flight"].is_null());
        } else {
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
        let mutations = requests
            .iter()
            .filter_map(|request| request.observation.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(mutations.len(), usize::from(variant == "missing"));
        if let Some(observed) = mutations.first() {
            assert_eq!(observed["done"], json!(["a"]));
            assert_eq!(observed["sets"], state["sets"]);
            let intent = &observed["in_flight"];
            assert_eq!(intent["key"], "b");
            assert_eq!(intent["operation"], "create");
            assert_eq!(intent["set_name"], SECOND);
            assert_eq!(intent["set_index"], 2);
            assert_eq!(intent["expected_before"], 0);
            assert_eq!(intent["title"], "T 2");
        }
        for (key, bytes) in ["a", "b"].iter().zip(inputs) {
            assert_eq!(
                std::fs::read(images.join(format!("{key}.png"))).unwrap(),
                *bytes
            );
        }
    }
}

fn recorded_sets(commands: &Commands<'_>, bytes: &[u8]) {
    for variant in ["deleted", "shrunken", "unknown", "different-base"] {
        let directory = commands.root.join(format!("recorded-{variant}"));
        let images = directory.join("src");
        std::fs::create_dir_all(&images).unwrap();
        let source = images.join("a.png");
        std::fs::write(&source, bytes).unwrap();
        let path = directory.join("state.json");
        let state = json!({"base":if variant == "different-base" {"OTHER"} else {"t"},"per_set":200,"done":["x"],"sent":[],"sets":[{"name":SET,"title":"T 1","count":5,"index":1}],"in_flight":null});
        atomic::write_json(&path, &state).unwrap();
        let before = std::fs::read(&path).unwrap();
        let mut replies = vec![me()];
        match variant {
            "deleted" => replies.push(missing(SET)),
            "shrunken" => replies.push(listing(&[member("f1"), member("f2"), member("f3")])),
            "unknown" => replies.push(unknown(SET)),
            _ => {}
        }
        println!("recorded ticker integrity command: {variant}");
        checked_call(
            commands,
            &images,
            &path,
            replies,
            &[],
            if variant == "unknown" { 3 } else { 4 },
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read(source).unwrap(), bytes);
    }
}
