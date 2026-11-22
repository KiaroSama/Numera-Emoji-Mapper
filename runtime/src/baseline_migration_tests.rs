//! Runtime-only real process interruption at the original plan-replacement boundary.
use crate::baseline_catalog_support as support;
use crate::{config::Config, migration_apply, process_owner::OwnedProcess};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use support::*;

// Reused from native/src/migration_crash_tests.rs. This module exists only in the unit-test binary.
pub(crate) fn barrier(stage: &str) {
    if let Ok(data) = std::env::var("NUMERA_BASELINE_IDENTITY_DATA") {
        let data = PathBuf::from(data);
        let action = std::env::var("NUMERA_BASELINE_IDENTITY_ACTION").unwrap_or_default();
        if stage == "second-move" && action == "exit73" {
            std::process::exit(73);
        }
        if stage == "files" && action == "absolute-paths" {
            // Original old_apply_files writes the trusted intent destination
            // literally, rather than reconstructing a new OS-specific alias.
            let journal = migration_apply::journal(&data).unwrap().unwrap();
            let db = rusqlite::Connection::open(data.join("catalog.db")).unwrap();
            for file in journal["files"].as_array().unwrap() {
                db.execute(
                    "UPDATE items SET file_path=? WHERE content_key=?",
                    rusqlite::params![
                        file["destination"].as_str().unwrap(),
                        file["key"].as_str().unwrap()
                    ],
                )
                .unwrap();
            }
        }
        if stage == "files" && action == "late-missing" {
            let doc = migration_apply::journal(&data).unwrap().unwrap();
            std::fs::remove_file(doc["files"][0]["destination"].as_str().unwrap()).unwrap();
        }
    }
    if std::env::var("NUMERA_MIGRATION_TEST_STAGE").as_deref() == Ok(stage) {
        let ready = std::env::var_os("NUMERA_MIGRATION_TEST_READY").unwrap();
        crate::atomic::write_json(Path::new(&ready), &json!({"stage":stage})).unwrap();
        if let Some(release) = std::env::var_os("NUMERA_BASELINE_RELEASE") {
            let deadline = Instant::now() + Duration::from_secs(30);
            while !Path::new(&release).exists() {
                assert!(Instant::now() < deadline, "writer release timed out");
                std::thread::sleep(Duration::from_millis(10));
            }
            std::process::exit(0);
        }
        std::thread::park_timeout(Duration::from_secs(30));
        panic!("migration test barrier was not terminated by its owner");
    }
}
#[path = "original_main_rust_migration_tests.rs"]
mod original_main_rust;

#[test]
fn migration_worker() {
    if let Some(data) = std::env::var_os("NUMERA_ORIGINAL_MAIN_MIGRATION_DATA") {
        original_main_rust::run_worker(data);
        return;
    }
    let Some(data) = std::env::var_os("NUMERA_BASELINE_MIGRATION_TEST_DATA") else {
        return;
    };
    let data = PathBuf::from(data);
    let root = data.parent().unwrap();
    let config = Config::load_with_environment(
        root.into(),
        &BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]),
    )
    .unwrap();
    migration_apply::apply(
        &config,
        &data,
        &Fingerprint {
            key: NEW.into(),
            phash: 9,
        },
        &runtime(),
        None,
    )
    .unwrap();
}
fn held_worker(data: &Path) -> (OwnedProcess, PathBuf) {
    let scratch = data.join("worker");
    std::fs::create_dir(&scratch).unwrap();
    let ready = scratch.join("ready.json");
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "migration_crash_tests::migration_worker",
            "--nocapture",
        ])
        .env("NUMERA_BASELINE_MIGRATION_TEST_DATA", data)
        .env("NUMERA_MIGRATION_TEST_STAGE", "pack_plan.json")
        .env("NUMERA_MIGRATION_TEST_READY", &ready)
        .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
        .env("TEMP", &scratch)
        .env("TMP", &scratch)
        .env("TMPDIR", &scratch)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(scratch.join("stderr.txt")).unwrap());
    for (name, _) in std::env::vars() {
        if ["TOKEN", "SECRET", "PASSWORD", "API_KEY"]
            .iter()
            .any(|part| name.to_uppercase().contains(part))
        {
            command.env_remove(name);
        }
    }
    (
        OwnedProcess::spawn_session(&mut command, scratch).unwrap(),
        ready,
    )
}
// tests.test_state_artifacts.CurationPlanMigration.test_plan_rewrite_interruption_replays_and_retains_the_journal
#[test]
fn baseline_state_artifacts_plan_rewrite_interruption_replays_and_retains_the_journal() {
    let fixture = PlanFixture::new();
    let (mut child, ready) = held_worker(&fixture.data);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !ready.is_file() {
        if Instant::now() >= deadline {
            let error =
                std::fs::read_to_string(fixture.data.join("worker/stderr.txt")).unwrap_or_default();
            child.finish().unwrap();
            panic!("plan-replacement barrier timed out: {error}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(read_json(&ready)["stage"], "pack_plan.json");
    assert!(fixture.journal().exists());
    assert_eq!(read_json(&fixture.journal())["stage"], "database");
    assert_eq!(
        plan_keys(&read_json(&fixture.path)),
        BTreeSet::from([NEW.into()])
    );
    // Kill the actual private process session, reap the root and prove descendant cleanup.
    child.finish().unwrap();
    assert!(!fixture.data.join("worker").exists());
    assert!(fixture.journal().exists());
    fixture.apply(NEW, 9).unwrap();
    assert_eq!(
        plan_keys(&read_json(&fixture.path)),
        BTreeSet::from([NEW.into()])
    );
    assert!(!fixture.journal().exists());
}

use crate::baseline_identity_support as identity_support;

#[path = "baseline_migration_tests/identity_integration.rs"]
mod identity_integration;
#[path = "baseline_migration_tests/identity_lazy_factory_order.rs"]
mod identity_lazy_factory_order;
#[path = "baseline_migration_tests/identity_lifecycle_cases.rs"]
mod identity_lifecycle_cases;
#[path = "baseline_migration_tests/identity_process.rs"]
mod identity_process;
#[path = "baseline_migration_tests/identity_reports.rs"]
mod identity_reports;
