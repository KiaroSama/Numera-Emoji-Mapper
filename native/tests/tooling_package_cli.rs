//! Copied-binary source-schema1 CLI contracts; no source build, install, Git write or live service.
use _native::{process_owner::OwnedProcess, tooling_manifest::digest};
use serde_json::Value;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

#[test]
fn source_package_options_keep_required_inputs_default_output_and_last_duplicate() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(project.join("tests/fixtures/native/tooling-package-source.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        fixture["sourceHashes"]["scripts/package_native.py"],
        "cb504e8f0e22789a278129f893bb14cf1954483d9a3f620254e6ac5da7a326b0"
    );
    assert_eq!(fixture["expected"]["files"].as_object().unwrap().len(), 26);
    let root = Path::new("source root with spaces");
    let args = vec![
        "--executable".into(),
        "build/executable".into(),
        "--wheel".into(),
        fixture["wheel_input"].as_str().unwrap().into(),
        "--dependencies".into(),
        "dependencies".into(),
    ];
    let parsed = _native::tooling_package_cli::options(root, &args).unwrap();
    assert_eq!(parsed.output, root.join("dist"));
    assert_eq!(parsed.executable, Path::new("build/executable"));
    let mut repeated = args;
    repeated.extend([
        "--out".into(),
        "first".into(),
        "--out".into(),
        "last".into(),
    ]);
    assert_eq!(
        _native::tooling_package_cli::options(root, &repeated)
            .unwrap()
            .output,
        Path::new("last")
    );
    let inherited = [
        ("GIT_DIR", "/foreign"),
        ("GIT_CONFIG_COUNT", "99"),
        ("TEST_TOKEN", "synthetic"),
        ("PATH", "ordinary"),
    ]
    .map(|(key, value)| (key.into(), value.into()))
    .into_iter()
    .collect();
    let environment = _native::tooling_package_cli::git_environment(&inherited);
    assert!(!environment.contains_key(std::ffi::OsStr::new("GIT_DIR")));
    assert!(!environment.contains_key(std::ffi::OsStr::new("GIT_CONFIG_COUNT")));
    assert!(!environment.contains_key(std::ffi::OsStr::new("TEST_TOKEN")));
    assert_eq!(environment[std::ffi::OsStr::new("GIT_OPTIONAL_LOCKS")], "0");
}

#[test]
fn original_package_argparse_keeps_attached_prefix_empty_negative_and_help_values() {
    use _native::tooling_package_cli::{Arguments, arguments};
    let root = Path::new("owning root café");
    let strings = |values: &[&str]| {
        values
            .iter()
            .map(|value| (*value).into())
            .collect::<Vec<_>>()
    };
    for (input, executable, wheel, dependencies, output) in [
        (
            vec![
                "--exe=build café",
                "--w=wheel.whl",
                "--dep=dependencies",
                "--o=dist space",
            ],
            "build café",
            "wheel.whl",
            "dependencies",
            "dist space",
        ),
        (
            vec![
                "--executable",
                "-1",
                "--wheel=",
                "--dependencies=--help",
                "--out=",
            ],
            "-1",
            ".",
            "--help",
            ".",
        ),
        (
            vec![
                "--executable=first",
                "--executable=last",
                "--wheel=w",
                "--dependencies=d",
                "--out=o",
            ],
            "last",
            "w",
            "d",
            "o",
        ),
    ] {
        let Arguments::Package(parsed) = arguments(root, &strings(&input)).unwrap() else {
            panic!("a path value must not become help or a refused option")
        };
        assert_eq!(parsed.executable, Path::new(executable));
        assert_eq!(parsed.wheel, Path::new(wheel));
        assert_eq!(parsed.dependencies, Path::new(dependencies));
        assert_eq!(parsed.output, Path::new(output));
    }
    for input in [
        vec![],
        vec!["--executable"],
        vec!["--bad"],
        vec!["--out", "--wheel=x"],
    ] {
        assert!(matches!(
            arguments(root, &strings(&input)).unwrap(),
            Arguments::Exit(2)
        ));
    }
    for input in [vec!["--help"], vec!["-h"], vec!["--executable=x", "--help"]] {
        assert!(matches!(
            arguments(root, &strings(&input)).unwrap(),
            Arguments::Exit(0)
        ));
    }
}

#[test]
fn copied_package_cli_help_verify_and_refusals_preserve_source_payloads() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let root = Root::new("tooling package cli");
    let captures: Value = serde_json::from_slice(
        &std::fs::read(project.join("tests/fixtures/native/tooling-source-contracts-v2.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        captures["sourceHashes"]["scripts/prebuilt_manifest.py"],
        "7db57eb04bbc3640a411533a3ceffc755c79b25b2312744953fde714a0c30d0c"
    );
    let name = if cfg!(windows) {
        "windows-valid"
    } else if cfg!(target_os = "linux") {
        "linux-valid"
    } else if cfg!(target_arch = "aarch64") {
        "mac-arm-valid"
    } else {
        "mac-intel-valid"
    };
    let fixture = captures["manifest"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap();
    for (relative, text) in fixture["payload"].as_object().unwrap() {
        let path = root.0.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text.as_str().unwrap().as_bytes()).unwrap();
    }
    std::fs::write(
        root.0.join("package.json"),
        serde_json::to_vec(&fixture["manifest"]).unwrap(),
    )
    .unwrap();
    let executable = root.0.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let package_before = fixture["manifest"]["files"]
        .as_object()
        .unwrap()
        .keys()
        .map(|name| (name.clone(), digest(&root.0.join(name)).unwrap()))
        .collect::<Vec<_>>();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let cases: Vec<(&str, Vec<String>, i32, &str)> = vec![
        (
            "package-native",
            vec!["--help".into()],
            0,
            "--executable EXECUTABLE",
        ),
        ("package-native", vec![], 2, "required"),
        (
            "package-native",
            vec!["--executable".into()],
            2,
            "expected one argument",
        ),
        ("package-native", vec!["--bad".into()], 2, "required"),
        (
            "package-native",
            vec![
                "--executable=--help".into(),
                "--wheel=w".into(),
                "--dependencies=d".into(),
            ],
            2,
            "Package root must be the owning Git repository",
        ),
        (
            "package-verify",
            vec!["--help".into()],
            0,
            "No installation or activation",
        ),
        (
            "package-verify",
            vec!["--bad".into()],
            2,
            "unknown package verification option",
        ),
        ("package-verify", vec![], 0, "Verified package 2.0.0"),
        // The copied package is inside another Git checkout but does not own it;
        // provenance must refuse rather than packaging a parent's HEAD by accident.
        (
            "package-native",
            vec![
                "--executable".into(),
                "native/runtime/numera-emoji".into(),
                "--wheel".into(),
                "wheels/numera_emoji_core-2.0.0-cp311-abi3-fixture.whl".into(),
                "--dependencies".into(),
                "wheels".into(),
            ],
            2,
            "Package root must be the owning Git repository",
        ),
    ];
    for (index, (command, args, expected, text)) in cases.into_iter().enumerate() {
        let owner_directory = root.0.join(format!("owner{index}"));
        std::fs::create_dir(&owner_directory).unwrap();
        let output = root.0.join(format!("stdout{index}"));
        let errors = root.0.join(format!("stderr{index}"));
        let mut child = Command::new(&executable);
        child
            .arg(command)
            .args(args)
            .current_dir(project)
            .env_clear()
            .envs(_native::tooling_package_cli::git_environment(
                &std::env::vars_os().collect(),
            ))
            .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::from(std::fs::File::create(&output).unwrap()))
            .stderr(Stdio::from(std::fs::File::create(&errors).unwrap()));
        let mut owner = OwnedProcess::spawn(&mut child, owner_directory.clone()).unwrap();
        drop(child);
        let status = runtime
            .block_on(owner.wait(Duration::from_secs(35), Duration::from_secs(20)))
            .unwrap();
        owner.finish().unwrap();
        assert_eq!(status.code(), Some(expected));
        let actual =
            std::fs::read_to_string(output).unwrap() + &std::fs::read_to_string(errors).unwrap();
        assert!(actual.contains(text), "{command}: {actual}");
        assert!(!owner_directory.exists());
        assert!(!root.0.join("dist").exists());
        for (name, before) in &package_before {
            assert_eq!(digest(&root.0.join(name)).unwrap(), *before);
        }
    }
    std::fs::write(root.0.join("assets/panel.html"), b"changed").unwrap();
    assert!(
        _native::tooling_manifest::verify(
            &root.0,
            &_native::tooling_manifest::Host {
                system: fixture["host"]["system"].as_str().unwrap().into(),
                machine: fixture["host"]["machine"].as_str().unwrap().into(),
                python: (3, 11),
                libc: "glibc".into(),
                libc_version: "2.39".into(),
                macos: "15.0".into(),
            }
        )
        .unwrap_err()
        .contains("Missing or changed package member")
    );
}
