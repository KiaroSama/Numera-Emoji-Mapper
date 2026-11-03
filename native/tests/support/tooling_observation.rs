//! Real libtest sidecars remain observations, never authenticated coverage or stdout-derived passes.
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub fn manifest(root: &Path, directory: &Path) -> PathBuf {
    let executable = std::env::current_exe().unwrap();
    let members = [
        ("source", "emojikit/identity.py"),
        ("target", "native/tests/tooling_test_runner.rs"),
        ("fixture", "tests/fixtures/native/static-codec.json"),
        ("support", "native/tests/support/tooling_observation.rs"),
        ("support", "native/tests/support/tooling_fixture.rs"),
        ("config", "native/Cargo.toml"),
        ("config", "native/Cargo.lock"),
        ("config", ".cargo/tests.toml"),
    ]
    .into_iter()
    .map(|(role, path)| {
        json!({"role":role,"path":path,"sha256":_native::tooling_manifest::digest(&root.join(path)).unwrap()})
    })
    .collect::<Vec<_>>();
    // This synthetic build/revision claim tests admission only; it is not a compiler or CI receipt.
    let value = json!({"schema":1,"target_revision":"a".repeat(40),
        "baseline_revision":"1a36a968b006236bd0665a1da65c541938c06061",
        "compiler_artifact":{"reason":"compiler-artifact","package_id":"synthetic-runner-policy",
            "manifest_path":root.join("native/Cargo.toml"),
            "target":{"name":"tooling_test_runner","kind":["test"],"crate_types":["bin"],
                "src_path":root.join("native/tests/tooling_test_runner.rs"),
                "edition":"2024","doc":false,"doctest":false,"test":true},
            "profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,
                "overflow_checks":true,"test":true},
            "features":[],"filenames":[executable],"executable":executable,"fresh":false},
        "files":members});
    let path = directory.join("observation-input.json");
    _native::atomic::write_json(&path, &value).unwrap();
    path
}

pub fn cargo_inputs(root: &Path, manifest: &Path) -> (PathBuf, PathBuf) {
    let value: Value = serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
    let directory = manifest.parent().unwrap();
    let plan = directory.join("cargo-plan.json");
    let build = directory.join("cargo-build.log");
    _native::atomic::write_json(
        &plan,
        &json!({
            "schema":1,"target_revision":value["target_revision"],
            "baseline_revision":value["baseline_revision"],
            "package_id":value["compiler_artifact"]["package_id"],"files":value["files"],
            "targets":[{"name":"tooling_test_runner","kind":["test"],
                "src_path":"native/tests/tooling_test_runner.rs","mode":"observe"}]
        }),
    )
    .unwrap();
    let mut prefix = serde_json::to_vec(&value["compiler_artifact"]).unwrap();
    prefix.extend_from_slice(
        b"\n{\"reason\":\"build-finished\",\"success\":true}\nignored runtime suffix\n",
    );
    _native::atomic::write_bytes(&build, &prefix).unwrap();
    assert!(plan.starts_with(root.join("logs/tmp")));
    (plan, build)
}

pub fn check(root: &Path, fixture: &str, code: i32, stderr: &str) {
    let marker = "test observation retained at ";
    let store = stderr
        .lines()
        .filter_map(|line| line.strip_prefix(marker))
        .next_back()
        .map(PathBuf::from)
        .expect("the actual runner must name its retained observation");
    _native::tooling_environment::reject_links(&store, "observation fixture cannot follow links")
        .unwrap();
    assert_eq!(
        store.parent(),
        Some(root.join("logs/test-observations").as_path())
    );
    let report = store.join("observation.json");
    if fixture == "runner_early_exit_fixture" {
        assert_ne!(code, 0, "exit zero without TeResult cannot certify a case");
        assert!(!report.exists());
        assert_eq!(std::fs::read(store.join("results.log")).unwrap(), b"");
        assert!(
            std::fs::read_to_string(store.join("listing.log"))
                .unwrap()
                .contains(fixture)
        );
    } else {
        let value: Value = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
        assert_eq!(value["result"], "observed");
        assert_eq!(value["trust"], "untrusted");
        assert_eq!(value["release_ready"], false);
        assert_eq!(value["target_revision"], "a".repeat(40));
        assert_eq!(value["listed"], json!([fixture]));
        assert_eq!(
            value["cases"],
            json!([{"name":fixture,"status":if code == 0 {"ok"} else {"failed"}}])
        );
        assert_eq!(value["completed"], code == 0);
        assert_eq!(
            value["executable_sha256"],
            _native::tooling_manifest::digest(&std::env::current_exe().unwrap()).unwrap()
        );
        let actual = std::fs::read_to_string(store.join("results.log")).unwrap();
        assert_eq!(
            actual,
            format!("{} {fixture}\n", if code == 0 { "ok" } else { "failed" })
        );
        assert_eq!(
            std::fs::read_to_string(store.join("dispositions.log")).unwrap(),
            actual
        );
        for (field, name) in [
            ("listing_sha256", "listing.log"),
            ("results_sha256", "results.log"),
            ("dispositions_sha256", "dispositions.log"),
        ] {
            assert_eq!(
                value[field],
                _native::tooling_manifest::digest(&store.join(name)).unwrap()
            );
        }
    }
    let mut members = std::fs::read_dir(&store)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            entry.file_name().into_string().unwrap()
        })
        .collect::<Vec<_>>();
    members.sort();
    let expected = if report.exists() {
        vec![
            "dispositions.log",
            "listing.log",
            "manifest.json",
            "observation.json",
            "results.log",
        ]
    } else {
        vec!["listing.log", "manifest.json", "results.log"]
    };
    assert_eq!(members, expected);
    let cargo = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("cargo observation inputs retained at "))
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    assert_eq!(
        cargo.len(),
        usize::from(fixture == "runner_environment_fixture")
    );
    for directory in cargo {
        _native::tooling_environment::reject_links(&directory, "Cargo fixture cannot follow links")
            .unwrap();
        assert_eq!(directory.parent(), Some(root.join("logs/tmp").as_path()));
        assert!(
            directory
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("cargo-observation-")
        );
        let mut names = std::fs::read_dir(&directory)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                assert!(entry.file_type().unwrap().is_file());
                entry.file_name().into_string().unwrap()
            })
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, ["binding.json", "completion.json", "manifest.json"]);
        let completion: Value =
            serde_json::from_slice(&std::fs::read(directory.join("completion.json")).unwrap())
                .unwrap();
        assert_eq!(
            completion,
            json!({"schema":1,"trust":"untrusted","observation":store.join("observation.json").strip_prefix(root).unwrap().to_str().unwrap().replace('\\', "/")})
        );
        let binding: Value =
            serde_json::from_slice(&std::fs::read(directory.join("binding.json")).unwrap())
                .unwrap();
        assert_eq!(binding["trust"], "untrusted");
        assert_eq!(binding["target"], "tooling_test_runner");
        assert_eq!(binding["mode"], "observe");
        assert_eq!(
            std::fs::read(directory.join("manifest.json")).unwrap(),
            std::fs::read(store.join("manifest.json")).unwrap()
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
    // Only exact verified disposable test observations are removed; failed assertions retain them.
    std::fs::remove_dir_all(store).unwrap();
}
