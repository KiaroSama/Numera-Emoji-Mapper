use std::process::Command;

#[path = "support/test_admission.rs"]
mod test_admission;

fn audit(dir: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_numera-emoji"))
        .args(["audit-publication", "--base", "fixture", "--data-dir"])
        .arg(dir)
        .output()
        .unwrap()
}

#[test]
fn reports_publication_anomalies_without_changing_files() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("publish_fixture.json");
    let database = dir.path().join("catalog.db");
    assert_eq!(audit(dir.path()).status.code(), Some(2));
    let clean = br#"{"sets":[{"name":"fixture_set","keys":["s:one"],"live":1,"logo":false}]}"#;
    std::fs::write(&state, clean).unwrap();
    assert_eq!(audit(dir.path()).status.code(), Some(2));
    assert!(!database.exists());
    let db = rusqlite::Connection::open(&database).unwrap();
    db.execute_batch(
        "CREATE TABLE items(content_key TEXT, uploaded INTEGER);
        CREATE TABLE publications(base TEXT, content_key TEXT, custom_emoji_id TEXT, set_name TEXT);
        INSERT INTO items VALUES('s:one', 1);
        INSERT INTO publications VALUES('fixture', 's:one', '111111111', 'fixture_set');",
    )
    .unwrap();
    drop(db);
    assert_eq!(audit(dir.path()).status.code(), Some(0));
    let broken =
        br#"{"sets":[{"name":"another_set","keys":["s:one","s:one","s:missing"],"live":3}]}"#;
    std::fs::write(&state, broken).unwrap();
    let before = std::fs::read(&database).unwrap();
    let result = audit(dir.path());
    assert_eq!(result.status.code(), Some(1));
    let stdout = String::from_utf8(result.stdout).unwrap();
    for label in [
        "MALFORMED_SET",
        "DUPLICATE_KEY",
        "MISSING_CATALOG_KEY",
        "SET_NAME_CONFLICT",
    ] {
        assert!(stdout.contains(label), "{stdout}");
    }
    assert_eq!(std::fs::read(state).unwrap(), broken);
    assert_eq!(std::fs::read(database).unwrap(), before);
}
