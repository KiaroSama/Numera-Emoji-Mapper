//! Original offline status outcomes through the actual command and read-only catalog queries.
use _native::config::Config;
use rusqlite::Connection;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

fn database(path: &Path, pending: bool) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let db = Connection::open(path).unwrap();
    db.execute_batch(
        "CREATE TABLE items(content_key TEXT, included INTEGER, file_path TEXT, format TEXT, keywords TEXT);
         CREATE TABLE publications(base TEXT, content_key TEXT, set_name TEXT, custom_emoji_id TEXT);
         INSERT INTO items VALUES('s:a',1,'./media/a.webp','static','[]'),
                                 ('s:b',1,'./media/b.webp','static','[]'),
                                 ('s:c',0,'./media/c.webp','static','[]');
         INSERT INTO publications VALUES('mypacks','s:a','set1','101');",
    ).unwrap();
    if !pending {
        db.execute(
            "INSERT INTO publications VALUES('mypacks','s:b','set1','102')",
            [],
        )
        .unwrap();
    }
    db.close().unwrap();
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut entries = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    let mut left = 64;
    while let Some(path) = pending.pop() {
        assert!(left > 0, "status fixture inventory exceeded its bound");
        left -= 1;
        _native::tooling_environment::reject_links(&path, "status fixture cannot follow links")
            .unwrap();
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        let relative = path.strip_prefix(root).unwrap().to_path_buf();
        if metadata.is_dir() {
            entries.insert(relative, None);
            pending.extend(
                std::fs::read_dir(&path)
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

#[test]
fn original_pending_count_is_read_only_and_unreadable_is_not_zero() {
    let root = Root::new("status pending café");
    let data = root.0.join("collection");
    std::fs::create_dir(&data).unwrap();
    let absent = data.join("absent.db");
    assert!(_native::catalog_read::pending(&absent, "mypacks").is_err());
    assert!(!absent.exists());
    let db = data.join("catalog.db");
    database(&db, true);
    let before = snapshot(&data);
    assert_eq!(_native::catalog_read::pending(&db, "mypacks").unwrap(), 1);
    assert_eq!(_native::catalog_read::pending(&db, "another").unwrap(), 2);
    assert_eq!(snapshot(&data), before);
    let broken = data.join("broken.db");
    std::fs::write(&broken, b"not a database").unwrap();
    let before = snapshot(&data);
    assert!(_native::catalog_read::pending(&broken, "mypacks").is_err());
    assert_eq!(snapshot(&data), before);
    root.clean().unwrap();
}

#[test]
fn original_status_outcomes_and_fixes_use_the_actual_command_without_data_writes() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = Root::new("status command café");
    let executable = root.0.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::create_dir(root.0.join("assets")).unwrap();
    std::fs::write(root.0.join("assets/panel.html"), b"isolated fixture").unwrap();
    std::fs::write(root.0.join("pyproject.toml"), b"# isolated fixture\n").unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let environment =
        _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
    let environment =
        _native::tooling_environment::environment(&root.0, &environment, false).unwrap();
    let log = _native::logging::RunLog::open(&root.0, "status-test-owner", vec![]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut logs = std::collections::BTreeSet::new();
    for label in [
        "stale-roster",
        "current",
        "stale-archive",
        "pending",
        "pending-stale",
        "unset-archive",
        "unset-base",
        "missing-catalog",
        "bad-catalog",
        "usage",
    ] {
        let directory = root.0.join(label);
        let data = directory.join("collection");
        let packs = directory.join("packs");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir(&packs).unwrap();
        let base = if label == "unset-base" { "" } else { "mypacks" };
        if label == "bad-catalog" {
            std::fs::write(data.join("catalog.db"), b"not a database").unwrap();
        } else if label != "missing-catalog" {
            database(
                &data.join("catalog.db"),
                ["pending", "pending-stale"].contains(&label),
            );
        }
        if label == "stale-archive" {
            _native::atomic::write_json(
                &data.join(format!("publish_{base}.json")),
                &json!({"sets":[{"name":"set1","title":"Pack 1","live":200}]}),
            )
            .unwrap();
        }
        let archive = directory.join("archive");
        let mut values = BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("COLLECTION_PACK_BASE".into(), base.into()),
        ]);
        if !["unset-archive", "bad-catalog", "pending", "pending-stale"].contains(&label) {
            values.insert("EMOJI_ARCHIVE_DIR".into(), archive.to_str().unwrap().into());
        }
        let config = Config::load_with_environment(directory.clone(), &values).unwrap();
        if !["stale-roster", "pending-stale"].contains(&label) {
            _native::atomic::write_json(
                &packs.join("index.json"),
                &json!({"inputs":_native::roster_cli::fingerprint(&config).unwrap()}),
            )
            .unwrap();
        }
        let before_data = snapshot(&data);
        let before_packs = snapshot(&packs);
        let mut child_env = environment.clone();
        for key in ["COLLECTION_PACK_BASE", "EMOJI_ARCHIVE_DIR"] {
            child_env.remove(key);
        }
        child_env.extend(values);
        // The binary resolves its project from its own physical location, not the caller CWD.
        let child = directory.join(executable.file_name().unwrap());
        std::fs::rename(&executable, &child).unwrap();
        std::fs::create_dir(directory.join("assets")).unwrap();
        std::fs::write(directory.join("assets/panel.html"), b"isolated fixture").unwrap();
        std::fs::write(directory.join("pyproject.toml"), b"# isolated fixture\n").unwrap();
        let mut args = vec![child.into_os_string(), "status".into()];
        if label == "usage" {
            args.push("--unknown".into());
        }
        let result = runtime
            .block_on(_native::tooling_process::capture(
                &root.0,
                &args,
                &child_env,
                Duration::from_secs(5),
                Duration::from_secs(3),
                &log,
            ))
            .unwrap();
        std::fs::rename(directory.join(executable.file_name().unwrap()), &executable).unwrap();
        let stale = ["stale-roster", "stale-archive", "pending-stale"].contains(&label);
        assert_eq!(
            result.code,
            if label == "usage" {
                2
            } else if stale {
                3
            } else {
                0
            },
            "{label}: {}",
            result.stderr
        );
        match label {
            "stale-roster" => assert!(
                result.stdout.contains("STALE roster:")
                    && result
                        .stdout
                        .contains("numera-emoji pack-manifest --refresh")
            ),
            "stale-archive" => assert!(
                result.stdout.contains("Pack 1: full but never archived")
                    && result.stdout.contains("numera-emoji pack-archive --sync")
            ),
            "pending" | "pending-stale" => assert!(
                result
                    .stdout
                    .contains("1 included emoji not yet published to mypacks")
                    && result
                        .stdout
                        .contains("numera-emoji build-collection --base mypacks")
            ),
            "current" => assert!(
                result
                    .stdout
                    .contains("every included emoji is published to mypacks")
                    && result.stdout.contains("every full pack is archived")
            ),
            "unset-archive" => assert!(
                result
                    .stdout
                    .contains("skip  archive: EMOJI_ARCHIVE_DIR is not set")
            ),
            "unset-base" | "missing-catalog" | "bad-catalog" => assert!(
                result
                    .stdout
                    .contains("skip  catalog: no readable catalog or COLLECTION_PACK_BASE unset")
            ),
            "usage" => assert!(result.stderr.contains("status takes no options")),
            _ => unreachable!(),
        }
        if ["pending", "pending-stale"].contains(&label) {
            assert!(
                result
                    .stdout
                    .contains("1 included emoji not yet published to mypacks")
            );
            assert!(result.stdout.contains("skip  archive"));
            if label == "pending-stale" {
                assert!(result.stdout.contains("STALE roster:"));
                assert!(
                    result
                        .stdout
                        .contains("numera-emoji pack-manifest --refresh")
                );
            }
        }
        if !stale {
            assert!(!result.stdout.contains("STALE"));
        }
        assert_eq!(snapshot(&data), before_data, "{label}");
        assert_eq!(snapshot(&packs), before_packs, "{label}");
        assert!(!archive.exists());
        let child_logs = std::fs::read_dir(directory.join("logs"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("log"))
            .collect::<Vec<_>>();
        assert_eq!(child_logs.len(), 1, "{label}");
        assert!(logs.insert(child_logs[0].clone()));
        let text = std::fs::read_to_string(&child_logs[0]).unwrap();
        assert!(text.contains(" UTC] [INFO] [runtime]") && text.contains("native backend started"));
        if label != "usage" {
            assert!(text.contains(if stale {
                "offline state is stale"
            } else {
                "offline state is current"
            }));
        }
    }
    runtime.shutdown_timeout(Duration::from_secs(1));
    drop(log);
    root.clean().unwrap();
}

#[test]
fn original_status_sources_refuse_before_runtime_or_workspace_access() {
    use _native::source_git_inventory::Source;
    let absent = PathBuf::from("must-not-be-inspected");
    let source = Source {
        baseline_revision: "0".repeat(40),
        path: "unverified.py".into(),
        sha256: "0".repeat(64),
        text: "raise AssertionError('must not execute')".into(),
    };
    assert!(
        _native::source_status_tests::observe(&absent, &absent, &[&source; 9])
            .unwrap_err()
            .contains("source identity drift")
    );
    assert!(_native::source_status_tests::source_identity(&[]).is_err());
}
