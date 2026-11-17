//! Original curation migration scenarios use real storage/leases with only the decoder controlled.
use _native::{
    atomic, config::Config, media_adapter::MediaCodec, migration_apply, migration_restore,
    migration_signature,
};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const OLD: &str = "v:aaaaaaaaaaaaaaaa";
const NEW: &str = "v:bbbbbbbbbbbbbbbb";
const NAME: &str = "007_video_aaaaaaaaaaaa.webm";

struct Decoder {
    unchanged: AtomicBool,
}
impl MediaCodec for Decoder {
    fn with_scratch(&self, _scratch: PathBuf) -> Self {
        Self {
            unchanged: AtomicBool::new(self.unchanged.load(Ordering::SeqCst)),
        }
    }
    async fn call(&self, request: Value) -> Result<Value, String> {
        if request["operation"] != "fingerprint" || request["format"] != "video" {
            return Err("unexpected controlled migration decoder operation".into());
        }
        let path = Path::new(request["source"].as_str().ok_or("missing decoder source")?);
        if std::fs::read(path).map_err(|error| error.to_string())? != media() {
            return Err("controlled migration input bytes changed".into());
        }
        let unchanged = self.unchanged.load(Ordering::SeqCst);
        Ok(
            json!({"content_key":if unchanged { OLD } else { NEW },"phash":if unchanged { 0 } else { 9 }}),
        )
    }
}
fn media() -> Vec<u8> {
    let mut bytes = b"\x1a\x45\xdf\xa3".to_vec();
    bytes.extend_from_slice(NAME.as_bytes());
    bytes
}
fn plan() -> Value {
    json!({"version":1,"known":[OLD],"excluded":[OLD],"targets":[[OLD,2]],
        "moves":[{"key":OLD,"label":OLD,"from_pack":1,"to_pack":2}],
        "held":[{"key":OLD,"label":"v:free-text","from_pack":1}]})
}
fn read(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn artifacts(data: &Path) -> Vec<PathBuf> {
    let mut files = std::fs::read_dir(data)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("catalog.before-video-identity-")
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}
fn database(data: &Path) -> String {
    migration_signature::signature(&data.join("catalog.db"), &[]).unwrap()
}
fn keys(data: &Path) -> Vec<String> {
    let connection = Connection::open(data.join("catalog.db")).unwrap();
    let mut query = connection
        .prepare("SELECT content_key FROM items ORDER BY content_key")
        .unwrap();
    query
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

pub fn check(root: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let source = root.join(NAME);
    std::fs::write(&source, media()).unwrap();
    let connection = Connection::open(root.join("catalog.db")).unwrap();
    connection.execute_batch("CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT,format TEXT,phash INTEGER);CREATE TABLE publications(base TEXT,content_key TEXT,set_name TEXT,custom_emoji_id TEXT);CREATE TABLE seen_files(file_unique_id TEXT PRIMARY KEY,content_key TEXT);").unwrap();
    connection
        .execute(
            "INSERT INTO items VALUES(?,?,'video',0)",
            rusqlite::params![OLD, source.to_str().unwrap()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO publications VALUES('base',?,'set1','CID')",
            [OLD],
        )
        .unwrap();
    connection
        .execute("INSERT INTO seen_files VALUES('FUID',?)", [OLD])
        .unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "delete"
    );
    connection.close().unwrap();
    let path = root.join("pack_plan.json");
    let original = plan();
    atomic::write_json(&path, &original).unwrap();
    let publisher_path = root.join("publish_audit.json");
    let publisher = json!({"sets":[{"keys":[OLD]}]});
    atomic::write_json(&publisher_path, &publisher).unwrap();
    let signature = database(root);
    let config = Config::load_with_environment(
        root.into(),
        &BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]),
    )
    .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let decoder = Decoder {
        unchanged: AtomicBool::new(false),
    };
    let destination = root.join("007_video_bbbbbbbbbbbb.webm");
    let original_bytes = std::fs::read(&path).unwrap();
    let publisher_bytes = std::fs::read(&publisher_path).unwrap();
    let publisher_lock =
        _native::locks::Lease::acquire(&root.join("publish_audit.lock"), "fixture UTC").unwrap();
    assert!(
        migration_apply::apply(&config, root, &decoder, &runtime, None)
            .unwrap_err()
            .contains("OS lock acquisition failed")
    );
    drop(publisher_lock);
    assert_eq!(database(root), signature);
    assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
    assert_eq!(std::fs::read(&publisher_path).unwrap(), publisher_bytes);
    assert_eq!(std::fs::read(&source).unwrap(), media());
    assert!(!destination.exists());
    assert!(artifacts(root).is_empty());
    assert!(!root.join("identity-migration.journal.json").exists());
    std::fs::write(&destination, b"unrelated sentinel").unwrap();
    assert!(
        migration_apply::apply(&config, root, &decoder, &runtime, None)
            .unwrap_err()
            .contains("destination contains unrelated bytes")
    );
    assert_eq!(std::fs::read(&destination).unwrap(), b"unrelated sentinel");
    assert_eq!(std::fs::read(&source).unwrap(), media());
    assert_eq!(database(root), signature);
    assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
    assert_eq!(std::fs::read(&publisher_path).unwrap(), publisher_bytes);
    assert!(artifacts(root).is_empty());
    assert!(!root.join("identity-migration.journal.json").exists());
    // Replacing only our sentinel models a destination present before the snapshot.
    std::fs::write(&destination, media()).unwrap();
    let applied = migration_apply::apply(&config, root, &decoder, &runtime, None).unwrap();
    assert_eq!(applied["moved"], 1);
    assert_eq!(
        migration_signature::signature(Path::new(applied["backup"].as_str().unwrap()), &[])
            .unwrap(),
        signature
    );
    assert_eq!(keys(root), [NEW]);
    assert!(!source.exists());
    assert_eq!(std::fs::read(&destination).unwrap(), media());
    assert_eq!(read(&publisher_path), json!({"sets":[{"keys":[NEW]}]}));
    let changed = read(&path);
    assert_eq!(changed["known"], json!([NEW]));
    assert_eq!(changed["excluded"], json!([NEW]));
    assert_eq!(changed["targets"], json!([[NEW, 2]]));
    assert_eq!(changed["moves"][0]["key"], NEW);
    assert_eq!(changed["moves"][0]["label"], OLD);
    assert_eq!(changed["held"][0]["key"], NEW);
    assert_eq!(changed["held"][0]["label"], "v:free-text");
    assert!(
        read(Path::new(applied["bundle"].as_str().unwrap()))["states"]
            .get("pack_plan.json")
            .is_some()
    );
    let before_repeat = artifacts(root);
    assert_eq!(
        migration_apply::apply(&config, root, &decoder, &runtime, None).unwrap(),
        json!({"noop":true,"moved":0,"state":[],"renamed":[]})
    );
    assert_eq!(artifacts(root), before_repeat);
    let bundle = Path::new(applied["bundle"].as_str().unwrap());
    let mut edited = changed.clone();
    edited["moves"][0]["to_pack"] = json!(3);
    atomic::write_json(&path, &edited).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let migrated_signature = database(root);
    assert!(
        migration_restore::restore(root, root, bundle, true)
            .unwrap_err()
            .contains("state changed")
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(database(root), migrated_signature);
    assert!(!root.join("identity-migration.journal.json").exists());
    atomic::write_json(&path, &changed).unwrap();
    let connection = Connection::open(root.join("catalog.db")).unwrap();
    connection
        .execute("UPDATE publications SET custom_emoji_id='LATER'", [])
        .unwrap();
    connection.close().unwrap();
    let later = database(root);
    assert!(
        migration_restore::restore(root, root, bundle, true)
            .unwrap_err()
            .contains("catalog changed outside migration")
    );
    assert_eq!(database(root), later);
    assert_eq!(read(&path), changed);
    assert_eq!(std::fs::read(&destination).unwrap(), media());
    assert!(!source.exists());
    assert_eq!(artifacts(root), before_repeat);
    assert!(!root.join("identity-migration.journal.json").exists());
    let connection = Connection::open(root.join("catalog.db")).unwrap();
    connection
        .execute("UPDATE publications SET custom_emoji_id='CID'", [])
        .unwrap();
    connection.close().unwrap();
    assert_eq!(database(root), migrated_signature);
    std::fs::write(&destination, b"later operator media").unwrap();
    assert!(
        migration_restore::restore(root, root, bundle, true)
            .unwrap_err()
            .contains("media changed or destination collided")
    );
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        b"later operator media"
    );
    assert_eq!(database(root), migrated_signature);
    assert_eq!(read(&path), changed);
    assert_eq!(artifacts(root), before_repeat);
    assert!(!root.join("identity-migration.journal.json").exists());
    std::fs::write(&destination, media()).unwrap();
    for _ in 0..2 {
        migration_restore::restore(root, root, bundle, true).unwrap();
        assert_eq!(read(&path), original);
        assert_eq!(read(&publisher_path), publisher);
        assert_eq!(database(root), signature);
        assert_eq!(keys(root), [OLD]);
        assert_eq!(std::fs::read(&source).unwrap(), media());
        assert_eq!(std::fs::read(&destination).unwrap(), media());
    }
    // No fresh bundle may be created when even an unchanged row has a missing required key.
    decoder.unchanged.store(true, Ordering::SeqCst);
    let mut missing = original.clone();
    missing["targets"] = json!([["v:missing", 2]]);
    atomic::write_json(&path, &missing).unwrap();
    let before = artifacts(root);
    let bytes = std::fs::read(&path).unwrap();
    assert!(
        migration_apply::apply(&config, root, &decoder, &runtime, None)
            .unwrap_err()
            .contains("required state")
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(database(root), signature);
    assert_eq!(artifacts(root), before);
    assert!(!root.join("identity-migration.journal.json").exists());
    decoder.unchanged.store(false, Ordering::SeqCst);
    for text in [
        "null",
        "{\"version\":999}",
        "{\"version\":1,\"moves\":[],\"held\":{},\"targets\":[]}",
    ] {
        atomic::write_bytes(&path, text.as_bytes()).unwrap();
        assert!(migration_apply::apply(&config, root, &decoder, &runtime, None).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), text.as_bytes());
        assert_eq!(database(root), signature);
        assert_eq!(artifacts(root), before);
        assert!(!root.join("identity-migration.journal.json").exists());
    }
    atomic::write_json(&path, &original).unwrap();
    let first_outputs = artifacts(root)
        .into_iter()
        .map(|path| {
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect::<BTreeMap<_, _>>();
    let second = migration_apply::apply(&config, root, &decoder, &runtime, None).unwrap();
    assert_ne!(second["backup"], applied["backup"]);
    assert_ne!(second["bundle"], applied["bundle"]);
    assert_eq!(
        migration_signature::signature(Path::new(second["backup"].as_str().unwrap()), &[]).unwrap(),
        signature
    );
    assert_eq!(artifacts(root).len(), 4);
    for (path, bytes) in &first_outputs {
        assert_eq!(std::fs::read(path).unwrap(), *bytes);
    }
    migration_restore::restore(
        root,
        root,
        Path::new(second["bundle"].as_str().unwrap()),
        true,
    )
    .unwrap();
    assert_eq!(database(root), signature);
    assert_eq!(read(&path), original);
    assert_eq!(read(&publisher_path), publisher);
    assert_eq!(std::fs::read(&source).unwrap(), media());
    assert_eq!(std::fs::read(&destination).unwrap(), media());
    assert!(!root.join("identity-migration.journal.json").exists());
    runtime.shutdown_timeout(Duration::from_secs(1));
    assert!(!_native::process_owner::cleanup_pending(root));
    assert!(Instant::now() < deadline);
}
