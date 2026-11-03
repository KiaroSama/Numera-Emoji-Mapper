//! Selected interpreter evidence uses a real disposable venv and the actual copied native command.
use _native::{logging::RunLog, tooling_process, tooling_runtime};
use serde_json::Value;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;
#[path = "support/tooling_check_fixture.rs"]
mod tooling_check_fixture;

#[test]
fn runtime_probe_environment_drops_import_overrides_credentials_and_oracle_selectors() {
    let original = [
        ("PATH", "runtime path"),
        ("LD_LIBRARY_PATH", "runtime libraries"),
        ("PYTHONHOME", "wrong home"),
        ("PYTHONPATH", "untrusted imports"),
        ("__PYVENV_LAUNCHER__", "another interpreter"),
        ("PYTHONSTARTUP", "untrusted startup"),
        ("VIRTUAL_ENV", "another environment"),
        ("BOT_TOKEN", "synthetic-token"),
        ("NUMERA_STATIC_CONVERT_ORACLE", "unrelated oracle"),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), value.into()))
    .collect::<BTreeMap<_, _>>();
    let before = original.clone();
    let actual = tooling_runtime::environment(&original);
    assert_eq!(actual.len(), 7);
    assert_eq!(actual["PATH"], "runtime path");
    assert_eq!(actual["LD_LIBRARY_PATH"], "runtime libraries");
    for key in [
        "NUMERA_EMOJI_MAPPER_NO_DOTENV",
        "PYTHONUTF8",
        "PYTHONNOUSERSITE",
        "PYTHONSAFEPATH",
        "PYTHONDONTWRITEBYTECODE",
    ] {
        assert_eq!(actual[key], "1");
    }
    for key in [
        "PYTHONHOME",
        "PYTHONPATH",
        "__PYVENV_LAUNCHER__",
        "PYTHONSTARTUP",
        "VIRTUAL_ENV",
        "BOT_TOKEN",
        "NUMERA_STATIC_CONVERT_ORACLE",
    ] {
        assert!(!actual.contains_key(key), "inherited {key}");
    }
    assert_eq!(original, before);
}

#[test]
fn copied_native_probe_selects_real_venv_without_python_application_or_build_tree() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let parent =
        _native::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
    let scratch = parent.join(format!("runtime selection café {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let scratch = _native::tooling_environment::physical_root(&scratch).unwrap();
    std::fs::create_dir(scratch.join("assets")).unwrap();
    for name in ["assets/panel.html", "pyproject.toml"] {
        std::fs::copy(project.join(name), scratch.join(name)).unwrap();
    }
    let binary = _native::tooling_install::install_executable(
        &scratch,
        Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
    )
    .unwrap();
    let original = std::env::vars().collect::<BTreeMap<_, _>>();
    let mut environment = tooling_runtime::environment(&original);
    environment.insert("PYTHONHOME".into(), "deliberately-wrong-home".into());
    environment.insert("PYTHONPATH".into(), "deliberately-wrong-imports".into());
    let log = RunLog::open(&scratch, "runtime-selection-test", vec![]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let invoke = |arguments: &[&str]| {
        let mut argv = vec![binary.clone().into_os_string()];
        argv.extend(arguments.iter().map(|argument| OsString::from(*argument)));
        runtime
            .block_on(tooling_process::capture(
                &scratch,
                &argv,
                &environment,
                Duration::from_secs(if arguments.first() == Some(&"doctor") {
                    45
                } else {
                    30
                }),
                Duration::from_secs(8),
                &log,
            ))
            .unwrap()
    };
    let help = invoke(&["runtime-check", "--help"]);
    assert_eq!(help.code, 0, "{}", help.stderr);
    assert!(help.stdout.contains("runtime-check"));
    let missing = invoke(&["runtime-check"]);
    assert_eq!(missing.code, 1);
    assert!(
        missing.stderr.contains("package .venv"),
        "{}",
        missing.stderr
    );
    assert!(!scratch.join(".venv").exists());
    let menu_help = invoke(&["menu", "--help"]);
    assert_eq!(menu_help.code, 0, "{}", menu_help.stderr);
    assert!(menu_help.stdout.contains("Usage: numera-emoji menu"));
    let check_help = invoke(&["check", "--help"]);
    assert_eq!(check_help.code, 0);
    assert!(check_help.stdout.contains("hermetic unittest discovery"));
    for flag in [
        "--tests",
        "--skip-compile",
        "--skip-lint",
        "--timeout-seconds",
        "--python",
    ] {
        assert!(check_help.stdout.contains(flag), "missing {flag}");
    }
    for (arguments, expected) in [
        (
            vec![
                "check",
                "--tests=test_catalog",
                "--skip-compile",
                "--skip-lint",
                "--timeout-seconds=7",
            ],
            "package .venv",
        ),
        (vec!["check", "--tests=../foreign.py"], "dotted test names"),
        (
            vec!["check", "--timeout-seconds=0"],
            "positive 32-bit integer",
        ),
        (
            vec!["check", "--python=foreign"],
            "interpreter is missing or unreadable",
        ),
    ] {
        let refused = invoke(&arguments);
        assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
        assert!(
            refused.stderr.contains(expected),
            "{}{}",
            refused.stdout,
            refused.stderr
        );
        assert!(!refused.stdout.contains("==>"));
        assert!(!scratch.join(".venv").exists());
        assert!(!scratch.join(".cache").exists());
        assert!(!scratch.join("native/target").exists());
    }
    let check_missing = invoke(&["check"]);
    assert_eq!(check_missing.code, 1);
    assert!(
        !check_missing
            .stdout
            .contains("All retained source checks passed")
    );
    let menu_eof = invoke(&["menu"]);
    assert_eq!(menu_eof.code, 0, "{}", menu_eof.stderr);
    assert!(menu_eof.stdout.contains("Bye."));
    assert!(
        !scratch.join(".venv").exists(),
        "EOF must not create prerequisites"
    );
    let menu_bad = invoke(&["menu", "--install"]);
    assert_eq!(menu_bad.code, 1);
    assert!(menu_bad.stderr.contains("accepts no arguments"));
    let missing_doctor = invoke(&["doctor"]);
    assert_eq!(missing_doctor.code, 1);
    assert!(!missing_doctor.stdout.contains("Environment check complete"));
    assert!(!scratch.join(".venv").exists());

    let configured = PathBuf::from(
        std::env::var_os("PYO3_PYTHON").expect("CI must select its build interpreter"),
    );
    let python = if configured.is_absolute() || configured.components().count() > 1 {
        _native::migration_bundle::resolved(&configured).unwrap()
    } else {
        let name = configured
            .to_str()
            .unwrap()
            .strip_suffix(".exe")
            .unwrap_or(configured.to_str().unwrap());
        _native::codec_process::executable(name).unwrap()
    };
    let clean = _native::tooling_environment::environment(
        &scratch,
        &tooling_runtime::environment(&original),
        false,
    )
    .unwrap();
    let venv = scratch.join(".venv");
    let created = runtime
        .block_on(tooling_process::capture(
            &scratch,
            &[
                python.into_os_string(),
                "-I".into(),
                "-B".into(),
                "-X".into(),
                "utf8".into(),
                "-m".into(),
                "venv".into(),
                "--without-pip".into(),
                "--system-site-packages".into(),
                venv.clone().into_os_string(),
            ],
            &clean,
            Duration::from_secs(20),
            Duration::from_secs(10),
            &log,
        ))
        .unwrap();
    assert_eq!(created.code, 0, "{}{}", created.stdout, created.stderr);
    let success = invoke(&["runtime-check"]);
    assert_eq!(success.code, 0, "{}{}", success.stdout, success.stderr);
    let report: Value = serde_json::from_str(&success.stdout).unwrap();
    assert_eq!(report["implementation"], "cpython");
    assert_eq!(report["version"][0], 3);
    assert_eq!(report["version"][1], 11);
    assert_eq!(
        _native::migration_bundle::resolved(Path::new(report["prefix"].as_str().unwrap())).unwrap(),
        venv
    );
    assert_eq!(report["prefix"], report["exec_prefix"]);
    assert_eq!(report["shared_sites"], true);
    assert_eq!(report["safe_path"], true);
    assert_eq!(report["user_site"], false);
    assert_eq!(report["dont_write_bytecode"], true);
    assert_eq!(report["pillow_roundtrip"], true);
    assert_eq!(report["dependencies"].as_array().unwrap().len(), 3);
    for key in ["scripts", "platlib"] {
        let path = Path::new(report[key].as_str().unwrap());
        assert!(
            path.is_absolute() && path.starts_with(&venv),
            "{key}: {path:?}"
        );
    }
    let suffixes = report["extension_suffixes"].as_array().unwrap();
    assert!(!suffixes.is_empty());
    assert!(report["maturin_present"].is_boolean());
    if report["maturin_present"] == true {
        assert!(!report["maturin_version"].as_str().unwrap().is_empty());
    } else {
        assert!(report["maturin_version"].is_null());
    }
    for dependency in report["dependencies"].as_array().unwrap() {
        assert!(Path::new(dependency["origin"].as_str().unwrap()).is_file());
    }
    let selected = tooling_runtime::selection(&scratch).unwrap();
    assert_eq!(
        Path::new(report["executable"].as_str().unwrap()),
        selected.executable
    );
    let independent = runtime
        .block_on(tooling_process::capture(
            &scratch,
            &[
                selected.executable.clone().into_os_string(),
                "-I".into(),
                "-B".into(),
                "-X".into(),
                "utf8".into(),
                "-m".into(),
                "sysconfig".into(),
            ],
            &clean,
            Duration::from_secs(5),
            Duration::from_secs(3),
            &log,
        ))
        .unwrap();
    assert_eq!(independent.code, 0, "{}", independent.stderr);
    assert!(
        independent
            .stdout
            .lines()
            .any(|line| line == "Python version: \"3.11\"")
    );
    let paths = independent
        .stdout
        .split_once("Paths: ")
        .expect("external sysconfig paths")
        .1
        .split_once("Variables: ")
        .expect("external sysconfig variables")
        .0;
    for (name, observed) in [
        ("data", &report["prefix"]),
        ("scripts", &report["scripts"]),
        ("platlib", &report["platlib"]),
    ] {
        let prefix = format!("{name} = \"");
        let values = paths
            .lines()
            .filter_map(|line| line.trim().strip_prefix(&prefix)?.strip_suffix('"'))
            .collect::<Vec<_>>();
        assert_eq!(values.len(), 1, "external sysconfig {name}");
        assert_eq!(
            _native::migration_bundle::resolved(Path::new(values[0])).unwrap(),
            _native::migration_bundle::resolved(Path::new(observed.as_str().unwrap())).unwrap(),
            "selected executable and embedded {name} differ"
        );
    }
    // Rename the lexical venv entry, never write through its external POSIX symlink.
    let saved_interpreter = selected.executable.with_file_name("original-interpreter");
    assert_eq!(
        std::fs::symlink_metadata(&saved_interpreter)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::NotFound
    );
    std::fs::rename(&selected.executable, &saved_interpreter).unwrap();
    std::fs::copy(&binary, &selected.executable).unwrap();
    let wrong_executable = invoke(&["runtime-check"]);
    assert_eq!(wrong_executable.code, 1);
    assert!(
        wrong_executable
            .stderr
            .contains("selected package interpreter failed"),
        "{}",
        wrong_executable.stderr
    );
    std::fs::remove_file(&selected.executable).unwrap();
    std::fs::rename(saved_interpreter, &selected.executable).unwrap();
    assert!(!scratch.join("emojikit").exists());
    assert!(!scratch.join("native/target").exists());
    // CI builds this library alongside the one binary; the test never invokes Cargo.
    let artifact = project.join("native/target/debug").join(if cfg!(windows) {
        "_native.dll"
    } else if cfg!(target_os = "macos") {
        "lib_native.dylib"
    } else {
        "lib_native.so"
    });
    assert!(
        artifact.is_file(),
        "CI must build the actual native library: {artifact:?}"
    );
    let package = scratch.join("emojikit");
    std::fs::create_dir(&package).unwrap();
    // Invalid source bytes make an accidental package import observable, without executing a script.
    std::fs::write(package.join("__init__.py"), b"\xff").unwrap();
    let extension = package.join(format!("_native{}", suffixes[0].as_str().unwrap()));
    std::fs::copy(&artifact, &extension).unwrap();
    let digest = _native::tooling_manifest::digest(&extension).unwrap();
    let extension_text = extension.to_str().unwrap();
    let loaded = invoke(&["runtime-check", "--extension", extension_text]);
    assert_eq!(loaded.code, 0, "{}{}", loaded.stdout, loaded.stderr);
    let loaded: Value = serde_json::from_str(&loaded.stdout).unwrap();
    assert_eq!(loaded["extension"]["api_version"], 1);
    assert_eq!(loaded["extension"]["sha256"], digest);
    assert_eq!(loaded["extension"]["package_imported"], false);
    assert_eq!(loaded["extension"]["greedy_callable"], true);
    assert_eq!(loaded["extension"]["near_callable"], true);
    assert_eq!(
        _native::migration_bundle::resolved(Path::new(
            loaded["extension"]["path"].as_str().unwrap()
        ))
        .unwrap(),
        extension
    );
    assert_eq!(
        _native::tooling_manifest::digest(&extension).unwrap(),
        digest
    );
    std::fs::copy(project.join(".env.example"), scratch.join(".env.example")).unwrap();
    let doctor = invoke(&["doctor"]);
    assert_eq!(doctor.code, 0, "{}{}", doctor.stdout, doctor.stderr);
    assert!(doctor.stdout.contains("Environment check complete"));
    assert!(!doctor.stdout.contains("TOKEN123"));
    assert_eq!(
        _native::tooling_manifest::digest(&extension).unwrap(),
        digest
    );
    let bad_arguments = invoke(&["doctor", "--install"]);
    assert_eq!(bad_arguments.code, 1);
    assert!(bad_arguments.stderr.contains("accepts no arguments"));
    if cfg!(any(windows, target_os = "linux")) {
        tooling_check_fixture::assert_original_subset(
            project,
            &scratch,
            Path::new(report["base_prefix"].as_str().unwrap()),
            &binary,
            &environment,
            &runtime,
            &log,
        );
        let input = scratch.join("scripted menu.txt");
        std::fs::write(&input, b"d2\nexit\n").unwrap();
        let menu = runtime
            .block_on(tooling_process::capture_with_input(
                &scratch,
                &[binary.clone().into_os_string(), "menu".into()],
                &environment,
                &input,
                Duration::from_secs(10),
                Duration::from_secs(5),
                &log,
            ))
            .unwrap();
        assert_eq!(menu.code, 0, "{}", menu.stderr);
        let menu = menu.stdout;
        assert!(menu.contains("STALE roster:"), "{menu}");
        assert!(menu.contains("Status partially finished"), "{menu}");
        assert!(menu.contains("Bye."), "{menu}");
        assert!(
            !scratch.join("collection").exists(),
            "offline status must not create a catalog"
        );
        assert!(!scratch.join("native/target").exists());
        // Production still uses the retained codec. Exercise the real installed menu
        // without presenting that transitional closure as Python-free acceptance.
        for name in [
            "cli_env.py",
            "errors.py",
            "identity.py",
            "logsetup.py",
            "media.py",
            "media_bridge.py",
            "media_lottie.py",
            "packstate.py",
            "repaint.py",
            "video_decode.py",
        ] {
            std::fs::copy(project.join("emojikit").join(name), package.join(name)).unwrap();
        }
        std::fs::write(package.join("__init__.py"), b"").unwrap();
        let images = scratch.join("input with spaces");
        let output = scratch.join("output with spaces");
        std::fs::create_dir(&images).unwrap();
        let art = project.join("assets/numera-emoji-mapper-logo.png");
        let art_hash = _native::tooling_manifest::digest(&art).unwrap();
        std::fs::copy(&art, images.join("art.png")).unwrap();
        let script = scratch.join("conversion menu.txt");
        std::fs::write(&script, "a2\ninput with spaces\noutput with spaces\nexit\n").unwrap();
        let mut conversion_environment = environment.clone();
        conversion_environment.remove("PYTHONHOME");
        conversion_environment.insert("PYTHONPATH".into(), scratch.to_str().unwrap().into());
        let converted = runtime
            .block_on(tooling_process::capture_with_input(
                &scratch,
                &[binary.clone().into_os_string(), "menu".into()],
                &conversion_environment,
                &script,
                Duration::from_secs(20),
                Duration::from_secs(10),
                &log,
            ))
            .unwrap();
        assert_eq!(
            converted.code, 0,
            "{}{}",
            converted.stdout, converted.stderr
        );
        assert!(converted.stdout.contains("Conversion finished"));
        assert!(converted.stdout.contains("Bye."));
        assert_eq!(
            _native::tooling_manifest::digest(&images.join("art.png")).unwrap(),
            art_hash
        );
        let png = std::fs::read(output.join("art.png")).unwrap();
        assert!(png.len() > 24);
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[16..24], &[0, 0, 0, 100, 0, 0, 0, 100]);
        assert!(!scratch.join("native/target").exists());
        let echo_root = scratch.join("foreground echo café");
        std::fs::create_dir_all(echo_root.join("assets")).unwrap();
        for name in ["assets/panel.html", "pyproject.toml"] {
            std::fs::copy(project.join(name), echo_root.join(name)).unwrap();
        }
        std::fs::create_dir_all(echo_root.join(if cfg!(windows) {
            ".venv/Scripts"
        } else {
            ".venv/bin"
        }))
        .unwrap();
        std::fs::write(
            echo_root.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            }),
            b"selection-only fixture; never executed",
        )
        .unwrap();
        let fixture = PathBuf::from(
            std::env::var_os("NUMERA_MEDIA_FIXTURE_WORKER").expect("CI lifecycle fixture required"),
        );
        let installed = _native::tooling_install::install_executable(&echo_root, &fixture).unwrap();
        let driver_directory = echo_root.join("native/menu-driver");
        std::fs::create_dir(&driver_directory).unwrap();
        let driver = driver_directory.join(binary.file_name().unwrap());
        std::fs::copy(&binary, &driver).unwrap();
        let scripted = echo_root.join("input.txt");
        std::fs::write(&scripted, "c1\nchild café 😀\nc1\nchild café 😀\nexit\n").unwrap();
        // The fixture accepts the bot action solely to verify shared stdin ownership,
        // not to impersonate a successful bot/network workflow.
        let mut echo_environment = environment.clone();
        echo_environment.insert("NUMERA_FOREGROUND_INPUT".into(), "1".into());
        let echo = runtime
            .block_on(tooling_process::capture_with_input(
                &echo_root,
                &[driver.into_os_string(), "menu".into()],
                &echo_environment,
                &scripted,
                Duration::from_secs(10),
                Duration::from_secs(5),
                &log,
            ))
            .unwrap();
        assert_eq!(echo.code, 0, "{}", echo.stderr);
        let echo = echo.stdout;
        assert_eq!(
            echo.matches("foreground input accepted").count(),
            2,
            "{echo}"
        );
        assert!(echo.contains("Bye."), "{echo}");
        assert!(installed.is_file());
        assert_eq!(
            std::fs::read_dir(echo_root.join("logs/tmp/tooling"))
                .unwrap()
                .count(),
            0
        );
    } else {
        let input = scratch.join("scripted menu.txt");
        std::fs::write(&input, b"exit\n").unwrap();
        let refused = runtime
            .block_on(tooling_process::capture_with_input(
                &scratch,
                &[binary.clone().into_os_string(), "menu".into()],
                &environment,
                &input,
                Duration::from_secs(10),
                Duration::from_secs(5),
                &log,
            ))
            .err()
            .expect("unsupported descendant custody must refuse before spawn");
        assert!(refused.contains("requires verified descendant custody"));
        eprintln!("Scripted menu execution untested on this platform: {refused}");
    }
    for (path, expected) in [
        (artifact.to_str().unwrap(), "owning project"),
        ("emojikit/_native.so", "must be absolute"),
        (package.to_str().unwrap(), "regular file"),
    ] {
        let refused = invoke(&["runtime-check", "--extension", path]);
        assert_eq!(refused.code, 1);
        assert!(refused.stderr.contains(expected), "{}", refused.stderr);
        assert_eq!(
            _native::tooling_manifest::digest(&extension).unwrap(),
            digest
        );
    }
    // Keep a separate real driver so absence of the installed executable is an
    // application prerequisite refusal, not an OS failure to start the test command.
    let driver = scratch.join(binary.file_name().unwrap());
    std::fs::copy(&binary, &driver).unwrap();
    let held_binary = binary.with_file_name("held-installed-binary");
    assert!(!held_binary.exists());
    std::fs::rename(&binary, &held_binary).unwrap();
    let refused = runtime
        .block_on(tooling_process::capture(
            &scratch,
            &[driver.into_os_string(), "doctor".into()],
            &environment,
            Duration::from_secs(10),
            Duration::from_secs(5),
            &log,
        ))
        .unwrap();
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert!(
        refused
            .stderr
            .contains("Installed native executable missing")
    );
    assert!(!scratch.join("native/target").exists());
    std::fs::rename(held_binary, &binary).unwrap();
    if cfg!(any(windows, target_os = "linux")) {
        use pyo3::prelude::*;
        Python::initialize();
        Python::attach(|py| -> PyResult<()> {
            let image = py
                .import("PIL.Image")?
                .call_method1("open", (scratch.join("output with spaces/art.png"),))?;
            let observed = (|| -> PyResult<(u32, u32)> {
                image.call_method0("load")?;
                image.getattr("size")?.extract()
            })();
            let closed = image.call_method0("close");
            assert_eq!(observed?, (100, 100));
            closed?;
            Ok(())
        })
        .unwrap();
    }
    // A fresh probe must load these bytes, never accept its own compiled-in module.
    std::fs::write(&extension, b"invalid native library fixture").unwrap();
    let bad_doctor = invoke(&["doctor"]);
    assert_eq!(bad_doctor.code, 1);
    assert!(!bad_doctor.stdout.contains("Environment check complete"));
    let corrupt = invoke(&["runtime-check", "--extension", extension_text]);
    assert_eq!(corrupt.code, 1);
    assert!(
        corrupt.stderr.contains("native extension"),
        "{}",
        corrupt.stderr
    );
    assert_eq!(
        std::fs::read(&extension).unwrap(),
        b"invalid native library fixture"
    );
    let source = package.join("_native.py");
    std::fs::write(&source, b"non-executable refusal fixture").unwrap();
    let refused = invoke(&["runtime-check", "--extension", source.to_str().unwrap()]);
    assert_eq!(refused.code, 1);
    assert!(
        refused.stderr.contains("native extension suffix"),
        "{}",
        refused.stderr
    );
    assert!(!package.join("__pycache__").exists());
    let config = venv.join("pyvenv.cfg");
    let before = std::fs::read(&config).unwrap();
    let version = report["version"].as_array().unwrap();
    let actual_version = version
        .iter()
        .map(|part| part.as_u64().unwrap().to_string())
        .collect::<Vec<_>>()
        .join(".");
    let mismatched = String::from_utf8(before.clone())
        .unwrap()
        .replace(&format!("version = {actual_version}"), "version = 3.11.999");
    assert_ne!(mismatched.as_bytes(), before);
    std::fs::write(&config, mismatched).unwrap();
    let wrong_runtime = invoke(&["runtime-check"]);
    assert_eq!(wrong_runtime.code, 1);
    assert!(
        wrong_runtime.stderr.contains("linked Python version"),
        "{}",
        wrong_runtime.stderr
    );
    for malformed in [
        b"version = 3.11.9\n".as_slice(),
        b"home = one\nhome = two\n",
        b"\xff",
    ] {
        std::fs::write(&config, malformed).unwrap();
        let refused = invoke(&["runtime-check"]);
        assert_eq!(refused.code, 1);
        assert!(refused.stderr.contains("pyvenv.cfg"), "{}", refused.stderr);
    }
    std::fs::remove_file(&config).unwrap();
    std::fs::create_dir(&config).unwrap();
    assert!(
        tooling_runtime::selection(&scratch)
            .unwrap_err()
            .contains("regular file")
    );
    std::fs::remove_dir(&config).unwrap();
    std::fs::write(config, before).unwrap();
    assert_eq!(
        std::fs::read_dir(scratch.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    drop(log);
    // No Drop cleanup: any failed observation retains its exact bounded-run diagnostics.
    std::fs::remove_dir_all(scratch).unwrap();
}
