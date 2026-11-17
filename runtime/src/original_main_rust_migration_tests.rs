// Original-main Rust behavioral port, separate from1516 Python IDs.
// Source native/src/migration_crash_tests.rs @9c3cbb43305b SHA256 92165c33d74771bb0bbf54979f4eadb565fcd42dba11353a9613117c2f6fc250
//! Test-only stage barriers are absent from every production executable and extension.
use crate::{
    config::Config, media_adapter::MediaAdapter, migration_apply, migration_bundle,
    migration_restore, process_owner::OwnedProcess,
};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct Workspace(PathBuf);
impl Drop for Workspace {
    fn drop(&mut self) {
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
pub(super) fn run_worker(data: std::ffi::OsString) {
    let data = PathBuf::from(data);
    let config = Config::load(data.parent().unwrap().to_path_buf()).unwrap();
    let media = MediaAdapter {
        executable: binary(),
        root: project(),
        scratch: data.join("codec"),
    };
    let runtime = runtime();
    match std::env::var("NUMERA_MIGRATION_TEST_ACTION")
        .unwrap()
        .as_str()
    {
        "apply" => {
            migration_apply::apply(&config, &data, &media, &runtime, None).unwrap();
        }
        "restore" => {
            migration_restore::restore(&data, data.parent().unwrap(), &manifest(&data), true)
                .unwrap();
        }
        other => panic!("unknown migration worker action: {other}"),
    }
}
fn binary() -> PathBuf {
    let name = if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    };
    let current = std::env::current_exe().unwrap();
    let profile = current
        .ancestors()
        .find(|p| {
            matches!(
                p.file_name().and_then(|s| s.to_str()),
                Some("debug" | "release")
            )
        })
        .unwrap();
    let path = profile.join(name);
    assert!(
        path.is_file(),
        "real standalone codec executable missing: {}",
        path.display()
    );
    path
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
        .env("NUMERA_ORIGINAL_MAIN_MIGRATION_DATA", data)
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
    (OwnedProcess::spawn(&mut command, scratch).unwrap(), ready)
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
fn kill_at(data: &Path, action: &str, stage: &str) {
    let (mut child, ready) = worker(data, action, Some(stage));
    let deadline = Instant::now() + Duration::from_secs(20);
    let runtime = runtime();
    while !ready.is_file() {
        assert!(Instant::now() < deadline, "stage {stage} barrier timed out");
        runtime.block_on(async { tokio::time::sleep(Duration::from_millis(10)).await });
    }
    let doc: serde_json::Value = serde_json::from_slice(
        &std::fs::read(data.join("identity-migration.journal.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        doc["stage"], stage,
        "barrier must follow the durable journal write"
    );
    // finish terminates the private Job/group, reaps its root and verifies all
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
    let workspace = Workspace(
        project()
            .join("logs")
            .join(format!("migration-death-{}", std::process::id())),
    );
    std::fs::create_dir_all(&workspace.0).unwrap();
    // The original tests._video_fixtures.encode arguments and RED/BLUE/RED RGBA frames.
    // Generate through a bounded, owned FFmpeg capture; no interpreter or new-engine oracle.
    let raw = workspace.0.join("source.rgba");
    let mut frames = Vec::new();
    for color in [
        [220_u8, 20, 20, 255],
        [20, 20, 220, 255],
        [220, 20, 20, 255],
    ] {
        frames.extend(color.repeat(100 * 100));
    }
    std::fs::write(&raw, frames).unwrap();
    let output = workspace.0.join("source.webm");
    let arguments = [
        "-y",
        "-v",
        "error",
        "-f",
        "rawvideo",
        "-pixel_format",
        "rgba",
        "-video_size",
        "100x100",
        "-framerate",
        "30",
        "-i",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain(std::iter::once(raw.to_str().unwrap().into()))
    .chain(
        [
            "-c:v",
            "libvpx-vp9",
            "-threads",
            "1",
            "-cpu-used",
            "8",
            "-lossless",
            "1",
            "-pix_fmt",
            "yuva420p",
            "-auto-alt-ref",
            "0",
        ]
        .into_iter()
        .map(str::to_owned),
    )
    .chain(std::iter::once(output.to_str().unwrap().into()))
    .collect::<Vec<_>>();
    runtime()
        .block_on(crate::codec_process::capture_with_budget(
            &workspace.0,
            &crate::codec_process::executable("ffmpeg").unwrap(),
            &arguments,
            256 * 1024,
            None,
            &std::env::vars_os().collect(),
            crate::codec_process::Budget {
                wall: Duration::from_secs(40),
                idle: Duration::from_secs(20),
                warning: None,
            },
        ))
        .unwrap();
    std::fs::remove_file(raw).unwrap();
    let original = std::fs::read(workspace.0.join("source.webm")).unwrap();
    for stage in [
        "backup", "database", "state", "files", "verified", "restore", "restored",
    ] {
        let data = workspace.0.join(stage);
        std::fs::create_dir(&data).unwrap();
        let source = data.join("001_video_aaaaaaaaaaaa.webm");
        std::fs::write(&source, &original).unwrap();
        let old = format!("v:{}", "a".repeat(32));
        let state = json!({"sets":[{"keys":[old]}],"skipped":[]});
        crate::atomic::write_json(&data.join("publish_fixture.json"), &state).unwrap();
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
        kill_at(
            &data,
            if stage == "restore" || stage == "restored" {
                "restore"
            } else {
                "apply"
            },
            stage,
        );
        if stage != "restore" && stage != "restored" {
            complete(&data, "apply");
            let doc: serde_json::Value =
                serde_json::from_slice(&std::fs::read(manifest(&data)).unwrap()).unwrap();
            migration_bundle::applied(&data, &project(), &doc).unwrap();
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
        assert_eq!(std::fs::read(&source).unwrap(), original, "{stage}");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(
                &std::fs::read(data.join("publish_fixture.json")).unwrap()
            )
            .unwrap(),
            state
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
}
