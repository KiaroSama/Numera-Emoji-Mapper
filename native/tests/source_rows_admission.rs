//! Forged shared-row sources must refuse before interpreter initialization or source execution.
use _native::source_git_inventory::Source;
use sha2::{Digest, Sha256};

fn forged(path: &str, claimed: &str) -> Source {
    Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: claimed.into(),
        text: "raise AssertionError('synthetic source must never execute')".into(),
    }
}

#[test]
fn shared_rows_generation_refuses_synthetic_sources_before_python() {
    for mutation in [
        "claimed hashes",
        "self-consistent hashes",
        "revision",
        "paths",
        "empty hash",
    ] {
        let mut rows = forged(
            "tests/oracles/pack_rows.py",
            "1c0c0fc15ab5b6abcecb182f138f89da55c566c7210102c1eaaa5ad115343121",
        );
        let mut test = forged(
            "tests/test_pack_rows.py",
            "800c96312877a469e224798dd456885d2e3bea4e2dc5077127bd1e91f6a19503",
        );
        match mutation {
            "claimed hashes" => {}
            "self-consistent hashes" => {
                for source in [&mut rows, &mut test] {
                    source.sha256 = Sha256::digest(source.text.as_bytes())
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect();
                }
            }
            "revision" => {
                rows.baseline_revision = "0".repeat(40);
                test.baseline_revision = "0".repeat(40);
            }
            "paths" => {
                rows.path = "tests/oracles/unverified_rows.py".into();
                test.path = "tests/unverified_rows.py".into();
            }
            "empty hash" => {
                rows.sha256.clear();
                test.sha256.clear();
            }
            _ => unreachable!(),
        }
        assert!(
            _native::source_rows_oracle::source_identity(&rows, &test)
                .unwrap_err()
                .contains("source identity drift"),
            "{mutation}"
        );
        assert!(
            _native::source_rows_oracle::generate(&rows, &test)
                .unwrap_err()
                .contains("source identity drift"),
            "{mutation}"
        );
    }
    // Real admitted blobs, independent per-source tampering and actual generated
    // observations belong to the parent's existing single historical Git snapshot.
}
