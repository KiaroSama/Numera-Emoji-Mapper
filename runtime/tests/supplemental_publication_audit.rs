use std::{
    path::Path,
    process::{Command, Output},
};

#[path = "support/test_admission.rs"]
mod test_admission;

fn audit(dir: &Path) -> Output {
    audit_with(dir, &[])
}

fn audit_with(dir: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_numera-emoji"))
        .args(["audit-publication", "--base", "fixture", "--data-dir"])
        .arg(dir)
        .args(extra)
        .output()
        .unwrap()
}

fn seed_catalog(path: &Path) {
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute_batch(
        "CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT);
        CREATE TABLE items(
            content_key TEXT PRIMARY KEY, format TEXT NOT NULL, file_path TEXT NOT NULL,
            emojis TEXT NOT NULL DEFAULT '[]', keywords TEXT NOT NULL DEFAULT '[]',
            sources TEXT NOT NULL DEFAULT '[]', phash INTEGER, custom_emoji_id TEXT,
            uploaded INTEGER NOT NULL DEFAULT 0, included INTEGER NOT NULL DEFAULT 1,
            created_utc TEXT NOT NULL, position INTEGER
        );
        CREATE TABLE seen_files(file_unique_id TEXT PRIMARY KEY, content_key TEXT NOT NULL);
        CREATE TABLE publications(
            base TEXT NOT NULL, content_key TEXT NOT NULL, set_name TEXT, custom_emoji_id TEXT,
            uploaded_utc TEXT NOT NULL, PRIMARY KEY(base, content_key)
        );
        INSERT INTO meta VALUES ('publications_migrated', '1'), ('media_paths', 'data-relative-2');
        INSERT INTO items(content_key, format, file_path, uploaded, created_utc) VALUES
            ('s:one', 'static', 'converted/one.png', 1, '2026-11-26T00:00:00Z'),
            ('v:two', 'video', 'converted/two.webm', 1, '2026-11-26T00:00:00Z'),
            ('s:no_publication', 'static', 'converted/absent.png', 0, '2026-11-26T00:00:00Z'),
            ('s:no_id', 'static', 'converted/no-id.png', 1, '2026-11-26T00:00:00Z');
        INSERT INTO publications VALUES
            ('fixture', 's:one', 'fixture_set', '111111111', '2026-11-26T00:00:00Z'),
            ('fixture', 'v:two', 'video_set', '222222222', '2026-11-26T00:00:00Z'),
            ('other', 's:no_publication', 'another_set', '333333333', '2026-11-26T00:00:00Z'),
            ('fixture', 's:no_id', 'another_set', NULL, '2026-11-26T00:00:00Z');",
    )
    .unwrap();
}

fn assert_exit(case: &str, result: &Output, exit: i32) {
    assert_eq!(
        result.status.code(),
        Some(exit),
        "{case}: stdout={} stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn reports_publication_anomalies_without_changing_files() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("publish_fixture.json");
    let database = dir.path().join("catalog.db");
    assert_eq!(audit(dir.path()).status.code(), Some(2));
    let clean =
        br#"{"sets":[{"name":"fixture_set","fmt":"static","index":1,"keys":["s:one"],"live":1,"logo":false}]}"#;
    std::fs::write(&state, clean).unwrap();
    assert_eq!(audit(dir.path()).status.code(), Some(2));
    assert!(!database.exists());
    seed_catalog(&database);
    assert_eq!(audit(dir.path()).status.code(), Some(0));
    let broken = br#"{"sets":[{"name":"another_set","fmt":"static","index":1,
        "keys":["s:one","s:one","s:missing","s:no_publication","s:no_id"],"live":5,"logo":"yes"}]}"#;
    std::fs::write(&state, broken).unwrap();
    let before = std::fs::read(&database).unwrap();
    let result = audit(dir.path());
    assert_eq!(result.status.code(), Some(1));
    let stdout = String::from_utf8(result.stdout).unwrap();
    for label in [
        "MALFORMED_STATE",
        "MALFORMED_SET",
        "DUPLICATE_KEY: \"s:one\"",
        "MISSING_CATALOG_KEY: \"s:missing\"",
        "MISSING_PUBLICATION: \"s:no_publication\"",
        "NOT_UPLOADED: \"s:no_publication\"",
        "MISSING_CUSTOM_EMOJI_ID: \"s:no_id\"",
        "SET_NAME_CONFLICT: \"s:one\"",
    ] {
        assert!(stdout.contains(label), "{stdout}");
    }
    assert_eq!(std::fs::read(state).unwrap(), broken);
    assert_eq!(std::fs::read(database).unwrap(), before);
}

#[test]
fn accepts_compatible_defaults_and_valid_membership() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("publish_fixture.json");
    let database = dir.path().join("catalog.db");
    seed_catalog(&database);
    let before = std::fs::read(&database).unwrap();
    for (case, json) in [
        ("empty document", r#"{}"#),
        (
            "empty set with defaults",
            r#"{"sets":[{"name":"fixture_set","fmt":"static","index":1}]}"#,
        ),
        (
            "missing logo",
            r#"{"base":"fixture","sets":[{"name":"fixture_set","fmt":"static","index":1,"keys":["s:one"],"live":1}]}"#,
        ),
        (
            "brand occupies one slot",
            r#"{"sets":[{"name":"fixture_set","fmt":"static","index":1,"keys":["s:one"],"live":2,"logo":true}]}"#,
        ),
        (
            "unattributed live tail",
            r#"{"sets":[{"name":"fixture_set","fmt":"static","index":1,"keys":["s:one"],"live":2}]}"#,
        ),
        (
            "independent advancing format indices",
            r#"{"sets":[
                {"name":"fixture_set","fmt":"static","index":1,"keys":["s:one"],"live":1},
                {"name":"video_set","fmt":"video","index":1,"keys":["v:two"],"live":1},
                {"name":"static_two","fmt":"static","index":2}]}"#,
        ),
    ] {
        std::fs::write(&state, json).unwrap();
        let result = audit(dir.path());
        assert_exit(case, &result, 0);
        assert_eq!(std::fs::read(&state).unwrap(), json.as_bytes(), "{case}");
        assert_eq!(std::fs::read(&database).unwrap(), before, "{case}");
    }
}

#[test]
fn reports_semantic_state_errors_as_findings() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("publish_fixture.json");
    let database = dir.path().join("catalog.db");
    seed_catalog(&database);
    let before = std::fs::read(&database).unwrap();
    for (case, json) in [
        ("wrong family", r#"{"base":"other"}"#),
        (
            "invalid format",
            r#"{"sets":[{"name":"fixture_set","fmt":"sideways","index":1}]}"#,
        ),
        (
            "missing format",
            r#"{"sets":[{"name":"fixture_set","index":1}]}"#,
        ),
        (
            "zero index",
            r#"{"sets":[{"name":"fixture_set","fmt":"static","index":0}]}"#,
        ),
        (
            "missing index",
            r#"{"sets":[{"name":"fixture_set","fmt":"static"}]}"#,
        ),
        (
            "backward index within a format",
            r#"{"sets":[{"name":"first","fmt":"static","index":2},{"name":"second","fmt":"static","index":1}]}"#,
        ),
        (
            "empty title",
            r#"{"sets":[{"name":"fixture_set","fmt":"static","index":1,"title":""}]}"#,
        ),
        (
            "duplicate set names",
            r#"{"sets":[{"name":"fixture_set","fmt":"static","index":1},{"name":"fixture_set","fmt":"static","index":2}]}"#,
        ),
        ("invalid sent entry", r#"{"sent":[null]}"#),
        ("invalid sent_full list", r#"{"sent_full":null}"#),
        ("invalid skipped entry", r#"{"skipped":[""]}"#),
        (
            "live below membership",
            r#"{"sets":[{"name":"fixture_set","fmt":"static","index":1,"keys":["s:one"],"live":0}]}"#,
        ),
        (
            "live below membership with brand",
            r#"{"sets":[{"name":"fixture_set","fmt":"static","index":1,"keys":["s:one"],"live":1,"logo":true}]}"#,
        ),
        (
            "live above capacity",
            r#"{"sets":[{"name":"fixture_set","fmt":"static","index":1,"live":201}]}"#,
        ),
    ] {
        std::fs::write(&state, json).unwrap();
        let result = audit(dir.path());
        assert_exit(case, &result, 1);
        let stdout = String::from_utf8(result.stdout).unwrap();
        assert!(stdout.contains("MALFORMED_STATE:"), "{case}: {stdout}");
        assert!(!stdout.contains("MALFORMED_SET:"), "{case}: {stdout}");
        assert_eq!(std::fs::read(&state).unwrap(), json.as_bytes(), "{case}");
        assert_eq!(std::fs::read(&database).unwrap(), before, "{case}");
    }
}

#[test]
fn reports_uninspectable_inputs_as_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("publish_fixture.json");
    let database = dir.path().join("catalog.db");
    seed_catalog(&database);
    for (case, json) in [
        ("invalid JSON", "{not-json"),
        ("non-object root", "[]"),
        ("non-array sets", r#"{"sets":null}"#),
    ] {
        std::fs::write(&state, json).unwrap();
        let result = audit(dir.path());
        assert_exit(case, &result, 2);
        let stderr = String::from_utf8(result.stderr).unwrap();
        assert!(stderr.contains("UNKNOWN:"), "{case}: {stderr}");
    }

    std::fs::write(&state, "{}").unwrap();
    let db = rusqlite::Connection::open(&database).unwrap();
    db.execute("DROP TABLE publications", []).unwrap();
    drop(db);
    let before = std::fs::read(&database).unwrap();
    let result = audit(dir.path());
    assert_exit("missing catalog schema", &result, 2);
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert!(stderr.contains("UNKNOWN:"), "{stderr}");
    assert_eq!(std::fs::read(&state).unwrap(), b"{}");
    assert_eq!(std::fs::read(&database).unwrap(), before);
}

#[test]
fn json_report_reads_uncheckpointed_catalog_without_changing_durable_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let unknown = audit_with(dir.path(), &["--json"]);
    let invalid = audit_with(dir.path(), &["--json", "--invalid"]);
    for (output, code) in [
        (unknown, "STATE_UNREADABLE"),
        (invalid, "INVALID_ARGUMENTS"),
    ] {
        assert_exit(code, &output, 2);
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["status"], "unknown");
        assert_eq!(report["findings"], serde_json::json!([]));
        assert_eq!(report["error"]["code"], code);
        assert!(output.stderr.is_empty());
    }
    let state = dir.path().join("publish_fixture.json");
    std::fs::write(
        &state,
        br#"{"sets":[{"name":"fixture_set","fmt":"static","index":1,"keys":["s:one"],"live":1,"logo":false}]}"#,
    )
    .unwrap();
    let db = rusqlite::Connection::open(dir.path().join("catalog.db")).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;
        CREATE TABLE items(content_key TEXT PRIMARY KEY, uploaded INTEGER NOT NULL);
        CREATE TABLE publications(base TEXT NOT NULL, content_key TEXT NOT NULL, set_name TEXT, custom_emoji_id TEXT, uploaded_utc TEXT NOT NULL, PRIMARY KEY(base,content_key));
        INSERT INTO items VALUES('s:one',1);
        INSERT INTO publications VALUES('fixture','s:one','fixture_set','111111111','fixture UTC');").unwrap();
    let paths =
        ["catalog.db", "catalog.db-wal", "publish_fixture.json"].map(|name| dir.path().join(name));
    let before = paths.each_ref().map(|path| std::fs::read(path).unwrap());
    let shm = dir.path().join("catalog.db-shm");
    let shm_length = std::fs::metadata(&shm).unwrap().len();
    assert_exit("text WAL report", &audit(dir.path()), 0);
    let clean = audit_with(dir.path(), &["--json"]);
    assert_exit("JSON WAL report", &clean, 0);
    let report: serde_json::Value = serde_json::from_slice(&clean.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["status"], "clean");
    assert!(report["error"].is_null());
    assert_eq!(
        paths.each_ref().map(|path| std::fs::read(path).unwrap()),
        before
    );
    assert_eq!(std::fs::metadata(shm).unwrap().len(), shm_length);
    db.execute("UPDATE items SET uploaded=0", []).unwrap();
    let findings = audit_with(dir.path(), &["--json"]);
    assert_exit("JSON finding", &findings, 1);
    let report: serde_json::Value = serde_json::from_slice(&findings.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["status"], "findings");
    assert_eq!(report["findings"][0]["code"], "NOT_UPLOADED");
    assert_eq!(report["findings"][0]["set_index"], 0);
    assert_eq!(report["findings"][0]["content_key"], "s:one");
    assert!(report["error"].is_null());
}
