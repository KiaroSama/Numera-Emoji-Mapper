//! Original tooling observations share the one admitted source corpus and bounded worker.
use _native::{source_git_inventory::Source, source_tooling_oracle as oracle};
use serde_json::json;
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let environment = corpus
        .iter()
        .find(|source| source.path == "scripts/local_environment.py")
        .unwrap();
    let manifest = corpus
        .iter()
        .find(|source| source.path == "scripts/prebuilt_manifest.py")
        .unwrap();
    oracle::source_identity(environment, manifest).unwrap();
    let fixture = oracle::fixture().unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let unavailable = parent.join(format!("tooling refused sources {}", std::process::id()));
    assert!(!unavailable.exists());
    for dependency in [false, true] {
        for mutation in ["revision", "path", "hash", "bytes"] {
            let source = if dependency { manifest } else { environment };
            let mut changed = Source {
                baseline_revision: source.baseline_revision.clone(),
                path: source.path.clone(),
                sha256: source.sha256.clone(),
                text: source.text.clone(),
            };
            match mutation {
                "revision" => changed.baseline_revision = "0".repeat(40),
                "path" => changed.path = "scripts/not-original.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                "bytes" => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
                _ => unreachable!(),
            }
            let (environment, manifest) = if dependency {
                (environment, &changed)
            } else {
                (&changed, manifest)
            };
            let error = runtime
                .block_on(oracle::observe_owned(
                    root,
                    &unavailable,
                    &unavailable,
                    environment,
                    manifest,
                    &fixture,
                ))
                .unwrap_err();
            assert!(error.contains("source identity drift"), "{error}");
            assert!(!unavailable.exists());
        }
    }
    let mut changed = fixture.clone();
    changed["manifest"][0]["payload"]["assets/panel.html"] = json!("coordinated input drift");
    changed["manifest"][0]["expected"] = json!({"manifest":{}});
    let error = runtime
        .block_on(oracle::observe_owned(
            root,
            &unavailable,
            &unavailable,
            environment,
            manifest,
            &changed,
        ))
        .unwrap_err();
    assert!(error.contains("recipe drift"), "{error}");
    assert!(!unavailable.exists());
    let workspace = parent.join(format!(
        "original tooling observations {}",
        std::process::id()
    ));
    std::fs::create_dir(&workspace).unwrap();
    let caller = workspace.join("caller");
    std::fs::write(&caller, b"caller bytes").unwrap();
    assert!(
        oracle::observe(root, &workspace, environment, manifest, &fixture)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(std::fs::read(&caller).unwrap(), b"caller bytes");
    std::fs::remove_file(caller).unwrap();
    assert!(
        oracle::observe(root, root, environment, manifest, &fixture)
            .unwrap_err()
            .contains("inside project scratch")
    );
    #[cfg(unix)]
    {
        let link = workspace.join("linked");
        std::os::unix::fs::symlink(&workspace, &link).unwrap();
        assert!(
            oracle::observe(root, &link, environment, manifest, &fixture)
                .unwrap_err()
                .contains("linked/reparse")
        );
        std::fs::remove_file(link).unwrap();
    }
    let mut observations = fixture.clone();
    observations["manifest"][0]["expected"] = json!({"error":"caller output is not authority"});
    observations["environment"][0]["expected"] = json!({});
    observations["environment"][0]["original_after"] = json!({});
    observations["environment"][0]["directories"] = json!([]);
    observations["confined"][0]["expected"] = json!({"path":"caller output is not authority"});
    let actual = runtime
        .block_on(oracle::observe_owned(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-compat")),
            &workspace,
            environment,
            manifest,
            &observations,
        ))
        .unwrap();
    assert_eq!(actual["source_revision"], environment.baseline_revision);
    assert_eq!(actual["source_runtime"], "CPython3.11.9");
    assert_eq!(
        actual["source_hashes"][&environment.path],
        environment.sha256
    );
    assert_eq!(actual["source_hashes"][&manifest.path], manifest.sha256);
    assert_eq!(actual["manifest_cases"], 24);
    assert_eq!(actual["environment_cases"], 2);
    assert_eq!(actual["confined_cases"], 4);
    assert_eq!(actual["observations"], fixture);
    assert!(
        actual["ports"]
            .as_str()
            .unwrap()
            .contains("no successful install/signing")
    );
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 0);
    std::fs::remove_dir(workspace).unwrap();
    package(root, runtime, corpus);
    sandbox(root, runtime, corpus);
}

fn package(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    use _native::source_package_oracle as oracle;
    let environment = corpus
        .iter()
        .find(|source| source.path == "scripts/local_environment.py")
        .unwrap();
    let source = corpus
        .iter()
        .find(|source| source.path == "scripts/package_native.py")
        .unwrap();
    let fixture = oracle::fixture().unwrap();
    oracle::source_identity(environment, source).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join(format!("package refused sources {}", std::process::id()));
    assert!(!missing.exists());
    for dependency in [false, true] {
        for mutation in ["revision", "path", "hash", "bytes"] {
            let original = if dependency { source } else { environment };
            let mut changed = Source {
                baseline_revision: original.baseline_revision.clone(),
                path: original.path.clone(),
                sha256: original.sha256.clone(),
                text: original.text.clone(),
            };
            match mutation {
                "revision" => changed.baseline_revision = "0".repeat(40),
                "path" => changed.path = "scripts/not-original.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                "bytes" => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
                _ => unreachable!(),
            }
            let (environment, source) = if dependency {
                (environment, &changed)
            } else {
                (&changed, source)
            };
            let error = runtime
                .block_on(oracle::observe_owned(
                    root,
                    &missing,
                    &missing,
                    environment,
                    source,
                    &fixture,
                ))
                .unwrap_err();
            assert!(error.contains("source identity drift"), "{error}");
            assert!(!missing.exists());
        }
    }
    let mut drift = fixture.clone();
    drift["inputs"]["README.md"] = json!("changed recipe");
    drift["expected"] = json!({});
    assert!(
        runtime
            .block_on(oracle::observe_owned(
                root,
                &missing,
                &missing,
                environment,
                source,
                &drift
            ))
            .unwrap_err()
            .contains("recipe drift")
    );
    assert!(!missing.exists());
    let workspace = parent.join(format!(
        "original package observations {}",
        std::process::id()
    ));
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("caller"), b"caller bytes").unwrap();
    assert!(
        oracle::observe(root, &workspace, environment, source, &fixture)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(
        std::fs::read(workspace.join("caller")).unwrap(),
        b"caller bytes"
    );
    std::fs::remove_file(workspace.join("caller")).unwrap();
    assert!(
        oracle::observe(root, root, environment, source, &fixture)
            .unwrap_err()
            .contains("inside project scratch")
    );
    #[cfg(unix)]
    {
        let link = workspace.join("linked");
        std::os::unix::fs::symlink(&workspace, &link).unwrap();
        assert!(
            oracle::observe(root, &link, environment, source, &fixture)
                .unwrap_err()
                .contains("linked/reparse")
        );
        std::fs::remove_file(link).unwrap();
    }
    let mut caller = fixture.clone();
    caller["expected"] = json!({"files":{},"platform":"untrusted"});
    let actual = runtime
        .block_on(oracle::observe_owned(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-compat")),
            &workspace,
            environment,
            source,
            &caller,
        ))
        .unwrap();
    assert_eq!(actual["source_revision"], source.baseline_revision);
    assert_eq!(actual["source_runtime"], "CPython3.11.9");
    assert_eq!(actual["source_hashes"][&source.path], source.sha256);
    assert_eq!(
        actual["source_hashes"][&environment.path],
        environment.sha256
    );
    assert_eq!(actual["inputs_checked"], 28);
    assert_eq!(actual["checksum_verified"], true);
    assert_eq!(actual["manifest"]["files"].as_object().unwrap().len(), 26);
    assert_eq!(actual["archive_members"].as_array().unwrap().len(), 27);
    let mut expected = fixture["expected"].clone();
    let hash = expected["files"]
        .as_object_mut()
        .unwrap()
        .remove("native/runtime/numera-emoji.exe")
        .unwrap();
    let target = actual["manifest"]["platform"].as_str().unwrap();
    assert_eq!(
        target,
        if cfg!(windows) {
            "windows-x64"
        } else if cfg!(target_os = "linux") {
            "linux-x64"
        } else if cfg!(target_arch = "aarch64") {
            "macos-arm64"
        } else {
            "macos-x64"
        }
    );
    expected["files"][if cfg!(windows) {
        "native/runtime/numera-emoji.exe"
    } else {
        "native/runtime/numera-emoji"
    }] = hash;
    expected["platform"] = json!(target);
    expected["minimum_os"] = json!(if cfg!(windows) {
        "Windows 10"
    } else if cfg!(target_os = "linux") {
        "Ubuntu 24.04 / glibc 2.39"
    } else {
        "macOS 15"
    });
    assert_eq!(actual["manifest"], expected);
    assert!(
        actual["ports"]
            .as_str()
            .unwrap()
            .contains("no native policy expectation or completed build/signing/install")
    );
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 0);
    std::fs::remove_dir(workspace).unwrap();
}

fn sandbox(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    use _native::source_sandbox_oracle as oracle;
    let source = corpus
        .iter()
        .find(|source| source.path == "scripts/panel_sandbox.py")
        .unwrap();
    oracle::source_identity(source).unwrap();
    let fixture = oracle::fixture().unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join(format!("sandbox refused source {}", std::process::id()));
    assert!(!missing.exists());
    for mutation in ["revision", "path", "hash", "bytes"] {
        let mut changed = Source {
            baseline_revision: source.baseline_revision.clone(),
            path: source.path.clone(),
            sha256: source.sha256.clone(),
            text: source.text.clone(),
        };
        match mutation {
            "revision" => changed.baseline_revision = "0".repeat(40),
            "path" => changed.path = "scripts/not-original.py".into(),
            "hash" => changed.sha256 = "0".repeat(64),
            "bytes" => changed
                .text
                .push_str("\nraise AssertionError('must not execute')"),
            _ => unreachable!(),
        }
        let error = runtime
            .block_on(oracle::observe_owned(
                root, &missing, &missing, &changed, &fixture,
            ))
            .unwrap_err();
        assert!(error.contains("source identity drift"), "{error}");
        assert!(!missing.exists());
    }
    let mut drift = fixture.clone();
    drift["arguments"][0]["input"] = json!(["--all"]);
    drift["arguments"][0]["expected"] = json!([]);
    assert!(
        runtime
            .block_on(oracle::observe_owned(
                root, &missing, &missing, source, &drift
            ))
            .unwrap_err()
            .contains("recipe drift")
    );
    assert!(!missing.exists());
    let workspace = parent.join(format!("original sandbox policy {}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("caller"), b"caller bytes").unwrap();
    assert!(
        oracle::observe(root, &workspace, source, &fixture)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(
        std::fs::read(workspace.join("caller")).unwrap(),
        b"caller bytes"
    );
    std::fs::remove_file(workspace.join("caller")).unwrap();
    assert!(
        oracle::observe(root, root, source, &fixture)
            .unwrap_err()
            .contains("inside project scratch")
    );
    #[cfg(unix)]
    {
        let link = workspace.join("linked");
        std::os::unix::fs::symlink(&workspace, &link).unwrap();
        assert!(
            oracle::observe(root, &link, source, &fixture)
                .unwrap_err()
                .contains("linked/reparse")
        );
        std::fs::remove_file(link).unwrap();
    }
    let mut observations = fixture.clone();
    observations["scrubbed"] = json!({});
    for case in observations["arguments"].as_array_mut().unwrap() {
        case["port"] = json!(1);
        case["all"] = json!(true);
        case["with_pack"] = json!([]);
        case["bot_username"] = json!("forged");
        case["expected"] = json!([]);
    }
    let actual = runtime
        .block_on(oracle::observe_owned(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-compat")),
            &workspace,
            source,
            &observations,
        ))
        .unwrap();
    assert_eq!(actual["source_revision"], source.baseline_revision);
    assert_eq!(actual["source_hash"], source.sha256);
    assert_eq!(actual["source_runtime"], "CPython3.11.9");
    assert_eq!(actual["argument_cases"], 5);
    assert_eq!(actual["scrub_cases"], 1);
    assert_eq!(actual["observations"], fixture);
    assert!(
        actual["ports"]
            .as_str()
            .unwrap()
            .contains("no clone, panel, account or target policy execution")
    );
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 0);
    std::fs::remove_dir(workspace).unwrap();
}
