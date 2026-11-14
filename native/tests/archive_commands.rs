//! Original native-backed archive/export commands exercise durable recovery without live writes.
use _native::{catalog::Catalog, config::Config};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::time::Duration;
#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use bot_api::{Reply, Server, TOKEN};
use tooling_fixture::Root;

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
        args: &[&str],
    ) -> _native::tooling_process::Completed {
        let mut environment = self.environment.clone();
        for key in [
            "COLLECTION_PACK_BASE",
            "GENERAL_BOT_TOKEN",
            "TELEGRAM_BOT_TOKEN",
            "EMOJI_ARCHIVE_DIR",
            "BRAND_LOGO_PATH",
        ] {
            environment.remove(key);
        }
        environment.extend(settings.clone());
        assert!(environment["NUMERA_TEST_API_BASE"].starts_with("http://127.0.0.1:"));
        let child = directory.join(self.executable.file_name().unwrap());
        std::fs::rename(self.executable, &child).unwrap();
        let mut argv = vec![child.into_os_string(), "pack-archive".into()];
        argv.extend(args.iter().map(|value| std::ffi::OsString::from(*value)));
        let result = self.runtime.block_on(_native::tooling_process::capture(
            self.root,
            &argv,
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
fn fixture(directory: &Path) -> BTreeMap<String, String> {
    std::fs::create_dir_all(directory.join("assets")).unwrap();
    std::fs::write(directory.join("assets/panel.html"), b"isolated fixture").unwrap();
    std::fs::write(directory.join("pyproject.toml"), b"# isolated fixture\n").unwrap();
    std::fs::create_dir(directory.join("collection")).unwrap();
    BTreeMap::from([
        ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
        ("COLLECTION_PACK_BASE".into(), "fixture".into()),
        ("GENERAL_BOT_TOKEN".into(), TOKEN.into()),
        ("BRAND_LOGO_PATH".into(), "logo.png".into()),
        ("EMOJI_ARCHIVE_DIR".into(), "archive".into()),
    ])
}
fn png() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests/fixtures/native/static-codec/alpha-edge.png");
    assert_eq!(
        _native::tooling_manifest::digest(&path).unwrap(),
        "d359d28b44f1871b588f04dc95ca56f8716b18768b6b91ef3f9aad66ed01aa71"
    );
    std::fs::read(path).unwrap()
}
fn read(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn no_staging(directory: &Path) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        _native::tooling_environment::reject_links(&path, "archive fixture cannot follow links")
            .unwrap();
        assert!(
            !path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(".tmp")
        );
    }
}
fn stored(database: &Path, key: &str) -> String {
    let db =
        Connection::open_with_flags(database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let value = db
        .query_row(
            "SELECT file_path FROM items WHERE content_key=?1",
            [key],
            |row| row.get(0),
        )
        .unwrap();
    db.close().unwrap();
    value
}

fn archive_recovery(commands: &Commands<'_>, original: &[u8]) {
    let directory = commands.root.join("archive recovery");
    let mut settings = fixture(&directory);
    let data = directory.join("collection");
    let database = data.join("catalog.db");
    let source = data.join("first.png");
    let missing = data.join("missing.png");
    std::fs::write(&source, original).unwrap();
    std::fs::write(directory.join("logo.png"), original).unwrap();
    let key = format!("s:{}", "a".repeat(32));
    let missing_key = format!("s:{}", "b".repeat(32));
    let mut catalog = Catalog::open(&database, "2026-10-07 00:00:00 UTC", &directory).unwrap();
    for (key, file, id, keyword) in [
        (&key, &source, "111111111", "first"),
        (&missing_key, &missing, "222222222", "missing"),
    ] {
        assert!(catalog.insert(&json!({"content_key":key,"fmt":"static","file_path":file,"emojis":["😀"],"keywords":[keyword],"sources":["fixture"]}), "2026-10-07 00:00:00 UTC").unwrap());
        catalog.uploaded(&json!({"content_key":key,"custom_emoji_id":id,"base":"fixture","set_name":"fixture_by_YourEmojiBot"}), "2026-10-07 00:00:00 UTC").unwrap();
    }
    drop(catalog);
    let db = Connection::open(&database).unwrap();
    db.execute(
        "UPDATE publications SET set_name=NULL WHERE content_key=?1",
        [&missing_key],
    )
    .unwrap();
    db.close().unwrap();
    let state = data.join("publish_fixture.json");
    _native::atomic::write_json(&state, &json!({"sets":[{"name":"fixture_by_YourEmojiBot","title":"Fixture Pack","fmt":"mixed","index":1,"live":200}]})).unwrap();
    let state_bytes = std::fs::read(&state).unwrap();
    let live = json!({"ok":true,"result":{"stickers":[{"custom_emoji_id":"000000000"},{"custom_emoji_id":"111111111","emoji":"😀"},{"custom_emoji_id":"222222222","emoji":"⭐"}]}});
    let mut server = Server::new(
        (0..4)
            .map(|_| {
                Reply::new(
                    "getStickerSet",
                    &[("name", "fixture_by_YourEmojiBot")],
                    live.clone(),
                )
            })
            .collect(),
    );
    settings.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
    let folder = directory.join("archive/Fixture Pack");
    let archived = folder.join(format!("002_static_{}.png", "a".repeat(12)));
    let result = commands.run(&directory, &settings, &["--sync"]);
    assert_eq!(result.code, 1, "{}", result.stderr);
    assert_eq!(std::fs::read(&archived).unwrap(), original);
    assert!(!source.exists());
    assert_eq!(
        _native::paths::resolve(&data, &stored(&database, &key), &directory),
        archived
    );
    assert!(!folder.join("_history.json").exists());
    assert_eq!(std::fs::read(&state).unwrap(), state_bytes);
    std::fs::write(&missing, original).unwrap();
    let db = Connection::open(&database).unwrap();
    db.execute(
        "UPDATE publications SET set_name=?1 WHERE content_key=?2",
        params!["fixture_by_YourEmojiBot", missing_key],
    )
    .unwrap();
    db.close().unwrap();
    let result = commands.run(&directory, &settings, &["--sync"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert_eq!(std::fs::read(&archived).unwrap(), original);
    assert!(!missing.exists());
    assert_eq!(
        std::fs::read(folder.join("001_logo.png")).unwrap(),
        original
    );
    let history = read(&folder.join("_history.json"));
    assert_eq!(history["emoji"][1]["content_key"], key);
    assert_eq!(history["emoji"][2]["content_key"], missing_key);
    assert_eq!(commands.run(&directory, &settings, &["--check"]).code, 0);
    let result = commands.run(&directory, &settings, &["--sync"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(result.stdout.contains("0 file(s) moved"));
    std::fs::write(&source, original).unwrap();
    let db = Connection::open(&database).unwrap();
    db.execute(
        "UPDATE items SET file_path=?1 WHERE content_key=?2",
        params![source.to_str().unwrap(), key],
    )
    .unwrap();
    db.close().unwrap();
    std::fs::write(&archived, b"interrupted-copy").unwrap();
    let result = commands.run(&directory, &settings, &["--sync"]);
    assert_eq!(result.code, 1, "{}", result.stderr);
    assert_eq!(std::fs::read(&source).unwrap(), original);
    assert_eq!(std::fs::read(&archived).unwrap(), b"interrupted-copy");
    assert_eq!(stored(&database, &key), source.to_str().unwrap());
    no_staging(&folder);
    for (key, option) in [
        ("COLLECTION_PACK_BASE", "--check"),
        ("EMOJI_ARCHIVE_DIR", "--check"),
        ("GENERAL_BOT_TOKEN", "--sync"),
    ] {
        let before = settings.insert(key.into(), String::new()).unwrap();
        assert_eq!(
            commands.run(&directory, &settings, &[option]).code,
            1,
            "{key}"
        );
        settings.insert(key.into(), before);
    }
    assert_eq!(std::fs::read(&state).unwrap(), state_bytes);
    assert_eq!(std::fs::read(directory.join("logo.png")).unwrap(), original);
    let requests = server.finish().unwrap();
    assert_eq!(requests.len(), 4);
    for request in requests {
        assert_eq!(request.method, "getStickerSet");
        assert_eq!(
            request.form,
            BTreeMap::from([("name".into(), "fixture_by_YourEmojiBot".into())])
        );
    }
}

fn export_preservation(commands: &Commands<'_>, original: &[u8]) {
    let directory = commands.root.join("ordered export");
    let mut settings = fixture(&directory);
    let mut server = Server::new(vec![]);
    settings.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
    let data = directory.join("collection");
    let packs = directory.join("packs");
    std::fs::create_dir(&packs).unwrap();
    let source = data.join("source.png");
    std::fs::write(&source, original).unwrap();
    std::fs::write(directory.join("logo.png"), original).unwrap();
    let database = data.join("catalog.db");
    let key = format!("s:{}", "a".repeat(32));
    let name = "fixture_by_YourEmojiBot";
    let db = Connection::open(&database).unwrap();
    db.execute_batch(
        "CREATE TABLE items(content_key TEXT,file_path TEXT,format TEXT,keywords TEXT);
        CREATE TABLE publications(base TEXT,content_key TEXT,set_name TEXT,custom_emoji_id TEXT);",
    )
    .unwrap();
    db.execute(
        "INSERT INTO items VALUES(?1,?2,?3,?4)",
        params![
            key,
            source.to_str().unwrap(),
            "static",
            "[\"fixture|keyword\"]"
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO publications VALUES(?1,?2,?3,?4)",
        params!["fixture", key, name, "222222222"],
    )
    .unwrap();
    db.close().unwrap();
    let database_bytes = std::fs::read(&database).unwrap();
    let doc = json!({"set_name":name,"title":"Fixture","family":"general","pack_index":6,"emoji":[
        {"slot":1,"role":"brand-logo","custom_emoji_id":"111111111","history_key":format!("logo:{name}")},
        {"slot":2,"role":"emoji","custom_emoji_id":"222222222","history_key":format!("ck:{key}")}]});
    _native::atomic::write_json(&packs.join(format!("{name}.json")), &doc).unwrap();
    let config = Config::load_with_environment(directory.clone(), &settings).unwrap();
    let inputs = _native::roster_cli::fingerprint(&config).unwrap();
    let index = packs.join("index.json");
    _native::atomic::write_json(&index, &json!({"inputs":inputs})).unwrap();
    let destination = directory.join("output.zip");
    let options = ["--export", "6", "--zip", destination.to_str().unwrap()];
    let result = commands.run(&directory, &settings, &options);
    assert_eq!(result.code, 0, "{}", result.stderr);
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&destination).unwrap()).unwrap();
    let mut entries = Vec::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).unwrap();
        assert!(entry.size() <= 16384);
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        entries.push((entry.name().to_owned(), bytes));
    }
    assert_eq!(
        entries
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        [
            "001_logo.png",
            "002_static_aaaaaaaaaaaa.png",
            "_manifest.md"
        ]
    );
    assert_eq!(entries[0].1, original);
    assert_eq!(entries[1].1, original);
    assert!(
        std::str::from_utf8(&entries[2].1)
            .unwrap()
            .contains("fixture\\|keyword")
    );
    drop(zip);
    assert_eq!(std::fs::read(&source).unwrap(), original);
    let complete = std::fs::read(&destination).unwrap();
    std::fs::remove_file(&source).unwrap();
    let result = commands.run(&directory, &settings, &options);
    assert_eq!(result.code, 1, "{}", result.stderr);
    assert!(result.stdout.contains("slot 2"));
    assert_eq!(std::fs::read(&destination).unwrap(), complete);
    no_staging(&directory);
    let mut inputs = inputs;
    inputs["catalog.db"] = Value::Null;
    _native::atomic::write_json(&index, &json!({"inputs":inputs})).unwrap();
    assert_eq!(commands.run(&directory, &settings, &options).code, 3);
    assert_eq!(std::fs::read(&destination).unwrap(), complete);
    assert_eq!(std::fs::read(&database).unwrap(), database_bytes);
    assert_eq!(std::fs::read(directory.join("logo.png")).unwrap(), original);
    assert!(server.finish().unwrap().is_empty());
}

#[test]
fn original_native_archive_and_export_commands_preserve_partial_commits_and_complete_outputs() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = Root::new("archive command café");
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
    let log = _native::logging::RunLog::open(&root.0, "archive-command-owner", vec![TOKEN.into()]);
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
    let original = png();
    archive_recovery(&commands, &original);
    export_preservation(&commands, &original);
    runtime.shutdown_timeout(Duration::from_secs(1));
    drop(log);
    root.clean().unwrap();
}
