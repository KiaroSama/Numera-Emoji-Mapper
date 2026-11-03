//! One admitted historical replay feeds the real local-ingest CLI comparison.
use _native::source_git_inventory::Source;
use serde_json::json;
use std::path::Path;

#[path = "local_ingest_cli.rs"]
mod local_ingest_cli;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = [
        "tests/reference/add_media.py",
        "tests/test_resume_safety.py",
        "tests/_pack_fixtures.py",
        "emojikit/cli_env.py",
        "emojikit/media.py",
        "emojikit/identity.py",
        "emojikit/errors.py",
        "tests/reference/catalog.py",
        "emojikit/media_paths.py",
        "tests/reference/ingest.py",
        "tests/_similarity_oracle.py",
    ]
    .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join(format!("local ingest refusal {}", std::process::id()));
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
                "path" => forged.path = "tests/unverified_local_ingest.py".into(),
                "claimed hash" => forged.sha256 = "0".repeat(64),
                "actual bytes" => forged
                    .text
                    .push_str("\nraise RuntimeError('must never execute')"),
                _ => unreachable!(),
            }
            let mut admitted = sources;
            admitted[selected] = &forged;
            assert!(
                _native::source_local_ingest_oracle::observe(root, &missing, &admitted)
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::observe_local_ingest(
                        root,
                        &parent.join("not-a-local-ingest-oracle-executable"),
                        &missing,
                        &admitted,
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!missing.exists());
        }
    }
    for count in 0..sources.len() {
        assert!(_native::source_local_ingest_oracle::source_identity(&sources[..count]).is_err());
    }
    let mut extra = sources.to_vec();
    extra.push(sources[0]);
    assert!(_native::source_local_ingest_oracle::source_identity(&extra).is_err());
    let workspace = parent.join(format!("original local ingest é {}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    let retained = workspace.join("caller.txt");
    std::fs::write(&retained, b"caller bytes survive refusal").unwrap();
    assert!(
        _native::source_local_ingest_oracle::observe(root, &workspace, &sources)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(
        std::fs::read(&retained).unwrap(),
        b"caller bytes survive refusal"
    );
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 1);
    assert!(
        _native::source_local_ingest_oracle::observe(root, root, &sources)
            .unwrap_err()
            .contains("project-contained workspace")
    );
    std::fs::remove_file(retained).unwrap();
    #[cfg(unix)]
    {
        let link = workspace.join("linked");
        std::os::unix::fs::symlink(&workspace, &link).unwrap();
        assert!(
            _native::source_local_ingest_oracle::observe(root, &link, &sources)
                .unwrap_err()
                .contains("linked/reparse")
        );
        std::fs::remove_file(link).unwrap();
    }
    let damaged_root = workspace.join("damaged-artwork-root");
    let assets = damaged_root.join("assets");
    let empty = damaged_root.join("scratch");
    std::fs::create_dir_all(&assets).unwrap();
    std::fs::create_dir(&empty).unwrap();
    assert!(
        _native::source_local_ingest_oracle::observe(&damaged_root, &empty, &sources)
            .unwrap_err()
            .contains("cannot open original artwork")
    );
    let artwork = assets.join("numera-emoji-mapper-logo.png");
    for bytes in [b"corrupt".to_vec(), vec![0; 277314]] {
        std::fs::write(&artwork, &bytes).unwrap();
        assert!(
            _native::source_local_ingest_oracle::observe(&damaged_root, &empty, &sources)
                .unwrap_err()
                .contains("artwork input identity drift")
        );
        assert_eq!(std::fs::read(&artwork).unwrap(), bytes);
        assert!(std::fs::read_dir(&empty).unwrap().next().is_none());
    }
    std::fs::remove_file(&artwork).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("assets/numera-emoji-mapper-logo.png"), &artwork)
            .unwrap();
        assert!(
            _native::source_local_ingest_oracle::observe(&damaged_root, &empty, &sources)
                .unwrap_err()
                .contains("artwork cannot follow links")
        );
        std::fs::remove_file(&artwork).unwrap();
    }
    std::fs::remove_dir(assets).unwrap();
    std::fs::remove_dir(empty).unwrap();
    std::fs::remove_dir(damaged_root).unwrap();
    let report = runtime
        .block_on(_native::source_oracle_worker::observe_local_ingest(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            &workspace,
            &sources,
        ))
        .unwrap();
    assert_eq!(report["baseline_revision"], sources[0].baseline_revision);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["Pillow"], "12.3.0");
    assert_eq!(report["tests_run"], 3);
    assert_eq!(
        report["original_methods"],
        json!([
            "AddMediaExitCode.test_all_inputs_unusable_exits_failed",
            "AddMediaExitCode.test_some_inputs_usable_exits_partial",
            "AddMediaExitCode.test_all_inputs_usable_exits_ok",
        ])
    );
    assert_eq!(
        report["source_hashes"].as_object().unwrap().len(),
        sources.len()
    );
    for source in sources {
        assert_eq!(report["source_hashes"][&source.path], source.sha256);
    }
    let observations = report["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 3);
    for (method, code, count) in [
        ("test_all_inputs_unusable_exits_failed", 4, 0),
        ("test_some_inputs_usable_exits_partial", 3, 1),
        ("test_all_inputs_usable_exits_ok", 0, 1),
    ] {
        let name = format!("AddMediaExitCode.{method}");
        let matching = observations
            .iter()
            .filter(|observation| observation["source_case"] == name)
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1, "{name}");
        let observation = matching[0];
        assert_eq!(observation["code"], code);
        let items = observation["items"].as_array().unwrap();
        assert_eq!(items.len(), count, "{name}");
        for item in items {
            // fingerprint catches dHash failures; the solid original fixture must not
            // silently produce None and turn a partial source replay into an oracle.
            assert_eq!(item["fmt"], "static");
            assert_eq!(item["phash"], 0);
            assert!(item["content_key"].as_str().unwrap().starts_with("s:"));
            assert_eq!(item["emojis"], json!(["😀"]));
            assert_eq!(item["keywords"], json!(["good"]));
            assert_eq!(item["sources"], json!(["local:good.png"]));
            assert_eq!(item["uploaded"], false);
            assert_eq!(item["included"], true);
            assert!(!item["media_bytes"].as_array().unwrap().is_empty());
        }
    }
    assert_eq!(report["artwork"]["size"], json!([100, 100]));
    assert_eq!(report["artwork"]["mode"], "RGBA");
    assert!(report["artwork"]["phash"].is_u64());
    assert!(
        report["artwork"]["content_key"]
            .as_str()
            .unwrap()
            .starts_with("s:")
    );
    assert_eq!(report["artwork"]["rgba_sha256"].as_str().unwrap().len(), 64);
    assert!(
        !report["artwork"]["media_bytes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    local_ingest_cli::check(root, runtime, &report);
    std::fs::remove_dir(workspace).unwrap();
}
