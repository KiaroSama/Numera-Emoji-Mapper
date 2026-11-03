//! Original report and preflight outcomes cross the actual copied command, never a report replica.
use super::{database, identifiers, item, recovery_outputs};
use _native::{atomic, config::Config, logging::RunLog, migration_signature, tooling_process};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

fn read_optional(path: &Path) -> Option<Vec<u8>> {
    match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!(
            "cannot inspect preserved fixture {}: {error}",
            path.display()
        ),
    }
}

pub fn check(root: &Path, executable: &Path, inherited: &BTreeMap<String, String>, log: &RunLog) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/video-fingerprint.json"
    ))
    .unwrap();
    let clip = &fixture["cases"][2];
    let input = project.join(clip["input"].as_str().unwrap());
    assert_eq!(
        _native::tooling_manifest::digest(&input).unwrap(),
        clip["input_sha256"]
    );
    let bytes = std::fs::read(&input).unwrap();
    let key = clip["expected"]["content_key"].as_str().unwrap();
    let hash = clip["expected"]["phash"].as_u64().unwrap();
    let eof = root.join("empty-input.txt");
    std::fs::write(&eof, b"").unwrap();
    let mut environment = inherited.clone();
    environment.insert("EMOJI_FFMPEG_TIMEOUT".into(), "3".into());
    let config = Config::load_with_environment(root.into(), &environment).unwrap();
    for (case, code, message) in [
        (
            "empty",
            0,
            "Clean: identities, media and all required references agree.",
        ),
        (
            "clean",
            0,
            "Clean: identities, media and all required references agree.",
        ),
        (
            "pending",
            3,
            "Pending: numera-emoji identity-repair migrate-video-keys --apply",
        ),
        ("missing", 4, "MISSING v:old"),
        ("undecodable", 4, "UNDECODABLE v:old"),
        ("partial-missing", 4, "MISSING v:gone"),
        ("all-undecodable", 4, "UNDECODABLE v:second"),
        ("collision", 4, "COLLISION"),
        (
            "obsolete",
            0,
            "Obsolete frozen-plan/skipped-history entries are retained",
        ),
        ("orphan", 4, "orphaned reference(s)"),
        (
            "required-state",
            3,
            "Pending: numera-emoji identity-repair migrate-video-keys --apply",
        ),
        (
            "interrupted",
            3,
            "AN INTERRUPTED MIGRATION IS RECORDED (stage: database, started 2000-01-01T00:00:00Z).",
        ),
        ("bad-journal", 4, ""),
    ] {
        let data = root.join(case);
        std::fs::create_dir(&data).unwrap();
        let db_path = data.join("catalog.db");
        let db = database(&db_path);
        let source = data.join("source.webm");
        let second = data.join("second.webm");
        if case != "empty" {
            let current = matches!(
                case,
                "clean" | "obsolete" | "orphan" | "interrupted" | "required-state"
            );
            item(
                &db,
                if current { key } else { "v:old" },
                "./source.webm",
                "video",
                Some(if current { hash } else { 0 }),
            );
            if case != "missing" {
                std::fs::write(
                    &source,
                    if matches!(case, "undecodable" | "all-undecodable") {
                        b"not a video".as_slice()
                    } else {
                        &bytes
                    },
                )
                .unwrap();
            }
        }
        match case {
            "partial-missing" => item(&db, "v:gone", "./second.webm", "video", Some(0)),
            "all-undecodable" => {
                item(&db, "v:second", "./second.webm", "video", Some(0));
                std::fs::write(&second, b"not a video").unwrap();
            }
            "collision" => {
                // A nonmoving row already owns the fresh key, not just two moving rows.
                item(&db, key, "./second.webm", "static", Some(hash));
                std::fs::write(&second, b"unchanged existing row").unwrap();
            }
            "orphan" => identifiers(&db, "v:absent", Some("orphan-fuid"), None),
            _ => {}
        }
        db.close().unwrap();
        let state_path = data.join("publish_fixture.json");
        let state = if case == "required-state" {
            json!({"sets":[{"keys":["v:lost"]}]})
        } else {
            json!({"sets":[]})
        };
        atomic::write_json(&state_path, &state).unwrap();
        let plan = data.join("publish_plan_fixture.json");
        if case == "obsolete" {
            atomic::write_json(&plan, &json!({"mixed":["v:ghost"]})).unwrap();
        }
        let journal = data.join("identity-migration.journal.json");
        match case {
            "interrupted" => atomic::write_json(
                &journal,
                &json!({"stage":"database","started_utc":"2000-01-01T00:00:00Z"}),
            )
            .unwrap(),
            "bad-journal" => std::fs::write(&journal, b"{not json").unwrap(),
            _ => {}
        }
        let preserved = [&source, &second, &state_path, &plan, &journal]
            .map(|path| (path.to_path_buf(), read_optional(path)));
        let signature = migration_signature::signature(&db_path, &[]).unwrap();
        let run = |options: &[&str]| {
            let mut argv = vec![executable.as_os_str().to_owned(), "identity-repair".into()];
            argv.extend(
                options
                    .iter()
                    .map(|option| std::ffi::OsString::from(*option)),
            );
            argv.extend(["--data-dir".into(), data.as_os_str().to_owned()]);
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                remaining > Duration::from_secs(1),
                "{case}: command deadline exhausted"
            );
            runtime
                .block_on(tooling_process::capture_with_input(
                    root,
                    &argv,
                    &environment,
                    &eof,
                    remaining.min(Duration::from_secs(10)),
                    remaining.min(Duration::from_secs(5)),
                    log,
                ))
                .unwrap()
        };
        let report = run(&["report"]);
        assert_eq!(
            report.code, code,
            "{case}: {} {}",
            report.stdout, report.stderr
        );
        if !message.is_empty() {
            assert!(report.stdout.contains(message), "{case}: {}", report.stdout);
        }
        if !matches!(case, "empty" | "clean" | "obsolete") {
            assert!(!report.stdout.contains("Clean:"), "{case}");
        }
        if matches!(
            case,
            "missing"
                | "undecodable"
                | "partial-missing"
                | "all-undecodable"
                | "collision"
                | "orphan"
                | "required-state"
                | "bad-journal"
        ) {
            let refused = run(&["migrate-video-keys", "--apply"]);
            assert_eq!(
                refused.code, 4,
                "{case}: {} {}",
                refused.stdout, refused.stderr
            );
        }
        assert_eq!(
            migration_signature::signature(&db_path, &[]).unwrap(),
            signature,
            "{case}"
        );
        for (path, expected) in preserved {
            assert_eq!(read_optional(&path), expected, "{case}: {}", path.display());
        }
        assert!(recovery_outputs(&data).is_empty(), "{case}");
        let scratch = data.join("tmp/identity-codec");
        if scratch.exists() {
            assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 0, "{case}");
        }
        assert!(!_native::process_owner::cleanup_pending(&scratch));
        assert!(
            _native::migration_apply::locks(&data, &config.root).is_ok(),
            "{case}"
        );
    }
    let data = root.join("collection");
    std::fs::create_dir(&data).unwrap();
    database(&data.join("catalog.db")).close().unwrap();
    let signature = migration_signature::signature(&data.join("catalog.db"), &[]).unwrap();
    let image = project.join("tests/fixtures/native/static-codec/opaque.png");
    assert_eq!(
        _native::tooling_manifest::digest(&image).unwrap(),
        "186ee151bdd1301c0e6c82e4e18550d248d4be36b8a0ae4c37ec6b91d44622f4"
    );
    let incoming = data.join("incoming.png");
    let image_bytes = std::fs::read(image).unwrap();
    std::fs::write(&incoming, &image_bytes).unwrap();
    let journal = data.join("identity-migration.journal.json");
    std::fs::write(&journal, b"{}").unwrap();
    let mut guarded = environment.clone();
    guarded.insert("GENERAL_BOT_TOKEN".into(), "test-only-token".into());
    guarded.insert("NUMERA_TEST_API_BASE".into(), "http://127.0.0.1:9".into());
    guarded.insert("EMOJI_ARCHIVE_DIR".into(), "archive".into());
    for (options, expected) in [
        (
            vec![
                "add-media",
                incoming.to_str().unwrap(),
                "--data-dir",
                data.to_str().unwrap(),
            ],
            2,
        ),
        (
            vec![
                "build-collection",
                "--base",
                "newfamily",
                "--title",
                "Fixture",
                "--dry-run",
                "--no-brand-logo",
                "--data-dir",
                data.to_str().unwrap(),
            ],
            4,
        ),
        (vec!["pack-archive", "--sync"], 1),
    ] {
        let mut argv = vec![executable.as_os_str().to_owned()];
        argv.extend(
            options
                .iter()
                .map(|option| std::ffi::OsString::from(*option)),
        );
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(remaining > Duration::from_secs(1));
        let result = runtime
            .block_on(tooling_process::capture_with_input(
                root,
                &argv,
                &guarded,
                &eof,
                remaining.min(Duration::from_secs(10)),
                remaining.min(Duration::from_secs(5)),
                log,
            ))
            .unwrap();
        assert_eq!(result.code, expected, "{} {}", result.stdout, result.stderr);
        assert!(
            result.stderr.contains("interrupted identity migration"),
            "{}",
            result.stderr
        );
        assert_eq!(
            migration_signature::signature(&data.join("catalog.db"), &[]).unwrap(),
            signature
        );
        assert_eq!(std::fs::read(&incoming).unwrap(), image_bytes);
        assert_eq!(std::fs::read(&journal).unwrap(), b"{}");
        assert!(!data.join("publish_newfamily.json").exists());
        assert!(!data.join("publish_plan_newfamily.json").exists());
        assert!(!data.join("pack_plan.json").exists());
        assert!(recovery_outputs(&data).is_empty());
        assert!(!root.join("archive").exists());
        if data.join("tmp").exists() {
            assert_eq!(std::fs::read_dir(data.join("tmp")).unwrap().count(), 0);
        }
    }
    assert_eq!(std::fs::read(input).unwrap(), bytes);
    assert_eq!(
        std::fs::read_dir(root.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    assert!(!_native::process_owner::cleanup_pending(root));
    runtime.shutdown_timeout(Duration::from_secs(1));
    assert!(
        Instant::now() < deadline,
        "identity commands exceeded60s; retained"
    );
}
