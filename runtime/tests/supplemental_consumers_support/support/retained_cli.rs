//! Real copied standalone CLI fixture, with no interpreter acquisition or compatibility closure.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn install(project: &Path, root: &Path) -> (PathBuf, BTreeMap<String, String>) {
    let project = _native::tooling_environment::physical_root(project).unwrap();
    let root = _native::tooling_environment::physical_root(root).unwrap();
    assert!(root.starts_with(project.join("logs/tmp")));
    assert_ne!(root, project.join("logs/tmp"));
    assert!(std::fs::read_dir(&root).unwrap().next().is_none());
    std::fs::create_dir(root.join("assets")).unwrap();
    std::fs::copy(
        project.join("assets/panel.html"),
        root.join("assets/panel.html"),
    )
    .unwrap();
    let executable = root.join(format!("numera-emoji{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let mut environment = crate::fixture::test_environment();
    // The original installed fixture preserved these deterministic outer-runner
    // settings. Keep archive admission valid so the interrupted-journal refusal,
    // rather than a missing-configuration error, is the behavior under test.
    for name in ["COLLECTION_PACK_BASE", "COIN_PACK_BASE", "COIN_PACK_TITLE"] {
        environment.insert(
            name.into(),
            std::env::var(name).expect("isolated runner fixture setting"),
        );
    }
    (executable, environment)
}
