//! Cheap validation variants use the public bundle boundary; codec replay is tested separately.
use _native::{atomic, migration_bundle, migration_files, migration_signature};
use rusqlite::Connection;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;

struct Workspace(PathBuf);
impl Drop for Workspace {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            eprintln!("fixture cleanup failed: {error}");
        }
    }
}

#[test]
fn inconsistent_state_version_or_backup_refuses_without_catalog_changes() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let workspace = Workspace(
        project
            .join("logs")
            .join(format!("bundle-validation-{}", std::process::id())),
    );
    std::fs::create_dir(&workspace.0).unwrap();
    let root = migration_bundle::resolved(&workspace.0).unwrap();
    let data = root.join("collection");
    std::fs::create_dir(&data).unwrap();
    let source = data.join("aaaaaaaaaaaa.png");
    std::fs::copy(project.join("assets/numera-emoji-mapper-logo.png"), &source).unwrap();
    let before = json!({"sets":[{"keys":["s:aaaaaaaaaaaa"]}]});
    let after = json!({"sets":[{"keys":["s:bbbbbbbbbbbb"]}]});
    atomic::write_json(&data.join("publish_fixture.json"), &before).unwrap();
    let database = data.join("catalog.db");
    let db = Connection::open(&database).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT,format TEXT,phash INTEGER);CREATE TABLE publications(content_key TEXT,custom_emoji_id TEXT);CREATE TABLE seen_files(content_key TEXT,file_unique_id TEXT);INSERT INTO items VALUES('s:aaaaaaaaaaaa','./aaaaaaaaaaaa.png','static',7);").unwrap();
    let backup = data.join("catalog.before-fixture.db");
    _native::sqlite_snapshot::backup(&db, &backup).unwrap();
    drop(db);
    let map = BTreeMap::from([("s:aaaaaaaaaaaa".to_owned(), "s:bbbbbbbbbbbb".to_owned())]);
    let files = migration_files::plan(&data, &root, &map).unwrap();
    let mut doc = migration_bundle::make(
        &data,
        &backup,
        &map,
        &BTreeMap::new(),
        &files,
        &json!({"publish_fixture.json":{"before":before,"after":after}}),
    )
    .unwrap();
    let manifest = backup.with_extension("rollback.json");
    doc["bundle"] = json!(manifest);
    atomic::write_json(&manifest, &doc).unwrap();
    migration_bundle::validate(&data, &root, &doc).unwrap();
    let signature = migration_signature::signature(&database, &[]).unwrap();
    let bytes = std::fs::read(&database).unwrap();
    let original_media = std::fs::read(&source).unwrap();
    for field in ["states", "version", "backup_sha256"] {
        let mut altered = doc.clone();
        match field {
            "states" => altered["states"]["publish_fixture.json"]["after"] = json!({"sets":[]}),
            "version" => altered["version"] = json!(999),
            _ => altered["backup_sha256"] = json!("0".repeat(64)),
        }
        assert!(
            migration_bundle::validate(&data, &root, &altered).is_err(),
            "{field} was accepted"
        );
        assert_eq!(
            migration_signature::signature(&database, &[]).unwrap(),
            signature
        );
        assert_eq!(std::fs::read(&database).unwrap(), bytes);
        assert_eq!(std::fs::read(&source).unwrap(), original_media);
        assert!(!data.join("identity-migration.journal.json").exists());
    }
}
