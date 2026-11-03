//! Physical installed copies and fresh-environment refusal; no runtime activation or compiler.
use _native::tooling_install;
use std::path::Path;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

#[test]
fn failed_copy_and_repeat_environment_preserve_prior_installed_bytes() {
    let root = Root::new("tooling install");
    let source = root.0.join("source executable");
    std::fs::write(&source, b"independent bytes fixture").unwrap();
    let installed = tooling_install::install_executable(&root.0, &source).unwrap();
    assert_eq!(installed.parent().unwrap(), root.0.join("native/runtime"));
    assert_eq!(
        std::fs::read(&installed).unwrap(),
        b"independent bytes fixture"
    );
    std::fs::write(&source, b"verified replacement bytes").unwrap();
    tooling_install::install_executable(&root.0, &source).unwrap();
    assert_eq!(
        std::fs::read(&installed).unwrap(),
        b"verified replacement bytes"
    );
    std::fs::write(&source, b"").unwrap();
    assert!(tooling_install::install_executable(&root.0, &source).is_err());
    assert_eq!(
        std::fs::read(&installed).unwrap(),
        b"verified replacement bytes"
    );
    std::fs::remove_file(&source).unwrap();
    assert!(tooling_install::install_executable(&root.0, &source).is_err());
    assert_eq!(
        std::fs::read(&installed).unwrap(),
        b"verified replacement bytes"
    );
    assert_eq!(
        std::fs::read_dir(installed.parent().unwrap())
            .unwrap()
            .count(),
        1
    );
    std::fs::create_dir(root.0.join(".venv")).unwrap();
    std::fs::write(
        root.0.join(".venv/pyvenv.cfg"),
        b"previous working environment",
    )
    .unwrap();
    assert!(
        tooling_install::require_fresh_environment(&root.0)
            .unwrap_err()
            .contains("Existing installation preserved")
    );
    assert_eq!(
        std::fs::read(root.0.join(".venv/pyvenv.cfg")).unwrap(),
        b"previous working environment"
    );
    assert!(!root.0.join("logs").exists());
    assert_eq!(std::fs::read_dir(root.0.join(".venv")).unwrap().count(), 1);
    root.clean().unwrap();
}

/// Migrates NativeInstall.test_installed_binary_runs_after_source_removed_and_failed_replace_preserves_it.
#[test]
fn installed_executable_runs_without_build_source_and_preserves_bytes_on_refused_replacement() {
    use _native::{logging::RunLog, tooling_launch, tooling_manifest::digest, tooling_process};
    use std::ffi::OsString;
    use std::time::Duration;

    let root = Root::new("tooling install");
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let build = root.0.join("native/target/release");
    std::fs::create_dir_all(&build).unwrap();
    let source = build.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &source).unwrap();
    let source_digest = digest(&source).unwrap();
    let installed = tooling_install::install_executable(&root.0, &source).unwrap();
    assert_eq!(digest(&installed).unwrap(), source_digest);
    std::fs::remove_file(&source).unwrap();
    std::fs::remove_dir(&build).unwrap();
    std::fs::remove_dir(root.0.join("native/target")).unwrap();
    assert!(!root.0.join("native/target").exists());

    std::fs::create_dir(root.0.join("assets")).unwrap();
    for member in ["assets/panel.html", "pyproject.toml"] {
        std::fs::copy(project.join(member), root.0.join(member)).unwrap();
    }
    let environment = tooling_launch::scrubbed_environment(
        &std::env::vars().collect(),
        &std::fs::read_to_string(project.join(".env.example")).unwrap(),
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let log = RunLog::open(&root.0, "tooling-installed-test", vec![]);
    for (argument, expected_code, expected_output) in [
        ("--help", 0, "identity-repair"),
        ("status", 3, "STALE roster"),
    ] {
        let result = runtime
            .block_on(tooling_process::capture(
                &root.0,
                &[installed.clone().into_os_string(), OsString::from(argument)],
                &environment,
                Duration::from_secs(5),
                Duration::from_secs(3),
                &log,
            ))
            .unwrap();
        assert_eq!(result.code, expected_code, "{argument}: {}", result.stderr);
        assert!(result.stdout.contains(expected_output), "{}", result.stdout);
        assert!(!root.0.join("native/target").exists());
        assert!(!root.0.join("collection/catalog.db").exists());
    }
    let status_logs = std::fs::read_dir(root.0.join("logs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("status_") && name.ends_with(".log"))
        })
        .collect::<Vec<_>>();
    assert_eq!(status_logs.len(), 1);
    let text = std::fs::read_to_string(&status_logs[0]).unwrap();
    assert!(text.contains("[runtime] native backend started"));
    assert!(text.contains("exit=3"));

    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Preserve read/write sharing but deny delete/rename to exercise the real
        // occupied-destination failure, not a mock that bypasses staged copying.
        let occupied = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0x0000_0001 | 0x0000_0002)
            .open(&installed)
            .unwrap();
        let replacement = root.0.join("replacement source");
        std::fs::write(&replacement, b"replacement bytes").unwrap();
        assert!(tooling_install::install_executable(&root.0, &replacement).is_err());
        assert_eq!(digest(&installed).unwrap(), source_digest);
        drop(occupied);
    }
    assert!(tooling_install::install_executable(&root.0, &source).is_err());
    assert_eq!(digest(&installed).unwrap(), source_digest);
    assert_eq!(
        std::fs::read_dir(installed.parent().unwrap())
            .unwrap()
            .count(),
        1
    );
    assert_eq!(
        std::fs::read_dir(root.0.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    drop(log);
    root.clean().unwrap();
}

#[test]
fn extension_keeps_repaired_wheel_relative_libraries() {
    let root = Root::new("tooling install");
    let installed = root.0.join("installed/emojikit");
    std::fs::create_dir_all(&installed).unwrap();
    let source = installed.join("_native.abi3.so");
    std::fs::write(&source, b"fixture extension").unwrap();
    let libraries = installed.parent().unwrap().join("numera_emoji_core.libs");
    std::fs::create_dir(&libraries).unwrap();
    std::fs::write(
        libraries.join("lib-fixture.so"),
        b"fixture shared dependency",
    )
    .unwrap();
    let checkout = root.0.join("checkout");
    std::fs::create_dir(&checkout).unwrap();
    let copied = tooling_install::install_extension(&checkout, &source).unwrap();
    assert_eq!(std::fs::read(copied).unwrap(), b"fixture extension");
    assert_eq!(
        std::fs::read(checkout.join("numera_emoji_core.libs/lib-fixture.so")).unwrap(),
        b"fixture shared dependency"
    );
}

#[test]
fn prebuilt_plan_is_isolated_offline_and_has_no_first_party_python_entrypoint() {
    let root = Root::new("offline plan café");
    let python = root.0.join("base interpreter with spaces");
    let wheel = "numera_emoji_core-2.0.0-fixture.whl";
    let steps = _native::tooling_prebuilt::steps(&root.0, &python, wheel).unwrap();
    assert_eq!(steps.len(), 2);
    assert_eq!(
        steps[0].argv,
        [
            python.as_os_str().to_owned(),
            "-I".into(),
            "-B".into(),
            "-X".into(),
            "utf8".into(),
            "-m".into(),
            "venv".into(),
            root.0.join(".venv").into_os_string()
        ]
    );
    assert_eq!(steps[0].wall, std::time::Duration::from_secs(60));
    assert_eq!(steps[1].wall, std::time::Duration::from_secs(180));
    assert!(
        steps
            .iter()
            .all(|step| !step.idle.is_zero() && step.idle <= step.wall)
    );
    assert_eq!(
        &steps[1].argv[1..5],
        ["-I", "-B", "-X", "utf8"].map(std::ffi::OsString::from)
    );
    assert!(steps[1].argv.contains(&"--no-index".into()));
    assert!(steps[1].argv.contains(&"--only-binary=:all:".into()));
    assert_eq!(
        steps[1].argv.last().unwrap(),
        root.0.join("wheels").join(wheel).as_os_str()
    );
    for step in steps {
        assert!(
            step.argv
                .iter()
                .all(|arg| arg != "-c" && arg != "--system-site-packages" && arg != "cargo")
        );
    }
    assert!(!root.0.join(".venv").exists());
    for wheel in ["../other.whl", "--flag.whl", "name:stream.whl"] {
        assert!(_native::tooling_prebuilt::steps(&root.0, &python, wheel).is_err());
    }
    let original = [
        ("PIP_TARGET", "foreign"),
        ("PIP_INDEX_URL", "https://invalid.example"),
        ("PYTHONHOME", "foreign"),
        ("BOT_TOKEN", "synthetic"),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect();
    let environment = _native::tooling_prebuilt::environment(&root.0, &original).unwrap();
    for key in ["PIP_TARGET", "PIP_INDEX_URL", "PYTHONHOME", "BOT_TOKEN"] {
        assert!(!environment.contains_key(key));
    }
    assert_eq!(environment["PIP_NO_INPUT"], "1");
    assert_eq!(environment["PIP_NO_INDEX"], "1");
    assert!(std::path::Path::new(&environment["TEMP"]).starts_with(&root.0));
    assert!(!root.0.join(".venv").exists());
    root.clean().unwrap();
}

#[test]
fn prebuilt_cli_refuses_activation_before_creating_an_environment() {
    use _native::{logging::RunLog, tooling_process};
    use std::time::Duration;
    let root = Root::new("offline cli café");
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::fs::create_dir(root.0.join("assets")).unwrap();
    for name in ["assets/panel.html", "pyproject.toml"] {
        std::fs::copy(project.join(name), root.0.join(name)).unwrap();
    }
    let binary =
        tooling_install::install_executable(&root.0, Path::new(env!("CARGO_BIN_EXE_numera-emoji")))
            .unwrap();
    let original = std::env::vars().collect();
    let environment = _native::tooling_runtime::environment(&original);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let log = RunLog::open(&root.0, "prebuilt-cli-test", vec![]);
    for (args, code, expected) in [
        (vec!["--help"], 0, "install-prebuilt"),
        (vec!["--install"], 1, "accepts no arguments"),
        (vec![], 1, "approved signing and identity verification"),
    ] {
        let mut argv = vec![binary.clone().into_os_string(), "install-prebuilt".into()];
        argv.extend(args.into_iter().map(std::ffi::OsString::from));
        let result = runtime
            .block_on(tooling_process::capture(
                &root.0,
                &argv,
                &environment,
                Duration::from_secs(5),
                Duration::from_secs(3),
                &log,
            ))
            .unwrap();
        assert_eq!(result.code, code, "{}{}", result.stdout, result.stderr);
        assert!(format!("{}{}", result.stdout, result.stderr).contains(expected));
        assert!(!root.0.join(".venv").exists());
        assert!(!root.0.join(".cache").exists());
    }
    std::fs::create_dir(root.0.join(".venv")).unwrap();
    let prior = root.0.join(".venv/pyvenv.cfg");
    std::fs::write(&prior, b"previous working environment").unwrap();
    let before = _native::tooling_manifest::digest(&binary).unwrap();
    let refused = runtime
        .block_on(tooling_process::capture(
            &root.0,
            &[binary.clone().into_os_string(), "install-prebuilt".into()],
            &environment,
            Duration::from_secs(5),
            Duration::from_secs(3),
            &log,
        ))
        .unwrap();
    assert_eq!(refused.code, 1);
    assert!(
        refused
            .stderr
            .contains("approved signing and identity verification")
    );
    assert_eq!(
        std::fs::read(prior).unwrap(),
        b"previous working environment"
    );
    assert_eq!(_native::tooling_manifest::digest(&binary).unwrap(), before);
    assert!(!root.0.join(".cache").exists());
    assert_eq!(
        std::fs::read_dir(root.0.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    drop(log);
    root.clean().unwrap();
}

#[test]
fn installed_extension_locator_requires_one_physical_selected_environment_member() {
    let root = Root::new("installed extension café");
    let package = root.0.join(".venv/site/emojikit");
    std::fs::create_dir_all(&package).unwrap();
    let report = serde_json::json!({
        "platlib": root.0.join(".venv/site"),
        "extension_suffixes": [".abi3.so", ".pyd"]
    });
    assert!(tooling_install::installed_extension(&root.0, &report).is_err());
    let extension = package.join("_native.abi3.so");
    std::fs::write(&extension, b"locator-only fixture").unwrap();
    assert_eq!(
        tooling_install::installed_extension(&root.0, &report).unwrap(),
        extension
    );
    std::fs::write(package.join("_native.pyd"), b"ambiguous fixture").unwrap();
    assert!(tooling_install::installed_extension(&root.0, &report).is_err());
    std::fs::remove_file(package.join("_native.pyd")).unwrap();
    std::fs::remove_file(&extension).unwrap();
    std::fs::create_dir(&extension).unwrap();
    assert!(tooling_install::installed_extension(&root.0, &report).is_err());
    let outside = serde_json::json!({"platlib": root.0, "extension_suffixes": [".abi3.so"]});
    assert!(tooling_install::installed_extension(&root.0, &outside).is_err());
    assert!(!root.0.join("emojikit").exists());
    root.clean().unwrap();
}

#[test]
fn offline_install_never_compiles_or_reads_an_index() {
    let argv = tooling_install::offline_install_arguments(
        Path::new("package with spaces"),
        "numera_emoji_core-2.0.0-fixture.whl",
    )
    .unwrap();
    assert_eq!(
        &argv[..6],
        [
            "-m",
            "pip",
            "install",
            "--no-index",
            "--only-binary=:all:",
            "--find-links"
        ]
    );
    assert_eq!(
        argv.last().unwrap(),
        &Path::new("package with spaces/wheels/numera_emoji_core-2.0.0-fixture.whl")
            .to_string_lossy()
    );
    for wheel in ["../outside.whl", "name:stream.whl", "source.tar.gz"] {
        assert!(tooling_install::offline_install_arguments(Path::new("root"), wheel).is_err());
    }
}
