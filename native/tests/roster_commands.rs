//! Actual roster commands preserve untouched families and refuse incomplete live reads.
use _native::config::Config;
use rusqlite::Connection;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use bot_api::{Reply, Server, TOKEN};
use tooling_fixture::Root;

fn read(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn snapshot(directory: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut entries = BTreeMap::new();
    let mut pending = vec![directory.to_path_buf()];
    let mut left = 64;
    while let Some(path) = pending.pop() {
        assert!(left > 0, "roster command inventory exceeded its bound");
        left -= 1;
        _native::tooling_environment::reject_links(&path, "roster fixture cannot follow links")
            .unwrap();
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        let relative = path.strip_prefix(directory).unwrap().to_path_buf();
        if metadata.is_dir() {
            entries.insert(relative, None);
            pending.extend(
                std::fs::read_dir(path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
        } else {
            assert!(metadata.is_file() && metadata.len() <= 65536);
            entries.insert(relative, Some(std::fs::read(path).unwrap()));
        }
    }
    entries
}
fn listing(name: &str, id: &str) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", name)],
        json!({"ok":true,"result":{"stickers":[{"custom_emoji_id":id,"emoji":"₿","is_animated":false,"is_video":false}]}}),
    )
}
fn fixture(root: &Path, two_sets: bool) -> (Config, BTreeMap<String, String>) {
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/panel.html"), b"isolated fixture").unwrap();
    std::fs::write(root.join("pyproject.toml"), b"# isolated fixture\n").unwrap();
    let data = root.join("collection");
    std::fs::create_dir(&data).unwrap();
    let db = Connection::open(data.join("catalog.db")).unwrap();
    db.execute_batch(
        "CREATE TABLE items(content_key TEXT,keywords TEXT,file_path TEXT);
        CREATE TABLE publications(content_key TEXT,custom_emoji_id TEXT);",
    )
    .unwrap();
    db.close().unwrap();
    _native::atomic::write_json(
        &data.join("publish_mypacks.json"),
        &json!({"sets":[{"name":"gen1_by_bot","title":"General 1","index":1}]}),
    )
    .unwrap();
    let mut sets = vec![json!({"name":"coin1_by_bot","title":"Coins 1","index":1})];
    if two_sets {
        sets.push(json!({"name":"coin2_by_bot","title":"Coins 2","index":2}));
    }
    _native::atomic::write_json(
        &root.join("coins/rebuild_dedup_state.json"),
        &json!({"sets":sets}),
    )
    .unwrap();
    _native::atomic::write_json(
        &root.join("coins/ticker_to_id.json"),
        &json!({"btc":"c-new","xbt":"c-new"}),
    )
    .unwrap();
    let packs = root.join("packs");
    std::fs::create_dir(&packs).unwrap();
    for (extension, bytes) in [
        (
            "json",
            b"{\"set_name\":\"gen1_by_bot\",\"emoji\":[]}".as_slice(),
        ),
        ("md", b"# untouched general report\n".as_slice()),
        ("html", b"<!doctype html><p>untouched</p>".as_slice()),
    ] {
        std::fs::write(packs.join(format!("gen1_by_bot.{extension}")), bytes).unwrap();
    }
    let old = json!({"packs":[
        {"set_name":"gen1_by_bot","title":"General 1","family":"general","pack_index":1,"link":"l","count":1},
        {"set_name":"coin1_by_bot","title":"Coins 1","family":"coins","pack_index":1,"link":"l","count":1}],
        "by_current_id":{"g1":{"set":"gen1_by_bot","slot":1,"index":0},"c-old":{"set":"coin1_by_bot","slot":1,"index":0}},
        "by_source_id":{"src-g":"g1","src-c":"c-old"},"by_previous_id":{"g0":"g1"},
        "inputs":{"catalog.db":[1,1],"publish_mypacks.json":[2,2],"rebuild_dedup_state.json":[3,3],"ticker_to_id.json":[4,4]}});
    _native::atomic::write_json(&packs.join("index.json"), &old).unwrap();
    let environment = BTreeMap::from([
        ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
        ("COLLECTION_PACK_BASE".into(), "mypacks".into()),
        ("TELEGRAM_BOT_TOKEN".into(), TOKEN.into()),
    ]);
    (
        Config::load_with_environment(root.into(), &environment).unwrap(),
        environment,
    )
}

struct Commands<'a> {
    root: &'a Path,
    executable: &'a Path,
    environment: &'a BTreeMap<String, String>,
    runtime: &'a tokio::runtime::Runtime,
    log: &'a _native::logging::RunLog,
}
impl Commands<'_> {
    fn run(
        &self,
        directory: &Path,
        settings: &BTreeMap<String, String>,
        options: &[&str],
    ) -> _native::tooling_process::Completed {
        let mut environment = self.environment.clone();
        for key in [
            "GENERAL_BOT_TOKEN",
            "TELEGRAM_BOT_TOKEN",
            "COLLECTION_PACK_BASE",
            "COIN_EMOJI_DIR",
            "BRAND_LOGO_PATH",
        ] {
            environment.remove(key);
        }
        environment.extend(settings.clone());
        assert!(environment["NUMERA_TEST_API_BASE"].starts_with("http://127.0.0.1:"));
        let child = directory.join(self.executable.file_name().unwrap());
        std::fs::rename(self.executable, &child).unwrap();
        let mut args = vec![child.into_os_string(), "pack-manifest".into()];
        args.extend(options.iter().map(|value| std::ffi::OsString::from(*value)));
        let result = self.runtime.block_on(_native::tooling_process::capture(
            self.root,
            &args,
            &environment,
            Duration::from_secs(5),
            Duration::from_secs(3),
            self.log,
        ));
        std::fs::rename(
            directory.join(self.executable.file_name().unwrap()),
            self.executable,
        )
        .unwrap();
        result.unwrap()
    }
}

fn native_lifecycle(commands: &Commands<'_>) {
    let directory = commands.root.join("native lifecycle");
    let (config, mut settings) = fixture(&directory, false);
    settings.insert("GENERAL_BOT_TOKEN".into(), TOKEN.into());
    let data = directory.join("collection");
    let database = data.join("catalog.db");
    _native::atomic::write_json(
        &data.join("publish_mypacks.json"),
        &json!({"sets":[{
            "name":"gen1_by_bot","title":"Fixture <&>","index":1,"logo":false
        }]}),
    )
    .unwrap();
    let coin_map = directory.join("coins/ticker_to_id.json");
    _native::atomic::write_json(&coin_map, &json!({"bbb":"222222222","aaa":"222222222"})).unwrap();
    let db = Connection::open(&database).unwrap();
    db.execute(
        "INSERT INTO items VALUES(?1,?2,?3)",
        (
            "s:fixture",
            "[\"fixture\",\"premium-id:444444444\",\"premium-id:111111111\"]",
            "missing.png",
        ),
    )
    .unwrap();
    db.execute(
        "INSERT INTO publications VALUES(?1,?2)",
        ("s:fixture", "111111111"),
    )
    .unwrap();
    db.close().unwrap();
    let mut server = Server::new(vec![
        listing("coin1_by_bot", "222222222"),
        listing("gen1_by_bot", "111111111"),
        listing("gen1_by_bot", "333333333"),
        Reply::new(
            "getStickerSet",
            &[("name", "coin1_by_bot")],
            json!({"ok":false,"description":"STICKERSET_INVALID"}),
        ),
        listing("coin1_by_bot", "222222222"),
        listing("gen1_by_bot", "333333333"),
        listing("coin1_by_bot", "222222222"),
        listing("gen1_by_bot", "333333333"),
    ]);
    settings.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
    let packs = directory.join("packs");
    let result = commands.run(&directory, &settings, &["--refresh"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let index = read(&packs.join("index.json"));
    assert_eq!(index["pack_count"], 2);
    assert_eq!(index["emoji_count"], 2);
    assert_eq!(index["by_current_id"]["111111111"]["slot"], 1);
    assert_eq!(index["by_source_id"], json!({"444444444":"111111111"}));
    let coin_document = packs.join("coin1_by_bot.json");
    assert_eq!(read(&coin_document)["emoji"][0]["name"], "aaa, bbb");
    let general_document = packs.join("gen1_by_bot.json");
    let general = read(&general_document);
    assert_eq!(general["emoji"][0]["role"], "emoji");
    assert_eq!(general["title"], "Fixture <&>");
    let html = std::fs::read_to_string(packs.join("gen1_by_bot.html")).unwrap();
    for marker in [
        "id=\"bg\"",
        "id=\"animLabel\"",
        "id=\"roster\"",
        "Fixture &lt;&amp;&gt;",
    ] {
        assert!(
            html.contains(marker),
            "missing original roster HTML marker {marker}"
        );
    }
    assert_eq!(commands.run(&directory, &settings, &["--check"]).code, 0);
    let coin_bytes = std::fs::read(&coin_document).unwrap();
    let coin_state = directory.join("coins/rebuild_dedup_state.json");
    let recorded = index["inputs"]["rebuild_dedup_state.json"].clone();
    let mut changed = std::fs::read(&coin_state).unwrap();
    changed.push(b' ');
    std::fs::write(&coin_state, changed).unwrap();
    let db = Connection::open(&database).unwrap();
    db.execute("UPDATE publications SET custom_emoji_id=?1", ["333333333"])
        .unwrap();
    db.close().unwrap();
    let inputs = snapshot(&data);
    let result = commands.run(&directory, &settings, &["--refresh", "--family", "general"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(snapshot(&data), inputs);
    let index = read(&packs.join("index.json"));
    assert_eq!(std::fs::read(&coin_document).unwrap(), coin_bytes);
    assert_eq!(index["inputs"]["rebuild_dedup_state.json"], recorded);
    assert_eq!(index["by_current_id"]["222222222"]["set"], "coin1_by_bot");
    assert_eq!(index["by_previous_id"]["111111111"], "333333333");
    assert_eq!(index["by_source_id"]["444444444"], "333333333");
    assert_eq!(commands.run(&directory, &settings, &["--check"]).code, 3);
    let before = snapshot(&packs);
    let result = commands.run(&directory, &settings, &["--refresh"]);
    assert_ne!(result.code, 0);
    assert_eq!(snapshot(&packs), before);
    assert_eq!(snapshot(&data), inputs);
    settings.insert("COLLECTION_PACK_BASE".into(), String::new());
    std::fs::write(packs.join("index.json"), b"{broken").unwrap();
    std::fs::write(&coin_map, b"{\"aaa\":222222222,\"bbb\":\"222222222\"}").unwrap();
    let result = commands.run(&directory, &settings, &["--refresh", "--family", "coins"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(read(&coin_document)["emoji"][0]["name"], "aaa, bbb");
    settings.insert("COLLECTION_PACK_BASE".into(), "mypacks".into());
    std::fs::write(&coin_map, b"{broken").unwrap();
    for options in [vec!["--refresh", "--family", "general"], vec!["--refresh"]] {
        let result = commands.run(&directory, &settings, &options);
        assert_eq!(result.code, 0, "{}", result.stderr);
    }
    let mut index = read(&packs.join("index.json"));
    index["inputs"]["publish_old.json"] = json!([0, 0]);
    _native::atomic::write_json(&packs.join("index.json"), &index).unwrap();
    let before = snapshot(&packs);
    assert_eq!(commands.run(&directory, &settings, &["--check"]).code, 3);
    assert_eq!(snapshot(&packs), before);
    assert_eq!(snapshot(&data), inputs);
    assert!(_native::roster_cli::stale(&config).unwrap().0);
    let requests = server.finish().unwrap();
    let expected = [
        "coin1_by_bot",
        "gen1_by_bot",
        "gen1_by_bot",
        "coin1_by_bot",
        "coin1_by_bot",
        "gen1_by_bot",
        "coin1_by_bot",
        "gen1_by_bot",
    ];
    assert_eq!(requests.len(), expected.len());
    for (request, name) in requests.iter().zip(expected) {
        assert_eq!(request.method, "getStickerSet");
        assert_eq!(request.form, BTreeMap::from([("name".into(), name.into())]));
    }
}

#[test]
fn original_family_refresh_and_usage_use_actual_commands_and_preserve_other_family_bytes() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = Root::new("roster command café");
    let executable = root.0.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let environment =
        _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
    let environment =
        _native::tooling_environment::environment(&root.0, &environment, false).unwrap();
    let log = _native::logging::RunLog::open(&root.0, "roster-command-owner", vec![TOKEN.into()]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let commands = Commands {
        root: &root.0,
        executable: &executable,
        environment: &environment,
        runtime: &runtime,
        log: &log,
    };
    native_lifecycle(&commands);
    for label in ["coins", "incomplete", "usage"] {
        let directory = root.0.join(label);
        let (config, settings) = fixture(&directory, label == "incomplete");
        let packs = directory.join("packs");
        let before = snapshot(&packs);
        let data = snapshot(&directory.join("collection"));
        let coins = snapshot(&directory.join("coins"));
        let replies = match label {
            "coins" => vec![listing("coin1_by_bot", "c-new")],
            "incomplete" => vec![
                listing("coin1_by_bot", "c-new"),
                Reply::new(
                    "getStickerSet",
                    &[("name", "coin2_by_bot")],
                    json!({"ok":false,"description":"Bad Request: STICKERSET_INVALID"}),
                ),
            ],
            "usage" => vec![],
            _ => unreachable!(),
        };
        let mut server = Server::new(replies);
        let mut settings = settings;
        settings.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
        let result = commands.run(
            &directory,
            &settings,
            &[
                if label == "usage" {
                    "--check"
                } else {
                    "--refresh"
                },
                "--family",
                "coins",
            ],
        );
        let requests = server.finish().unwrap();
        assert_eq!(
            result.code,
            if label == "coins" { 0 } else { 2 },
            "{label}: {}",
            result.stderr
        );
        assert_eq!(
            requests.len(),
            if label == "coins" {
                1
            } else if label == "incomplete" {
                2
            } else {
                0
            }
        );
        for (position, request) in requests.iter().enumerate() {
            assert_eq!(request.method, "getStickerSet");
            assert_eq!(
                request.form,
                BTreeMap::from([("name".into(), format!("coin{}_by_bot", position + 1))])
            );
        }
        assert_eq!(snapshot(&directory.join("collection")), data);
        assert_eq!(snapshot(&directory.join("coins")), coins);
        if label == "coins" {
            let index = read(&packs.join("index.json"));
            assert_eq!(index["pack_count"], 2);
            assert!(index["by_current_id"].get("g1").is_some());
            assert!(index["by_current_id"].get("c-new").is_some());
            assert!(index["by_current_id"].get("c-old").is_none());
            assert_eq!(index["by_source_id"], json!({"src-g":"g1"}));
            assert_eq!(index["by_previous_id"], json!({"g0":"g1"}));
            assert_eq!(index["inputs"]["catalog.db"], json!([1, 1]));
            assert_eq!(index["inputs"]["publish_mypacks.json"], json!([2, 2]));
            let now = _native::roster_cli::fingerprint(&config).unwrap();
            for key in ["rebuild_dedup_state.json", "ticker_to_id.json"] {
                assert_eq!(index["inputs"][key], now[key]);
            }
            assert!(
                _native::roster_cli::stale(&config).unwrap().0,
                "the untouched general inputs must remain stale"
            );
            for extension in ["json", "md", "html"] {
                let name = PathBuf::from(format!("gen1_by_bot.{extension}"));
                assert_eq!(
                    Some(std::fs::read(packs.join(&name)).unwrap()),
                    before[&name]
                );
            }
            let document = read(&packs.join("coin1_by_bot.json"));
            assert_eq!(document["emoji"][0]["name"], "btc, xbt");
            assert_eq!(document["emoji"][0]["role"], "emoji");
            assert_eq!(document["emoji"][0]["custom_emoji_id"], "c-new");
            let html = std::fs::read_to_string(packs.join("coin1_by_bot.html")).unwrap();
            assert!(html.contains("no brand logo") && html.contains("no preview</div>"));
            assert!(!packs.join(".codec").exists() && !packs.join(".thumbs").exists());
        } else {
            assert_eq!(snapshot(&packs), before);
            assert!(result.stderr.contains(if label == "usage" {
                "--check covers all families"
            } else {
                "cannot read the live set"
            }));
        }
    }
    runtime.shutdown_timeout(Duration::from_secs(1));
    drop(log);
    root.clean().unwrap();
}
