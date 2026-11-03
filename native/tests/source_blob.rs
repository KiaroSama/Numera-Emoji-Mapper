//! Explicit historical selections use the same owning-repository policy as the full corpus.
use _native::source_git_inventory::{read_source, read_sources};
use std::path::Path;

#[test]
fn invalid_source_selection_refuses_before_git_or_workspace_creation() {
    let root = Path::new("must-not-be-inspected-before-source-admission");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let baseline = "1a36a968b006236bd0665a1da65c541938c06061";
    for (revision, path) in [
        ("HEAD", "tests/_similarity_oracle.py"),
        (baseline, "../outside.py"),
        (baseline, "/absolute.py"),
        (baseline, "tests/../outside.py"),
        (baseline, "tests/file.txt"),
        (baseline, "tests/file\ncommand.py"),
    ] {
        assert!(runtime.block_on(read_source(root, revision, path)).is_err());
        assert!(
            runtime
                .block_on(read_sources(root, revision, &[path]))
                .is_err()
        );
    }
    for paths in [
        vec![],
        vec!["tests/a.py", "tests/a.py"],
        vec!["tests/a.py"; 4097],
    ] {
        assert!(
            runtime
                .block_on(read_sources(root, baseline, &paths))
                .is_err()
        );
    }
}
