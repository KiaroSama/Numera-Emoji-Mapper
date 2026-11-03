//! Test-only stage barriers are absent from every production executable and extension.
use crate::{
    codec_video::VideoDecoder, config::Config, media_adapter::MediaCodec, migration_apply,
    migration_bundle, migration_restore, process_owner::OwnedProcess,
};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
#[path = "migration_move_cases.rs"]
mod moves;
#[path = "migration_writer_cases.rs"]
mod writers;

pub(crate) fn barrier(stage: &str) {
    if std::env::var("NUMERA_MIGRATION_TEST_STAGE").as_deref() == Ok(stage) {
        let ready = std::env::var_os("NUMERA_MIGRATION_TEST_READY").unwrap();
        crate::atomic::write_json(Path::new(&ready), &json!({"stage":stage})).unwrap();
        // Only the test executable reaches this path. The parent kills its owned
        // process tree after observing durable state; the deadline prevents a lost parent hang.
        std::thread::park_timeout(Duration::from_secs(30));
        panic!("migration test barrier was not terminated by its owner");
    }
}
struct Workspace(PathBuf);
impl Drop for Workspace {
    fn drop(&mut self) {
        if std::thread::panicking() || crate::process_owner::cleanup_pending(&self.0) {
            eprintln!("ERROR: migration fixture retained at {}", self.0.display());
            return;
        }
        std::fs::remove_dir_all(&self.0).expect("migration fixture cleanup failed");
    }
}
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}
struct Codec {
    decoder: VideoDecoder,
    scratch: PathBuf,
    fail_verification: bool,
}
impl MediaCodec for Codec {
    fn with_scratch(&self, scratch: PathBuf) -> Self {
        Self {
            decoder: VideoDecoder::new(&scratch).unwrap(),
            scratch,
            fail_verification: self.fail_verification,
        }
    }
    async fn call(&self, request: serde_json::Value) -> Result<serde_json::Value, String> {
        if request["operation"] != "fingerprint" || request["format"] != "video" {
            return Err("unexpected native migration decoder request".into());
        }
        if self.fail_verification
            && migration_apply::journal(self.scratch.parent().ok_or("missing codec parent")?)?
                .is_some_and(|journal| journal["stage"] == "files")
        {
            return Err("controlled decoder loss after files".into());
        }
        self.decoder
            .fingerprint(Path::new(
                request["source"].as_str().ok_or("missing decoder source")?,
            ))
            .await
    }
}
#[test]
fn migration_worker() {
    let Some(data) = std::env::var_os("NUMERA_MIGRATION_TEST_DATA") else {
        return;
    };
    let data = PathBuf::from(data);
    let action = std::env::var("NUMERA_MIGRATION_TEST_ACTION").unwrap();
    if action.starts_with("hold-") {
        writers::run(&data, &action);
        return;
    }
    let config = Config::load(data.parent().unwrap().to_path_buf()).unwrap();
    let scratch = data.join("codec");
    std::fs::create_dir_all(&scratch).unwrap();
    let media = Codec {
        decoder: VideoDecoder::new(&scratch).unwrap(),
        scratch,
        fail_verification: action == "refuse-decoder",
    };
    let runtime = runtime();
    match action.as_str() {
        "apply" => {
            migration_apply::apply(&config, &data, &media, &runtime, None).unwrap();
        }
        "refuse-backup" => {
            let before = retained(&data);
            let error = migration_apply::apply(&config, &data, &media, &runtime, None).unwrap_err();
            assert_eq!(error, "controlled backup verification failure");
            assert_eq!(retained(&data), before);
            assert!(!data.join("identity-migration.journal.json").exists());
        }
        "refuse-binding" | "refuse-missing" | "refuse-decoder" => {
            let error = migration_apply::apply(&config, &data, &media, &runtime, None).unwrap_err();
            assert!(
                error.contains(match action.as_str() {
                    "refuse-binding" => "catalog binding does not match",
                    "refuse-missing" => "migration media is missing",
                    _ => "migration verification incomplete",
                }),
                "{action}: {error}"
            );
            assert!(data.join("identity-migration.journal.json").is_file());
        }
        "restore" => {
            migration_restore::restore(&data, data.parent().unwrap(), &manifest(&data), true)
                .unwrap();
        }
        other => panic!("unknown migration worker action: {other}"),
    }
    assert!(!crate::process_owner::cleanup_pending(&media.scratch));
    let captures = media.scratch.join("logs/tmp");
    if captures.exists() {
        assert_eq!(
            std::fs::read_dir(&captures).unwrap().count(),
            0,
            "nested decoder capture must settle before fixture cleanup"
        );
    }
}
fn manifest(data: &Path) -> PathBuf {
    let paths = std::fs::read_dir(data)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".rollback.json"))
        .collect::<Vec<_>>();
    assert_eq!(paths.len(), 1, "one original recovery bundle must survive");
    paths[0].clone()
}
fn worker(data: &Path, action: &str, stage: Option<&str>) -> (OwnedProcess, PathBuf) {
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
        .env("NUMERA_MIGRATION_TEST_DATA", data)
        .env("NUMERA_MIGRATION_TEST_ACTION", action)
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
fn complete(data: &Path, action: &str) {
    let (mut child, _) = worker(data, action, None);
    let status = runtime()
        .block_on(child.wait(Duration::from_secs(30), Duration::from_secs(20)))
        .unwrap();
    let error = std::fs::read_to_string(data.join("worker/stderr.txt")).unwrap();
    child.finish().unwrap();
    assert!(status.success(), "migration replay failed: {error}");
}
fn retained(data: &Path) -> (String, std::collections::BTreeMap<PathBuf, Vec<u8>>) {
    let files = std::fs::read_dir(data)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            let name = path.file_name().unwrap().to_str().unwrap();
            name.ends_with(".json")
                || name.ends_with(".webm")
                || name.starts_with("catalog.before-video-identity-")
        })
        .map(|path| {
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    (
        crate::migration_signature::signature(&data.join("catalog.db"), &[]).unwrap(),
        files,
    )
}
fn refused(data: &Path, action: &str) {
    let before = retained(data);
    complete(data, action);
    assert_eq!(retained(data), before, "{action} changed recovery evidence");
    assert!(data.join("identity-migration.journal.json").is_file());
}
fn held(data: &Path, action: &str, stage: &str) -> OwnedProcess {
    let (child, ready) = worker(data, action, Some(stage));
    let deadline = Instant::now() + Duration::from_secs(20);
    let runtime = runtime();
    while !ready.is_file() {
        assert!(Instant::now() < deadline, "stage {stage} barrier timed out");
        runtime.block_on(async { tokio::time::sleep(Duration::from_millis(10)).await });
    }
    let observed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&ready).unwrap()).unwrap();
    assert_eq!(observed["stage"], stage, "wrong readiness acknowledgement");
    child
}
fn kill_at(data: &Path, action: &str, stage: &str) {
    let mut child = held(data, action, stage);
    let journal = data.join("identity-migration.journal.json");
    if stage == "retired" {
        assert_eq!(
            std::fs::read(&journal).unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
    } else {
        let doc: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&journal).unwrap()).unwrap();
        let expected = match stage {
            "database-before" | "database-after" => "backup",
            "publish_fixture.json" | "publish_plan_fixture.json" => "database",
            "second-move" => "state",
            _ => stage,
        };
        assert_eq!(
            doc["stage"], expected,
            "unexpected durable stage at {stage}"
        );
    }
    if stage == "backup" {
        let before = retained(data);
        let nested = data.join("nested");
        std::fs::create_dir(&nested).unwrap();
        let alias = nested.join("..");
        assert_eq!(
            migration_bundle::canonical(&alias).unwrap(),
            migration_bundle::canonical(data).unwrap()
        );
        for directory in [data, alias.as_path()] {
            let error = crate::catalog::Catalog::open(
                &directory.join("catalog.db"),
                "fixture UTC",
                &project(),
            )
            .err()
            .expect("a second catalog writer reached a held migration");
            assert!(error.contains("catalog ownership refused"), "{error}");
            let error = crate::ownership::Ownership::acquire(
                directory,
                crate::ownership::Mode::Maintenance,
                "fixture UTC",
            )
            .err()
            .expect("a second migration acquired the canonical lease");
            assert!(error.contains("catalog ownership refused"), "{error}");
        }
        assert_eq!(
            retained(data),
            before,
            "refused writers changed recovery evidence"
        );
    }
    // finish terminates the private Job/session, reaps its root and verifies all
    // descendants are gone before deleting scratch; normal return cannot release this barrier.
    child.finish().unwrap();
    assert!(!data.join("worker").exists());
    let owner =
        crate::ownership::Ownership::acquire(data, crate::ownership::Mode::Maintenance, "fixture")
            .unwrap();
    let families = migration_apply::locks(data, data.parent().unwrap()).unwrap();
    drop(families);
    drop(owner);
}
#[test]
fn real_process_death_replays_each_durable_migration_stage() {
    let parent =
        crate::tooling_environment::confined_directory(&project(), "logs/tmp", true).unwrap();
    let workspace = Workspace(parent.join(format!("migration-death-{}", std::process::id())));
    std::fs::create_dir(&workspace.0).unwrap();
    let fixture = project().join("tests/fixtures/native/video-codec/opaque.webm");
    assert_eq!(
        crate::migration_files::digest(&fixture).unwrap(),
        "9ad49bae33aec4cc5e754476800285ddc0f7f7ba4738e39e1f4245d87e4551e1"
    );
    let original = std::fs::read(&fixture).unwrap();
    for (case, stage) in [
        ("backup", "backup"),
        ("database", "database"),
        ("state", "state"),
        ("files", "files"),
        ("verified", "verified"),
        ("restore", "restore"),
        ("restored", "restored"),
        ("moved-before-path", "state"),
        ("linked-before-unlink", "state"),
        ("database-before", "database-before"),
        ("database-after", "database-after"),
        ("publisher-replaced", "publish_fixture.json"),
        ("frozen-replaced", "publish_plan_fixture.json"),
        ("retired", "retired"),
    ] {
        let data = workspace.0.join(case);
        std::fs::create_dir(&data).unwrap();
        let source = data.join("001_video_aaaaaaaaaaaa.webm");
        std::fs::write(&source, &original).unwrap();
        let old = format!("v:{}", "a".repeat(32));
        let state = json!({"sets":[{"keys":[old]}],"skipped":[]});
        crate::atomic::write_json(&data.join("publish_fixture.json"), &state).unwrap();
        let frozen = json!({"mixed":[old]});
        crate::atomic::write_json(&data.join("publish_plan_fixture.json"), &frozen).unwrap();
        let plan = json!({"version":1,"known":[old],"excluded":[old],"targets":[[old,2]],
            "moves":[{"key":old,"label":old,"from_pack":1,"to_pack":2}],
            "held":[{"key":old,"label":"v:free-text","from_pack":1}]});
        crate::atomic::write_json(&data.join("pack_plan.json"), &plan).unwrap();
        let db = rusqlite::Connection::open(data.join("catalog.db")).unwrap();
        db.execute_batch("CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT,format TEXT,phash INTEGER);CREATE TABLE publications(content_key TEXT,custom_emoji_id TEXT);CREATE TABLE seen_files(content_key TEXT,file_unique_id TEXT);").unwrap();
        db.execute(
            "INSERT INTO items VALUES(?,'./001_video_aaaaaaaaaaaa.webm','video',0)",
            [&old],
        )
        .unwrap();
        db.execute("INSERT INTO publications VALUES(?,'111111111')", [&old])
            .unwrap();
        db.execute("INSERT INTO seen_files VALUES(?,'fixture-unique')", [&old])
            .unwrap();
        drop(db);
        let before = crate::migration_signature::signature(&data.join("catalog.db"), &[]).unwrap();
        if stage == "restore" || stage == "restored" {
            complete(&data, "apply");
        }
        if stage == "backup" {
            let before_refusal = retained(&data);
            complete(&data, "refuse-backup");
            assert_eq!(retained(&data), before_refusal);
            assert_eq!(std::fs::read(&source).unwrap(), original);
        }
        kill_at(
            &data,
            if stage == "restore" || stage == "restored" {
                "restore"
            } else {
                "apply"
            },
            stage,
        );
        if stage == "backup" {
            let path = data.join("identity-migration.journal.json");
            let bytes = std::fs::read(&path).unwrap();
            let mut journal: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            journal["data_dir"] = json!(data.join("elsewhere"));
            crate::atomic::write_json(&path, &journal).unwrap();
            refused(&data, "refuse-binding");
            crate::atomic::write_bytes(&path, &bytes).unwrap();
        }
        if stage == "files" {
            let journal = migration_apply::journal(&data).unwrap().unwrap();
            let destination = Path::new(journal["files"][0]["destination"].as_str().unwrap());
            assert_eq!(std::fs::read(destination).unwrap(), original);
            std::fs::remove_file(destination).unwrap();
            refused(&data, "refuse-missing");
            std::fs::write(destination, &original).unwrap();
            refused(&data, "refuse-decoder");
            let connection = rusqlite::Connection::open(data.join("catalog.db")).unwrap();
            connection
                .execute(
                    "UPDATE items SET file_path=?",
                    [destination.to_str().unwrap()],
                )
                .unwrap();
            connection.close().unwrap();
            migration_bundle::applied(&data, &project(), &journal).unwrap();
        }
        if matches!(
            stage,
            "backup"
                | "database"
                | "state"
                | "database-before"
                | "database-after"
                | "publish_fixture.json"
                | "publish_plan_fixture.json"
        ) {
            let connection = rusqlite::Connection::open(data.join("catalog.db")).unwrap();
            let actual: String = connection
                .query_row("SELECT content_key FROM items", [], |row| row.get(0))
                .unwrap();
            assert_eq!(
                actual,
                if matches!(stage, "backup" | "database-before") {
                    old.as_str()
                } else {
                    "v:7cae73765ab8408268f0a31a7b39c4bb"
                },
                "halfway SQL: {stage}"
            );
            connection.close().unwrap();
            let actual: serde_json::Value = serde_json::from_slice(
                &std::fs::read(data.join("publish_plan_fixture.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(
                actual,
                if matches!(stage, "state" | "publish_plan_fixture.json") {
                    json!({"mixed":["v:7cae73765ab8408268f0a31a7b39c4bb"]})
                } else {
                    frozen.clone()
                },
                "halfway frozen plan: {stage}"
            );
            let publisher: serde_json::Value =
                serde_json::from_slice(&std::fs::read(data.join("publish_fixture.json")).unwrap())
                    .unwrap();
            assert_eq!(
                publisher,
                if matches!(
                    stage,
                    "state" | "publish_fixture.json" | "publish_plan_fixture.json"
                ) {
                    json!({"sets":[{"keys":["v:7cae73765ab8408268f0a31a7b39c4bb"]}],"skipped":[]})
                } else {
                    state.clone()
                },
                "halfway publisher: {stage}"
            );
            assert_eq!(std::fs::read(&source).unwrap(), original);
        }
        let destination = data.join("001_video_7cae73765ab8.webm");
        if matches!(case, "moved-before-path" | "linked-before-unlink") {
            // Seed the interrupted transfer after real state-stage death, not a
            // fabricated journal or a claim that death occurred inside the syscall.
            if case == "moved-before-path" {
                crate::archive_move::transfer(&source, &destination, true).unwrap();
                assert!(!source.exists());
            } else {
                std::fs::hard_link(&source, &destination).unwrap();
                assert_eq!(std::fs::read(&source).unwrap(), original);
            }
            assert_eq!(std::fs::read(&destination).unwrap(), original);
            let connection = rusqlite::Connection::open(data.join("catalog.db")).unwrap();
            let stored: String = connection
                .query_row("SELECT file_path FROM items", [], |row| row.get(0))
                .unwrap();
            assert_eq!(stored, "./001_video_aaaaaaaaaaaa.webm", "{case}");
            connection.close().unwrap();
            assert_eq!(
                migration_apply::journal(&data).unwrap().unwrap()["stage"],
                "state"
            );
        }
        if stage != "restore" && stage != "restored" && case != "moved-before-path" {
            complete(&data, "apply");
            assert!(!source.exists(), "{case}");
            let connection = rusqlite::Connection::open(data.join("catalog.db")).unwrap();
            let stored: String = connection
                .query_row("SELECT file_path FROM items", [], |row| row.get(0))
                .unwrap();
            assert_eq!(stored, "./001_video_7cae73765ab8.webm", "{case}");
            connection.close().unwrap();
            let doc: serde_json::Value =
                serde_json::from_slice(&std::fs::read(manifest(&data)).unwrap()).unwrap();
            migration_bundle::applied(&data, &project(), &doc).unwrap();
            let migrated: serde_json::Value =
                serde_json::from_slice(&std::fs::read(data.join("pack_plan.json")).unwrap())
                    .unwrap();
            let new = doc["key_map"][&old].as_str().unwrap();
            assert_eq!(new, "v:7cae73765ab8408268f0a31a7b39c4bb");
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(
                    &std::fs::read(data.join("publish_plan_fixture.json")).unwrap(),
                )
                .unwrap(),
                json!({"mixed":[new]}),
                "{stage}"
            );
            for field in ["known", "excluded"] {
                assert_eq!(migrated[field], json!([new]), "{stage} {field}");
            }
            assert_eq!(migrated["targets"], json!([[new, 2]]), "{stage}");
            for field in ["moves", "held"] {
                assert_eq!(migrated[field][0]["key"], new, "{stage} {field}");
                assert_eq!(
                    migrated[field][0]["label"], plan[field][0]["label"],
                    "{stage} {field}"
                );
            }
            assert_eq!(
                std::fs::read(Path::new(doc["files"][0]["destination"].as_str().unwrap())).unwrap(),
                original
            );
        }
        complete(&data, "restore");
        assert_eq!(
            crate::migration_signature::signature(&data.join("catalog.db"), &[]).unwrap(),
            before,
            "{stage}"
        );
        assert_eq!(std::fs::read(&source).unwrap(), original, "{case}");
        assert!(
            !destination.exists(),
            "restore left a new destination: {case}"
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(
                &std::fs::read(data.join("publish_fixture.json")).unwrap()
            )
            .unwrap(),
            state
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(
                &std::fs::read(data.join("pack_plan.json")).unwrap(),
            )
            .unwrap(),
            plan,
            "{stage}"
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(
                &std::fs::read(data.join("publish_plan_fixture.json")).unwrap(),
            )
            .unwrap(),
            frozen,
            "{stage}"
        );
        assert!(!data.join("identity-migration.journal.json").exists());
        let db = rusqlite::Connection::open(data.join("catalog.db")).unwrap();
        assert_eq!(
            db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        drop(db);
    }
    moves::check(&workspace.0.join("second-move"));
    writers::check(&workspace.0.join("writers"));
}
