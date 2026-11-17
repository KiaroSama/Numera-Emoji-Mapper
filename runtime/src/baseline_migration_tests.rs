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

#[cfg(test)]
mod identity_reports {
    use super::identity_support::*;
    use crate::{identity_cli, logging::RunLog};
    use serde_json::json;
    fn cli(f: &Fixture, media: &Fake, args: &[&str]) -> u8 {
        let log = RunLog::open(&f.root.0, "baseline-identity", Vec::new());
        let mut args = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        args.extend(["--data-dir".into(), f.data.to_str().unwrap().into()]);
        identity_cli::run_with_codec_factory(&f.config, &args, &log, |_, _| Ok(media.clone()))
            .unwrap()
    }
    // tests.test_identity_migration.NothingIsAppliedFromAnIncompletePicture.test_report_mode_says_incomplete_rather_than_clean
    #[test]
    fn identity_report_mode_says_incomplete_rather_than_clean() {
        let f = Fixture::new();
        f.add(
            "v:gone",
            &f.data.join("absent.webm"),
            "video",
            None,
            None,
            None,
        );
        assert_eq!(cli(&f, &Fake::default(), &["report"]), 4);
    }
    // tests.test_identity_migration.NothingIsAppliedFromAnIncompletePicture.test_an_empty_catalog_is_clean_not_pending
    #[test]
    fn identity_an_empty_catalog_is_clean_not_pending() {
        let f = Fixture::new();
        assert_eq!(cli(&f, &Fake::default(), &["report"]), 0);
    }
    // tests.test_identity_migration.NothingIsAppliedFromAnIncompletePicture.test_a_clean_catalog_reports_clean
    #[test]
    fn identity_a_clean_catalog_reports_clean() {
        let f = Fixture::new();
        f.add(
            "v:same",
            &f.media("clip.webm"),
            "video",
            Some(7),
            None,
            None,
        );
        assert_eq!(
            cli(
                &f,
                &f.fake(&[("clip.webm", "v:same", Some(7))]),
                &["report"]
            ),
            0
        );
    }
    // tests.test_identity_migration.NothingIsAppliedFromAnIncompletePicture.test_a_pending_catalog_reports_stale_and_writes_nothing
    #[test]
    fn identity_a_pending_catalog_reports_stale_and_writes_nothing() {
        let f = Fixture::new();
        let before = std::fs::read(f.data.join("catalog.db")).unwrap();
        f.add("v:old", &f.media("clip.webm"), "video", None, None, None);
        assert_eq!(
            cli(&f, &f.fake(&[("clip.webm", "v:new", Some(1))]), &["report"]),
            3
        );
        assert_eq!(f.col("items", "content_key"), ["v:old"]);
        assert!(!before.is_empty());
    }
    // tests.test_identity_migration.NothingIsAppliedFromAnIncompletePicture.test_only_obsolete_plan_entries_are_informational
    #[test]
    fn identity_only_obsolete_plan_entries_are_informational() {
        let f = Fixture::new();
        f.add(
            "v:same",
            &f.media("clip.webm"),
            "video",
            Some(1),
            None,
            None,
        );
        f.plan("pk", &json!({"mixed":["v:ghost"]}));
        assert_eq!(
            cli(
                &f,
                &f.fake(&[("clip.webm", "v:same", Some(1))]),
                &["report"]
            ),
            0
        );
    }
}

mod identity_process {
    use super::identity_support::*;
    use crate::{
        config::Config, identity_cli, logging::RunLog, media_adapter::MediaCodec,
        process_owner::OwnedProcess,
    };
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    #[derive(Clone)]
    struct LateDecoder {
        data: PathBuf,
        media: Fake,
    }
    impl MediaCodec for LateDecoder {
        fn with_scratch(&self, _: PathBuf) -> Self {
            self.clone()
        }
        async fn call(&self, request: Value) -> Result<Value, String> {
            if crate::migration_apply::journal(&self.data)?
                .is_some_and(|doc| doc["stage"] == "files")
            {
                Err("decoder lost".into())
            } else {
                self.media.call(request).await
            }
        }
    }
    pub(super) fn cli(f: &Fixture, media: &impl MediaCodec, args: &[&str]) -> u8 {
        let log = RunLog::open(&f.root.0, "baseline-identity-cli", Vec::new());
        let mut argv = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        argv.extend(["--data-dir".into(), f.data.to_str().unwrap().into()]);
        // Borrowing the supplied codec keeps construction lazy and exercises actual CLI dispatch.
        struct Borrowed<'a, M>(&'a M);
        impl<M: MediaCodec> MediaCodec for Borrowed<'_, M> {
            fn with_scratch(&self, _: PathBuf) -> Self {
                Self(self.0)
            }
            async fn call(&self, request: Value) -> Result<Value, String> {
                self.0.call(request).await
            }
        }
        identity_cli::run_with_codec_factory(&f.config, &argv, &log, |_, _| Ok(Borrowed(media)))
            .unwrap()
    }
    #[test]
    fn identity_worker() {
        let Some(raw) = std::env::var_os("NUMERA_BASELINE_IDENTITY_DATA") else {
            return;
        };
        let data = PathBuf::from(raw);
        let root = data.parent().unwrap();
        let action = std::env::var("NUMERA_BASELINE_IDENTITY_ACTION").unwrap();
        if action.starts_with("hold-") {
            writer(&data, &action);
            return;
        }
        let fake = Fake::load(&root.join("fixture-codec.json"));
        let config = Config::load_with_environment(
            root.into(),
            &BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]),
        )
        .unwrap();
        let log = RunLog::open(root, "baseline-worker", Vec::new());
        let args = [
            "migrate-video-keys".into(),
            "--apply".into(),
            "--data-dir".into(),
            data.to_str().unwrap().into(),
        ];
        let code = if action == "late-decode" {
            identity_cli::run_with_codec_factory(&config, &args, &log, |_, _| {
                Ok(LateDecoder {
                    data: data.clone(),
                    media: fake,
                })
            })
            .unwrap()
        } else {
            identity_cli::run_with_codec_factory(&config, &args, &log, |_, _| Ok(fake)).unwrap()
        };
        crate::atomic::write_json(&root.join("worker-result.json"), &json!({"code":code})).unwrap();
    }
    fn spawn(
        f: &Fixture,
        media: &Fake,
        action: &str,
        stage: Option<&str>,
        release: bool,
    ) -> (OwnedProcess, PathBuf) {
        media.save(&f.root.0.join("fixture-codec.json"));
        let scratch = f.data.join("identity-worker");
        std::fs::create_dir(&scratch).unwrap();
        let ready = scratch.join("ready.json");
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "migration_crash_tests::identity_process::identity_worker",
                "--nocapture",
            ])
            .env("NUMERA_BASELINE_IDENTITY_DATA", &f.data)
            .env("NUMERA_BASELINE_IDENTITY_ACTION", action)
            .env("NUMERA_MIGRATION_TEST_READY", &ready)
            .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
            .env("TEMP", &scratch)
            .env("TMP", &scratch)
            .env("TMPDIR", &scratch)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(scratch.join("stderr.txt")).unwrap());
        if let Some(stage) = stage {
            command.env("NUMERA_MIGRATION_TEST_STAGE", stage);
        } else {
            command.env_remove("NUMERA_MIGRATION_TEST_STAGE");
        }
        if release {
            command.env("NUMERA_BASELINE_RELEASE", f.root.0.join("writer-release"));
        } else {
            command.env_remove("NUMERA_BASELINE_RELEASE");
        }
        if action == "refuse-backup" {
            command.env("NUMERA_MIGRATION_TEST_ACTION", "refuse-backup");
        } else {
            command.env_remove("NUMERA_MIGRATION_TEST_ACTION");
        }
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
    pub(super) fn held(
        f: &Fixture,
        media: &Fake,
        action: &str,
        stage: &str,
        release: bool,
    ) -> OwnedProcess {
        let (mut child, ready) = spawn(f, media, action, Some(stage), release);
        let deadline = Instant::now() + Duration::from_secs(20);
        while !ready.is_file() {
            if Instant::now() >= deadline {
                let errors = std::fs::read_to_string(f.data.join("identity-worker/stderr.txt"))
                    .unwrap_or_default();
                child.finish().unwrap();
                panic!("{stage} not reached: {errors}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(read_json(&ready)["stage"], stage);
        child
    }
    pub(super) fn kill(f: &Fixture, media: &Fake, stage: &str) {
        let mut child = held(f, media, "migrate", stage, false);
        child.finish().unwrap();
        assert!(!f.data.join("identity-worker").exists());
    }
    pub(super) fn completed(f: &Fixture, media: &Fake, action: &str) -> i32 {
        let (mut child, _) = spawn(f, media, action, None, false);
        let status = runtime()
            .block_on(child.wait(Duration::from_secs(20), Duration::from_secs(20)))
            .unwrap();
        let error = std::fs::read_to_string(f.data.join("identity-worker/stderr.txt")).unwrap();
        child.finish().unwrap();
        assert!(status.success() || action == "exit73", "{error}");
        status.code().unwrap()
    }
    pub(super) fn result(f: &Fixture) -> u64 {
        read_json(&f.root.0.join("worker-result.json"))["code"]
            .as_u64()
            .unwrap()
    }
    fn writer(data: &Path, action: &str) {
        use crate::{catalog::Catalog, media_native_adapter::NativeMediaAdapter};
        use std::sync::{Arc, Mutex};
        let root = data.parent().unwrap();
        let config = Config::load_with_environment(
            root.into(),
            &BTreeMap::from([
                ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
                ("BRAND_LOGO_BOTS".into(), "".into()),
                ("GENERAL_BOT_TOKEN".into(), "test-only-token".into()),
                ("COLLECTION_PACK_BASE".into(), "newfamily".into()),
                ("EMOJI_ARCHIVE_DIR".into(), "archive".into()),
            ]),
        )
        .unwrap();
        let log = Arc::new(RunLog::open(root, "baseline-held-writer", Vec::new()));
        match action {
            "hold-catalog" => {
                let _cat = Catalog::open(&data.join("catalog.db"), UTC, root).unwrap();
                super::barrier("writer-catalog");
            }
            "hold-publisher" => {
                crate::publish_cli::run(
                    &config,
                    &[
                        "--base",
                        "newfamily",
                        "--title",
                        "Fixture",
                        "--dry-run",
                        "--no-brand-logo",
                        "--data-dir",
                        data.to_str().unwrap(),
                    ]
                    .map(str::to_owned),
                    &log,
                )
                .unwrap();
            }
            "hold-archive" => {
                let tg = crate::telegram::Telegram::new(
                    "test-only-token".into(),
                    "http://127.0.0.1:9".into(),
                )
                .unwrap();
                crate::archive_sync::run(&config, &tg, &log).unwrap();
            }
            "hold-ingest" => {
                crate::local_cli::run_with_codec(
                    &config,
                    &[
                        data.join("incoming.png").to_str().unwrap().to_owned(),
                        "--data-dir".into(),
                        data.to_str().unwrap().into(),
                    ],
                    &log,
                    |scratch| {
                        Ok(NativeMediaAdapter {
                            executable: "must-not-decode-held-writer".into(),
                            root: root.into(),
                            scratch,
                        })
                    },
                )
                .unwrap();
            }
            "hold-panel" => {
                let panel = crate::panel_http::Panel {
                    root: root.into(),
                    db: data.join("catalog.db"),
                    token: "writer-fixture".into(),
                    options: json!({"show_published":false,"keep_sets":{},"branded":false}),
                    branding: None,
                    detected_bot: Mutex::new(String::new()),
                    view: Mutex::new(json!({"view":[],"by_key":{},"hidden":0})),
                    session: json!({}),
                    last_refreshed: std::sync::Mutex::new(None),
                    utc: || UTC.into(),
                    log: log.clone(),
                    events: Mutex::new(crate::panel_events::Events::default()),
                    previews: crate::panel_preview::Previews::new(
                        NativeMediaAdapter {
                            executable: "must-not-decode-held-panel".into(),
                            root: root.into(),
                            scratch: data.join("panel-codec"),
                        },
                        data,
                        1,
                    ),
                };
                runtime().block_on(async {
                    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                    let address = listener.local_addr().unwrap();
                    let server = tokio::spawn(async move {
                        axum::serve(listener, crate::panel_http::router(Arc::new(panel))).await
                    });
                    let response = reqwest::Client::builder()
                        .no_proxy()
                        .timeout(Duration::from_secs(25))
                        .build()
                        .unwrap()
                        .post(format!("http://{address}/api/save"))
                        .header("x-panel-token", "writer-fixture")
                        .json(&json!({"excluded":[],"known":[]}))
                        .send()
                        .await;
                    server.abort();
                    panic!("held panel returned: {response:?}");
                });
            }
            _ => panic!("unknown held writer {action}"),
        }
        panic!("writer returned before barrier");
    }
}

mod identity_lifecycle_cases {
    use super::{identity_process as process, identity_support::*};
    use crate::{
        catalog::Catalog, locks, migration_apply, migration_database, ownership, reconcile,
        sqlite_snapshot,
    };
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::time::Duration;
    fn pending_identity() -> (Fixture, Fake) {
        let f = Fixture::new();
        f.add(
            "v:old",
            &f.media("clip.webm"),
            "video",
            None,
            Some("cid-1"),
            Some("F-1"),
        );
        f.plan("pk", &json!({"mixed":["v:old"]}));
        let fake = f.fake(&[("clip.webm", "v:new", Some(1))]);
        (f, fake)
    }
    // tests.test_identity_migration.MigratedPathsStayDataRelative.test_rows_written_absolute_by_the_old_code_still_verify
    #[test]
    fn identity_rows_written_absolute_by_the_old_code_still_verify() {
        let (f, fake) = relative_rows();
        assert_eq!(process::completed(&f, &fake, "absolute-paths"), 0);
        assert_eq!(process::result(&f), 0);
        assert!(Path::new(&paths(&f)["v:keep"]).is_absolute());
    }
    // tests.test_identity_migration.TheBackupIsWalSafe.test_an_unverifiable_backup_stops_before_any_change
    #[test]
    fn identity_an_unverifiable_backup_stops_before_any_change() {
        let f = Fixture::new();
        f.add("v:old", &f.media("clip.webm"), "video", None, None, None);
        let fake = f.fake(&[("clip.webm", "v:new", Some(1))]);
        assert_eq!(process::completed(&f, &fake, "refuse-backup"), 0);
        assert_eq!(process::result(&f), 4);
        assert_eq!(f.col("items", "content_key"), ["v:old"]);
    }
    // tests.test_identity_migration.AnInterruptedMigrationIsResumable.test_a_crash_after_the_backup_resumes
    #[test]
    fn identity_a_crash_after_the_backup_resumes() {
        let (f, fake) = pending_identity();
        process::kill(&f, &fake, "backup");
        assert!(migration_apply::journal(&f.data).unwrap().is_some());
        assert_eq!(
            process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
            0
        );
        assert_eq!(f.col("items", "content_key"), ["v:new"]);
        assert_eq!(
            read_json(&f.data.join("publish_plan_pk.json")),
            json!({"mixed":["v:new"]})
        );
        assert!(migration_apply::journal(&f.data).unwrap().is_none());
    }
    // tests.test_identity_migration.AnInterruptedMigrationIsResumable.test_a_crash_after_the_database_stage_resumes
    #[test]
    fn identity_a_crash_after_the_database_stage_resumes() {
        let (f, fake) = pending_identity();
        process::kill(&f, &fake, "database");
        assert_eq!(f.col("items", "content_key"), ["v:new"]);
        assert_eq!(
            read_json(&f.data.join("publish_plan_pk.json")),
            json!({"mixed":["v:old"]})
        );
        assert_eq!(
            process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
            0
        );
        assert_eq!(f.col("items", "content_key"), ["v:new"]);
        assert_eq!(
            read_json(&f.data.join("publish_plan_pk.json")),
            json!({"mixed":["v:new"]})
        );
        assert!(migration_apply::journal(&f.data).unwrap().is_none());
    }
    // tests.test_identity_migration.AnInterruptedMigrationIsResumable.test_a_crash_after_the_state_stage_resumes
    #[test]
    fn identity_a_crash_after_the_state_stage_resumes() {
        let (f, fake) = pending_identity();
        process::kill(&f, &fake, "state");
        assert_eq!(
            process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
            0
        );
        assert_eq!(f.col("items", "content_key"), ["v:new"]);
        assert_eq!(
            read_json(&f.data.join("publish_plan_pk.json")),
            json!({"mixed":["v:new"]})
        );
        assert!(migration_apply::journal(&f.data).unwrap().is_none());
    }
    // tests.test_identity_migration.AnInterruptedMigrationIsResumable.test_report_names_an_interrupted_migration
    #[test]
    fn identity_report_names_an_interrupted_migration() {
        let (f, fake) = pending_identity();
        process::kill(&f, &fake, "database");
        assert_eq!(process::cli(&f, &fake, &["report"]), 3);
        assert!(f.journal().is_file());
    }
    // tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_actual_publisher_lock_prevents_migration
    #[test]
    fn lifecycle_actual_publisher_lock_prevents_migration() {
        let f = Fixture::new();
        let (src, dest, fake) = f.pending();
        let _held = locks::Lease::acquire(&f.data.join("publish_audit.lock"), UTC).unwrap();
        assert!(f.apply(&fake).is_err());
        assert!(src.is_file());
        assert!(!dest.exists());
        assert_eq!(f.col("items", "content_key"), ["v:aaaaaaaaaaaaaaaa"]);
    }
    // tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_unrelated_rename_destination_is_never_overwritten
    #[test]
    fn lifecycle_unrelated_rename_destination_is_never_overwritten() {
        let f = Fixture::new();
        let (src, dest, fake) = f.pending();
        std::fs::write(&dest, b"unrelated sentinel").unwrap();
        assert_eq!(
            process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
            4
        );
        assert_eq!(std::fs::read(&dest).unwrap(), b"unrelated sentinel");
        assert!(src.is_file());
        assert_eq!(f.col("items", "content_key"), ["v:aaaaaaaaaaaaaaaa"]);
    }
    // tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_required_stale_manifest_cannot_be_called_plan_noise
    #[test]
    fn lifecycle_required_stale_manifest_cannot_be_called_plan_noise() {
        let f = Fixture::new();
        f.add(
            "v:correct",
            &f.media("clip.webm"),
            "video",
            Some(7),
            None,
            None,
        );
        let state = f.state("audit", &json!({"sets":[{"keys":["v:lost"]}]}));
        let fake = f.fake(&[("clip.webm", "v:correct", Some(7))]);
        assert_eq!(
            process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
            4
        );
        assert_ne!(process::cli(&f, &fake, &["report"]), 0);
        assert_eq!(read_json(&state)["sets"][0]["keys"], json!(["v:lost"]));
    }
    // tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_late_missing_media_retains_journal_and_refuses_success
    #[test]
    fn lifecycle_late_missing_media_retains_journal_and_refuses_success() {
        let f = Fixture::new();
        let (src, _, fake) = f.pending();
        assert_eq!(process::completed(&f, &fake, "late-missing"), 0);
        assert_eq!(process::result(&f), 4);
        assert!(f.journal().is_file());
        assert!(!src.exists());
    }
    // tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_completed_migration_restores_files_state_and_identifiers
    #[test]
    fn lifecycle_completed_migration_restores_files_state_and_identifiers() {
        super::identity_lazy_factory_order::help_does_not_construct_the_codec();
        super::identity_lazy_factory_order::missing_catalog_does_not_construct_the_codec();
        super::identity_lazy_factory_order::missing_restore_bundle_does_not_construct_the_codec();
        super::identity_lazy_factory_order::successful_restore_does_not_construct_the_codec();
        let f = Fixture::new();
        let (src, dest, fake) = f.pending();
        let before = f.signature();
        let original = std::fs::read(&src).unwrap();
        assert_eq!(
            process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
            0
        );
        let manifest = f.manifest();
        assert_eq!(
            process::cli(
                &f,
                &fake,
                &["restore", "--bundle", manifest.to_str().unwrap(), "--apply"]
            ),
            0
        );
        assert_eq!(f.signature(), before);
        assert_eq!(std::fs::read(&src).unwrap(), original);
        assert!(!dest.exists());
        assert_eq!(f.col("publications", "custom_emoji_id"), ["CID"]);
        assert_eq!(f.col("seen_files", "file_unique_id"), ["FUID"]);
        assert_eq!(
            read_json(&f.data.join("publish_audit.json"))["sets"][0]["keys"],
            json!(["v:aaaaaaaaaaaaaaaa"])
        );
        assert_eq!(
            process::cli(
                &f,
                &fake,
                &["restore", "--bundle", manifest.to_str().unwrap(), "--apply"]
            ),
            0
        );
    }
    // tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_restore_preserves_an_identical_preexisting_destination
    #[test]
    fn lifecycle_restore_preserves_an_identical_preexisting_destination() {
        let f = Fixture::new();
        let (src, dest, fake) = f.pending();
        std::fs::copy(&src, &dest).unwrap();
        assert_eq!(
            process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
            0
        );
        let manifest = f.manifest();
        assert_eq!(
            process::cli(
                &f,
                &fake,
                &["restore", "--bundle", manifest.to_str().unwrap(), "--apply"]
            ),
            0
        );
        assert_eq!(std::fs::read(&src).unwrap(), std::fs::read(&dest).unwrap());
    }
    // tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_restore_refuses_later_catalog_or_file_changes
    #[test]
    fn lifecycle_restore_refuses_later_catalog_or_file_changes() {
        let f = Fixture::new();
        let (src, dest, fake) = f.pending();
        assert_eq!(
            process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
            0
        );
        let manifest = f.manifest();
        f.db()
            .execute("UPDATE publications SET custom_emoji_id='LATER'", [])
            .unwrap();
        assert_eq!(
            process::cli(
                &f,
                &fake,
                &["restore", "--bundle", manifest.to_str().unwrap(), "--apply"]
            ),
            4
        );
        assert_eq!(f.col("publications", "custom_emoji_id"), ["LATER"]);
        assert!(dest.is_file());
        assert!(!src.exists());
    }
    // tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_journal_bound_to_another_directory_refuses_without_mutation
    #[test]
    fn lifecycle_journal_bound_to_another_directory_refuses_without_mutation() {
        let f = Fixture::new();
        let (src, dest, fake) = f.pending();
        process::kill(&f, &fake, "backup");
        let mut doc = read_json(&f.journal());
        doc["data_dir"] = json!(f.data.join("elsewhere"));
        crate::atomic::write_json(&f.journal(), &doc).unwrap();
        assert_eq!(
            process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
            4
        );
        assert!(src.is_file());
        assert!(!dest.exists());
    }
    // tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_restore_after_a_move_before_database_path_commit
    #[test]
    fn lifecycle_restore_after_a_move_before_database_path_commit() {
        let f = Fixture::new();
        let (src, dest, fake) = f.pending();
        process::kill(&f, &fake, "move-before-path");
        assert!(!src.exists());
        assert_eq!(f.col("items", "file_path"), [src.to_str().unwrap()]);
        let manifest = f.manifest();
        assert_eq!(
            process::cli(
                &f,
                &fake,
                &["restore", "--bundle", manifest.to_str().unwrap(), "--apply"]
            ),
            0
        );
        assert!(src.is_file());
        assert!(!dest.exists());
        assert_eq!(f.col("items", "content_key"), ["v:aaaaaaaaaaaaaaaa"]);
    }
    // tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_existing_link_after_interruption_is_replayed_without_losing_bytes
    #[test]
    fn lifecycle_existing_link_after_interruption_is_replayed_without_losing_bytes() {
        let f = Fixture::new();
        let (src, dest, fake) = f.pending();
        let original = std::fs::read(&src).unwrap();
        process::kill(&f, &fake, "linked-before-unlink");
        assert_eq!(std::fs::read(&src).unwrap(), std::fs::read(&dest).unwrap());
        assert_eq!(
            process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
            0
        );
        assert!(!src.exists());
        let paths = f.col("items", "file_path");
        assert_eq!(
            paths,
            [format!("./{}", dest.file_name().unwrap().to_str().unwrap())]
        );
        let stored = crate::paths::resolve(&f.data, &paths[0], &f.root.0);
        assert_eq!(stored.canonicalize().unwrap(), dest.canonicalize().unwrap());
        assert_eq!(std::fs::read(&stored).unwrap(), original);
    }
    // tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_partial_legacy_migration_requires_full_backup_coverage
    #[test]
    fn lifecycle_partial_legacy_migration_requires_full_backup_coverage() {
        let f = Fixture::new();
        let (src, dest, fake) = f.pending();
        let original = f.data.join("catalog.before-original.db");
        sqlite_snapshot::backup(&f.db(), &original).unwrap();
        migration_database::rewrite(
            &mut f.db(),
            &BTreeMap::from([("v:aaaaaaaaaaaaaaaa".into(), "v:bbbbbbbbbbbbbbbb".into())]),
            &BTreeMap::from([("v:bbbbbbbbbbbbbbbb".into(), Some(7))]),
        )
        .unwrap();
        let insufficient = f.data.join("catalog.before-insufficient.db");
        sqlite_snapshot::backup(&f.db(), &insufficient).unwrap();
        assert_eq!(
            process::cli(
                &f,
                &fake,
                &[
                    "migrate-video-keys",
                    "--apply",
                    "--from-backup",
                    insufficient.to_str().unwrap()
                ]
            ),
            4
        );
        assert!(src.is_file());
        assert_eq!(
            process::cli(
                &f,
                &fake,
                &[
                    "migrate-video-keys",
                    "--apply",
                    "--from-backup",
                    original.to_str().unwrap()
                ]
            ),
            0
        );
        assert_eq!(process::cli(&f, &fake, &["report"]), 0);
        assert!(dest.is_file());
        assert!(!src.exists());
    }
    // tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_late_decode_failure_cannot_retire_the_journal
    #[test]
    fn lifecycle_late_decode_failure_cannot_retire_the_journal() {
        let f = Fixture::new();
        let (_, _, fake) = f.pending();
        assert_eq!(process::completed(&f, &fake, "late-decode"), 0);
        assert_eq!(process::result(&f), 4);
        assert!(f.journal().is_file());
    }
    // tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_orphaned_identifiers_prevent_a_clean_report_or_noop
    #[test]
    fn lifecycle_orphaned_identifiers_prevent_a_clean_report_or_noop() {
        let f = Fixture::new();
        f.add(
            "v:correct",
            &f.media("clip.webm"),
            "video",
            Some(7),
            None,
            None,
        );
        f.db()
            .execute("INSERT INTO seen_files VALUES('FOREIGN','v:absent')", [])
            .unwrap();
        let fake = f.fake(&[("clip.webm", "v:correct", Some(7))]);
        assert_eq!(process::cli(&f, &fake, &["report"]), 4);
        assert_eq!(
            process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
            4
        );
    }
    // tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_cli_recovers_after_each_state_replacement_and_journal_retirement
    #[test]
    fn lifecycle_cli_recovers_after_each_state_replacement_and_journal_retirement() {
        for point in [
            "database-before",
            "database-after",
            "publish_audit.json",
            "publish_plan_audit.json",
            "verified",
        ] {
            let f = Fixture::new();
            let (src, dest, fake) = f.pending();
            process::kill(&f, &fake, point);
            assert_eq!(
                process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
                0,
                "{point}"
            );
            assert_eq!(process::cli(&f, &fake, &["report"]), 0, "{point}");
            assert!(dest.is_file(), "{point}");
            assert!(!src.exists(), "{point}");
        }
    }
    // tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_a_second_catalog_connection_cannot_write_during_migration
    #[test]
    fn lifecycle_a_second_catalog_connection_cannot_write_during_migration() {
        let f = Fixture::new();
        let (_, _, fake) = f.pending();
        let mut child = process::held(&f, &fake, "migrate", "state", false);
        let error = Catalog::open(&f.data.join("catalog.db"), UTC, &f.root.0)
            .err()
            .expect("second writer entered migration");
        assert!(error.contains("catalog ownership refused"), "{error}");
        child.finish().unwrap();
        assert_eq!(
            process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
            0
        );
        assert_eq!(f.col("publications", "custom_emoji_id"), ["CID"]);
    }
    // tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_native_death_after_second_move_replays_via_cli
    #[test]
    fn lifecycle_native_death_after_second_move_replays_via_cli() {
        let f = Fixture::new();
        let mut values = Vec::new();
        let mut names = Vec::new();
        let mut keys = Vec::new();
        for i in 0..3u8 {
            let key = format!("v:{}", format!("{i:02x}").repeat(16));
            let name = format!("{i:03}_video_{}.webm", &key[2..14]);
            let path = f.data.join(&name);
            let mut bytes = b"\x1a\x45\xdf\xa3fixture".to_vec();
            bytes.push(i);
            std::fs::write(&path, bytes).unwrap();
            f.add(
                &key,
                &path,
                "video",
                None,
                Some(&format!("CID{i}")),
                Some(&format!("FUID{i}")),
            );
            names.push(name);
            keys.push(format!("v:{}", format!("{:02x}", i + 16).repeat(16)));
        }
        for i in 0..3 {
            values.push((names[i].as_str(), keys[i].as_str(), Some(i as u64)));
        }
        let fake = f.fake(&values);
        assert_eq!(process::completed(&f, &fake, "exit73"), 73);
        assert!(f.journal().is_file());
        assert_eq!(process::completed(&f, &fake, "migrate"), 0);
        assert_eq!(process::result(&f), 0);
        assert!(!f.journal().exists());
        assert_eq!(f.col("items", "content_key"), keys);
        for path in f.col("items", "file_path") {
            assert!(crate::paths::resolve(&f.data, &path, &f.root.0).is_file());
        }
    }
    fn writer_fixture() -> Fixture {
        let f = Fixture::real();
        image::RgbaImage::from_pixel(100, 100, image::Rgba([255, 0, 0, 255]))
            .save(f.data.join("incoming.png"))
            .unwrap();
        f
    }
    fn release_writer(f: &Fixture, mut child: crate::process_owner::OwnedProcess) {
        std::fs::write(f.root.0.join("writer-release"), b"release").unwrap();
        let status = runtime()
            .block_on(child.wait(Duration::from_secs(10), Duration::from_secs(10)))
            .unwrap();
        child.finish().unwrap();
        assert_eq!(status.code(), Some(0));
    }
    // tests.test_migration_lifecycle.ActualWriterOwnership.test_first_family_publisher_and_catalog_exclude_migration_in_real_children
    #[test]
    fn lifecycle_first_family_publisher_and_catalog_exclude_migration_in_real_children() {
        for (kind, stage) in [
            ("hold-publisher", "writer-publisher"),
            ("hold-catalog", "writer-catalog"),
            ("hold-archive", "writer-archive"),
            ("hold-panel", "writer-panel"),
            ("hold-ingest", "writer-ingest"),
        ] {
            let f = writer_fixture();
            let fake = Fake::default();
            let child = process::held(&f, &fake, kind, stage, true);
            assert!(
                f.apply(&fake).unwrap_err().contains("ownership refused"),
                "{kind}"
            );
            assert!(!f.journal().exists(), "{kind}");
            release_writer(&f, child);
        }
    }
    // tests.test_migration_lifecycle.ActualWriterOwnership.test_pending_journal_blocks_ingest_archive_publisher_and_panel_save
    #[test]
    fn lifecycle_pending_journal_blocks_ingest_archive_publisher_and_panel_save() {
        use crate::{logging::RunLog, media_native_adapter::NativeMediaAdapter};
        use std::sync::{Arc, Mutex};
        let f = writer_fixture();
        std::fs::write(f.journal(), "{}").unwrap();
        let log = Arc::new(RunLog::open(&f.root.0, "pending-writer", Vec::new()));
        let image = f.data.join("incoming.png");
        let error = crate::local_cli::run_with_codec(
            &f.config,
            &[
                image.to_str().unwrap().into(),
                "--data-dir".into(),
                f.data.to_str().unwrap().into(),
            ],
            &log,
            |_| Ok(Fake::default()),
        )
        .unwrap_err();
        assert!(error.contains("interrupted identity migration"), "{error}");
        assert_ne!(
            crate::publish_cli::run(
                &f.config,
                &[
                    "--base",
                    "newfamily",
                    "--title",
                    "Fixture",
                    "--dry-run",
                    "--no-brand-logo",
                    "--data-dir",
                    f.data.to_str().unwrap()
                ]
                .map(str::to_owned),
                &log
            )
            .unwrap(),
            0
        );
        let tg =
            crate::telegram::Telegram::new("test-only-token".into(), "http://127.0.0.1:9".into())
                .unwrap();
        assert!(
            crate::archive_sync::run(&f.config, &tg, &log)
                .unwrap_err()
                .contains("interrupted identity migration")
        );
        let panel = crate::panel_http::Panel {
            root: f.root.0.clone(),
            db: f.data.join("catalog.db"),
            token: "fixture-token".into(),
            options: json!({"show_published":false,"keep_sets":{},"branded":false}),
            branding: None,
            detected_bot: Mutex::new(String::new()),
            view: Mutex::new(json!({"view":[],"by_key":{},"hidden":0})),
            session: json!({}),
            last_refreshed: std::sync::Mutex::new(None),
            utc: || UTC.into(),
            log,
            events: Mutex::new(crate::panel_events::Events::default()),
            previews: crate::panel_preview::Previews::new(
                NativeMediaAdapter {
                    executable: "must-not-decode-pending-journal".into(),
                    root: f.root.0.clone(),
                    scratch: f.data.join("panel-codec"),
                },
                &f.data,
                1,
            ),
        };
        runtime().block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                axum::serve(listener, crate::panel_http::router(Arc::new(panel))).await
            });
            let response = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap()
                .post(format!("http://{address}/api/save"))
                .header("x-panel-token", "fixture-token")
                .json(&json!({"excluded":[],"known":[]}))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), 503);
            drop(response);
            server.abort();
            let _ = server.await;
        });
        assert!(image.is_file());
        assert_eq!(
            f.db()
                .query_row("SELECT count(*) FROM items", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    // tests.test_migration_lifecycle.ActualWriterOwnership.test_canonical_alias_and_second_migration_use_the_same_native_lock
    #[test]
    fn lifecycle_canonical_alias_and_second_migration_use_the_same_native_lock() {
        let f = Fixture::real();
        std::fs::create_dir(f.data.join("nested")).unwrap();
        let alias = f.data.join("nested/..");
        std::thread::scope(|scope| {
            let (ready, started) = std::sync::mpsc::channel();
            let (release, done) = std::sync::mpsc::channel();
            let data = &f.data;
            let thread = scope.spawn(move || {
                let _owner =
                    ownership::Ownership::acquire(data, ownership::Mode::Maintenance, UTC).unwrap();
                ready.send(()).unwrap();
                done.recv_timeout(Duration::from_secs(10)).unwrap();
            });
            started.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(
                migration_apply::apply(&f.config, &alias, &Fake::default(), &runtime(), None)
                    .unwrap_err()
                    .contains("ownership refused")
            );
            assert!(
                Catalog::open(&f.data.join("catalog.db"), UTC, &f.root.0)
                    .err()
                    .unwrap()
                    .contains("ownership refused")
            );
            release.send(()).unwrap();
            thread.join().unwrap();
        });
    }
    fn unchanged_reconcile(cat: &mut Catalog, rec: &Value) -> Value {
        let mut publications = serde_json::Map::new();
        let mut seen = serde_json::Map::new();
        for row in cat.all(None).unwrap() {
            let key = row["content_key"].as_str().unwrap();
            publications.insert(
                key.into(),
                cat.dispatch(json!({"operation":"custom_id","base":"audit","content_key":key}))
                    .unwrap(),
            );
        }
        let stickers = (0..3)
            .rev()
            .map(|i| {
                let fuid = format!("FUID{i}");
                seen.insert(
                    fuid.clone(),
                    cat.dispatch(json!({"operation":"seen","file_unique_id":fuid}))
                        .unwrap(),
                );
                json!({"file_unique_id":format!("FUID{i}"),"custom_emoji_id":format!("CID{i}")})
            })
            .collect::<Vec<_>>();
        reconcile::reconcile(&json!({"set":rec,"stickers":stickers,"resolved":[null,null,null],"publications":publications,"seen":seen})).unwrap()
    }
    // tests.test_migration_lifecycle.ActualWriterOwnership.test_real_catalog_order_and_unchanged_pack_resume_survive_migration_and_restore
    #[test]
    fn lifecycle_real_catalog_order_and_unchanged_pack_resume_survive_migration_and_restore() {
        let f = Fixture::real();
        let mut cat = Catalog::open(&f.data.join("catalog.db"), UTC, &f.root.0).unwrap();
        let mut keys = Vec::new();
        let mut mapping = Vec::new();
        let mut names = Vec::new();
        let mut new = Vec::new();
        for i in 0..3u8 {
            let key = format!("v:{}", format!("{i:02x}").repeat(16));
            let name = format!("{i:03}_video_{}.webm", &key[2..14]);
            let path = f.data.join(&name);
            let mut bytes = b"\x1a\x45\xdf\xa3fixture".to_vec();
            bytes.push(i);
            std::fs::write(&path, bytes).unwrap();
            cat.insert(
                &json!({"content_key":key,"fmt":"video","file_path":path,"phash":i}),
                UTC,
            )
            .unwrap();
            cat.uploaded(&json!({"content_key":key,"custom_emoji_id":format!("CID{i}"),"base":"audit","set_name":"audit1"}),UTC).unwrap();
            cat.record_seen(&format!("FUID{i}"), &key).unwrap();
            keys.push(key);
            names.push(name);
            new.push(format!("v:{}", format!("{:02x}", i + 16).repeat(16)));
        }
        let reversed = keys.iter().rev().cloned().collect::<Vec<_>>();
        cat.order(&reversed).unwrap();
        cat.inclusion(&[keys[1].clone()]).unwrap();
        drop(cat);
        let rec = json!({"name":"audit1","index":1,"fmt":"video","live":3,"keys":reversed});
        let state = f.state("audit", &json!({"base":"audit","sets":[rec]}));
        f.plan("audit", &json!({"video":reversed}));
        for i in 0..3 {
            mapping.push((names[i].as_str(), new[i].as_str(), Some(i as u64)));
        }
        let fake = f.fake(&mapping);
        assert_eq!(
            process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
            0
        );
        let mut cat = Catalog::open(&f.data.join("catalog.db"), UTC, &f.root.0).unwrap();
        let migrated = read_json(&state)["sets"][0].clone();
        assert_eq!(unchanged_reconcile(&mut cat, &migrated)["set"]["live"], 3);
        assert_eq!(
            cat.all(None)
                .unwrap()
                .iter()
                .map(|row| row["included"].as_bool().unwrap())
                .collect::<Vec<_>>(),
            [true, false, true]
        );
        drop(cat);
        let manifest = f.manifest();
        assert_eq!(
            process::cli(
                &f,
                &fake,
                &["restore", "--apply", "--bundle", manifest.to_str().unwrap()]
            ),
            0
        );
        let mut cat = Catalog::open(&f.data.join("catalog.db"), UTC, &f.root.0).unwrap();
        assert_eq!(
            cat.all(None)
                .unwrap()
                .iter()
                .map(|row| row["content_key"].as_str().unwrap())
                .collect::<Vec<_>>(),
            reversed
        );
        assert_eq!(
            cat.all(None)
                .unwrap()
                .iter()
                .map(|row| row["included"].as_bool().unwrap())
                .collect::<Vec<_>>(),
            [true, false, true]
        );
        assert_eq!(unchanged_reconcile(&mut cat, &rec)["set"]["live"], 3);
    }
}

mod identity_lazy_factory_order {
    use super::identity_support::*;
    use crate::{identity_cli, logging::RunLog};
    use std::cell::Cell;
    pub(super) fn help_does_not_construct_the_codec() {
        let f = Fixture::new();
        let log = RunLog::open(&f.root.0, "lazy-help", Vec::new());
        let calls = Cell::new(0);
        assert_eq!(
            identity_cli::run_with_codec_factory(&f.config, &["--help".into()], &log, |_, _| {
                calls.set(calls.get() + 1);
                Ok(Fake::default())
            })
            .unwrap(),
            0
        );
        assert_eq!(calls.get(), 0);
    }
    pub(super) fn missing_catalog_does_not_construct_the_codec() {
        let f = Fixture::new();
        let log = RunLog::open(&f.root.0, "lazy-missing", Vec::new());
        let calls = Cell::new(0);
        assert_eq!(
            identity_cli::run_with_codec_factory(
                &f.config,
                &["report".into(), "--data-dir".into(), "missing".into()],
                &log,
                |_, _| {
                    calls.set(calls.get() + 1);
                    Ok(Fake::default())
                }
            )
            .unwrap(),
            2
        );
        assert_eq!(calls.get(), 0);
    }
    pub(super) fn missing_restore_bundle_does_not_construct_the_codec() {
        let f = Fixture::new();
        let log = RunLog::open(&f.root.0, "lazy-restore", Vec::new());
        let calls = Cell::new(0);
        assert_eq!(
            identity_cli::run_with_codec_factory(
                &f.config,
                &[
                    "restore".into(),
                    "--data-dir".into(),
                    f.data.to_str().unwrap().into()
                ],
                &log,
                |_, _| {
                    calls.set(calls.get() + 1);
                    Ok(Fake::default())
                }
            )
            .unwrap(),
            2
        );
        assert_eq!(calls.get(), 0);
    }
    pub(super) fn successful_restore_does_not_construct_the_codec() {
        let f = Fixture::new();
        let (_, _, fake) = f.pending();
        f.apply(&fake).unwrap();
        let manifest = f.manifest();
        let log = RunLog::open(&f.root.0, "lazy-real-restore", Vec::new());
        let calls = Cell::new(0);
        assert_eq!(
            identity_cli::run_with_codec_factory(
                &f.config,
                &[
                    "restore".into(),
                    "--apply".into(),
                    "--data-dir".into(),
                    f.data.to_str().unwrap().into(),
                    "--bundle".into(),
                    manifest.to_str().unwrap().into()
                ],
                &log,
                |_, _| {
                    calls.set(calls.get() + 1);
                    Ok(Fake::default())
                }
            )
            .unwrap(),
            0
        );
        assert_eq!(calls.get(), 0);
    }
}
