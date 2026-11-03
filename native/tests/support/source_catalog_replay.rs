//! Historical catalog observations and native storage share one pinned Git corpus and fixture paths.
use _native::{catalog::Catalog, locks::Lease, source_git_inventory::Source};
use serde_json::{Value, json};
use std::path::Path;

#[path = "registered_catalog_replay.rs"]
mod registered_catalog_replay;

fn check_original_cases(
    root: &Path,
    runtime: &tokio::runtime::Runtime,
    corpus: &[Source],
    parent: &Path,
) {
    let paths = [
        "tests/reference/catalog.py",
        "emojikit/media_paths.py",
        "tests/test_catalog.py",
        "tests/reference/ingest.py",
        "tests/_similarity_oracle.py",
        "emojikit/errors.py",
        "tests/test_catalog_order.py",
        "tests/test_media_paths.py",
        "emojikit/similarity.py",
        "tests/test_native_similarity.py",
        "tests/test_native_catalog.py",
        "tests/reference/maintenance.py",
        "tests/reference/packstate.py",
        "tests/test_native_real_catalog.py",
        "emojikit/sqlite_snapshot.py",
    ];
    let sources = paths.map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    _native::source_catalog_tests_oracle::source_identity(&sources).unwrap();
    let missing = parent.join(format!("catalog allcases refusal {}", std::process::id()));
    assert!(!missing.exists());
    for selected in 0..sources.len() {
        for mutation in ["revision", "path", "claimed hash", "actual bytes"] {
            let source = sources[selected];
            let mut changed = Source {
                baseline_revision: source.baseline_revision.clone(),
                path: source.path.clone(),
                sha256: source.sha256.clone(),
                text: source.text.clone(),
            };
            match mutation {
                "revision" => changed.baseline_revision = "0".repeat(40),
                "path" => changed.path = "unverified.py".into(),
                "claimed hash" => changed.sha256 = "0".repeat(64),
                _ => changed
                    .text
                    .push_str("\nraise RuntimeError('must never execute')"),
            }
            let mut admitted = sources;
            admitted[selected] = &changed;
            assert!(_native::source_catalog_tests_oracle::source_identity(&admitted).is_err());
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::observe_catalog_tests(
                        root,
                        &parent.join("not-a-catalog-executable"),
                        &missing,
                        &admitted,
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!missing.exists());
        }
    }
    let workspace = parent.join(format!("original catalog cases {}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    let retained = workspace.join("caller-data.txt");
    std::fs::write(&retained, b"caller data must survive admission refusal").unwrap();
    assert!(
        _native::source_catalog_tests_oracle::observe(root, &workspace, &sources)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(
        std::fs::read(&retained).unwrap(),
        b"caller data must survive admission refusal"
    );
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 1);
    assert!(
        _native::source_catalog_tests_oracle::observe(root, root, &sources)
            .unwrap_err()
            .contains("project-contained workspace")
    );
    std::fs::remove_file(retained).unwrap();
    #[cfg(unix)]
    {
        let link = workspace.join("linked workspace");
        std::os::unix::fs::symlink(&workspace, &link).unwrap();
        assert!(
            _native::source_catalog_tests_oracle::observe(root, &link, &sources)
                .unwrap_err()
                .contains("linked/reparse")
        );
        assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 1);
        std::fs::remove_file(link).unwrap();
    }
    let observed = runtime
        .block_on(_native::source_oracle_worker::observe_catalog_tests(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            &workspace,
            &sources,
        ))
        .unwrap();
    assert_eq!(
        observed["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(observed["source_runtime"], "CPython3.11.9");
    assert_eq!(observed["tests_run"], 35);
    let real = &observed["real_catalog"];
    if std::env::var_os("NUMERA_REAL_CATALOG_FIXTURE").is_some() {
        assert_eq!(real["selected"], true);
        assert_eq!(real["tests_run"], 1);
        assert_eq!(real["source_unchanged"], true);
        assert_eq!(
            real["case"],
            "RealCatalogReplay.test_copied_operator_rows_replay_and_python_reads_native_changes"
        );
    } else {
        assert_eq!(real["selected"], false);
        assert_eq!(real["tests_run"], 0);
        assert!(
            real["reason"]
                .as_str()
                .unwrap()
                .contains("no real-data parity claim")
        );
    }
    let combined = &observed["combined_binding"];
    assert_eq!(combined["tests_run"], 1);
    assert_eq!(
        combined["cases"],
        json!([
            "NativeCatalogParity.test_order_inclusion_signed_hash_and_family_publications_match_source"
        ])
    );
    for field in ["errors", "failures", "skipped"] {
        assert_eq!(combined[field], 0);
    }
    for field in ["native_imports", "identity_patches"] {
        assert_eq!(combined[field], 1);
    }
    let prerequisite = &observed["missing_native"];
    assert_eq!(prerequisite["tests_run"], 2);
    assert_eq!(prerequisite["temporary_calls"], 1);
    assert_eq!(
        prerequisite["cases"],
        json!([
            "NativeSimilarity.test_missing_module_never_uses_a_python_fallback",
            "NativeSimilarity.test_missing_native_cannot_create_catalog_state",
        ])
    );
    for field in ["errors", "failures", "skipped"] {
        assert_eq!(prerequisite[field], 0);
    }
    assert_eq!(observed["Pillow"], "12.3.0");
    for field in ["errors", "failures"] {
        assert_eq!(observed[field], 0);
    }
    let platform = if cfg!(windows) {
        "win32"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    };
    assert_eq!(observed["source_platform"], platform);
    let skipped = if cfg!(windows) {
        json!([])
    } else {
        json!([{"source_case":"MediaPathTests.test_windows_case_does_not_make_a_path_outside",
            "reason":"case-insensitive paths are a Windows property"}])
    };
    assert_eq!(observed["skipped"], skipped.as_array().unwrap().len());
    assert_eq!(observed["skipped_cases"], skipped);
    let coverage: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/native/coverage.json")).unwrap();
    let mut expected = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| {
            matches!(
                case["source_file"].as_str(),
                Some(
                    "tests/test_catalog.py"
                        | "tests/test_catalog_order.py"
                        | "tests/test_media_paths.py"
                )
            )
        })
        .map(|case| case["source_case"].as_str().unwrap())
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(observed["cases"], json!(expected));
    for source in sources {
        assert_eq!(observed["source_hashes"][&source.path], source.sha256);
    }
    assert_eq!(
        observed["boundary_calls"],
        json!({"prerequisite":48,"writer":47,"enter":47,"exit":47,"temporary":35})
    );
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
}

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, sources: &[Source]) {
    let get = |path: &str| sources.iter().find(|source| source.path == path).unwrap();
    let catalog = get("tests/reference/catalog.py");
    let paths = get("emojikit/media_paths.py");
    let original = get("tests/test_native_catalog.py");
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    check_original_cases(root, runtime, sources, &parent);
    let workspace = parent.join(format!("catalog source replay {}", std::process::id()));
    let missing = parent.join(format!(
        "catalog tamper must not create {}",
        std::process::id()
    ));
    assert!(!missing.exists());
    for selected in 0..3 {
        for mutation in ["revision", "path", "claimed hash", "actual bytes"] {
            let source = [catalog, paths, original][selected];
            let mut changed = Source {
                baseline_revision: source.baseline_revision.clone(),
                path: source.path.clone(),
                sha256: source.sha256.clone(),
                text: source.text.clone(),
            };
            match mutation {
                "revision" => changed.baseline_revision = "0".repeat(40),
                "path" => changed.path = "unverified.py".into(),
                "claimed hash" => changed.sha256 = "0".repeat(64),
                _ => changed
                    .text
                    .push_str("\nraise RuntimeError('must never execute')"),
            }
            let mut admitted = [catalog, paths, original];
            admitted[selected] = &changed;
            assert!(
                _native::source_catalog_oracle::source_identity(
                    admitted[0],
                    admitted[1],
                    admitted[2]
                )
                .is_err()
            );
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::observe_catalog(
                        root,
                        &parent.join("not-a-catalog-executable"),
                        &missing,
                        admitted[0],
                        admitted[1],
                        admitted[2]
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!missing.exists());
        }
    }
    std::fs::create_dir(&workspace).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let report = runtime
            .block_on(_native::source_oracle_worker::observe_catalog(
                root,
                Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
                &workspace,
                catalog,
                paths,
                original,
            ))
            .unwrap();
        assert_eq!(report["source_hash"], catalog.sha256);
        assert_eq!(report["paths_source_hash"], paths.sha256);
        assert_eq!(report["test_source_hash"], original.sha256);
        assert_eq!(report["source_runtime"], "CPython3.11.9");
        assert_eq!(
            report["stub_calls"],
            json!({"require_native":1,"writer":1,"enter":1,"exit":1,"catalog_identity_keys":["a","b","c"]})
        );
        assert_eq!(
            std::fs::read_dir(&workspace).unwrap().count(),
            0,
            "source database must be closed and cleaned"
        );
        registered_catalog_replay::check(root, &workspace, &report);
        let data = workspace.join("new");
        let database = data.join("catalog.db");
        let mut native = Catalog::open(&database, "2026-10-07 00:00:00 UTC", root).unwrap();
        for (key, phash) in [("a", Some(u64::MAX)), ("b", Some(0)), ("c", None)] {
            assert!(native.insert(&json!({"content_key":key,"fmt":"static","file_path":workspace.join(format!("{key}.png")),"emojis":["😀"],"phash":phash}), "2026-10-07 00:00:00 UTC").unwrap());
        }
        let order = native
            .order(&["c".into(), "a".into(), "missing".into()])
            .unwrap();
        let inclusion = native.inclusion(&["b".into()]).unwrap();
        for (id, set) in [(Some("111111111"), Some("fixture1")), (None, None)] {
            native.uploaded(&json!({"content_key":"a","custom_emoji_id":id,"base":"fixture","set_name":set}), "2026-10-07 00:00:00 UTC").unwrap();
        }
        let actual = json!([
            order,
            inclusion,
            native.all(None).unwrap(),
            native.pending(None, Some("fixture")).unwrap(),
            native.pending(None, Some("other")).unwrap(),
            native
                .dispatch(json!({"operation":"custom_id","base":"fixture","content_key":"a"}))
                .unwrap(),
            native.stats().unwrap()
        ]);
        assert_eq!(actual, report["observation"]);
        // A distinct OS lease is a real exclusion check; the source generator's writer is only a stub.
        let lock = data.join(".maintenance.lock");
        assert!(Lease::acquire(&lock, "contender").is_err());
        drop(native);
        assert!(
            lock.is_file(),
            "stable lock target must survive catalog close"
        );
        drop(Lease::acquire(&lock, "released").unwrap());
        let moved = data.join("closed.db");
        std::fs::rename(&database, &moved).unwrap();
        std::fs::write(workspace.join("keep.txt"), b"caller data").unwrap();
        assert!(
            _native::source_catalog_oracle::observe(root, &workspace, catalog, paths, original)
                .unwrap_err()
                .contains("empty")
        );
        assert_eq!(
            std::fs::read(workspace.join("keep.txt")).unwrap(),
            b"caller data"
        );
    }));
    // Failed source/target ownership retains its inputs and diagnostic database, never erased on unwind.
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    std::fs::remove_dir_all(workspace).unwrap();
}
