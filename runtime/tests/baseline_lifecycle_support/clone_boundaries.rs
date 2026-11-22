use super::*;
// Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_cloning_does_not_migrate_or_change_source_journal_mode
#[test]
fn test_cloning_does_not_migrate_or_change_source_journal_mode() {
    let root = Root::new();
    let source = root.source(&["s:a"], false);
    let db = source.join("catalog.db");
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection
        .execute_batch("PRAGMA journal_mode=DELETE; DROP TABLE meta;")
        .unwrap();
    drop(connection);
    let plan = br#"{"version":1,"targets":[],"known":[],"excluded":[],"moves":[],"held":[]}"#;
    root.file("source/pack_plan.json", plan);
    let before = database_state(&db);
    let bytes = std::fs::read(&db).unwrap();
    let session = root.clone(&source);
    assert_eq!(database_state(&db), before);
    assert_eq!(std::fs::read(&db).unwrap(), bytes);
    assert_eq!(std::fs::read(source.join("pack_plan.json")).unwrap(), plan);
    let copied = Catalog::open(&session.directory.join("catalog.db"), UTC, &root.0).unwrap();
    assert_eq!(copied.all(None).unwrap().len(), 1);
    drop(copied);
    assert_eq!(database_state(&db), before);
    session.finish().unwrap();
}
// Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_a_refused_clone_also_leaves_the_source_database_unchanged
#[test]
fn test_a_refused_clone_also_leaves_the_source_database_unchanged() {
    let root = Root::new();
    let source = root.source(&["s:a"], false);
    let db = source.join("catalog.db");
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection
        .execute_batch("PRAGMA journal_mode=DELETE")
        .unwrap();
    drop(connection);
    std::fs::remove_file(source.join("media/0.png")).unwrap();
    let before = std::fs::read(&db).unwrap();
    assert!(tooling_sandbox::Session::create(&root.0, &source, &root.destination()).is_err());
    assert_eq!(std::fs::read(&db).unwrap(), before);
    assert!(!root.destination().exists());
}
// Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_project_relative_path_is_not_shadowed_by_a_source_relative_file
#[test]
fn test_project_relative_path_is_not_shadowed_by_a_source_relative_file() {
    let root = Root::new();
    let source = root.source(&["s:a"], false);
    root.file("media/0.png", b"PROJECT-original");
    root.file("source/media/0.png", b"SOURCE-decoy");
    set_media_path(&source, "media/0.png");
    let session = root.clone(&source);
    assert_eq!(
        std::fs::read(&paths(&session.directory)[0]).unwrap(),
        b"PROJECT-original"
    );
    session.finish().unwrap();
}
// Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_data_relative_path_is_not_shadowed_by_a_project_relative_file
#[test]
fn test_data_relative_path_is_not_shadowed_by_a_project_relative_file() {
    let root = Root::new();
    let source = root.source(&["s:a"], false);
    root.file("media/0.png", b"PROJECT-decoy");
    root.file("source/media/0.png", b"SOURCE-original");
    set_media_path(&source, "./media/0.png");
    let session = root.clone(&source);
    assert_eq!(
        std::fs::read(&paths(&session.directory)[0]).unwrap(),
        b"SOURCE-original"
    );
    session.finish().unwrap();
}
// Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_portable_names_do_not_alias_distinct_catalog_keys
#[test]
fn test_portable_names_do_not_alias_distinct_catalog_keys() {
    let root = Root::new();
    let source = root.source(&["s:a_b", "s_a:b"], false);
    let session = root.clone(&source);
    let names = paths(&session.directory);
    assert_eq!(names.len(), 2);
    assert_ne!(names[0], names[1]);
    let bodies = names
        .iter()
        .map(|path| std::fs::read(path).unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        bodies,
        [b"\x89PNG-0".to_vec(), b"\x89PNG-1".to_vec()]
            .into_iter()
            .collect()
    );
    session.finish().unwrap();
}
// Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_wrapper_keeps_ownership_from_marker_through_complete_cleanup
#[test]
fn test_wrapper_keeps_ownership_from_marker_through_complete_cleanup() {
    let root = Root::new();
    root.install();
    let source = root.source(&["s:a"], false);
    let mut live = Live::wrapper(&root, &source, port());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    live.ready();
    let clone = active_clone(&root);
    let base = clone.parent().unwrap().to_path_buf();
    let lock = tooling_sandbox::lifetime_lock_path(&clone).unwrap();
    assert_eq!(tooling_sandbox::sweep_stale(&base).unwrap(), 0);
    live.ready_for_catalog(deadline);
    assert_eq!(
        Catalog::open(&clone.join("catalog.db"), UTC, &root.0)
            .unwrap()
            .all(None)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(live.graceful(), 130);
    assert!(!clone.exists());
    assert!(lock.is_file());
    assert_eq!(tooling_sandbox::sweep_stale(&base).unwrap(), 0);
}
// Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_old_marker_protocol_is_left_alone_not_mistaken_for_abandonment
#[test]
fn test_old_marker_protocol_is_left_alone_not_mistaken_for_abandonment() {
    let root = Root::new();
    let source = root.source(&["s:a"], false);
    let session = root.clone(&source);
    let directory = session.directory.clone();
    drop(session);
    let marker = directory.join(".sandbox-owner.json");
    let mut document: Value = serde_json::from_slice(&std::fs::read(&marker).unwrap()).unwrap();
    document["version"] = json!(1);
    std::fs::write(marker, serde_json::to_vec(&document).unwrap()).unwrap();
    assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
    assert!(directory.join("catalog.db").is_file());
}
// Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_process_death_releases_construction_lease_for_safe_reclamation
#[test]
fn test_process_death_releases_construction_lease_for_safe_reclamation() {
    let root = Root::new();
    let source = root.source(&["s:a"], false);
    let mut child = Construction::spawn(&root, &source);
    child.ready();
    let before = runtime::package::digest(&root.destination().join("catalog.db")).unwrap();
    assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
    assert_eq!(
        runtime::package::digest(&root.destination().join("catalog.db")).unwrap(),
        before
    );
    child.stop();
    assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 1);
    assert!(!root.destination().join("catalog.db").exists());
    assert!(
        tooling_sandbox::lifetime_lock_path(&root.destination())
            .unwrap()
            .is_file()
    );
}
// Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_real_wrapper_http_save_changes_only_the_owned_clone
#[test]
fn test_real_wrapper_http_save_changes_only_the_owned_clone() {
    let root = Root::new();
    root.install();
    let source = root.source(
        &[
            "s:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "s:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ],
        false,
    );
    let before = database_state(&source.join("catalog.db"));
    let mut live = Live::wrapper(&root, &source, port());
    live.ready();
    let clone = active_clone(&root);
    let (_, page) = live.get("/");
    let page = String::from_utf8(page).unwrap();
    let token = page
        .split("const TOKEN = \"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let body = page
        .split("<script id=\"items-data\" type=\"application/json\">")
        .nth(1)
        .unwrap()
        .split("</script>")
        .next()
        .unwrap();
    let cards: Vec<Value> = serde_json::from_str(body).unwrap();
    let keys = cards
        .iter()
        .filter(|card| card["isLogo"] != true)
        .map(|card| card["key"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(keys.len(), 2);
    let response: Value = reqwest::blocking::Client::new()
        .post(format!("{}/api/save", live.url))
        .header("X-Panel-Token", token)
        .json(&json!({"known":keys,"excluded":[keys[0]],"packs":[]}))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(response["ok"], true, "{response}");
    let connection = rusqlite::Connection::open_with_flags(
        clone.join("catalog.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let excluded = connection
        .prepare("SELECT content_key FROM items WHERE included=0")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(excluded, [keys[0].clone()]);
    drop(connection);
    assert_eq!(database_state(&source.join("catalog.db")), before);
    assert_eq!(
        tooling_sandbox::sweep_stale(clone.parent().unwrap()).unwrap(),
        0
    );
    live.owner.finish().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let address = live
        .url
        .strip_prefix("http://")
        .unwrap()
        .parse::<std::net::SocketAddr>()
        .unwrap();
    while std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_millis(100))
        .is_ok()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "actual panel still listened ten seconds after controller death"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // Native safety disposition: abrupt owner death cannot certify every
    // descendant stopped. Its pre-spawn recovery copy is retained rather
    // than promoting a released lease into automatic reclamation proof.
    assert!(clone.join(".sandbox-recovery.json").is_file());
    assert_eq!(
        tooling_sandbox::sweep_stale(clone.parent().unwrap()).unwrap(),
        0
    );
    assert!(clone.join("catalog.db").is_file());
    assert_eq!(database_state(&source.join("catalog.db")), before);
}
// Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_panel_exception_cleans_owned_data_and_restores_environment
#[test]
fn test_panel_exception_cleans_owned_data_and_restores_environment() {
    let root = Root::new();
    root.install();
    let source = root.source(&["s:a"], false);
    let before = std::env::vars().collect::<BTreeMap<_, _>>();
    let occupied = Legacy::new();
    let result = root.dev(&[
        "panel-sandbox",
        "--source",
        source.to_str().unwrap(),
        "--port",
        &occupied.port.to_string(),
    ]);
    assert_eq!(result.code, 2, "{} {}", result.stdout, result.stderr);
    assert!(std::env::vars().collect::<BTreeMap<_, _>>() == before);
    assert!(
        !std::fs::read_dir(root.0.join("logs/tmp"))
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(tooling_sandbox::PREFIX))
    );
    assert!(source.join("catalog.db").is_file());
}
