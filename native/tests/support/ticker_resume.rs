//! Original skipped-image recovery and capacity cases share one copied ticker command fixture.
use super::{Commands, SET, add, download, listing, member};
use crate::bot_api::{Reply, Server};
use _native::atomic;
use serde_json::{Value, json};

pub(super) fn run(commands: &Commands<'_>, inputs: &[Vec<u8>; 3]) {
    for variant in [
        "legacy", "behind", "masked", "foreign", "drift", "corrupt", "unknown", "intents",
    ] {
        let directory = commands.root.join(format!("resume-{variant}"));
        let images = directory.join("src");
        std::fs::create_dir_all(&images).unwrap();
        let paths = [
            images.join("a.png"),
            images.join("b.png"),
            images.join("c.png"),
        ];
        for (path, bytes) in
            paths
                .iter()
                .zip([b"".as_slice(), inputs[0].as_slice(), inputs[1].as_slice()])
        {
            std::fs::write(path, bytes).unwrap();
        }
        let path = directory.join("state.json");
        let count = if variant == "masked" { 2 } else { 0 };
        let mut state = json!({"base":"t","per_set":200,"done":[],"sent":[SET],
            "sets":[{"name":SET,"title":"T 1","count":count,"index":1}],"in_flight":null});
        if variant == "legacy" {
            state["in_flight"] = json!("b");
        }
        if matches!(variant, "behind" | "masked" | "foreign" | "unknown") {
            state["in_flight"] = json!({"key":"b","operation":"add","set_name":SET,"set_index":1,"expected_before":count});
        }
        if variant == "intents" {
            state["sets"] = json!([]);
        }
        if variant == "corrupt" {
            std::fs::write(&path, b"{\"base\":\"t\",\"done\":[trunca").unwrap();
        } else {
            atomic::write_json(&path, &state).unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        let ours = member("recovered-b");
        let foreign = member("foreign");
        let live = if matches!(variant, "behind" | "masked" | "drift") {
            vec![foreign.clone(), ours.clone()]
        } else {
            vec![if variant == "foreign" {
                foreign.clone()
            } else {
                ours.clone()
            }]
        };
        let mut replies = vec![Reply::new(
            "getMe",
            &[],
            json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
        )];
        if variant == "unknown" {
            replies.push(Reply::new("getStickerSet", &[("name", SET)], json!([])));
        }
        if !matches!(variant, "corrupt" | "intents" | "unknown") {
            replies.push(listing(&live));
            if variant != "drift" {
                if variant != "legacy" {
                    download(&mut replies, "foreign", &inputs[2]);
                }
                if variant != "foreign" {
                    download(&mut replies, "recovered-b", &inputs[0]);
                }
            }
            if matches!(variant, "legacy" | "behind" | "masked") {
                replies.push(listing(&live));
                replies.push(listing(&live));
                replies.push(add(
                    &path,
                    &inputs[1],
                    "c",
                    live.len() as u64,
                    json!({"ok":true,"result":true}),
                ));
            }
        }
        if variant == "intents" {
            let mut creation =
                Reply::new("createNewStickerSet", &[], json!({"ok":true,"result":true}));
            creation.fields = std::collections::BTreeMap::from([
                ("user_id".into(),"111111111".into()),("name".into(),SET.into()),("title".into(),"T 1".into()),
                ("sticker_type".into(),"custom_emoji".into()),("stickers".into(),json!([{"sticker":"attach://file0","format":"static","emoji_list":["🪙"],"keywords":["b"]}]).to_string()),
            ]);
            creation.files.insert("file0".into(), inputs[0].clone());
            creation.observation = Some((path.clone(), "/in_flight/key".into(), json!("b")));
            replies.push(creation);
            replies.push(listing(std::slice::from_ref(&ours)));
            download(&mut replies, "recovered-b", &inputs[0]);
            replies.push(listing(std::slice::from_ref(&ours)));
            replies.push(add(
                &path,
                &inputs[1],
                "c",
                1,
                json!({"ok":true,"result":true}),
            ));
        }
        let expected = replies
            .iter()
            .map(|r| (r.method.clone(), r.fields.clone()))
            .collect::<Vec<_>>();
        let mut server = Server::new(replies);
        println!("ticker resume command: {variant}");
        let result = commands.call(&images, &path, &server.base, &[]);
        let refused = matches!(variant, "foreign" | "drift" | "corrupt");
        assert_eq!(
            result.code,
            if refused { 4 } else { 3 },
            "{variant}: {} {}",
            result.stdout,
            result.stderr
        );
        if refused || variant == "unknown" {
            assert_eq!(std::fs::read(&path).unwrap(), before);
        } else {
            let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(saved["done"], json!(["b", "c"]));
            assert!(saved["in_flight"].is_null());
            assert_eq!(
                saved["sets"][0]["count"],
                if variant == "intents" {
                    2
                } else {
                    live.len() + 1
                }
            );
        }
        let requests = server.finish().unwrap();
        assert_eq!(requests.len(), expected.len());
        for (actual, (method, fields)) in requests.iter().zip(expected) {
            assert_eq!(actual.method, method);
            assert_eq!(actual.form, fields);
        }
        let intents = requests
            .iter()
            .filter_map(|r| r.observation.as_ref())
            .map(|v| &v["in_flight"])
            .collect::<Vec<_>>();
        assert_eq!(
            intents
                .iter()
                .map(|v| v["key"].as_str().unwrap())
                .collect::<Vec<_>>(),
            if refused || variant == "unknown" {
                vec![]
            } else if variant == "intents" {
                vec!["b", "c"]
            } else {
                vec!["c"]
            }
        );
        if variant == "intents" {
            assert_eq!(
                intents
                    .iter()
                    .map(|v| v["operation"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                ["create", "add"]
            );
        }
        for intent in intents {
            assert_eq!(intent["set_name"], SET);
        }
        for (source, bytes) in
            paths
                .iter()
                .zip([b"".as_slice(), inputs[0].as_slice(), inputs[1].as_slice()])
        {
            assert_eq!(std::fs::read(source).unwrap(), bytes);
        }
    }
    let directory = commands.root.join("capacity");
    std::fs::create_dir(&directory).unwrap();
    let source = directory.join("a.png");
    std::fs::write(&source, &inputs[0]).unwrap();
    let state = directory.join("unused-state.json");
    for (options, code) in [
        (vec!["--dry-run", "--per-set", "0"], 2),
        (vec!["--dry-run", "--per-set", "400"], 2),
        (vec!["--dry-run", "--start", "-5"], 2),
        (vec!["--dry-run", "--per-set", "200"], 0),
        (vec!["--dry-run"], 0),
    ] {
        let result = commands.call(&directory, &state, "http://127.0.0.1:9", &options);
        assert_eq!(result.code, code, "capacity {options:?}: {}", result.stderr);
        if code == 0 {
            assert!(result.stdout.contains("of up to 200"));
        }
        assert!(!state.exists());
        assert_eq!(std::fs::read(&source).unwrap(), inputs[0]);
    }
}
