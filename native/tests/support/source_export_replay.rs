//! Original export assertions reuse the admitted corpus and one bounded historical worker.
use _native::{source_export_tests as oracle, source_git_inventory::Source};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = [
        "tests/reference/pack_export.py",
        "tests/test_pack_export.py",
        "tests/_media_fixtures.py",
        "tests/reference/pack_archive.py",
        "tests/oracles/pack_rows.py",
        "emojikit/media_paths.py",
        "emojikit/operator_config.py",
        "emojikit/errors.py",
        "tests/reference/packstate.py",
    ]
    .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    oracle::source_identity(&sources).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let absent = parent.join(format!("export source refusal {}", std::process::id()));
    assert!(!absent.exists());
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
                "path" => changed.path = "tests/unverified_export.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                "bytes" => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
                _ => unreachable!(),
            }
            let mut admitted = sources;
            admitted[index] = &changed;
            assert!(
                oracle::observe(root, &absent, &admitted)
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(
                runtime
                    .block_on(oracle::observe_owned(root, &absent, &absent, &admitted))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!absent.exists());
        }
    }
    let workspace = parent.join(format!("original export café {}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    let caller = workspace.join("caller");
    std::fs::write(&caller, b"caller bytes").unwrap();
    assert!(
        oracle::observe(root, &workspace, &sources)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(std::fs::read(&caller).unwrap(), b"caller bytes");
    std::fs::remove_file(caller).unwrap();
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
    assert_eq!(report["Pillow"], "12.3.0");
    assert_eq!(report["tests_run"], 4);
    assert_eq!(report["temporary_calls"], 4);
    assert_eq!(report["codes"], json!([0, 0, 3, 1]));
    for source in sources {
        assert_eq!(report["source_hashes"][&source.path], source.sha256);
    }
    for field in [
        "errors",
        "failures",
        "skipped",
        "expected_failures",
        "unexpected_successes",
    ] {
        assert_eq!(report[field], 0);
    }
    let coverage: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/native/coverage.json")).unwrap();
    let expected = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["source_file"] == sources[1].path)
        .map(|case| case["source_case"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    assert_eq!(
        cases
            .iter()
            .map(|case| case.as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        expected
    );
    let outputs = report["outputs"].as_array().unwrap();
    assert_eq!(outputs.len(), 4);
    for (index, output) in outputs.iter().enumerate() {
        assert_eq!(output["case"], cases[index]);
        let entries = output["entries"].as_array().unwrap();
        if index >= 2 {
            assert!(entries.is_empty());
            continue;
        }
        assert_eq!(entries.len(), 4);
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "001_logo.png",
                "002_video_bbbbbbbbbbbb.webm",
                "003_static_aaaaaaaaaaaa.webp",
                "_manifest.md"
            ]
        );
        let manifest = entries[3]["text"].as_str().unwrap();
        assert!(manifest.contains("beta") && manifest.contains("alpha"));
        assert!(manifest.find("beta").unwrap() < manifest.find("alpha").unwrap());
        assert!(!manifest.contains("001_logo.png"));
        assert_eq!(entries[0]["sha256"], entries[2]["sha256"]);
        for entry in entries {
            let hash = entry["sha256"].as_str().unwrap();
            assert_eq!(hash.len(), 64);
            assert!(
                hash.bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            );
        }
    }
    assert_eq!(outputs[0]["entries"], outputs[1]["entries"]);
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
}
