//! Original keyword assertions reuse the admitted corpus and bounded historical worker.
use _native::{source_git_inventory::Source, source_keywords_tests as oracle};
use serde_json::json;
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = oracle::source_paths()
        .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    oracle::source_identity(&sources).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join(format!("keyword source refusal {}", std::process::id()));
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
                "path" => changed.path = "tests/not-original-keywords.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                _ => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
            }
            let mut admitted = sources;
            admitted[index] = &changed;
            assert!(
                runtime
                    .block_on(oracle::observe_owned(root, &missing, &missing, &admitted,))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!missing.exists());
        }
    }
    let workspace = parent.join(format!("original keywords {}", std::process::id()));
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
    assert_eq!(actual["tests_run"], 3);
    assert_eq!(actual["temporary_calls"], 3);
    assert_eq!(
        actual["cases"],
        json!([
            "KeywordsFileHasOneWriter.test_a_crash_before_the_swap_leaves_the_old_file_intact",
            "KeywordsFileHasOneWriter.test_an_empty_new_name_keeps_the_old_one",
            "KeywordsFileHasOneWriter.test_an_invalid_cached_png_is_not_listed",
        ])
    );
    for field in [
        "errors",
        "failures",
        "skipped",
        "expected_failures",
        "unexpected_successes",
    ] {
        assert_eq!(actual[field], 0, "{field}");
    }
    for source in sources {
        assert_eq!(actual["source_hashes"][&source.path], source.sha256);
    }
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
}
