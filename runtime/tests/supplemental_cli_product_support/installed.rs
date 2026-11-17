//! Actual standalone executable installation; no retained extension or interpreter.
use numera_runtime as rt;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
pub fn install(project: &Path, root: &Path) -> (PathBuf, BTreeMap<String, String>) {
    let project = rt::tooling_environment::physical_root(project).unwrap();
    let root = rt::tooling_environment::physical_root(root).unwrap();
    assert!(root.starts_with(project.join("logs/tmp")));
    assert_ne!(root, project.join("logs/tmp"));
    assert!(std::fs::read_dir(&root).unwrap().next().is_none());
    std::fs::create_dir(root.join("assets")).unwrap();
    std::fs::copy(
        project.join("assets/panel.html"),
        root.join("assets/panel.html"),
    )
    .unwrap();
    let binary = root.join(format!("numera-emoji{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &binary).unwrap();
    let environment = crate::test_environment();
    let environment = rt::tooling_environment::environment(&root, &environment, false).unwrap();
    (binary, environment)
}
