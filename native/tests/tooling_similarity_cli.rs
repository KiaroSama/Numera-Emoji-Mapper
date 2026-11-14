//! Contributor benchmark command refusal/help and actual same-input replay through the binary.
use _native::{logging::RunLog, tooling_environment, tooling_process};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

#[test]
fn actual_benchmark_command_replays_read_only_catalog_without_operator_configuration() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let parent = tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let scratch = parent.join(format!("similarity command {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let catalog = scratch.join("catalog with spaces.db");
        let db = rusqlite::Connection::open(&catalog).unwrap();
        db.execute_batch(concat!(
            "CREATE TABLE items(content_key TEXT, position INTEGER, phash INTEGER);",
            "INSERT INTO items VALUES ('z',2,7),('b',1,-1),('a',1,0);"
        ))
        .unwrap();
        db.close().unwrap();
        let before = std::fs::read(&catalog).unwrap();
        let mut environment = BTreeMap::new();
        for name in [
            "PATH",
            "SystemRoot",
            "SYSTEMROOT",
            "WINDIR",
            "LD_LIBRARY_PATH",
            "DYLD_LIBRARY_PATH",
            "DYLD_FALLBACK_LIBRARY_PATH",
            "PYTHONHOME",
        ] {
            if let Ok(value) = std::env::var(name) {
                environment.insert(name.into(), value);
            }
        }
        environment.insert("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into());
        environment.insert("PYTHONNOUSERSITE".into(), "1".into());
        environment.insert("PYTHONSAFEPATH".into(), "1".into());
        let environment = tooling_environment::environment(&scratch, &environment, false).unwrap();
        let log = RunLog::open(&scratch, "similarity-command-test", vec![]);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let cases = [
            (vec!["--help".to_owned()], 0, "--catalog"),
            (vec!["--unknown".to_owned()], 2, "unrecognized arguments"),
            (
                vec!["--catalog".to_owned(), catalog.to_str().unwrap().into()],
                0,
                "Exact parity: hashes=3",
            ),
            (
                vec![
                    "--catalog".to_owned(),
                    scratch.join("missing.db").to_str().unwrap().into(),
                ],
                1,
                "Verification failed:",
            ),
        ];
        for (args, code, expected) in cases {
            let mut argv = vec![
                OsString::from(env!("CARGO_BIN_EXE_numera-compat")),
                "benchmark-similarity".into(),
            ];
            argv.extend(args.into_iter().map(OsString::from));
            let actual = runtime
                .block_on(tooling_process::capture(
                    root,
                    &argv,
                    &environment,
                    Duration::from_secs(25),
                    Duration::from_secs(8),
                    &log,
                ))
                .unwrap();
            assert_eq!(actual.code, code, "{}", actual.stderr);
            assert!(
                format!("{}{}", actual.stdout, actual.stderr).contains(expected),
                "{}{}",
                actual.stdout,
                actual.stderr
            );
            assert_eq!(std::fs::read(&catalog).unwrap(), before);
            assert!(!scratch.join("missing.db").exists());
        }
        log.finish(0);
    }));
    // A failed command can retain a nested owner marker. Keep its input/diagnostics for recovery.
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    std::fs::remove_dir_all(&scratch).unwrap();
}
