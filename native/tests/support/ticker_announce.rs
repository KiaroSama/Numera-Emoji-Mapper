//! The copied ticker command must select the Worker's bot from its actual token option.
use super::{Commands, SET, download, listing, member};
use crate::bot_api::{Reply, Server, TOKEN};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn run(commands: &Commands<'_>, bytes: &[u8]) {
    for (token, bot) in [
        ("TELEGRAM_BOT_TOKEN", "coin"),
        ("GENERAL_BOT_TOKEN", "general"),
    ] {
        let directory = commands.root.join(format!("ticker-announce-{bot}"));
        let images = directory.join("src");
        std::fs::create_dir_all(&images).unwrap();
        let source = images.join("a.png");
        std::fs::write(&source, bytes).unwrap();
        let state = directory.join("state.json");
        let mut creation = Reply::new("createNewStickerSet", &[], json!({"ok":true,"result":true}));
        creation.fields = BTreeMap::from([
            ("user_id".into(), "111111111".into()),
            ("name".into(), SET.into()),
            ("title".into(), "T 1".into()),
            ("sticker_type".into(), "custom_emoji".into()),
            ("stickers".into(), json!([{"sticker":"attach://file0","format":"static","emoji_list":["🪙"],"keywords":["a"]}]).to_string()),
        ]);
        creation.files.insert("file0".into(), bytes.to_vec());
        creation.observation = Some((state.clone(), "/in_flight/key".into(), json!("a")));
        let mut replies = vec![
            Reply::new(
                "getMe",
                &[],
                json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
            ),
            creation,
            listing(&[member("announced-a")]),
        ];
        download(&mut replies, "announced-a", bytes);
        let body = json!({"bot":bot,"packs":[{"name":SET,"title":"T 1","index":1,"count":1}],"style":"cards"});
        let mut announcement = Reply::new("POST /publish", &[], json!({"ok":true}));
        announcement.fields.insert("json".into(), body.to_string());
        // The upload must be committed, but the send must not yet be recorded.
        announcement.observation = Some((state.clone(), "/in_flight".into(), Value::Null));
        replies.push(announcement);
        let expected = replies
            .iter()
            .map(|reply| (reply.method.clone(), reply.fields.clone()))
            .collect::<Vec<_>>();
        let mut server = Server::new(replies);
        let mut environment = commands.environment.clone();
        environment.insert(token.into(), TOKEN.into());
        environment.insert(
            "WORKER_PUBLISH_URL".into(),
            format!("{}/publish", server.base),
        );
        environment.insert(
            "WORKER_PUBLISH_SECRET".into(),
            "test-only-worker-secret".into(),
        );
        let announcer = Commands {
            environment: &environment,
            ..*commands
        };
        println!("ticker announcement command: {bot}");
        let completed = announcer.call(&images, &state, &server.base, &["--token-env", token]);
        assert_eq!(
            completed.code, 0,
            "{bot}: {} {}",
            completed.stdout, completed.stderr
        );
        let requests = server.finish().unwrap();
        assert_eq!(requests.len(), expected.len());
        for (request, (method, form)) in requests.iter().zip(expected) {
            assert_eq!(request.method, method);
            assert_eq!(request.form, form);
        }
        let announced = requests
            .iter()
            .filter(|request| request.method == "POST /publish")
            .collect::<Vec<_>>();
        assert_eq!(announced.len(), 1);
        assert_eq!(
            serde_json::from_str::<Value>(&announced[0].form["json"]).unwrap(),
            body
        );
        let observed = announced[0].observation.as_ref().unwrap();
        assert_eq!(observed["done"], json!(["a"]));
        assert_eq!(observed["sets"], body["packs"]);
        assert_eq!(observed["sent"], json!([]));
        let saved: Value = serde_json::from_slice(&std::fs::read(&state).unwrap()).unwrap();
        assert_eq!(saved["done"], json!(["a"]));
        assert_eq!(saved["sets"], body["packs"]);
        assert_eq!(saved["sent"], json!([SET]));
        assert!(saved["in_flight"].is_null());
        assert_eq!(std::fs::read(source).unwrap(), bytes);
        assert!(
            !requests
                .iter()
                .any(|request| request.method == "sendMessage")
        );
    }
}
