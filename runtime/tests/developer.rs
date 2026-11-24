//! Contributor behavior over real Rust code, temporary catalogs and bounded native children.
use numera_runtime::developer::{self, Command, NpmCommand, Options, Step, Tools};
use numera_runtime::{logging::RunLog, tooling_environment, tooling_process};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Root(PathBuf);
impl Root {
    fn new(label: &str) -> Self {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let base = tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
        let path = base.join(format!(
            "developer {label} café {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(tooling_environment::physical_root(&path).unwrap())
    }
    fn file(&self, relative: &str, bytes: &[u8]) {
        let path = self.0.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("developer evidence retained: {}", self.0.display());
        } else {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}
fn tools() -> Tools {
    Tools {
        cargo: PathBuf::from("selected cargo"),
        rustc: PathBuf::from("selected rustc"),
        npm: Some(NpmCommand::Executable(PathBuf::from("selected npm"))),
        driver: PathBuf::from("selected numera-dev"),
    }
}
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn contributor_options_reject_unknown_flags_and_keep_explicit_suite_selection() {
    assert_eq!(
        developer::arguments(&strings(&["--help"])).unwrap(),
        Command::Help
    );
    assert_eq!(
        developer::arguments(&strings(&["build"])).unwrap(),
        Command::Build { release: true }
    );
    assert_eq!(
        developer::arguments(&strings(&["build", "--debug"])).unwrap(),
        Command::Build { release: false }
    );
    let Command::Check(options) = developer::arguments(&strings(&[
        "check",
        "--tests",
        "static_codec,video_codec",
        "--test",
        "tooling_standalone",
        "--skip-format",
        "--skip-lint",
        "--timeout-seconds=7",
    ]))
    .unwrap() else {
        panic!("check command expected")
    };
    assert_eq!(
        options.tests,
        ["static_codec", "video_codec", "tooling_standalone"]
    );
    assert!(!options.format);
    assert!(!options.lint);
    assert_eq!(options.timeout, Duration::from_secs(7));
    for values in [
        vec!["build", "--install-maturin"],
        vec!["test", "--test", "../foreign"],
        vec!["test", "--tests", ""],
        vec!["test", "--test-threads=8"],
        vec!["check", "--timeout-seconds=0"],
        vec!["check", "--timeout-seconds", "bad"],
        vec!["benchmark-similarity", "--count=0"],
        vec!["benchmark-similarity", "--repeats=2"],
        vec!["benchmark-similarity", "--catalog"],
    ] {
        assert!(
            developer::arguments(&strings(&values)).is_err(),
            "accepted {values:?}"
        );
    }
}

#[test]
fn original_check_subset_skip_and_timeout_keep_the_tests_package_guard() {
    let root = Root::new("guard plan");
    let Command::Check(options) = developer::arguments(&strings(&[
        "check",
        "--test",
        "static_codec",
        "--skip-format",
        "--skip-lint",
        "--timeout-seconds=7",
    ]))
    .unwrap() else {
        panic!("check expected")
    };
    let steps = developer::plan(&root.0, &Command::Check(options), &tools()).unwrap();
    assert_eq!(steps.len(), 2);
    assert_eq!(
        steps[0].argv,
        strings(&[
            "selected cargo",
            "build",
            "--manifest-path",
            "runtime/Cargo.toml",
            "--locked",
            "--release",
            "--bin",
            "numera-runtime-tests"
        ])
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>()
    );
    assert_eq!(
        steps[1].argv,
        strings(&[
            "selected cargo",
            "test",
            "--config",
            ".cargo/runtime-tests.toml",
            "--manifest-path",
            "runtime/Cargo.toml",
            "--locked",
            "--test",
            "static_codec",
            "--",
            "--test-threads=1"
        ])
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>()
    );
    for step in steps {
        assert_eq!(step.wall, Duration::from_secs(7));
        assert!(step.idle > Duration::ZERO && step.idle <= step.wall);
    }
}

#[test]
fn check_plan_runs_rustfmt_clippy_and_available_browser_and_worker_checks() {
    let root = Root::new("web plan");
    root.file("assets/node_modules/@playwright/test/package.json", b"{}");
    root.file("worker/node_modules/typescript/package.json", b"{}");
    root.file("worker/node_modules/vitest/package.json", b"{}");
    let steps = developer::plan(&root.0, &Command::Check(Options::default()), &tools()).unwrap();
    assert_eq!(steps.len(), 8);
    let argv = |index: usize| {
        steps[index]
            .argv
            .iter()
            .map(|value| value.to_str().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        argv(0),
        [
            "selected cargo",
            "fmt",
            "--manifest-path",
            "runtime/Cargo.toml",
            "--",
            "--check"
        ]
    );
    assert_eq!(
        argv(1),
        [
            "selected cargo",
            "clippy",
            "--manifest-path",
            "runtime/Cargo.toml",
            "--locked",
            "--all-targets",
            "--",
            "-D",
            "warnings"
        ]
    );
    assert_eq!(
        argv(4),
        ["selected npm", "--prefix", "worker", "run", "typecheck"]
    );
    let config = root.0.join("assets/jsconfig.json");
    assert_eq!(
        argv(5),
        [
            "selected npm",
            "--prefix",
            "worker",
            "exec",
            "--",
            "tsc",
            "-p",
            config.to_str().unwrap(),
            "--noEmit"
        ]
    );
    assert_eq!(argv(6), ["selected npm", "--prefix", "worker", "test"]);
    assert_eq!(argv(7), ["selected npm", "--prefix", "assets", "test"]);
    let mut unavailable = tools();
    unavailable.npm = None;
    assert_eq!(
        developer::plan(&root.0, &Command::Check(Options::default()), &unavailable)
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn contributor_build_worker_limit_keeps_the_original_positive_environment_ceiling() {
    assert_eq!(developer::worker_limit(None, &BTreeMap::new()), 1);
    assert_eq!(developer::worker_limit(Some(64), &BTreeMap::new()), 4);
    let inputs = BTreeMap::from([
        ("CARGO_BUILD_JOBS".into(), "2".into()),
        ("HOOKMAKER_MAX_TEST_WORKERS".into(), "1".into()),
    ]);
    assert_eq!(developer::worker_limit(Some(64), &inputs), 1);
    let invalid = BTreeMap::from([
        ("CARGO_BUILD_JOBS".into(), "-1".into()),
        ("HOOKMAKER_MAX_TEST_WORKERS".into(), "garbage".into()),
    ]);
    assert_eq!(developer::worker_limit(Some(8), &invalid), 4);
}

#[test]
fn contributor_environment_scrubs_operator_credentials_and_preserves_installed_tools() {
    let root = Root::new("scrub");
    let input = BTreeMap::from([
        ("PATH".into(), "installed tools".into()),
        ("RUSTUP_HOME".into(), "installed toolchains".into()),
        ("GENERAL_BOT_TOKEN".into(), "synthetic credential".into()),
        ("PYO3_PYTHON".into(), "invalid interpreter".into()),
    ]);
    let before = input.clone();
    let env = developer::environment(&root.0, &input).unwrap();
    assert_eq!(env["PATH"], "installed tools");
    assert_eq!(env["RUSTUP_HOME"], "installed toolchains");
    assert_eq!(env["RUSTUP_TOOLCHAIN"], "1.100.0");
    assert_eq!(env["RUSTUP_AUTO_INSTALL"], "0");
    assert_eq!(env["NUMERA_EMOJI_MAPPER_NO_DOTENV"], "1");
    assert_eq!(env["TELEGRAM_API_BASE"], "http://127.0.0.1:9");
    assert_eq!(
        env["CARGO_HOME"],
        root.0.join(".cache/cargo").to_str().unwrap()
    );
    assert_eq!(
        env["CARGO_TARGET_DIR"],
        root.0.join("runtime/target").to_str().unwrap()
    );
    assert!(!env.contains_key("GENERAL_BOT_TOKEN"));
    assert!(!env.contains_key("PYO3_PYTHON"));
    assert_eq!(input, before);
}

#[test]
fn catalog_input_is_read_only_ordered_masked_and_closed_even_on_failure() {
    let root = Root::new("catalog");
    let path = root.0.join("catalog café # literal%20.db");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT,position INTEGER,phash); INSERT INTO items VALUES('z',2,7),('b',1,-1),('a',1,NULL),('a',1,'18446744073709551616'),('c',1,3.75);").unwrap();
    db.close().unwrap();
    let before = std::fs::read(&path).unwrap();
    assert_eq!(developer::load_hashes(&path).unwrap(), [0, u64::MAX, 3, 7]);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 1);
    let moved = root.0.join("moved.db");
    std::fs::rename(&path, &moved).unwrap();
    let db = rusqlite::Connection::open(&moved).unwrap();
    db.execute(
        "UPDATE items SET phash='not an integer' WHERE content_key='z'",
        [],
    )
    .unwrap();
    db.close().unwrap();
    let before = std::fs::read(&moved).unwrap();
    assert!(
        developer::load_hashes(&moved)
            .unwrap_err()
            .contains("invalid integer")
    );
    assert_eq!(std::fs::read(&moved).unwrap(), before);
    std::fs::rename(&moved, &path).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DELETE FROM items; INSERT INTO items VALUES ('negative real',0,-3.75),('wide real',1,1.8446744073709552e19),('decimal blob',2,X'2D335F30'),('wide text',3,'+18446744073709551623');").unwrap();
    db.close().unwrap();
    let numeric_bytes = std::fs::read(&path).unwrap();
    assert_eq!(
        developer::load_hashes(&path).unwrap(),
        [u64::MAX - 2, 0, u64::MAX - 29, 7]
    );
    assert_eq!(std::fs::read(&path).unwrap(), numeric_bytes);
    assert!(developer::load_hashes(&root.0.join("missing.db")).is_err());
    assert!(!root.0.join("missing.db").exists());
}

#[test]
fn catalog_cardinality_refuses_before_similarity_computation() {
    let root = Root::new("cardinality");
    let path = root.0.join("bounded.db");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT,position INTEGER,phash);")
        .unwrap();
    assert!(developer::load_hashes(&path).is_err());
    db.execute_batch("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<10001) INSERT INTO items SELECT printf('%05d',x),x,x FROM n;").unwrap();
    db.close().unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(
        developer::load_hashes(&path)
            .unwrap_err()
            .contains("1..10000")
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn five_sample_medians_report_rust_timing_without_claiming_source_speedup() {
    let result = developer::benchmark(&[0, 3, 1, 2], 5).unwrap();
    assert_eq!(result.order, [0, 2, 1, 3]);
    assert_eq!(result.hashes, 4);
    assert_eq!(result.samples, 5);
    assert_eq!(result.candidates, 20);
    assert!(result.median_secs.is_finite() && result.median_secs >= 0.0);
    assert!(
        result
            .line()
            .starts_with("Rust similarity: hashes=4 samples=5 median=")
    );
    assert!(!result.line().contains("speedup"));
    assert_eq!(developer::median(&[9.0, 1.0, 2.0, 8.0, 3.0]).unwrap(), 3.0);
    for samples in [vec![], vec![f64::NAN], vec![-1.0]] {
        assert!(developer::median(&samples).is_err());
    }
    assert!(developer::benchmark(&[], 5).is_err());
    assert!(developer::benchmark(&[0], 2).is_err());
    assert_eq!(
        developer::generated_hashes(3, 0).unwrap(),
        [0xe220a8397b1dcdaf, 0x6e789e6aa1b965f4, 0x06c45d188009454f]
    );
    assert_eq!(
        developer::arguments(&strings(&["benchmark-similarity"])).unwrap(),
        Command::Benchmark {
            catalog: None,
            count: 1500,
            seed: 321,
            repeats: 5
        }
    );
}

#[test]
fn developer_child_fixture() {
    let Ok(value) = std::env::var("NUMERA_DEV_FIXTURE_EXIT") else {
        return;
    };
    println!("bounded native fixture");
    std::process::exit(value.parse().unwrap());
}

#[test]
fn developer_second_fixture() {
    let Ok(path) = std::env::var("NUMERA_DEV_FIXTURE_MARKER") else {
        return;
    };
    std::fs::write(path, b"second step ran").unwrap();
}

#[test]
fn contributor_execution_preserves_failure_exit_and_never_starts_the_next_step() {
    let root = Root::new("fail stop");
    let marker = root.0.join("second-step");
    let fixture = |name: &str| Step {
        label: "bounded fixture".into(),
        argv: vec![
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            name.into(),
            "--nocapture".into(),
            "--test-threads=1".into(),
        ],
        wall: Duration::from_secs(8),
        idle: Duration::from_secs(4),
    };
    let mut env = BTreeMap::from([("NUMERA_DEV_FIXTURE_EXIT".into(), "7".into())]);
    env.insert("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into());
    if let Ok(marker) = std::env::var("NUMERA_RUNTIME_TEST_ADMISSION") {
        env.insert("NUMERA_RUNTIME_TEST_ADMISSION".into(), marker);
    }
    #[cfg(windows)]
    env.insert("SystemRoot".into(), std::env::var("SystemRoot").unwrap());
    let log = RunLog::open(&root.0, "developer-fail-stop", vec![]);
    // A second actual command would create a marker; the first failure must stop it.
    env.insert(
        "NUMERA_DEV_FIXTURE_MARKER".into(),
        marker.to_str().unwrap().into(),
    );
    let code = runtime()
        .block_on(developer::execute(
            &root.0,
            &[
                fixture("developer_child_fixture"),
                fixture("developer_second_fixture"),
            ],
            &env,
            &log,
        ))
        .unwrap();
    assert_eq!(code, 7);
    assert!(!marker.exists());
    assert_eq!(
        std::fs::read_dir(root.0.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    drop(log);
}

#[test]
fn contributor_builder_help_and_missing_compiler_are_real_bounded_commands() {
    let root = Root::new("CLI");
    root.file(
        "runtime/Cargo.toml",
        b"[package]\nname='fixture'\nversion='0.0.0'\n",
    );
    let binary = root.0.join(if cfg!(windows) {
        "numera-dev.exe"
    } else {
        "numera-dev"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-dev"), &binary).unwrap();
    let env = BTreeMap::from([
        ("PATH".into(), "".into()),
        ("PYO3_PYTHON".into(), "invalid interpreter".into()),
        ("PYTHONHOME".into(), "invalid interpreter".into()),
    ]);
    #[cfg(windows)]
    let env = {
        let mut env = env;
        env.insert("SystemRoot".into(), std::env::var("SystemRoot").unwrap());
        env
    };
    let log = RunLog::open(&root.0, "developer-cli-test", vec![]);
    for (args, code, text) in [
        (vec!["--help"], 0, "benchmark-similarity"),
        (vec!["build"], 2, "Rust 1.100.0"),
        (
            vec!["__execute", "benchmark-similarity", "--count=4"],
            2,
            "requires an owned numera-dev invocation",
        ),
        (
            vec!["benchmark-similarity", "--count=4", "--seed=0"],
            0,
            "Rust similarity: hashes=4 samples=5",
        ),
    ] {
        let mut argv = vec![binary.clone().into_os_string()];
        argv.extend(args.iter().map(OsString::from));
        let result = runtime()
            .block_on(tooling_process::capture(
                &root.0,
                &argv,
                &env,
                Duration::from_secs(15),
                Duration::from_secs(8),
                &log,
            ))
            .unwrap();
        assert_eq!(result.code, code, "{}{}", result.stdout, result.stderr);
        assert!(format!("{}{}", result.stdout, result.stderr).contains(text));
    }
    assert!(!root.0.join(".venv").exists());
    assert!(!root.0.join("native").exists());
    drop(log);
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
