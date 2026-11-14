//! Three unchanged import assertions share the existing pinned corpus, never reduced namespaces.
use _native::source_git_inventory::Source;
use std::path::Path;

pub fn check(
    root: &Path,
    runtime: &tokio::runtime::Runtime,
    sources: &[Source],
    inventory: &serde_json::Value,
    coverage: &serde_json::Value,
) {
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join(format!(
        "original import replay missing {}",
        std::process::id()
    ));
    assert!(!missing.exists());
    for mutation in ["revision", "path", "hash", "bytes", "missing", "duplicate"] {
        let mut changed = sources
            .iter()
            .map(|source| Source {
                baseline_revision: source.baseline_revision.clone(),
                path: source.path.clone(),
                sha256: source.sha256.clone(),
                text: source.text.clone(),
            })
            .collect::<Vec<_>>();
        match mutation {
            "revision" => changed[0].baseline_revision = "0".repeat(40),
            "path" => changed[0].path = "tests/unverified-import.py".into(),
            "hash" => changed[0].sha256 = "0".repeat(64),
            "bytes" => changed[0]
                .text
                .push_str("\nraise RuntimeError('must not execute')"),
            "missing" => {
                changed.pop();
            }
            _ => changed.push(Source {
                baseline_revision: sources[0].baseline_revision.clone(),
                path: sources[0].path.clone(),
                sha256: sources[0].sha256.clone(),
                text: sources[0].text.clone(),
            }),
        }
        assert!(_native::source_import_replay::stage(root, &missing, &changed).is_err());
        assert!(!missing.exists(), "invalid source created a stage");
    }
    let directory = parent.join(format!("original import replay {}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let sentinel = directory.join("caller.txt");
    std::fs::write(&sentinel, b"caller bytes").unwrap();
    assert!(
        _native::source_import_replay::stage(root, &directory, sources)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"caller bytes");
    std::fs::remove_file(sentinel).unwrap();
    _native::source_import_replay::stage(root, &directory, sources).unwrap();
    let asset = directory.join("assets/panel.html");
    let original_asset = std::fs::read(&asset).unwrap();
    std::fs::write(&asset, b"changed original asset").unwrap();
    assert!(
        runtime
            .block_on(_native::source_import_replay::observe_owned(
                root, &directory
            ))
            .unwrap_err()
            .contains("source identity drift")
    );
    std::fs::write(&asset, original_asset).unwrap();
    let clip = directory.join("private-fixture.webm");
    std::fs::write(&clip, b"").unwrap();
    assert!(_native::source_import_replay::real_video_fixture(root, &clip).is_err());
    std::fs::write(&clip, b"admission-only; never decoded").unwrap();
    assert_eq!(
        _native::source_import_replay::real_video_fixture(root, &clip).unwrap(),
        clip
    );
    assert!(
        _native::source_import_replay::real_video_fixture(root, Path::new("private-fixture.webm"))
            .is_err()
    );
    std::fs::remove_file(clip).unwrap();
    let extra = directory.join("unexpected.py");
    std::fs::write(&extra, b"raise RuntimeError('must not execute')").unwrap();
    assert!(
        runtime
            .block_on(_native::source_import_replay::observe_owned(
                root, &directory
            ))
            .unwrap_err()
            .contains("unexpected whole import file")
    );
    std::fs::remove_file(extra).unwrap();
    let original = directory.join("tests/test_native_consumer_boundary.py");
    let bytes = std::fs::read(&original).unwrap();
    std::fs::write(&original, b"raise RuntimeError('must not execute')").unwrap();
    assert!(
        runtime
            .block_on(_native::source_import_replay::observe_owned(
                root, &directory
            ))
            .unwrap_err()
            .contains("source identity drift")
    );
    std::fs::write(&original, &bytes).unwrap();
    #[cfg(unix)]
    {
        let link = directory.join("linked source");
        std::os::unix::fs::symlink(root, &link).unwrap();
        assert!(
            runtime
                .block_on(_native::source_import_replay::observe_owned(
                    root, &directory
                ))
                .unwrap_err()
                .contains("cannot follow links")
        );
        std::fs::remove_file(link).unwrap();
    }
    let actual = runtime
        .block_on(_native::source_import_replay::observe_owned(
            root, &directory,
        ))
        .unwrap();
    assert_eq!(actual["source_revision"], sources[0].baseline_revision);
    assert_eq!(actual["source_runtime"], "CPython3.11.9");
    assert_eq!(actual["tests_run"], 3);
    assert_eq!(actual["source_files"], 277);
    for key in ["errors", "failures", "skipped"] {
        assert_eq!(actual[key], 0);
    }
    assert_eq!(actual["cases"].as_array().unwrap().len(), 3);
    assert!(
        actual["ports"]
            .as_str()
            .unwrap()
            .contains("not native-only package acceptance")
    );
    let guard = sources
        .iter()
        .find(|source| source.path == "tests/__init__.py")
        .unwrap();
    let mut changed_guard = Source {
        baseline_revision: guard.baseline_revision.clone(),
        path: guard.path.clone(),
        sha256: guard.sha256.clone(),
        text: guard.text.clone(),
    };
    changed_guard
        .text
        .push_str("\nraise RuntimeError('must not execute')");
    assert!(
        runtime
            .block_on(_native::source_runtime_discovery::observe_owned(
                root,
                Path::new("missing-executable"),
                &directory,
                &changed_guard,
            ))
            .unwrap_err()
            .contains("guard identity drift")
    );
    let collected = runtime
        .block_on(_native::source_runtime_discovery::observe_owned(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-compat")),
            &directory,
            guard,
        ))
        .unwrap();
    assert_eq!(collected["source_revision"], sources[0].baseline_revision);
    assert_eq!(collected["source_runtime"], "CPython3.11.9");
    assert_eq!(collected["collection_only"], true);
    assert_eq!(collected["tests_run"], 0);
    assert_eq!(collected["guard_imported_first"], true);
    assert_eq!(collected["loader_errors"], 0);
    assert_eq!(
        collected["case_fields"],
        serde_json::json!([
            "id",
            "defined_in_file",
            "defined_in_hash",
            "code_line",
            "class_skipped",
            "code_qualname",
            "method_skipped"
        ])
    );
    let rows = collected["cases"].as_array().unwrap();
    assert!(
        !rows.is_empty(),
        "actual collection cannot be inferred from declaration counts"
    );
    let mut identities = std::collections::BTreeSet::new();
    for row in rows {
        assert_eq!(row.as_array().unwrap().len(), 7);
        assert!(identities.insert(row[0].as_str().unwrap()));
        let source = sources.iter().find(|source| source.path == row[1]).unwrap();
        assert_eq!(row[2], source.sha256);
        assert!(row[3].as_u64().is_some_and(|line| line > 0));
        assert!(row[4].is_boolean());
        assert!(row[5].as_str().is_some_and(|name| !name.is_empty()));
        assert!(row[6].is_boolean());
    }
    // These original decorators live on methods, not their class or unwrapped bodies.
    for (method, skipped) in [
        (
            "test_a_transient_windows_refusal_is_retried_to_completion",
            !cfg!(windows),
        ),
        (
            "test_a_persistent_refusal_still_fails_and_cleans_up",
            !cfg!(windows),
        ),
        (
            "test_rewriting_an_existing_file_keeps_its_permissions",
            !cfg!(unix),
        ),
    ] {
        let id = format!("tests.test_atomic_state_files.AtomicStateFiles.{method}");
        let row = rows.iter().find(|row| row[0] == id).unwrap();
        assert_eq!(row[4], false, "{id}");
        assert_eq!(row[6], skipped, "{id}");
    }
    let inherited: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/coverage-inherited.json"
    ))
    .unwrap();
    for row in inherited["cases"].as_array().unwrap() {
        let module = row["source_file"]
            .as_str()
            .unwrap()
            .strip_suffix(".py")
            .unwrap()
            .replace('/', ".");
        let id = format!("{module}.{}", row["source_case"].as_str().unwrap());
        assert!(identities.contains(id.as_str()));
        let observed = rows.iter().find(|observed| observed[0] == id).unwrap();
        assert_eq!(observed[1], row["source_file"]);
        assert_eq!(observed[2], row["source_hash"]);
        assert_eq!(observed[3], row["origin"]["method_line"]);
    }
    let joined =
        _native::source_runtime_discovery::reconcile(inventory, coverage, &collected).unwrap();
    assert_eq!(joined["runtime_cases"], rows.len());
    assert_eq!(joined["static_discovery_complete"], false);
    assert_eq!(joined["coverage_verified_by_collection"], 0);
    assert_eq!(joined["release_ready"], false);
    assert_eq!(
        joined["definition_matches"].as_array().unwrap().len()
            + joined["definition_gaps"].as_array().unwrap().len()
            + joined["runtime_only"].as_array().unwrap().len(),
        rows.len()
    );
    assert_eq!(
        joined["original_definition_matches"]
            .as_array()
            .unwrap()
            .len()
            + joined["original_definition_gaps"].as_array().unwrap().len(),
        rows.len()
    );
    for row in inherited["cases"].as_array().unwrap() {
        let module = row["source_file"]
            .as_str()
            .unwrap()
            .strip_suffix(".py")
            .unwrap()
            .replace('/', ".");
        let id = serde_json::json!(format!("{module}.{}", row["source_case"].as_str().unwrap()));
        assert!(
            joined["definition_matches"]
                .as_array()
                .unwrap()
                .contains(&id)
        );
        assert!(joined["observed_pending"].as_array().unwrap().contains(&id));
        let original = joined["original_definition_matches"]
            .as_array()
            .unwrap()
            .iter()
            .find(|observed| observed["id"] == id)
            .unwrap();
        assert_eq!(original["method_line"], row["origin"]["method_line"]);
        assert_eq!(
            original["class_identity"],
            row["origin"]["defined_in_identity"]
        );
    }
    // Keep the actual roster after successful envelope cleanup; this is not an authenticated receipt.
    let log = _native::logging::RunLog::open(root, "source-collection", vec![]);
    let path = log
        .path
        .as_ref()
        .expect("collection evidence needs a durable file");
    log.event("INFO", "collection", &format!("observation={collected}"));
    log.event("INFO", "collection", &format!("reconciliation={joined}"));
    log.finish(0);
    let retained = std::fs::read_to_string(path).unwrap();
    assert!(retained.contains(&format!("observation={collected}")));
    assert!(retained.contains(&format!("reconciliation={joined}")));
    println!(
        "original collection cases={} definition_matches={} gaps={} runtime_only={} unobserved={}; retained unverified observation at {}",
        rows.len(),
        joined["definition_matches"].as_array().unwrap().len(),
        joined["definition_gaps"].as_array().unwrap().len(),
        joined["runtime_only"].as_array().unwrap().len(),
        joined["static_unobserved"].as_array().unwrap().len(),
        path.display()
    );
    _native::source_import_replay::validate(root, &directory).unwrap();
    // Only these staged, byte-verified copies are disposable; Git/original sources stay intact.
    std::fs::remove_dir_all(directory).unwrap();
}
