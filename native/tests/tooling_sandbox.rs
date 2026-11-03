//! Independent copied-catalog behavior, lifetime exclusion and refusal; no panel or operator source.
use _native::tooling_sandbox::{self, Session};
use rusqlite::Connection;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

fn seeded(root: &Path) -> PathBuf {
    let source = root.join("source");
    std::fs::create_dir_all(source.join("media")).unwrap();
    let mut catalog =
        _native::catalog::Catalog::open(&source.join("catalog.db"), "fixture", root).unwrap();
    // These original clone fixtures test storage, not PNG decoding.
    for (key, name, bytes) in [
        ("s:a_b", "0.png", b"\x89PNG-0"),
        ("s_a:b", "1.png", b"\x89PNG-1"),
    ] {
        let path = source.join("media").join(name);
        std::fs::write(&path, bytes).unwrap();
        assert!(
            catalog
                .insert(
                    &json!({"content_key":key,"fmt":"static","file_path":path}),
                    "fixture"
                )
                .unwrap()
        );
    }
    drop(catalog);
    source
}

fn stored_paths(directory: &Path) -> Vec<PathBuf> {
    let db = Connection::open_with_flags(
        directory.join("catalog.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut statement = db
        .prepare("SELECT file_path FROM items ORDER BY content_key")
        .unwrap();
    statement
        .query_map([], |row| row.get::<_, String>(0).map(PathBuf::from))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}

#[test]
fn clone_contains_wal_rows_independent_media_and_state_while_lifetime_excludes_sweep() {
    let root = Root::new("sandbox clone");
    let source = root.0.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("image.bin"), b"real independently copied bytes").unwrap();
    std::fs::write(
        source.join("pack_plan.json"),
        b"{\"version\":1,\"known\":[]}",
    )
    .unwrap();
    let db = Connection::open(source.join("catalog.db")).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT); INSERT INTO items VALUES('s:legacy/key','./image.bin');").unwrap();
    let session = Session::create(&root.0, &source, &root.0.join("panel-sandbox-copy")).unwrap();
    assert_eq!(session.count, 1);
    assert!(tooling_sandbox::marker_is_self_describing(
        &session.directory
    ));
    assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
    assert_eq!(
        std::fs::read(session.directory.join("pack_plan.json")).unwrap(),
        b"{\"version\":1,\"known\":[]}"
    );
    let clone = Connection::open(session.directory.join("catalog.db")).unwrap();
    let path: String = clone
        .query_row("SELECT file_path FROM items", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"real independently copied bytes"
    );
    assert!(Path::new(&path).starts_with(&session.directory));
    std::fs::write(&path, b"private clone mutation").unwrap();
    assert_eq!(
        std::fs::read(source.join("image.bin")).unwrap(),
        b"real independently copied bytes"
    );
    clone.close().unwrap();
    let lock = tooling_sandbox::lifetime_lock_path(&session.directory).unwrap();
    let directory = session.directory.clone();
    session.finish().unwrap();
    assert!(!directory.exists());
    assert!(
        lock.is_file(),
        "lifetime tombstone must survive clone cleanup"
    );
    db.close().unwrap();
}

#[test]
fn recovery_marker_prevents_reclamation_after_uncertain_child_cleanup() {
    let root = Root::new("sandbox clone");
    let source = root.0.join("source");
    std::fs::create_dir(&source).unwrap();
    let db = Connection::open(source.join("catalog.db")).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT);")
        .unwrap();
    db.close().unwrap();
    let session =
        Session::create(&root.0, &source, &root.0.join("panel-sandbox-recovery")).unwrap();
    let directory = session.directory.clone();
    let owner_bytes = std::fs::read(directory.join(".sandbox-owner.json")).unwrap();
    session.retain_for_recovery().unwrap();
    assert!(
        !directory.join(".sandbox-owner.json").exists(),
        "no reclaimable marker may remain before spawn"
    );
    assert_eq!(
        std::fs::read(directory.join(".sandbox-recovery.json")).unwrap(),
        owner_bytes
    );
    session.retain_for_recovery().unwrap();
    std::fs::write(
        directory.join(".sandbox-recovery.json"),
        b"{\"version\":2,\"directory\":\"foreign\"}",
    )
    .unwrap();
    assert!(
        session.retain_for_recovery().is_err(),
        "existing malformed/foreign recovery is not readiness"
    );
    std::fs::write(directory.join(".sandbox-recovery.json"), &owner_bytes).unwrap();
    assert!(!tooling_sandbox::marker_is_self_describing(&directory));
    assert!(directory.join(".sandbox-recovery.json").is_file());
    drop(session);
    assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
    assert!(directory.join("catalog.db").is_file());
}

#[test]
fn occupied_recovery_refuses_without_replacing_owner_or_foreign_bytes() {
    let root = Root::new("sandbox clone");
    let source = root.0.join("source");
    std::fs::create_dir(&source).unwrap();
    let db = Connection::open(source.join("catalog.db")).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT);")
        .unwrap();
    db.close().unwrap();
    let session = Session::create(&root.0, &source, &root.0.join("panel-sandbox-refused")).unwrap();
    let owner = session.directory.join(".sandbox-owner.json");
    let recovery = session.directory.join(".sandbox-recovery.json");
    let owner_bytes = std::fs::read(&owner).unwrap();
    std::fs::write(&recovery, b"foreign preserved").unwrap();
    assert!(session.retain_for_recovery().is_err());
    assert_eq!(std::fs::read(&owner).unwrap(), owner_bytes);
    assert_eq!(std::fs::read(&recovery).unwrap(), b"foreign preserved");
    session.finish().unwrap();
}

#[test]
fn original_clone_preserves_source_schema_journal_and_independent_media_preview_and_plan() {
    let deadline = Instant::now() + Duration::from_secs(10);
    let root = Root::new("sandbox source preservation");
    let source = seeded(&root.0);
    let database = source.join("catalog.db");
    let db = Connection::open(&database).unwrap();
    db.execute_batch("PRAGMA journal_mode=DELETE; DROP TABLE meta;")
        .unwrap();
    db.close().unwrap();
    std::fs::create_dir(source.join("preview")).unwrap();
    let preview = source.join("preview/a.webp");
    std::fs::write(&preview, b"RIFF-preview").unwrap();
    _native::atomic::write_json(
        &source.join("pack_plan.json"),
        &json!({"version":1,"targets":[],"known":[],"excluded":[],"moves":[],"held":[]}),
    )
    .unwrap();
    let before = std::fs::read(&database).unwrap();
    let plan = std::fs::read(source.join("pack_plan.json")).unwrap();
    let target = root.0.join("panel-sandbox-original");
    let session = Session::create(&root.0, &source, &target).unwrap();
    assert_eq!(session.count, 2);
    assert_eq!(std::fs::read(&database).unwrap(), before);
    assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
    let paths = stored_paths(&target);
    assert_eq!(paths.len(), 2);
    assert_ne!(paths[0], paths[1], "portable keys must not alias");
    for (path, original) in paths.iter().zip([b"\x89PNG-0", b"\x89PNG-1"]) {
        assert!(path.starts_with(target.join("media")));
        assert!(!path.file_name().unwrap().to_str().unwrap().contains(':'));
        assert_eq!(std::fs::read(path).unwrap(), original);
        std::fs::write(path, b"owned media mutation").unwrap();
    }
    for (index, original) in [b"\x89PNG-0", b"\x89PNG-1"].iter().enumerate() {
        assert_eq!(
            std::fs::read(source.join(format!("media/{index}.png"))).unwrap(),
            *original
        );
    }
    assert_eq!(
        std::fs::read(target.join("preview/a.webp")).unwrap(),
        b"RIFF-preview"
    );
    std::fs::write(target.join("preview/a.webp"), b"owned preview mutation").unwrap();
    assert_eq!(std::fs::read(&preview).unwrap(), b"RIFF-preview");
    assert_eq!(std::fs::read(target.join("pack_plan.json")).unwrap(), plan);
    _native::atomic::write_json(&target.join("pack_plan.json"), &json!({"version":1})).unwrap();
    assert_eq!(std::fs::read(source.join("pack_plan.json")).unwrap(), plan);
    // Only the copied Catalog may upgrade its schema and journal mode.
    let copied =
        _native::catalog::Catalog::open(&target.join("catalog.db"), "clone", &root.0).unwrap();
    assert_eq!(copied.all(None).unwrap().len(), 2);
    drop(copied);
    let read =
        Connection::open_with_flags(&database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        read.query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "delete"
    );
    assert_eq!(
        read.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='meta'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    read.close().unwrap();
    assert_eq!(std::fs::read(&database).unwrap(), before);
    let lease = tooling_sandbox::lifetime_lock_path(&target).unwrap();
    assert_ne!(lease, target.join(".maintenance.lock"));
    session.finish().unwrap();
    assert!(!target.exists());
    assert!(lease.is_file());
    assert!(Instant::now() < deadline);
    root.clean().unwrap();
}

#[test]
fn derived_preview_failure_does_not_refuse_a_complete_independent_clone() {
    let root = Root::new("sandbox preview refusal");
    let source = seeded(&root.0);
    // read_dir fails on this owned file; this is not the original copyfile fault.
    std::fs::write(source.join("preview"), b"not a preview directory").unwrap();
    let before = std::fs::read(source.join("catalog.db")).unwrap();
    let target = root.0.join("panel-sandbox-preview");
    let session = Session::create(&root.0, &source, &target).unwrap();
    assert_eq!(session.count, 2);
    assert_eq!(
        session.warnings,
        ["Preview cache not copied completely; previews render on demand"]
    );
    let paths = stored_paths(&target);
    assert_eq!(paths.len(), 2);
    assert!(paths.iter().all(|path| path.is_file()));
    assert!(!target.join("preview").exists());
    assert_eq!(
        std::fs::read(source.join("preview")).unwrap(),
        b"not a preview directory"
    );
    assert_eq!(std::fs::read(source.join("catalog.db")).unwrap(), before);
    session.finish().unwrap();
    assert!(!target.exists());
    root.clean().unwrap();
}

#[test]
fn original_path_precedence_and_external_media_are_preserved_without_source_migration() {
    let deadline = Instant::now() + Duration::from_secs(10);
    let root = Root::new("sandbox path precedence");
    let source = seeded(&root.0);
    let database = source.join("catalog.db");
    let project_media = root.0.join("media");
    std::fs::create_dir(&project_media).unwrap();
    std::fs::write(project_media.join("0.png"), b"PROJECT-original").unwrap();
    std::fs::write(source.join("media/0.png"), b"SOURCE-decoy").unwrap();
    let external = root.0.join("archive.png");
    std::fs::write(&external, b"ARCHIVE-original").unwrap();
    for (label, stored, expected) in [
        ("project", "media/0.png", b"PROJECT-original".as_slice()),
        ("data", "./media/0.png", b"SOURCE-decoy".as_slice()),
        (
            "archive",
            external.to_str().unwrap(),
            b"ARCHIVE-original".as_slice(),
        ),
    ] {
        let db = Connection::open(&database).unwrap();
        db.execute_batch("PRAGMA journal_mode=DELETE;").unwrap();
        db.execute(
            "UPDATE items SET file_path=? WHERE content_key='s:a_b'",
            [stored],
        )
        .unwrap();
        db.close().unwrap();
        let before = std::fs::read(&database).unwrap();
        let target = root.0.join(format!("panel-sandbox-{label}"));
        let session = Session::create(&root.0, &source, &target).unwrap();
        assert_eq!(session.count, 2);
        assert!(!target.join("pack_plan.json").exists());
        let paths = stored_paths(&target);
        assert_eq!(paths.len(), 2);
        assert_eq!(std::fs::read(&paths[0]).unwrap(), expected, "{label}");
        assert!(
            paths
                .iter()
                .all(|path| path.starts_with(&target) && path.is_file())
        );
        std::fs::write(&paths[0], b"owned clone changed").unwrap();
        assert_eq!(std::fs::read(&database).unwrap(), before);
        assert_eq!(
            std::fs::read(project_media.join("0.png")).unwrap(),
            b"PROJECT-original"
        );
        assert_eq!(
            std::fs::read(source.join("media/0.png")).unwrap(),
            b"SOURCE-decoy"
        );
        assert_eq!(std::fs::read(&external).unwrap(), b"ARCHIVE-original");
        session.finish().unwrap();
        assert!(!target.exists());
    }
    assert!(Instant::now() < deadline);
    root.clean().unwrap();
}

#[test]
fn original_clone_refusals_preserve_source_bytes_and_leave_no_partial_copy() {
    let deadline = Instant::now() + Duration::from_secs(10);
    let root = Root::new("sandbox clone refusal");
    let source = seeded(&root.0);
    let database = source.join("catalog.db");
    let db = Connection::open(&database).unwrap();
    db.execute_batch("PRAGMA journal_mode=DELETE;").unwrap();
    db.close().unwrap();
    let before = std::fs::read(&database).unwrap();
    let target = root.0.join("panel-sandbox-refused");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("caller"), b"precious").unwrap();
    assert!(Session::create(&root.0, &source, &target).is_err());
    assert_eq!(std::fs::read(target.join("caller")).unwrap(), b"precious");
    let nested = source.join("inner");
    assert!(Session::create(&root.0, &source, &nested).is_err());
    assert!(!nested.exists());
    let refused = root.0.join("panel-sandbox-unfinished");
    _native::atomic::write_json(&source.join("identity-migration.journal.json"), &json!({}))
        .unwrap();
    let error = Session::create(&root.0, &source, &refused).err().unwrap();
    assert!(error.contains("interrupted identity migration"), "{error}");
    assert!(!refused.exists());
    std::fs::remove_file(source.join("identity-migration.journal.json")).unwrap();
    // An actual foreign thread owns the source lease; same-thread nesting is allowed.
    std::thread::scope(|scope| {
        let lease = _native::ownership::Ownership::acquire(
            &source,
            _native::ownership::Mode::Writer,
            "fixture",
        )
        .unwrap();
        let error = scope
            .spawn(|| Session::create(&root.0, &source, &refused).err().unwrap())
            .join()
            .unwrap();
        assert!(error.contains("catalog ownership refused"), "{error}");
        assert!(!refused.exists());
        drop(lease);
    });
    std::fs::remove_file(source.join("media/0.png")).unwrap();
    assert!(Session::create(&root.0, &source, &refused).is_err());
    assert!(!refused.exists());
    assert_eq!(std::fs::read(&database).unwrap(), before);
    assert_eq!(
        std::fs::read(source.join("media/1.png")).unwrap(),
        b"\x89PNG-1"
    );
    assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
    assert!(Instant::now() < deadline);
    root.clean().unwrap();
}

#[test]
fn original_stale_marker_variants_preserve_foreign_data_and_stable_lease_files() {
    let deadline = Instant::now() + Duration::from_secs(10);
    let root = Root::new("sandbox stale markers");
    let foreign = root.0.join("foreign");
    std::fs::create_dir(&foreign).unwrap();
    std::fs::write(foreign.join("precious"), b"not a sandbox").unwrap();
    for variant in [
        "abandoned",
        "held",
        "unmarked",
        "foreign",
        "malformed",
        "old",
    ] {
        let directory = root.0.join(format!("panel-sandbox-{variant}"));
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("catalog.db"), b"owned marker fixture").unwrap();
        let marker = directory.join(".sandbox-owner.json");
        match variant {
            "unmarked" => {}
            "malformed" => std::fs::write(&marker, b"{not json").unwrap(),
            _ => _native::atomic::write_json(
                &marker,
                &json!({
                    "version":if variant == "old" {1} else {2},
                    "directory":if variant == "foreign" {&foreign} else {&directory},
                }),
            )
            .unwrap(),
        }
        let lease = tooling_sandbox::lifetime_lock_path(&directory).unwrap();
        let held = _native::locks::Lease::acquire(&lease, "fixture").unwrap();
        if variant != "held" {
            drop(held);
        } else {
            assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
            assert_eq!(
                std::fs::read(directory.join("catalog.db")).unwrap(),
                b"owned marker fixture"
            );
            drop(held);
            // Retain the previously live fixture as foreign before the next sweep.
            std::fs::remove_file(&marker).unwrap();
        }
        let expected = usize::from(variant == "abandoned");
        assert_eq!(
            tooling_sandbox::sweep_stale(&root.0).unwrap(),
            expected,
            "{variant}"
        );
        assert_eq!(directory.exists(), expected == 0, "{variant}");
        assert!(lease.is_file(), "lifetime tombstone must remain: {variant}");
    }
    #[cfg(unix)]
    {
        let link = root.0.join("panel-sandbox-link");
        std::os::unix::fs::symlink(&foreign, &link).unwrap();
        assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
        assert!(link.is_symlink());
        std::fs::remove_file(link).unwrap();
    }
    assert_eq!(
        std::fs::read(foreign.join("precious")).unwrap(),
        b"not a sandbox"
    );
    assert!(Instant::now() < deadline);
    root.clean().unwrap();
}

#[test]
fn sandbox_construction_process_fixture() {
    if !std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .any(|pair| pair == ["--exact", "sandbox_construction_process_fixture"])
    {
        return;
    }
    let root = PathBuf::from(std::env::var_os("NUMERA_SANDBOX_CONSTRUCTION_FIXTURE").unwrap());
    let root = _native::tooling_environment::physical_root(&root).unwrap();
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    assert!(root.starts_with(project.join("logs/tmp")));
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let session = Session::create(
        &root,
        &root.join("source"),
        &root.join("panel-sandbox-killed"),
    )
    .unwrap();
    assert_eq!(session.count, 2);
    _native::atomic::write_json(
        &root.join("ready.json"),
        &json!({"pid":std::process::id(),"count":session.count}),
    )
    .unwrap();
    // The parent must kill this actual owner, not ask Session::finish or Drop to clean it.
    std::thread::park_timeout(Duration::from_secs(5));
    _native::atomic::write_json(&root.join("expired.json"), &json!({"expired":true})).unwrap();
    panic!("sandbox fixture exceeded its parent-owned deadline");
}

#[test]
fn abrupt_construction_owner_death_releases_lease_for_exactly_one_safe_sweep() {
    use _native::process_owner::OwnedProcess;
    use std::process::{Command, Stdio};
    let root = Root::new("sandbox abrupt construction death");
    let source = seeded(&root.0);
    let database = source.join("catalog.db");
    let db = Connection::open(&database).unwrap();
    db.execute_batch("PRAGMA journal_mode=DELETE;").unwrap();
    db.close().unwrap();
    let original_database = std::fs::read(&database).unwrap();
    let originals = [source.join("media/0.png"), source.join("media/1.png")].map(|path| {
        let bytes = std::fs::read(&path).unwrap();
        (path, bytes)
    });
    let owner = root.0.join("logs/tmp/tooling/construction");
    std::fs::create_dir_all(&owner).unwrap();
    let stderr = root.0.join("child-stderr.log");
    let environment =
        _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "sandbox_construction_process_fixture",
            "--nocapture",
            "--test-threads=1",
        ])
        .current_dir(&root.0)
        .env_clear()
        .envs(environment)
        .env("NUMERA_SANDBOX_CONSTRUCTION_FIXTURE", &root.0)
        .env("TEMP", &root.0)
        .env("TMP", &root.0)
        .env("TMPDIR", &root.0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&stderr).unwrap());
    let mut child = OwnedProcess::spawn(&mut command, owner).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let started = Instant::now();
    let ready = root.0.join("ready.json");
    runtime.block_on(async {
        let monitor = child.wait(Duration::from_secs(5), Duration::from_secs(3));
        tokio::pin!(monitor);
        let readiness = async {
            loop {
                if ready.is_file() { break; }
                assert!(started.elapsed() < Duration::from_secs(3), "sandbox owner never acknowledged clone; {}", std::fs::read_to_string(&stderr).unwrap());
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        tokio::select! {
            outcome = &mut monitor => panic!("sandbox construction owner stopped before acknowledgement: {outcome:?}"),
            () = readiness => {}
        }
    });
    let acknowledgment: serde_json::Value =
        serde_json::from_slice(&std::fs::read(ready).unwrap()).unwrap();
    assert_eq!(acknowledgment["count"], 2);
    assert_ne!(
        acknowledgment["pid"].as_u64().unwrap(),
        u64::from(std::process::id())
    );
    let clone = root.0.join("panel-sandbox-killed");
    assert!(tooling_sandbox::marker_is_self_describing(&clone));
    let lease = tooling_sandbox::lifetime_lock_path(&clone).unwrap();
    let lock_bytes = std::fs::read(&lease).unwrap();
    let clone_bytes = std::fs::read(clone.join("catalog.db")).unwrap();
    let paths = stored_paths(&clone);
    assert_eq!(paths.len(), 2);
    let cloned_media = paths
        .iter()
        .map(|path| std::fs::read(path).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
    assert_eq!(
        std::fs::read(clone.join("catalog.db")).unwrap(),
        clone_bytes
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "parent missed acknowledged death window"
    );
    child.finish().unwrap();
    assert!(
        !root.0.join("expired.json").exists(),
        "child deadline, not parent termination, ended ownership"
    );
    assert_eq!(
        std::fs::read(clone.join("catalog.db")).unwrap(),
        clone_bytes
    );
    assert_eq!(
        paths
            .iter()
            .map(|path| std::fs::read(path).unwrap())
            .collect::<Vec<_>>(),
        cloned_media
    );
    assert_eq!(std::fs::read(&lease).unwrap(), lock_bytes);
    assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 1);
    assert!(!clone.exists());
    assert!(
        lease.is_file(),
        "stable lifetime lock survives stale reclamation"
    );
    assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
    assert_eq!(std::fs::read(database).unwrap(), original_database);
    for (path, bytes) in originals {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    assert!(
        std::fs::read_dir(root.0.join("logs/tmp/tooling"))
            .unwrap()
            .next()
            .is_none()
    );
    assert!(!_native::process_owner::cleanup_pending(&root.0));
    runtime.shutdown_timeout(Duration::from_secs(1));
    root.clean().unwrap();
}

#[test]
fn foreign_or_missing_source_does_not_create_or_erase_target() {
    let root = Root::new("sandbox clone");
    let source = root.0.join("source");
    let target = root.0.join("panel-sandbox-occupied");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep"), b"precious").unwrap();
    assert!(Session::create(&root.0, &source, &target).is_err());
    assert_eq!(std::fs::read(target.join("keep")).unwrap(), b"precious");
    assert!(Session::create(&root.0, &source, &root.0.join("panel-sandbox-new")).is_err());
    assert!(!root.0.join("panel-sandbox-new").exists());
    assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
    assert_eq!(tooling_sandbox::safe_key("a:b/é"), "613a622fc3a9");
}
