//! Original rebuild state/lock/plan methods reuse the shared Git corpus and bounded worker.
use _native::{source_git_inventory::Source, source_rebuild_tests as oracle};
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = oracle::source_paths()
        .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    oracle::source_identity(&sources).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join(format!("rebuild source refusal {}", std::process::id()));
    assert!(!missing.exists());
    for index in 0..sources.len() {
        for mutation in ["revision", "path", "hash", "bytes"] {
            let original = sources[index];
            let mut changed = Source {
                baseline_revision: original.baseline_revision.clone(),
                path: original.path.clone(),
                sha256: original.sha256.clone(),
                text: original.text.clone(),
            };
            match mutation {
                "revision" => changed.baseline_revision = "0".repeat(40),
                "path" => changed.path = "tests/not-original-rebuild.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                _ => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
            }
            let mut admitted = sources;
            admitted[index] = &changed;
            assert!(
                runtime
                    .block_on(oracle::observe_owned(root, &missing, &missing, &admitted))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!missing.exists());
        }
    }
    let workspace = parent.join(format!(
        "original rebuild assertions {}",
        std::process::id()
    ));
    std::fs::create_dir(&workspace).unwrap();
    let caller = workspace.join("caller");
    std::fs::write(&caller, b"caller bytes").unwrap();
    assert!(
        oracle::observe(root, &workspace, &sources)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(std::fs::read(&caller).unwrap(), b"caller bytes");
    assert!(
        oracle::observe(root, root, &sources)
            .unwrap_err()
            .contains("inside project scratch")
    );
    std::fs::remove_file(caller).unwrap();
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
    let actual = runtime
        .block_on(oracle::observe_owned(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            &workspace,
            &sources,
        ))
        .unwrap();
    assert_eq!(actual["source_revision"], sources[0].baseline_revision);
    assert_eq!(actual["source_runtime"], "CPython3.11.9");
    assert_eq!(actual["Pillow"], "12.3.0");
    assert_eq!(actual["tests_run"], 76);
    assert_eq!(actual["owner_reloads"], 4);
    assert_eq!(actual["primary_contexts"], 70);
    assert_eq!(actual["proof_contexts"], 16);
    assert_eq!(actual["comparator_imports"], 4);
    assert_eq!(actual["map_imports"], 3);
    assert_eq!(actual["temporary_calls"], 86);
    for field in [
        "errors",
        "failures",
        "skipped",
        "expected_failures",
        "unexpected_successes",
    ] {
        assert_eq!(actual[field], 0);
    }
    assert_eq!(
        actual["source_hashes"].as_object().unwrap().len(),
        sources.len()
    );
    for source in sources {
        assert_eq!(actual["source_hashes"][&source.path], source.sha256);
    }
    let inventory = sources[8..]
        .iter()
        .filter(|source| source.path.starts_with("tests/test_"))
        .map(|source| _native::source_test_inventory::parse(&source.path, &source.text).unwrap())
        .collect::<Vec<_>>();
    let excluded =
        ["SharedLogoGuard.test_the_real_committed_map_is_checked_against_the_real_groups"];
    let mut expected = inventory
        .iter()
        .flat_map(|file| file["classes"].as_array().unwrap())
        .flat_map(|class| {
            class["methods"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|method| method["name"].as_str().unwrap().starts_with("test_"))
                .map(move |method| {
                    format!(
                        "{}.{}",
                        class["name"].as_str().unwrap(),
                        method["name"].as_str().unwrap()
                    )
                })
        })
        .filter(|name| !excluded.contains(&name.as_str()))
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(expected.len(), 76);
    let mut observed = actual["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    observed.sort();
    assert_eq!(observed, expected);
    assert_eq!(actual["excluded"], serde_json::json!(excluded));
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
}
