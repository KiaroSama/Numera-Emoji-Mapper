//! The complete static report/refusal sequence reuses historical output and copied command custody.
use _native::{atomic, logging::RunLog, tooling_process};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::path::Path;
use std::time::{Duration, Instant};

#[path = "retained_cli.rs"]
mod retained_cli;
#[path = "tooling_fixture.rs"]
mod tooling_fixture;

pub fn check(project: &Path, runtime: &tokio::runtime::Runtime, original: &Value) {
    assert_eq!(
        original["content_key"],
        "s:1745338c58098cc08d6406ac9d629f6a"
    );
    assert_eq!(original["phash"], 1096366658209276687_u64);
    assert_eq!(original["report"]["code"], 0);
    assert_eq!(original["preview"]["code"], 0);
    let deadline = Instant::now() + Duration::from_secs(60);
    let root = tooling_fixture::Root::new("original static identity commands");
    let (executable, environment) = retained_cli::install(project, &root.0);
    let log = RunLog::open(&root.0, "static-identity-command-fixture", Vec::new());
    let data = root.0.join("collection");
    std::fs::create_dir(&data).unwrap();
    let input = project.join("assets/numera-emoji-mapper-logo.png");
    assert_eq!(
        _native::tooling_manifest::digest(&input).unwrap(),
        "6f85376de3893dfa1e78045ce264103b0055acec400dd8a35222405d5c4f6951"
    );
    let bytes = std::fs::read(&input).unwrap();
    let source = data.join("source.png");
    std::fs::write(&source, &bytes).unwrap();
    let catalog = data.join("catalog.db");
    let db = Connection::open(&catalog).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT,file_path TEXT,format TEXT,phash INTEGER);CREATE TABLE publications(content_key TEXT);CREATE TABLE seen_files(content_key TEXT);").unwrap();
    let key = original["content_key"].as_str().unwrap();
    db.execute(
        "INSERT INTO items VALUES(?,?,?,?)",
        params![
            key,
            "./source.png",
            "static",
            original["phash"].as_u64().unwrap() as i64
        ],
    )
    .unwrap();
    db.close().unwrap();
    let before = std::fs::read(&catalog).unwrap();
    let plan = data.join("publish_plan_fixture.json");
    atomic::write_json(&plan, &json!({"static":[key,"s:obsolete"]})).unwrap();
    let plan_bytes = std::fs::read(&plan).unwrap();
    let eof = root.0.join("empty-input.txt");
    std::fs::write(&eof, b"").unwrap();
    let run = |options: &[&str]| {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            remaining > Duration::from_secs(1),
            "static report deadline exhausted; retained"
        );
        let mut argv = vec![
            executable.as_os_str().to_owned(),
            OsString::from("identity-repair"),
        ];
        argv.extend(options.iter().map(|option| OsString::from(*option)));
        runtime
            .block_on(tooling_process::capture_with_input(
                &root.0,
                &argv,
                &environment,
                &eof,
                remaining.min(Duration::from_secs(10)),
                remaining.min(Duration::from_secs(5)),
                &log,
            ))
            .unwrap()
    };
    let preserved = || {
        assert_eq!(std::fs::read(&catalog).unwrap(), before);
        assert_eq!(std::fs::read(&source).unwrap(), bytes);
        assert_eq!(std::fs::read(&plan).unwrap(), plan_bytes);
    };
    let lease =
        _native::locks::Lease::acquire(&data.join(".maintenance.lock"), "fixture UTC").unwrap();
    let blocked = run(&["report"]);
    assert_eq!(blocked.code, 4, "{} {}", blocked.stdout, blocked.stderr);
    assert!(
        !data.join("tmp/identity-codec").exists(),
        "ownership must precede decoding"
    );
    preserved();
    drop(lease);
    let report = run(&["report"]);
    assert_eq!(report.code, 0, "{} {}", report.stdout, report.stderr);
    let original_path = original["catalog"].as_str().unwrap();
    let prefix = format!("catalog: {original_path}\n");
    let expected = original["report"]["stdout"].as_str().unwrap();
    let remainder = expected
        .strip_prefix(&prefix)
        .expect("original report must identify its exact input");
    assert_eq!(
        report.stdout,
        format!("catalog: {}\n{remainder}", catalog.display())
    );
    assert!(report.stdout.contains("Clean: identities, media"));
    assert!(report.stdout.contains("s:obsolete"));
    preserved();
    std::fs::remove_file(&plan).unwrap();
    let preview = run(&["migrate-video-keys"]);
    assert_eq!(
        preview.code,
        original["preview"]["code"].as_i64().unwrap() as i32,
        "{} {}",
        preview.stdout,
        preview.stderr
    );
    assert_eq!(
        preview.stdout,
        original["preview"]["stdout"].as_str().unwrap()
    );
    assert_eq!(std::fs::read(&catalog).unwrap(), before);
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    assert!(!plan.exists());
    atomic::write_json(&plan, &json!({"static":[key,"s:obsolete"]})).unwrap();
    let journal = data.join("identity-migration.journal.json");
    for corrupt in ["{truncated", "{}", "[]"] {
        std::fs::write(&journal, corrupt.as_bytes()).unwrap();
        let refused = run(&["report"]);
        assert_eq!(refused.code, 4, "{} {}", refused.stdout, refused.stderr);
        assert_eq!(std::fs::read(&journal).unwrap(), corrupt.as_bytes());
        preserved();
    }
    std::fs::remove_file(&journal).unwrap();
    let absent = run(&["report", "--data-dir", "missing-catalog"]);
    assert_eq!(absent.code, 2, "{} {}", absent.stdout, absent.stderr);
    assert!(!root.0.join("missing-catalog").exists());
    preserved();
    for options in [
        vec!["migrate-video-keys", "--from-backup", "missing.db"],
        vec![
            "migrate-video-keys",
            "--from-backup",
            "missing.db",
            "--apply",
        ],
    ] {
        let absent = run(&options);
        assert_eq!(absent.code, 2, "{} {}", absent.stdout, absent.stderr);
        assert!(!data.join("missing.db").exists());
        preserved();
    }
    let curation = data.join("pack_plan.json");
    atomic::write_json(&curation, &json!({"version":1,"moves":{},"held":[]})).unwrap();
    let curation_bytes = std::fs::read(&curation).unwrap();
    let malformed = run(&["report"]);
    assert_eq!(
        malformed.code, 4,
        "{} {}",
        malformed.stdout, malformed.stderr
    );
    assert_eq!(std::fs::read(&curation).unwrap(), curation_bytes);
    preserved();
    std::fs::remove_file(&curation).unwrap();
    std::fs::remove_file(&source).unwrap();
    let missing = run(&["report"]);
    assert_eq!(missing.code, 4, "{} {}", missing.stdout, missing.stderr);
    assert!(
        missing.stdout.contains("missing catalog media"),
        "{}",
        missing.stdout
    );
    assert_eq!(std::fs::read(&catalog).unwrap(), before);
    assert!(!source.exists());
    atomic::write_json(
        &data.join("publish_fixture.json"),
        &json!({"sets":[{"keys":["s:missing"]}]}),
    )
    .unwrap();
    let required_bytes = std::fs::read(data.join("publish_fixture.json")).unwrap();
    let required = run(&["report"]);
    assert_eq!(required.code, 3, "{} {}", required.stdout, required.stderr);
    assert_eq!(std::fs::read(&catalog).unwrap(), before);
    assert_eq!(
        std::fs::read(data.join("publish_fixture.json")).unwrap(),
        required_bytes
    );
    assert_eq!(std::fs::read(&plan).unwrap(), plan_bytes);
    assert!(!source.exists() && !journal.exists());

    let static_data = root.0.join("static-apply");
    std::fs::create_dir(&static_data).unwrap();
    let static_source = static_data.join("s_fixture.png");
    std::fs::write(&static_source, &bytes).unwrap();
    let static_catalog = static_data.join("catalog.db");
    let db = Connection::open(&static_catalog).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT,format TEXT,phash INTEGER);CREATE TABLE publications(content_key TEXT,custom_emoji_id TEXT);CREATE TABLE seen_files(content_key TEXT,file_unique_id TEXT);").unwrap();
    db.execute(
        "INSERT INTO items VALUES(?,?,?,?)",
        params![
            key,
            "static-apply/s_fixture.png",
            "static",
            original["phash"].as_u64().unwrap() as i64
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO publications VALUES(?,?)",
        params![key, "111111111"],
    )
    .unwrap();
    db.execute(
        "INSERT INTO seen_files VALUES(?,?)",
        params![key, "fixture-unique"],
    )
    .unwrap();
    db.close().unwrap();
    let static_before = std::fs::read(&static_catalog).unwrap();
    let static_state = static_data.join("publish_fixture.json");
    atomic::write_json(
        &static_state,
        &json!({"sets":[{"keys":[key]}],"skipped":[]}),
    )
    .unwrap();
    let state_before = std::fs::read(&static_state).unwrap();
    let unchanged = || {
        assert_eq!(std::fs::read(&static_catalog).unwrap(), static_before);
        assert_eq!(std::fs::read(&static_source).unwrap(), bytes);
        assert_eq!(std::fs::read(&static_state).unwrap(), state_before);
        for entry in std::fs::read_dir(&static_data).unwrap() {
            let entry = entry.unwrap();
            assert!(
                !entry
                    .file_name()
                    .to_str()
                    .unwrap()
                    .ends_with(".rollback.json")
            );
        }
        assert!(!static_data.join("identity-migration.journal.json").exists());
    };
    let applied = run(&[
        "migrate-video-keys",
        "--apply",
        "--data-dir",
        "static-apply",
    ]);
    assert_eq!(applied.code, 0, "{} {}", applied.stdout, applied.stderr);
    let result: Value = serde_json::from_str(applied.stdout.trim()).unwrap();
    assert_eq!(
        result,
        json!({"noop":true,"moved":0,"state":[],"renamed":[]})
    );
    unchanged();
    let dotted = static_data.join("publish_x.y.json");
    std::fs::write(&dotted, &state_before).unwrap();
    let family_path = _native::locks::pack_family_path(&root.0, "x.y");
    let family = _native::locks::Lease::acquire(&family_path, "fixture UTC").unwrap();
    let refused = run(&[
        "migrate-video-keys",
        "--apply",
        "--data-dir",
        "static-apply",
    ]);
    assert_eq!(refused.code, 4, "{} {}", refused.stdout, refused.stderr);
    assert_eq!(std::fs::read(&dotted).unwrap(), state_before);
    unchanged();
    drop(family);
    assert!(family_path.is_file());
    std::fs::remove_file(&dotted).unwrap();
    let frozen = static_data.join("publish_plan_fixture.json");
    std::fs::write(&frozen, b"{truncated").unwrap();
    let refused = run(&[
        "migrate-video-keys",
        "--apply",
        "--data-dir",
        "static-apply",
    ]);
    assert_eq!(refused.code, 4, "{} {}", refused.stdout, refused.stderr);
    assert_eq!(std::fs::read(&frozen).unwrap(), b"{truncated");
    unchanged();
    if static_data.join("tmp/identity-codec").exists() {
        assert!(
            std::fs::read_dir(static_data.join("tmp/identity-codec"))
                .unwrap()
                .next()
                .is_none()
        );
    }
    assert_eq!(std::fs::read(&input).unwrap(), bytes);
    if data.join("tmp/identity-codec").exists() {
        assert!(
            std::fs::read_dir(data.join("tmp/identity-codec"))
                .unwrap()
                .next()
                .is_none()
        );
    }
    assert!(!_native::process_owner::cleanup_pending(&root.0));
    assert!(
        std::fs::read_dir(root.0.join("logs/tmp/tooling"))
            .unwrap()
            .next()
            .is_none()
    );
    assert!(
        Instant::now() < deadline,
        "static report command cohort exceeded60s; retained"
    );
    log.finish(0);
    drop(log);
    root.clean().unwrap();
}
