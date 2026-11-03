//! Original and native engines consume each other's actual version2 documents, not parallel models.
use _native::{
    atomic, logging::RunLog, source_git_inventory::Source, source_identity_tests, tooling_process,
};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[path = "retained_cli.rs"]
mod retained_cli;
#[path = "tooling_fixture.rs"]
mod tooling_fixture;

const OLD: &str = "v:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const KEY: &str = "v:3cf3deb8098c04f13c4f989370acac89";
const HASH: u64 = 5300177295401782857;
const SOURCE: &str = "001_video_aaaaaaaaaaaa.webm";
const DEST: &str = "001_video_3cf3deb8098c.webm";
fn document(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn state(key: &str) -> Value {
    json!({"sets":[{"keys":[key]}],"skipped":[]})
}

pub fn check(project: &Path, runtime: &tokio::runtime::Runtime, sources: &[&Source]) {
    let fixture = tooling_fixture::Root::new("original bundle replay café");
    let root = _native::migration_bundle::canonical(&fixture.0).unwrap();
    let (executable, environment) = retained_cli::install(project, &root);
    let deadline = Instant::now() + Duration::from_secs(60);
    let data = root.join("collection");
    std::fs::create_dir(&data).unwrap();
    let original_path = project.join("tests/fixtures/native/video-fingerprint/fast30.webm");
    assert_eq!(
        _native::tooling_manifest::digest(&original_path).unwrap(),
        "9bbe7eb2e975eb1f325a733fba5e5f97918e3423bb19e5e10d25942aff78ef57"
    );
    let original = std::fs::read(&original_path).unwrap();
    let source = data.join(SOURCE);
    let destination = data.join(DEST);
    std::fs::write(&source, &original).unwrap();
    let database = data.join("catalog.db");
    let db = Connection::open(&database).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT,format TEXT,phash INTEGER);CREATE TABLE publications(content_key TEXT,custom_emoji_id TEXT);CREATE TABLE seen_files(content_key TEXT,file_unique_id TEXT);").unwrap();
    db.execute(
        "INSERT INTO items VALUES(?,?,?,?)",
        params![OLD, format!("collection/{SOURCE}"), "video", HASH as i64],
    )
    .unwrap();
    db.execute(
        "INSERT INTO publications VALUES(?,?)",
        params![OLD, "111111111"],
    )
    .unwrap();
    db.execute(
        "INSERT INTO seen_files VALUES(?,?)",
        params![OLD, "fixture-unique"],
    )
    .unwrap();
    db.close().unwrap();
    let state_path = data.join("publish_fixture.json");
    atomic::write_json(&state_path, &state(OLD)).unwrap();
    let initial_signature = _native::migration_signature::signature(&database, &[]).unwrap();
    let journal = data.join("identity-migration.journal.json");
    let protected = root.join("assets/panel.html");
    let protected_bytes = std::fs::read(&protected).unwrap();
    let eof = root.join("command-eof");
    std::fs::write(&eof, b"").unwrap();
    let log = RunLog::open(&root, "bundle-interoperability", vec![]);
    let command = |options: &[&str]| {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            remaining > Duration::from_secs(1),
            "bundle command deadline exhausted; retained"
        );
        let mut argv = vec![
            executable.as_os_str().to_owned(),
            OsString::from("identity-repair"),
        ];
        argv.extend(options.iter().map(|option| OsString::from(*option)));
        runtime
            .block_on(tooling_process::capture_with_input(
                &root,
                &argv,
                &environment,
                &eof,
                remaining.min(Duration::from_secs(10)),
                remaining.min(Duration::from_secs(5)),
                &log,
            ))
            .unwrap()
    };
    let run = |options: &[&str]| {
        let result = command(options);
        assert_eq!(result.code, 0, "{} {}", result.stdout, result.stderr);
        serde_json::from_str::<Value>(result.stdout.trim()).unwrap()
    };
    let original_action = |action: &str, manifest: Option<&Path>| {
        assert!(
            deadline.saturating_duration_since(Instant::now()) >= Duration::from_secs(15),
            "bundle replay has insufficient remaining worker budget; retained"
        );
        runtime
            .block_on(source_identity_tests::bundle_owned(
                project,
                Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
                &data,
                manifest,
                action,
                sources,
            ))
            .unwrap()
    };
    let assert_restored = || {
        assert_eq!(
            _native::migration_signature::signature(&database, &[]).unwrap(),
            initial_signature
        );
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert!(!destination.exists() && !journal.exists());
        assert_eq!(document(&state_path), state(OLD));
    };

    // A native-origin bundle must be independently readable and restorable by the original engine.
    let first = run(&["migrate-video-keys", "--apply"]);
    let first_manifest = PathBuf::from(first["bundle"].as_str().unwrap());
    let first_doc = document(&first_manifest);
    assert_eq!(first_doc["version"], 2);
    assert_eq!(first_doc["direction"], "forward");
    assert_eq!(first_doc["phash"], json!({KEY:HASH as i64}));
    assert_eq!(first_doc["key_map"], json!({OLD:KEY}));
    assert_eq!(std::fs::read(&destination).unwrap(), original);
    assert!(!source.exists() && !journal.exists());
    // Raw storage has a new relative path; only the original normalized boundary is comparable.
    assert_ne!(
        _native::migration_signature::signature(&database, &[]).unwrap(),
        first_doc["database_signature"].as_str().unwrap()
    );
    let refuse = |action: &str, manifest: Option<&Path>, expected: &str| {
        let before = std::fs::read(&database).unwrap();
        let error = runtime
            .block_on(source_identity_tests::bundle_owned(
                project,
                &root.join("must-not-start"),
                &data,
                manifest,
                action,
                sources,
            ))
            .unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert_eq!(std::fs::read(&database).unwrap(), before);
        assert!(!root.join("must-not-start").exists());
    };
    refuse(
        "unexpected",
        Some(&first_manifest),
        "invalid original bundle replay action",
    );
    refuse("verify", None, "invalid original bundle replay action");
    let extra = data.join("publish_unexpected.json");
    atomic::write_json(&extra, &state(KEY)).unwrap();
    refuse(
        "verify",
        Some(&first_manifest),
        "unexpected bundle replay member",
    );
    assert_eq!(document(&extra), state(KEY));
    std::fs::remove_file(&extra).unwrap();
    let cache = data.join("tmp/identity-codec");
    std::fs::create_dir_all(&cache).unwrap();
    let marker = cache.join("codec-cleanup-pending.json");
    std::fs::write(&marker, b"retained owner evidence").unwrap();
    refuse(
        "verify",
        Some(&first_manifest),
        "bundle replay codec scratch is not settled",
    );
    assert_eq!(std::fs::read(&marker).unwrap(), b"retained owner evidence");
    std::fs::remove_file(marker).unwrap();
    #[cfg(unix)]
    {
        std::fs::remove_dir(&cache).unwrap();
        std::os::unix::fs::symlink(root.join("assets"), &cache).unwrap();
        refuse(
            "verify",
            Some(&first_manifest),
            "bundle replay cache cannot follow links",
        );
        assert_eq!(std::fs::read(&protected).unwrap(), protected_bytes);
        std::fs::remove_file(&cache).unwrap();
        std::fs::create_dir(&cache).unwrap();
    }
    assert_eq!(
        original_action("verify", Some(&first_manifest))["signature"],
        first_doc["database_signature"]
    );
    let first_text = first_manifest.to_str().unwrap();
    let preview = command(&["restore", "--bundle", first_text]);
    assert_eq!(preview.code, 3, "{} {}", preview.stdout, preview.stderr);
    assert!(!source.exists());
    original_action("restore", Some(&first_manifest));
    assert_restored();

    // Recover a previous SQL-only migration from immutable IDs, then let the original restore it.
    let db = Connection::open(&database).unwrap();
    for table in ["items", "publications", "seen_files"] {
        db.execute(
            &format!("UPDATE {table} SET content_key=? WHERE content_key=?"),
            params![KEY, OLD],
        )
        .unwrap();
    }
    db.close().unwrap();
    let partial = std::fs::read(&database).unwrap();
    let backup = first["backup"].as_str().unwrap();
    let preview = command(&["migrate-video-keys", "--from-backup", backup]);
    assert_eq!(preview.code, 3, "{} {}", preview.stdout, preview.stderr);
    assert!(preview.stdout.contains("recovered 1 old->new pair(s)"));
    assert_eq!(std::fs::read(&database).unwrap(), partial);
    assert!(!journal.exists());
    let legacy = run(&["migrate-video-keys", "--apply", "--from-backup", backup]);
    let legacy_manifest = PathBuf::from(legacy["bundle"].as_str().unwrap());
    original_action("restore", Some(&legacy_manifest));
    assert_eq!(std::fs::read(&source).unwrap(), original);
    let restored = command(&["restore", "--bundle", first_text, "--apply"]);
    assert_eq!(restored.code, 0, "{} {}", restored.stdout, restored.stderr);
    assert_restored();

    // Prepare an ORIGINAL manifest, then compare original/native completion of the same partial state.
    let prepared = original_action("prepare", None);
    let manifest = PathBuf::from(prepared["manifest"].as_str().unwrap());
    let manifest_text = manifest.to_str().unwrap();
    let doc = document(&manifest);
    let before = std::fs::read(&database).unwrap();
    for field in ["phash", "bundle"] {
        let mut altered = doc.clone();
        if field == "phash" {
            altered["phash"][KEY] = json!((HASH as i64) ^ 1);
        } else {
            altered["bundle"] = json!(protected);
        }
        atomic::write_json(&journal, &altered).unwrap();
        let rejected = command(&["migrate-video-keys", "--apply"]);
        assert_eq!(rejected.code, 4, "{} {}", rejected.stdout, rejected.stderr);
        assert_eq!(std::fs::read(&database).unwrap(), before);
        assert_eq!(document(&journal), altered);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert_eq!(std::fs::read(&protected).unwrap(), protected_bytes);
    }
    atomic::write_json(&journal, &doc).unwrap();
    let staged = original_action("stage", Some(&manifest));
    let expected = &staged["expected"];
    assert_eq!(expected["moved"], 0);
    assert_eq!(expected["key_map"], json!({OLD:KEY}));
    assert_eq!(expected["state"], json!(["publish_fixture.json"]));
    assert_eq!(expected["renamed"], json!([DEST]));
    let applied = run(&["migrate-video-keys", "--apply"]);
    assert_eq!(applied, staged["expected"]);
    assert_eq!(
        original_action("verify", Some(&manifest))["signature"],
        doc["database_signature"]
    );
    assert!(!journal.exists());
    assert_eq!(document(&state_path), state(KEY));
    assert_eq!(std::fs::read(&destination).unwrap(), original);

    original_action("stage-restore", Some(&manifest));
    let reverse = document(&journal);
    let alternate = data.join("other.rollback.json");
    atomic::write_json(&alternate, &reverse).unwrap();
    let before_refusal = (
        std::fs::read(&database).unwrap(),
        std::fs::read(&journal).unwrap(),
    );
    let rejected = command(&[
        "restore",
        "--bundle",
        alternate.to_str().unwrap(),
        "--apply",
    ]);
    assert_eq!(rejected.code, 4, "{} {}", rejected.stdout, rejected.stderr);
    assert!(rejected.stderr.contains("requested bundle differs"));
    assert_eq!(
        (
            std::fs::read(&database).unwrap(),
            std::fs::read(&journal).unwrap()
        ),
        before_refusal
    );
    let restored = command(&["restore", "--bundle", manifest_text, "--apply"]);
    assert_eq!(restored.code, 0, "{} {}", restored.stdout, restored.stderr);
    assert_restored();

    // Original apply finishes a native-origin SQL journal; a separate late edit must still refuse.
    let last = run(&["migrate-video-keys", "--apply"]);
    let last_manifest = PathBuf::from(last["bundle"].as_str().unwrap());
    original_action("restore", Some(&last_manifest));
    original_action("finish-native", Some(&last_manifest));
    assert_restored();
    let db = Connection::open(&database).unwrap();
    db.execute("UPDATE publications SET custom_emoji_id='222222222'", [])
        .unwrap();
    db.close().unwrap();
    let edited = std::fs::read(&database).unwrap();
    let rejected = command(&["restore", "--bundle", first_text, "--apply"]);
    assert_eq!(rejected.code, 4, "{} {}", rejected.stdout, rejected.stderr);
    assert!(
        rejected
            .stderr
            .contains("catalog changed outside migration")
    );
    assert_eq!(std::fs::read(&database).unwrap(), edited);
    assert!(!journal.exists());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    assert_eq!(std::fs::read(original_path).unwrap(), original);
    assert_eq!(std::fs::read(&protected).unwrap(), protected_bytes);
    if data.join("tmp/identity-codec").exists() {
        assert!(
            std::fs::read_dir(data.join("tmp/identity-codec"))
                .unwrap()
                .next()
                .is_none()
        );
    }
    assert!(
        std::fs::read_dir(root.join("logs/tmp/tooling"))
            .unwrap()
            .next()
            .is_none()
    );
    assert!(!_native::process_owner::cleanup_pending(&root));
    assert!(
        Instant::now() < deadline,
        "bundle cohort exceeded60s; retained"
    );
    log.finish(0);
    drop(log);
    fixture.clean().unwrap();
}
