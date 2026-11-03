//! Original inventory assertions and independent observations share the admitted Git corpus.
use _native::source_git_inventory::Source;
use serde_json::json;
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = [
        "tests/reference/fetch_emoji_ids.py",
        "tests/test_fetch_emoji_ids.py",
    ]
    .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join("unverified-inventory-must-not-create");
    assert!(!missing.exists());
    for index in 0..2 {
        for change in ["revision", "path", "hash", "bytes"] {
            let original = sources[index];
            let mut forged = Source {
                baseline_revision: original.baseline_revision.clone(),
                path: original.path.clone(),
                sha256: original.sha256.clone(),
                text: original.text.clone(),
            };
            match change {
                "revision" => forged.baseline_revision = "0".repeat(40),
                "path" => forged.path = "tests/not-the-inventory-source.py".into(),
                "hash" => forged.sha256 = "0".repeat(64),
                "bytes" => forged
                    .text
                    .push_str("\nraise RuntimeError('must not execute')"),
                _ => unreachable!(),
            }
            let mut selected = sources;
            selected[index] = &forged;
            assert!(
                _native::source_emoji_ids_oracle::observe(root, &missing, selected[0], selected[1])
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::observe_emoji_ids(
                        root,
                        &missing,
                        &missing,
                        selected[0],
                        selected[1],
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!missing.exists());
        }
    }
    let workspace = parent.join(format!("original inventory é {}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    let sentinel = workspace.join("caller.txt");
    std::fs::write(&sentinel, b"caller data survives").unwrap();
    assert!(
        _native::source_emoji_ids_oracle::observe(root, &workspace, sources[0], sources[1])
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"caller data survives");
    std::fs::remove_file(sentinel).unwrap();
    assert!(_native::source_emoji_ids_oracle::observe(root, root, sources[0], sources[1]).is_err());
    #[cfg(unix)]
    {
        let link = workspace.join("linked");
        std::os::unix::fs::symlink(&workspace, &link).unwrap();
        assert!(
            _native::source_emoji_ids_oracle::observe(root, &link, sources[0], sources[1])
                .unwrap_err()
                .contains("linked/reparse")
        );
        std::fs::remove_file(link).unwrap();
    }
    let report = runtime
        .block_on(_native::source_oracle_worker::observe_emoji_ids(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            &workspace,
            sources[0],
            sources[1],
        ))
        .unwrap();
    assert_eq!(report["source_revision"], sources[0].baseline_revision);
    assert_eq!(report["source_hash"], sources[0].sha256);
    assert_eq!(report["test_source_hash"], sources[1].sha256);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    let expected = [
        "ExtractRealIds.test_bare_entry_lines",
        "ExtractRealIds.test_example_prose_line_is_ignored",
        "ExtractRealIds.test_trailing_label_and_emoji_still_captured",
        "ExtractRealIds.test_bullet_and_spacing_variants",
        "ExtractRealIds.test_bare_number_list_fallback",
        "ExtractRealIds.test_short_or_nonnumeric_noise_ignored",
        "ExtractRealIds.test_overlong_number_not_truncated",
        "WithinFileDuplicates.test_detects_true_within_file_duplicate",
        "WithinFileDuplicates.test_example_line_is_not_a_duplicate",
        "CollectIdsDedup.test_cross_file_dedup_preserves_first_seen_order",
        "CollectIdsDedup.test_within_and_cross_file_and_inline_all_collapse",
        "CollectIdsDedup.test_example_ids_never_downloaded",
    ];
    assert_eq!(report["cases"], json!(expected));
    assert_eq!(report["tests_run"], 12);
    for field in [
        "errors",
        "failures",
        "skipped",
        "expected_failures",
        "unexpected_successes",
    ] {
        assert_eq!(report[field], 0, "{field}");
    }
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 0);
    let entries = report["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 8);
    for case in entries {
        let text = case["text"].as_str().unwrap();
        assert_eq!(json!(_native::emoji_ids::entries(text)), case["ids"]);
        assert_eq!(
            json!(_native::emoji_ids::within_file_duplicates(text)),
            case["duplicates"]
        );
    }
    let collections = report["collections"].as_array().unwrap();
    assert_eq!(collections.len(), 5);
    for case in collections {
        let files = case["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| {
                let path = workspace.join(file["name"].as_str().unwrap());
                let text = file["text"].as_str().unwrap();
                std::fs::write(&path, text.as_bytes()).unwrap();
                path.to_str().unwrap().to_owned()
            })
            .collect::<Vec<_>>();
        let inline: Vec<String> = serde_json::from_value(case["inline"].clone()).unwrap();
        let mut messages = Vec::new();
        let actual = _native::emoji_ids::collect_with_report(&files, &inline, |message| {
            messages.push(message.to_owned());
        })
        .unwrap();
        assert_eq!(json!(actual), case["ids"], "{}", case["name"]);
        assert_eq!(json!(messages), case["messages"], "{}", case["name"]);
        for (path, file) in files.iter().zip(case["files"].as_array().unwrap()) {
            assert_eq!(
                std::fs::read(path).unwrap(),
                file["text"].as_str().unwrap().as_bytes()
            );
            std::fs::remove_file(path).unwrap();
        }
    }
    assert!(report["ports"].as_str().unwrap().contains("strict UTF-8"));
    // Failed comparisons retain the owned workspace; no source module is imported here.
    std::fs::remove_dir(workspace).expect("clean verified inventory replay workspace");
}
