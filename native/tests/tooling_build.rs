//! Contributor plan bounds and worker ceilings; no Cargo process or dependency installation.
use _native::tooling_build;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

#[test]
fn contributor_builder_requires_explicit_dependency_consent_and_a_separate_driver() {
    assert!(!tooling_build::arguments(&[]).unwrap());
    assert!(tooling_build::arguments(&["--install-maturin".into()]).unwrap());
    for arguments in [
        vec!["--apply".into()],
        vec!["--install-maturin".into(), "extra".into()],
    ] {
        assert!(tooling_build::arguments(&arguments).is_err());
    }
    let root = std::env::current_dir().unwrap();
    for path in [
        "native/runtime/numera-emoji",
        "native/target/release/numera-emoji",
        "native/target/release/numera-emoji.exe",
    ] {
        assert!(tooling_build::driver_location(&root, &root.join(path)).is_err());
    }
    assert!(
        tooling_build::driver_location(&root, &root.join("native/target/debug/numera-build"))
            .is_ok()
    );
}

#[test]
fn contributor_builder_help_and_missing_compiler_are_real_bounded_commands() {
    use _native::{logging::RunLog, tooling_environment, tooling_process};
    use std::ffi::OsString;
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture = tooling_fixture::Root::new("native builder café");
    let root = fixture.0.clone();
    std::fs::create_dir(root.join("assets")).unwrap();
    for name in ["assets/panel.html", "pyproject.toml"] {
        std::fs::copy(project.join(name), root.join(name)).unwrap();
    }
    let binary = root.join(if cfg!(windows) {
        "numera-build.exe"
    } else {
        "numera-build"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-build"), &binary).unwrap();
    let original = std::env::vars().collect::<BTreeMap<_, _>>();
    let mut environment = _native::tooling_runtime::environment(&original);
    // Keep loader prerequisites, but put no compiler on the child's PATH.
    let python = Path::new(&std::env::var("PYO3_PYTHON").unwrap_or_default()).to_path_buf();
    let loader = if python.is_absolute() {
        python.parent().unwrap().to_path_buf()
    } else {
        _native::codec_process::executable("python")
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    };
    environment.insert("PATH".into(), loader.to_str().unwrap().into());
    let environment = tooling_environment::environment(&root, &environment, false).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let log = RunLog::open(&root, "native-builder-test", vec![]);
    for (args, code, text) in [
        (vec!["--help"], 0, "numera-build"),
        (vec![], 2, "Rust 1.99+"),
    ] {
        let mut argv = vec![binary.clone().into_os_string()];
        argv.extend(args.into_iter().map(OsString::from));
        let result = runtime
            .block_on(tooling_process::capture(
                &root,
                &argv,
                &environment,
                Duration::from_secs(8),
                Duration::from_secs(4),
                &log,
            ))
            .unwrap();
        assert_eq!(result.code, code, "{}{}", result.stdout, result.stderr);
        assert!(format!("{}{}", result.stdout, result.stderr).contains(text));
        assert!(!root.join(".venv").exists());
        assert!(!root.join("native/target").exists());
        assert!(!root.join("native/runtime").exists());
    }
    let builder_logs = std::fs::read_dir(root.join("logs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("build_native_") && name.ends_with(".log"))
        })
        .collect::<Vec<_>>();
    assert_eq!(builder_logs.len(), 1);
    let diagnostic = std::fs::read_to_string(&builder_logs[0]).unwrap();
    assert!(diagnostic.contains("[ERROR] [build] Rust 1.99+"));
    assert!(diagnostic.contains("exit=2"));
    let retained = [
        ".venv/pyvenv.cfg",
        "native/runtime/retained-application",
        "emojikit/retained-extension",
    ];
    for name in retained {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"existing installation fixture").unwrap();
    }
    // A file named cargo without the compiler toolchain must not start any child.
    let tools = root.join("tools");
    std::fs::create_dir(&tools).unwrap();
    std::fs::copy(
        &binary,
        tools.join(if cfg!(windows) { "cargo.exe" } else { "cargo" }),
    )
    .unwrap();
    let mut guarded_environment = environment.clone();
    guarded_environment.insert(
        "PATH".into(),
        std::env::join_paths([tools, loader])
            .unwrap()
            .into_string()
            .unwrap(),
    );
    for args in [vec![], vec!["--install-maturin"]] {
        let mut argv = vec![binary.clone().into_os_string()];
        argv.extend(args.into_iter().map(OsString::from));
        let result = runtime
            .block_on(tooling_process::capture(
                &root,
                &argv,
                &guarded_environment,
                Duration::from_secs(8),
                Duration::from_secs(4),
                &log,
            ))
            .unwrap();
        assert_eq!(result.code, 1, "{}{}", result.stdout, result.stderr);
        assert!(
            result
                .stderr
                .contains("installed compiler directory is missing rustc")
        );
        assert!(!result.stderr.contains("Maturin 1.15.0 is required"));
        for name in retained {
            assert_eq!(
                std::fs::read(root.join(name)).unwrap(),
                b"existing installation fixture"
            );
        }
        assert!(!root.join("native/target").exists());
        assert!(!root.join(".cache/rustup").exists());
        assert!(!root.join(".cache/cargo").exists());
    }
    let failed = tooling_build::Step {
        argv: vec![binary.into_os_string(), "--invalid".into()],
        wall: Duration::from_secs(8),
        idle: Duration::from_secs(4),
    };
    let must_not_start = tooling_build::Step {
        argv: vec![],
        wall: Duration::from_secs(1),
        idle: Duration::from_secs(1),
    };
    assert_eq!(
        runtime
            .block_on(tooling_build::execute(
                &root,
                &[failed, must_not_start],
                &environment,
                &log
            ))
            .unwrap(),
        1
    );
    let diagnostic = std::fs::read_to_string(log.path.as_ref().unwrap()).unwrap();
    assert!(diagnostic.contains("build step failed exit=1"));
    assert!(!diagnostic.contains("numera-build accepts only --install-maturin"));
    assert_eq!(
        std::fs::read_dir(root.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    drop(log);
    fixture.clean().unwrap();
}

#[test]
fn build_worker_ceiling_preserves_source_positive_overrides() {
    for (cores, hook, cargo, expected) in [
        (None, None, None, 1),
        (Some(16), None, None, 4),
        (Some(8), Some("2"), Some("3"), 2),
        (Some(1), Some("0"), Some("-1"), 1),
        (Some(8), Some("bad"), Some(" 1 "), 1),
    ] {
        let mut environment = BTreeMap::new();
        if let Some(value) = hook {
            environment.insert("HOOKMAKER_MAX_TEST_WORKERS".into(), value.into());
        }
        if let Some(value) = cargo {
            environment.insert("CARGO_BUILD_JOBS".into(), value.into());
        }
        assert_eq!(tooling_build::worker_limit(cores, &environment), expected);
    }
}

#[test]
fn source_locked_build_commands_are_explicit_and_finite() {
    let steps =
        tooling_build::wheel_steps(Path::new("root with spaces"), Path::new("python"), true);
    assert_eq!(steps.len(), 1);
    assert_eq!(
        &steps[0].argv[..10],
        [
            "python",
            "-I",
            "-B",
            "-X",
            "utf8",
            "-m",
            "maturin",
            "build",
            "--release",
            "--locked"
        ]
        .map(std::ffi::OsString::from)
    );
    assert!(steps[0].argv.windows(2).any(|pair| {
        pair == [
            std::ffi::OsString::from("--interpreter"),
            std::ffi::OsString::from("python"),
        ]
    }));
    assert_eq!(steps[0].wall, Duration::from_secs(1800));
    assert!(steps[0].idle < steps[0].wall);
    let bootstrap = tooling_build::wheel_steps(Path::new("root"), Path::new("python"), false);
    assert_eq!(bootstrap.len(), 2);
    assert_eq!(bootstrap[0].argv.last().unwrap(), "maturin==1.15.0");
    assert_eq!(
        &bootstrap[0].argv[1..5],
        ["-I", "-B", "-X", "utf8"].map(std::ffi::OsString::from)
    );
    assert_eq!(bootstrap[0].wall, Duration::from_secs(180));
    assert!(bootstrap[0].idle < bootstrap[0].wall);
    assert_eq!(
        tooling_build::executable_step(Path::new("root"))
            .argv
            .last()
            .unwrap(),
        "numera-emoji"
    );
}
