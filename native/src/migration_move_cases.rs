//! Second-transfer process death preserves every media intent until replay and restore settle.
use super::{complete, kill_at, manifest, project};
use crate::{migration_apply, migration_bundle};
use serde_json::{Value, json};
use std::path::Path;

pub(super) fn check(data: &Path) {
    std::fs::create_dir(data).unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/video-fingerprint.json"
    ))
    .unwrap();
    let db_path = data.join("catalog.db");
    let db = rusqlite::Connection::open(&db_path).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT,format TEXT,phash INTEGER);CREATE TABLE publications(content_key TEXT,custom_emoji_id TEXT);CREATE TABLE seen_files(content_key TEXT,file_unique_id TEXT);").unwrap();
    let old = (0..3)
        .map(|i| format!("v:{}", format!("{i:02x}").repeat(16)))
        .collect::<Vec<_>>();
    let mut expected = std::collections::BTreeMap::new();
    let mut originals = Vec::new();
    for (i, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let source = project().join(case["input"].as_str().unwrap());
        assert_eq!(
            crate::migration_files::digest(&source).unwrap(),
            case["input_sha256"]
        );
        let bytes = std::fs::read(&source).unwrap();
        let name = format!("{i:03}_video_{}.webm", &old[i][2..14]);
        let path = data.join(&name);
        std::fs::write(&path, &bytes).unwrap();
        db.execute(
            "INSERT INTO items VALUES(?,?,'video',0)",
            rusqlite::params![old[i], format!("./{name}")],
        )
        .unwrap();
        db.execute(
            "INSERT INTO publications VALUES(?,?)",
            rusqlite::params![old[i], format!("CID{i}")],
        )
        .unwrap();
        db.execute(
            "INSERT INTO seen_files VALUES(?,?)",
            rusqlite::params![old[i], format!("FUID{i}")],
        )
        .unwrap();
        expected.insert(
            old[i].clone(),
            case["expected"]["content_key"].as_str().unwrap().to_owned(),
        );
        originals.push((source, path, bytes));
    }
    db.close().unwrap();
    assert_eq!(originals.len(), 3);
    let before = crate::migration_signature::signature(&db_path, &[]).unwrap();
    let state = json!({"sets":[{"keys":old.iter().rev().collect::<Vec<_>>()}]});
    let plan = json!({"video":old.iter().rev().collect::<Vec<_>>()});
    crate::atomic::write_json(&data.join("publish_fixture.json"), &state).unwrap();
    crate::atomic::write_json(&data.join("publish_plan_fixture.json"), &plan).unwrap();
    kill_at(data, "apply", "second-move");
    let journal = migration_apply::journal(data).unwrap().unwrap();
    assert_eq!(journal["key_map"], json!(expected));
    let files = journal["files"].as_array().unwrap();
    assert_eq!(files.len(), 3);
    let db = rusqlite::Connection::open(&db_path).unwrap();
    for (i, file) in files.iter().enumerate() {
        let source = Path::new(file["source"].as_str().unwrap());
        let destination = Path::new(file["destination"].as_str().unwrap());
        assert_eq!(source.exists(), i == 2, "second-move source {i}");
        assert_eq!(destination.exists(), i < 2, "second-move destination {i}");
        let present = if i < 2 { destination } else { source };
        assert_eq!(
            crate::migration_files::digest(present).unwrap(),
            file["sha256"]
        );
        let stored: String = db
            .query_row(
                "SELECT file_path FROM items WHERE content_key=?",
                [file["key"].as_str().unwrap()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            stored,
            if i == 0 {
                crate::paths::store(data, destination).unwrap()
            } else {
                file["stored_source"].as_str().unwrap().to_owned()
            }
        );
    }
    db.close().unwrap();
    complete(data, "apply");
    assert!(!data.join("identity-migration.journal.json").exists());
    let bundle = manifest(data);
    let applied: Value = serde_json::from_slice(&std::fs::read(&bundle).unwrap()).unwrap();
    migration_bundle::applied(data, &project(), &applied).unwrap();
    let db = rusqlite::Connection::open(&db_path).unwrap();
    for (i, old) in old.iter().enumerate() {
        let key = &expected[old];
        let (stored, cid, fuid): (String,String,String) = db.query_row(
            "SELECT i.file_path,p.custom_emoji_id,s.file_unique_id FROM items i JOIN publications p USING(content_key) JOIN seen_files s USING(content_key) WHERE i.content_key=?",
            [key], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        ).unwrap();
        assert_eq!(cid, format!("CID{i}"));
        assert_eq!(fuid, format!("FUID{i}"));
        assert_eq!(stored, format!("./{i:03}_video_{}.webm", &key[2..14]));
        assert_eq!(std::fs::read(data.join(&stored)).unwrap(), originals[i].2);
        assert!(!originals[i].1.exists());
    }
    db.close().unwrap();
    complete(data, "restore");
    assert_eq!(
        crate::migration_signature::signature(&db_path, &[]).unwrap(),
        before
    );
    for (name, expected) in [
        ("publish_fixture.json", state),
        ("publish_plan_fixture.json", plan),
    ] {
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(data.join(name)).unwrap()).unwrap(),
            expected
        );
    }
    for (source, original, bytes) in originals {
        assert_eq!(std::fs::read(source).unwrap(), bytes);
        assert_eq!(std::fs::read(original).unwrap(), bytes);
    }
    for file in files {
        assert!(!Path::new(file["destination"].as_str().unwrap()).exists());
    }
    assert!(!data.join("identity-migration.journal.json").exists());
}
