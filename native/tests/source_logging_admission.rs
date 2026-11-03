//! Claimed logging-source hashes cannot admit forged bodies into the historical worker.
use _native::source_git_inventory::Source;
use sha2::{Digest, Sha256};

#[test]
fn original_logging_replay_rejects_forged_bodies_before_python() {
    for self_consistent in [false, true] {
        let mut sources = [
            (
                "emojikit/logsetup.py",
                "97c34490fde4b280ac005e0095d187ff1a38904a25521f5680930d915367eca6",
            ),
            (
                "tests/test_logsetup.py",
                "93a6bfb0cb7fea9679dfbab3014aaa1e9c273d59235c4c807e2e83a1cd8c4405",
            ),
        ]
        .map(|(path, hash)| Source {
            baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
            path: path.into(),
            sha256: hash.into(),
            text: "raise RuntimeError('untrusted source must never execute')".into(),
        });
        if self_consistent {
            for source in &mut sources {
                source.sha256 = Sha256::digest(source.text.as_bytes())
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect();
            }
        }
        assert!(
            _native::source_logging_oracle::source_identity(&sources[0], &sources[1])
                .unwrap_err()
                .contains("source identity drift")
        );
        assert!(
            _native::source_logging_oracle::generate(&sources[0], &sources[1])
                .unwrap_err()
                .contains("source identity drift")
        );
    }
}

#[test]
fn original_secret_scan_refuses_forged_sources_before_workspace_or_python() {
    let sources = [
        (
            "emojikit/logsetup.py",
            "97c34490fde4b280ac005e0095d187ff1a38904a25521f5680930d915367eca6",
        ),
        (
            "tests/test_logsetup.py",
            "93a6bfb0cb7fea9679dfbab3014aaa1e9c273d59235c4c807e2e83a1cd8c4405",
        ),
    ]
    .map(|(path, sha256)| Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: sha256.into(),
        text: "raise RuntimeError('untrusted source must never execute')".into(),
    });
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let workspace = root.join("logs/tmp/forged-secret-scan-must-not-create");
    assert!(!workspace.exists());
    assert!(
        _native::source_logging_secrets::generate(root, &workspace, &sources[0], &sources[1])
            .unwrap_err()
            .contains("source identity drift")
    );
    assert!(!workspace.exists());
}

#[test]
fn original_config_replay_refuses_forged_sources_before_workspace_or_python() {
    let sources = [
        (
            "emojikit/cli_env.py",
            "9e4c0a045f031bffd23fe5ff9e51a132363e9952a1cdde47d071bfc81234e887",
        ),
        (
            "tests/test_cli_env.py",
            "671c810ad43670f0fbe6cd1833b6b05b606c061a03f19899da20c1924555be99",
        ),
        (
            "tests/test_resume_safety.py",
            "0af175ef5368ed7a6613ce487f65ff5b8f42e1751fcfa9e787b17faea956b3d4",
        ),
        (
            "tests/reference/announce.py",
            "a10d73deec77e1de3a7b1e64b8c78dbde43bb5e5af031aee38d26fe7a85db4c8",
        ),
    ]
    .map(|(path, sha256)| Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: sha256.into(),
        text: "raise RuntimeError('untrusted source must never execute')".into(),
    });
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let workspace = root.join("logs/tmp/forged-config-source-must-not-create");
    assert!(!workspace.exists());
    assert!(
        _native::source_config_oracle::generate(
            root,
            &workspace,
            &sources[0],
            &sources[1],
            &sources[2],
            &sources[3]
        )
        .unwrap_err()
        .contains("source identity drift")
    );
    assert!(!workspace.exists());
}

#[test]
fn original_local_ingest_refuses_missing_sources_before_workspace_or_python() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let workspace = root.join("logs/tmp/missing-local-ingest-source-must-not-create");
    assert!(!workspace.exists());
    assert!(
        _native::source_local_ingest_oracle::observe(root, &workspace, &[])
            .unwrap_err()
            .contains("exactly eleven pinned sources")
    );
    assert!(!workspace.exists());
}

#[test]
fn original_retention_refuses_forged_environment_source_before_workspace_or_python() {
    let sources = [
        (
            "emojikit/logsetup.py",
            "97c34490fde4b280ac005e0095d187ff1a38904a25521f5680930d915367eca6",
        ),
        (
            "tests/test_logsetup.py",
            "93a6bfb0cb7fea9679dfbab3014aaa1e9c273d59235c4c807e2e83a1cd8c4405",
        ),
        (
            "emojikit/cli_env.py",
            "9e4c0a045f031bffd23fe5ff9e51a132363e9952a1cdde47d071bfc81234e887",
        ),
    ]
    .map(|(path, sha256)| Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: sha256.into(),
        text: "raise RuntimeError('untrusted source must never execute')".into(),
    });
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let workspace = root.join("logs/tmp/forged-logging-source-must-not-create");
    assert!(!workspace.exists());
    assert!(
        _native::source_logging_oracle::generate_retention(
            root,
            &workspace,
            &[&sources[0], &sources[1], &sources[2]]
        )
        .unwrap_err()
        .contains("source identity drift")
    );
    assert!(!workspace.exists());
}
