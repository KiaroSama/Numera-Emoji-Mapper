//! Original ticker bookkeeping through the actual copied command, wire and real codec readback.
use super::bot_api::{Reply, Server, TOKEN};
use super::tooling_fixture::Root;
use _native::{atomic, logging::RunLog, tooling_process};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

#[path = "ticker_announce.rs"]
mod announce;
#[path = "publication_commands.rs"]
mod publication_commands;
#[path = "ticker_resume.rs"]
mod resume;
#[path = "retained_cli.rs"]
mod retained_cli;
#[path = "ticker_unresolved.rs"]
mod unresolved;

struct Commands<'a> {
    root: &'a Path,
    executable: &'a Path,
    environment: &'a BTreeMap<String, String>,
    runtime: &'a tokio::runtime::Runtime,
    log: &'a RunLog,
    input: &'a Path,
}
impl Commands<'_> {
    fn call(
        &self,
        images: &Path,
        state: &Path,
        api: &str,
        extra: &[&str],
    ) -> tooling_process::Completed {
        let mut environment = self.environment.clone();
        environment.insert("NUMERA_TEST_API_BASE".into(), api.into());
        let mut argv = vec![
            self.executable.as_os_str().to_owned(),
            "build-pack".into(),
            "--base".into(),
            "t".into(),
            "--title".into(),
            "T".into(),
            "--user-id".into(),
            "111111111".into(),
            "--source-dir".into(),
            images.as_os_str().to_owned(),
            "--state".into(),
            state.as_os_str().to_owned(),
            "--token-env".into(),
            "GENERAL_BOT_TOKEN".into(),
        ];
        argv.extend(extra.iter().map(|value| std::ffi::OsString::from(*value)));
        self.capture(&argv, &environment)
    }
    fn capture(
        &self,
        argv: &[std::ffi::OsString],
        environment: &BTreeMap<String, String>,
    ) -> tooling_process::Completed {
        self.runtime
            .block_on(tooling_process::capture_with_input(
                self.root,
                argv,
                environment,
                self.input,
                Duration::from_secs(25),
                Duration::from_secs(8),
                self.log,
            ))
            .unwrap()
    }
}

const SET: &str = "t1_by_YourEmojiBot";
fn member(id: &str) -> Value {
    json!({"file_id":id,"file_unique_id":id})
}
fn listing(members: &[Value]) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", SET)],
        json!({"ok":true,"result":{"stickers":members}}),
    )
}
fn download(replies: &mut Vec<Reply>, id: &str, bytes: &[u8]) {
    let name = format!("{id}.png");
    replies.push(Reply::new(
        "getFile",
        &[("file_id", id)],
        json!({"ok":true,"result":{"file_path":name}}),
    ));
    let mut reply = Reply::new("getFile", &[], Value::Null);
    reply.method = format!("GET {name}");
    reply.download = Some(bytes.to_vec());
    replies.push(reply);
}
fn add(path: &Path, bytes: &[u8], key: &str, before: u64, answer: Value) -> Reply {
    let mut reply = Reply::new("addStickerToSet", &[], answer);
    reply.fields = BTreeMap::from([
        ("user_id".into(), "111111111".into()), ("name".into(), SET.into()),
        ("sticker".into(), json!({"sticker":"attach://file0","format":"static","emoji_list":["🪙"],"keywords":[key]}).to_string()),
    ]);
    reply.files.insert("file0".into(), bytes.to_vec());
    reply.observation = Some((
        path.into(),
        "/in_flight/expected_before".into(),
        json!(before),
    ));
    reply
}

pub fn run() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = Root::new("ticker bookkeeping café");
    let project = _native::tooling_environment::physical_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
    )
    .unwrap();
    let (executable, mut environment) = retained_cli::install(&project, &root.0);
    environment.insert("GENERAL_BOT_TOKEN".into(), TOKEN.into());
    environment.insert("BRAND_LOGO_BOTS".into(), String::new());
    let input = root.0.join("empty-input");
    std::fs::write(&input, b"").unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    let inputs = ["opaque", "alpha-edge", "transparent-rgb"].map(|name| {
        let row = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == name)
            .unwrap();
        let path = project.join(row["input"].as_str().unwrap());
        assert_eq!(
            _native::tooling_manifest::digest(&path).unwrap(),
            row["input_sha256"]
        );
        std::fs::read(path).unwrap()
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let log = RunLog::open(&root.0, "ticker-bookkeeping-owner", vec![TOKEN.into()]);
    let old = member("live0");
    let foreign = member("foreign");
    let ours = member("ours2");
    let commands = Commands {
        root: &root.0,
        executable: &executable,
        environment: &environment,
        runtime: &runtime,
        log: &log,
        input: &input,
    };
    for variant in ["retried", "foreign-readback", "unknown-readback", "clean"] {
        let directory = root.0.join(variant);
        let images = directory.join("src");
        std::fs::create_dir_all(&images).unwrap();
        let paths = [images.join("b.png"), images.join("c.png")];
        for (path, bytes) in paths.iter().zip(&inputs) {
            std::fs::write(path, bytes).unwrap();
        }
        let state = directory.join("state.json");
        let original = json!({"base":"t","per_set":200,"done":["a"],"sent":[SET],
            "sets":[{"name":SET,"title":"T 1","count":1,"index":1}],"in_flight":null});
        atomic::write_json(&state, &original).unwrap();
        let mut replies = vec![
            Reply::new(
                "getMe",
                &[],
                json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
            ),
            listing(std::slice::from_ref(&old)),
            listing(std::slice::from_ref(&old)),
            add(
                &state,
                &inputs[0],
                "b",
                1,
                if variant == "clean" {
                    json!({"ok":true,"result":true})
                } else {
                    json!([])
                },
            ),
        ];
        if variant != "clean" {
            replies.push(listing(&[old.clone(), foreign.clone()]));
            download(&mut replies, "foreign", &inputs[2]);
            replies.push(add(
                &state,
                &inputs[0],
                "b",
                1,
                json!({"ok":true,"result":true}),
            ));
            if variant == "unknown-readback" {
                replies.push(Reply::new("getStickerSet", &[("name", SET)], json!([])));
            } else {
                let live = if variant == "foreign-readback" {
                    vec![old.clone(), foreign.clone()]
                } else {
                    vec![old.clone(), foreign.clone(), ours.clone()]
                };
                replies.push(listing(&live));
                download(&mut replies, "foreign", &inputs[2]);
                if variant == "retried" {
                    download(&mut replies, "ours2", &inputs[0]);
                }
            }
        }
        if matches!(variant, "retried" | "clean") {
            let live = if variant == "clean" {
                vec![old.clone(), ours.clone()]
            } else {
                vec![old.clone(), foreign.clone(), ours.clone()]
            };
            replies.push(listing(&live));
            replies.push(add(
                &state,
                &inputs[1],
                "c",
                live.len() as u64,
                json!({"ok":true,"result":true}),
            ));
        }
        let expected = replies
            .iter()
            .map(|r| (r.method.clone(), r.fields.clone()))
            .collect::<Vec<_>>();
        let mut server = Server::new(replies);
        println!("ticker bookkeeping command: {variant}");
        let completed = commands.call(&images, &state, &server.base, &[]);
        let success = matches!(variant, "retried" | "clean");
        assert_eq!(
            completed.code,
            if success { 0 } else { 3 },
            "{variant}: {} {}",
            completed.stdout,
            completed.stderr
        );
        let saved: Value = serde_json::from_slice(&std::fs::read(&state).unwrap()).unwrap();
        assert_eq!(
            saved["done"],
            if success {
                json!(["a", "b", "c"])
            } else {
                json!(["a"])
            }
        );
        assert_eq!(
            saved["sets"][0]["count"],
            if variant == "retried" {
                4
            } else if variant == "clean" {
                3
            } else {
                1
            }
        );
        assert_eq!(saved["sent"], json!([SET]));
        if success {
            assert!(saved["in_flight"].is_null());
        } else {
            assert_eq!(saved["in_flight"]["key"], "b");
            assert_eq!(saved["in_flight"]["expected_before"], 1);
            assert_eq!(saved["in_flight"]["operation"], "add");
        }
        let requests = server.finish().unwrap();
        assert_eq!(requests.len(), expected.len());
        for (request, (method, fields)) in requests.iter().zip(&expected) {
            assert_eq!(&request.method, method);
            assert_eq!(&request.form, fields);
        }
        let mutations = requests
            .iter()
            .filter(|r| r.method == "addStickerToSet")
            .collect::<Vec<_>>();
        let intents = mutations
            .iter()
            .map(|r| &r.observation.as_ref().unwrap()["in_flight"])
            .collect::<Vec<_>>();
        assert_eq!(
            intents
                .iter()
                .map(|i| i["key"].as_str().unwrap())
                .collect::<Vec<_>>(),
            if variant == "retried" {
                vec!["b", "b", "c"]
            } else if variant == "clean" {
                vec!["b", "c"]
            } else {
                vec!["b", "b"]
            }
        );
        assert_eq!(
            intents
                .iter()
                .map(|i| i["expected_before"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            if variant == "retried" {
                vec![1, 1, 3]
            } else if variant == "clean" {
                vec![1, 2]
            } else {
                vec![1, 1]
            }
        );
        for intent in intents {
            assert_eq!(intent["operation"], "add");
            assert_eq!(intent["set_name"], SET);
            assert_eq!(intent["set_index"], 1);
            assert_eq!(intent["format"], "static");
            assert!(intent["started_utc"].as_str().unwrap().ends_with('Z'));
        }
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "getStickerSet")
                .count(),
            if variant == "retried" {
                5
            } else if variant == "clean" {
                3
            } else {
                4
            }
        );
        for (path, bytes) in paths.iter().zip(&inputs) {
            assert_eq!(&std::fs::read(path).unwrap(), bytes);
        }
        assert!(!_native::process_owner::cleanup_pending(&root.0));
        assert!(
            !std::fs::read_dir(root.0.join("logs"))
                .unwrap()
                .any(|entry| entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("ticker-codec-"))
        );
    }
    resume::run(&commands, &inputs);
    unresolved::run(&commands, &inputs);
    announce::run(&commands, &inputs[0]);
    publication_commands::run(&commands, &project);
    assert!(!_native::process_owner::cleanup_pending(&root.0));
    assert!(
        !std::fs::read_dir(root.0.join("logs"))
            .unwrap()
            .any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("ticker-codec-")
            })
    );
    runtime.shutdown_timeout(Duration::from_secs(1));
    log.finish(0);
    drop(log);
    root.clean().unwrap();
}
