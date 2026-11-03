//! Pure admission refusals; positive original-source observations belong to the shared Git worker.
use _native::{source_benchmark_oracle, source_git_inventory::Source};

#[test]
fn claimed_benchmark_hashes_never_authorize_untrusted_source_execution() {
    let sources = [
        (
            "scripts/benchmark_similarity.py",
            "9240f9b4e83c72db4a20910b8618564e198fddd8f8033f92ca4d6b4fcd6a65ab",
        ),
        (
            "tests/_similarity_oracle.py",
            "9be48166b64ab2052798c67e2f7426c2dd0008fc27b4d668a785877fba7ac44e",
        ),
    ]
    .map(|(path, sha256)| Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: sha256.into(),
        text: "raise RuntimeError('untrusted source must never execute')".into(),
    });
    // Admission checks bytes as well as claimed metadata, before interpreter initialization.
    assert!(
        source_benchmark_oracle::source_identity(&sources[0], &sources[1])
            .unwrap_err()
            .contains("source identity drift")
    );
    assert!(
        source_benchmark_oracle::generate(&sources[0], &sources[1])
            .unwrap_err()
            .contains("source identity drift")
    );
}
