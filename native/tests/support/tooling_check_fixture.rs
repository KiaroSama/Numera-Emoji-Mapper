//! Original guard assertions through the real check command, sharing its caller's venv and owner.
use _native::{logging::RunLog, tooling_process};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

pub fn assert_original_subset(
    project: &Path,
    root: &Path,
    base_runtime: &Path,
    binary: &Path,
    environment: &BTreeMap<String, String>,
    runtime: &tokio::runtime::Runtime,
    log: &RunLog,
) {
    // Darwin still refuses descendant custody. This caller branch never represents an OS skip as parity.
    assert!(cfg!(any(windows, target_os = "linux")));
    let tests = root.join("tests");
    std::fs::create_dir(&tests).unwrap();
    for (name, expected) in [
        (
            "__init__.py",
            "e7daf7810ca29292f5d2e798566241821f7b9b6f55644c9da1b12034ce3f404c",
        ),
        (
            "test_entry_points.py",
            "60dc4ff3b31f1c05a4118721ed52674985a7d7b18b4c47e72729b6f083a5620f",
        ),
    ] {
        let source = std::fs::read_to_string(project.join("tests").join(name)).unwrap();
        let source = source.replace("\r\n", "\n");
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(source.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(hash, expected, "original guard source drift: {name}");
        std::fs::write(tests.join(name), source.as_bytes()).unwrap();
    }
    let scripts = root.join(if cfg!(windows) {
        ".venv/Scripts"
    } else {
        ".venv/bin"
    });
    let mut child_environment = environment.clone();
    child_environment.insert("PATH".into(), scripts.to_str().unwrap().into());
    #[cfg(windows)]
    {
        // Keep only the linked CPython loader location, not inherited Git/ruff/compiler PATH entries.
        let python = base_runtime.join("python.exe");
        assert!(python.is_file());
        for name in ["git.exe", "ruff.exe", "cargo.exe"] {
            assert!(
                !base_runtime.join(name).exists(),
                "unexpected tool in Python loader directory"
            );
        }
        child_environment.insert(
            "PATH".into(),
            std::env::join_paths([scripts.as_path(), base_runtime])
                .unwrap()
                .into_string()
                .unwrap(),
        );
    }
    #[cfg(not(windows))]
    assert!(base_runtime.is_absolute());
    child_environment.insert("NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS".into(), "1".into());
    // Deliberately wrong input cannot select a foreign tests package; the command owns this binding.
    child_environment.insert(
        "PYTHONPATH".into(),
        root.join("foreign-imports").to_str().unwrap().into(),
    );
    let initializer = root.join("emojikit/__init__.py");
    let original_initializer = std::fs::read(&initializer).unwrap();
    // The first refusal keeps this caller's invalid initializer; find_spec must import the parent.
    let prefix = "test_entry_points.SuiteIsHermetic.";
    let selected = [
        "test_the_guard_was_installed_before_the_test_modules",
        "test_outbound_connections_are_refused",
        "test_loopback_still_works",
    ]
    .map(|name| format!("{prefix}{name}"))
    .join(",");
    let interpreter = scripts.join(if cfg!(windows) {
        "python.exe"
    } else {
        "python"
    });
    let alias = scripts.join(if cfg!(windows) {
        "check-python.exe"
    } else {
        "check-python"
    });
    assert!(!alias.exists());
    // Preserve the actual venv entry; an ignored override cannot fall back to the default name.
    std::fs::rename(&interpreter, &alias).unwrap();
    let config = root.join(".venv/pyvenv.cfg");
    let original_config = std::fs::read(&config).unwrap();
    let mut initializer_refused = false;
    for (suite, expected_code, expected_text) in [
        (
            "test_entry_points.SuiteIsHermetic.test_nonexistent_selection",
            2,
            "check prerequisite",
        ),
        (selected.as_str(), 0, "Ran 3 tests"),
        (
            "test_entry_points.SuiteIsHermetic.test_nonexistent_selection",
            1,
            "FAILED (errors=1)",
        ),
    ] {
        let mut argv = vec![
            binary.as_os_str().to_owned(),
            "check".into(),
            "--skip-compile".into(),
            "--skip-lint".into(),
            "--timeout-seconds=10".into(),
            OsString::from(format!("--tests={suite}")),
        ];
        // Reuse both existing invocations: absolute selection then borrowed-PATH name selection.
        argv.extend([
            "--python".into(),
            if expected_code == 0 {
                alias.clone().into_os_string()
            } else {
                "check-python".into()
            },
        ]);
        let observed = runtime
            .block_on(tooling_process::capture(
                root,
                &argv,
                &child_environment,
                Duration::from_secs(30),
                Duration::from_secs(8),
                log,
            ))
            .unwrap();
        assert_eq!(
            observed.code, expected_code,
            "{}{}",
            observed.stdout, observed.stderr
        );
        assert!(
            observed.stderr.contains(expected_text),
            "{}{}",
            observed.stdout,
            observed.stderr
        );
        if expected_code == 2 {
            assert!(!observed.stdout.contains("==> Unit tests"));
            assert!(
                !observed
                    .stdout
                    .contains("All retained source checks passed")
            );
            assert_eq!(std::fs::read(&initializer).unwrap(), original_initializer);
            assert_eq!(std::fs::read(&config).unwrap(), original_config);
            assert_eq!(
                std::fs::read_dir(root.join("logs/tmp/tooling"))
                    .unwrap()
                    .count(),
                0
            );
            std::fs::write(&initializer, b"").unwrap();
            initializer_refused = true;
            continue;
        }
        assert!(initializer_refused);
        assert_eq!(observed.stdout.matches("==> Unit tests").count(), 1);
        assert!(!observed.stdout.contains("Byte-compile"));
        assert!(!observed.stdout.contains("Lint (ruff)"));
        assert_eq!(
            observed
                .stdout
                .contains("All retained source checks passed"),
            expected_code == 0
        );
        assert!(!root.join("native/target").exists());
        assert!(!root.join("collection/catalog.db").exists());
    }
    assert!(!interpreter.exists());
    assert_eq!(std::fs::read(&config).unwrap(), original_config);
    std::fs::rename(&alias, &interpreter).unwrap();
    std::fs::write(&initializer, &original_initializer).unwrap();
    for name in ["__init__.py", "test_entry_points.py"] {
        assert!(
            !tests.join("__pycache__").exists(),
            "subset must not leave bytecode"
        );
        assert_eq!(
            std::fs::read(tests.join(name)).unwrap(),
            std::fs::read_to_string(project.join("tests").join(name))
                .unwrap()
                .replace("\r\n", "\n")
                .into_bytes()
        );
    }
    assert_eq!(
        std::fs::read_dir(root.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
}
