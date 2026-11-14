//! Exact original wrapper assertions and real reference/native exclusion share the pinned corpus.
use _native::source_git_inventory::Source;
use serde_json::json;
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = [
        "emojikit/packstate.py",
        "tests/reference/packstate.py",
        "tests/test_native_state_compat.py",
        "tests/test_pack_locks.py",
        "tests/test_resume_safety.py",
        "tests/test_native_storage_recovery.py",
    ]
    .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    _native::source_state_compat_oracle::source_identity(&sources).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join(format!("state source refusal {}", std::process::id()));
    assert!(!missing.exists());
    for selected in 0..sources.len() {
        for mutation in ["revision", "path", "claimed hash", "actual bytes"] {
            let original = sources[selected];
            let mut forged = Source {
                baseline_revision: original.baseline_revision.clone(),
                path: original.path.clone(),
                sha256: original.sha256.clone(),
                text: original.text.clone(),
            };
            match mutation {
                "revision" => forged.baseline_revision = "0".repeat(40),
                "path" => forged.path = "tests/unverified_state.py".into(),
                "claimed hash" => forged.sha256 = "0".repeat(64),
                "actual bytes" => forged
                    .text
                    .push_str("\nraise RuntimeError('must never execute')"),
                _ => unreachable!(),
            }
            let mut admitted = sources;
            admitted[selected] = &forged;
            assert!(
                _native::source_state_compat_oracle::observe(root, &missing, &admitted)
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::observe_state_compatibility(
                        root,
                        &parent.join("not-a-state-oracle-executable"),
                        &missing,
                        &admitted,
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!missing.exists());
        }
    }
    let workspace = parent.join(format!(
        "original state compatibility {}",
        std::process::id()
    ));
    std::fs::create_dir(&workspace).unwrap();
    let retained = workspace.join("caller-data.txt");
    std::fs::write(&retained, b"caller data must survive refusal").unwrap();
    assert!(
        _native::source_state_compat_oracle::observe(root, &workspace, &sources)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(
        std::fs::read(&retained).unwrap(),
        b"caller data must survive refusal"
    );
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 1);
    assert!(
        _native::source_state_compat_oracle::observe(root, root, &sources)
            .unwrap_err()
            .contains("project-contained workspace")
    );
    std::fs::remove_file(retained).unwrap();
    #[cfg(unix)]
    {
        let link = workspace.join("linked");
        std::os::unix::fs::symlink(&workspace, &link).unwrap();
        assert!(
            _native::source_state_compat_oracle::observe(root, &link, &sources)
                .unwrap_err()
                .contains("linked/reparse")
        );
        assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 1);
        std::fs::remove_file(link).unwrap();
    }
    let report = runtime
        .block_on(_native::source_oracle_worker::observe_state_compatibility(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-compat")),
            &workspace,
            &sources,
        ))
        .unwrap();
    assert_eq!(report["source_revision"], sources[0].baseline_revision);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    let platform = if cfg!(windows) {
        "win32"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    };
    assert_eq!(report["source_platform"], platform);
    assert_eq!(report["tests_run"], 1);
    for field in ["errors", "failures", "skipped"] {
        assert_eq!(report[field], 0);
    }
    assert_eq!(
        report["cases"],
        json!(["NativeStateCompatibility.test_exported_state_lease_callback_and_atomic_bytes"])
    );
    assert_eq!(report["source_hashes"].as_object().unwrap().len(), 6);
    for source in sources {
        assert_eq!(report["source_hashes"][&source.path], source.sha256);
    }
    assert_eq!(
        report["boundary_calls"],
        json!({
            "native_import":14, "utc_import":5, "own_pid_liveness":1
        })
    );
    let storage = &report["storage"];
    assert_eq!(storage["tests_run"], 2);
    assert_eq!(
        storage["cases"],
        json!([
            "NativeStorage.test_atomic_utf8_replaces_and_leaves_no_sibling",
            "NativeStorage.test_legacy_and_native_exclude_each_other_and_release",
        ])
    );
    for field in ["errors", "failures", "skipped"] {
        assert_eq!(storage[field], 0);
    }
    assert_eq!(
        report["storage_boundary_calls"],
        json!({"native_import":2,"utc_import":0,"own_pid_liveness":1})
    );
    let diagnostics = &report["diagnostics"];
    assert_eq!(diagnostics["tests_run"], 4);
    assert_eq!(diagnostics["import_calls"], 4);
    let outcomes = diagnostics["observations"].as_array().unwrap();
    assert_eq!(outcomes.len(), 4);
    use _native::lock_diagnostics::Presence;
    for (observation, (method, result, presence)) in outcomes.iter().zip([
        (
            "test_no_such_process_is_reported_dead",
            Err(std::io::Error::from_raw_os_error(3)),
            Presence::Gone,
        ),
        (
            "test_permission_denied_means_it_exists",
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
            Presence::Present,
        ),
        (
            "test_unknown_failure_stays_conservative",
            Err(std::io::Error::other("weird")),
            Presence::Unknown,
        ),
        ("test_a_running_process_is_alive", Ok(()), Presence::Present),
    ]) {
        assert_eq!(
            observation["case"],
            format!("PosixStaleLockReclaim.{method}")
        );
        assert_eq!(observation["pid"], 4242);
        assert_eq!(observation["signal"], 0);
        let actual = Presence::from_posix_probe(result);
        assert_eq!(actual, presence);
        assert_eq!(observation["alive"], actual != Presence::Gone);
    }
    let shapes = &report["shapes"];
    assert_eq!(shapes["tests_run"], 6);
    assert_eq!(
        shapes["cases"],
        json!([
            "StateShapeValidation.test_a_sound_state_passes",
            "StateShapeValidation.test_wrong_base_is_rejected",
            "StateShapeValidation.test_negative_and_oversized_counts_are_rejected",
            "StateShapeValidation.test_duplicate_and_backwards_indexes_are_rejected",
            "StateShapeValidation.test_malformed_intent_is_rejected",
            "StateShapeValidation.test_done_must_hold_strings",
        ])
    );
    let observations = shapes["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 10);
    pyo3::Python::initialize();
    pyo3::Python::attach(|py| {
        for observation in observations {
            let state = &observation["state"];
            let before = state.clone();
            assert_eq!(observation["base"], "t");
            assert_eq!(observation["per_set"], 200);
            let result = _native::state_bindings::state_validate_json(
                state.to_string(),
                observation["base"].as_str().unwrap().into(),
                observation["per_set"].as_i64().unwrap(),
            );
            if observation["accepted"].as_bool().unwrap() {
                result.unwrap();
                assert!(observation["error"].is_null());
            } else {
                assert!(
                    observation["error"]
                        .as_str()
                        .is_some_and(|text| !text.is_empty())
                );
                let error = result.unwrap_err();
                assert!(error.is_instance_of::<pyo3::exceptions::PyValueError>(py));
                assert!(!error.is_instance_of::<pyo3::exceptions::PyOSError>(py));
            }
            assert_eq!(*state, before);
        }
    });
    let atomic = &report["atomic"];
    assert_eq!(atomic["tests_run"], 3);
    assert_eq!(atomic["writer_calls"], 5);
    assert_eq!(atomic["temporary_calls"], 3);
    assert_eq!(
        atomic["cases"],
        json!([
            "AtomicJsonWrite.test_writes_and_leaves_no_temp_file",
            "AtomicJsonWrite.test_previous_content_survives_a_failed_write",
            "AtomicJsonWrite.test_overwrites_in_place",
        ])
    );
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
    collection_state(root, runtime, corpus);
}

fn collection_state(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    use _native::source_collection_state_oracle as oracle;
    let sources = [
        "tests/reference/collection_state.py",
        "tests/reference/packstate.py",
        "tests/test_build_collection_state.py",
    ]
    .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    oracle::source_identity(&sources).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join(format!("collection source refusal {}", std::process::id()));
    assert!(!missing.exists());
    for selected in 0..sources.len() {
        for mutation in ["revision", "path", "hash", "bytes"] {
            let original = sources[selected];
            let mut changed = Source {
                baseline_revision: original.baseline_revision.clone(),
                path: original.path.clone(),
                sha256: original.sha256.clone(),
                text: original.text.clone(),
            };
            match mutation {
                "revision" => changed.baseline_revision = "0".repeat(40),
                "path" => changed.path = "tests/not-original.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                "bytes" => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
                _ => unreachable!(),
            }
            let mut admitted = sources;
            admitted[selected] = &changed;
            let error = runtime
                .block_on(oracle::observe_owned(root, &missing, &missing, &admitted))
                .unwrap_err();
            assert!(
                error.contains("source identity drift"),
                "{mutation}: {error}"
            );
            assert!(!missing.exists());
        }
    }
    let workspace = parent.join(format!("original collection state {}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("caller"), b"caller bytes").unwrap();
    assert!(
        oracle::observe(root, &workspace, &sources)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(
        std::fs::read(workspace.join("caller")).unwrap(),
        b"caller bytes"
    );
    std::fs::remove_file(workspace.join("caller")).unwrap();
    assert!(
        oracle::observe(root, root, &sources)
            .unwrap_err()
            .contains("inside project scratch")
    );
    #[cfg(unix)]
    {
        let link = workspace.join("linked");
        std::os::unix::fs::symlink(&workspace, &link).unwrap();
        assert!(
            oracle::observe(root, &link, &sources)
                .unwrap_err()
                .contains("linked/reparse")
        );
        std::fs::remove_file(link).unwrap();
    }
    let report = runtime
        .block_on(oracle::observe_owned(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-compat")),
            &workspace,
            &sources,
        ))
        .unwrap();
    assert_eq!(report["source_revision"], sources[0].baseline_revision);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["tests_run"], 19);
    assert_eq!(report["cases"].as_array().unwrap().len(), 19);
    assert_eq!(report["excluded"].as_array().unwrap().len(), 2);
    for field in [
        "errors",
        "failures",
        "skipped",
        "expected_failures",
        "unexpected_successes",
    ] {
        assert_eq!(report[field], 0);
    }
    assert_eq!(report["fault_patches"], 1);
    for source in sources {
        assert_eq!(report["source_hashes"][&source.path], source.sha256);
    }
    let observations = report["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 19);
    assert_eq!(observations[0]["files"], json!({}));
    let failed = observations
        .iter()
        .find(|case| {
            case["case"] == "StateFileContract.test_a_failed_write_leaves_the_previous_state_intact"
        })
        .unwrap();
    let preserved: serde_json::Value =
        serde_json::from_str(failed["files"]["publish_pk.json"].as_str().unwrap()).unwrap();
    assert_eq!(preserved["sent"], json!(["first"]));
    assert!(
        report["ports"]
            .as_str()
            .unwrap()
            .contains("synthetic two-key fixture instead of real image catalog")
    );
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
}
