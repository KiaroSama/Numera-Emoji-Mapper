//! Bounded original provider/remap artifact admission and success-only teardown.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::{directories, required};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub(super) struct Groups<'a> {
    pub primary: &'a BTreeSet<PathBuf>,
    pub inventory: &'a BTreeSet<PathBuf>,
    pub maps: &'a BTreeSet<PathBuf>,
    pub remap: &'a BTreeSet<PathBuf>,
    pub verify: &'a BTreeSet<PathBuf>,
    pub replacements: &'a BTreeSet<PathBuf>,
}

pub(super) fn clean(
    workspace: &Path,
    source: &Source,
    bindings: &Bound<'_, PyDict>,
    paths: &[PathBuf],
    groups: &Groups<'_>,
) -> PyResult<()> {
    let Groups {
        primary,
        inventory,
        maps,
        remap,
        verify,
        replacements,
    } = groups;
    let py = bindings.py();
    let actual = std::fs::read_dir(workspace)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    let source_path = workspace.join("provider-source.txt");
    let metadata = std::fs::symlink_metadata(&source_path)?;
    if !metadata.is_file()
        || crate::tooling_environment::reject_links(&source_path, "linked provider source").is_err()
        || metadata.len() != source.text.len() as u64
        || std::fs::read(&source_path)? != source.text.as_bytes()
    {
        return Err(PyValueError::new_err(
            "original provider source bytes drift; retained",
        ));
    }
    let mut expected = paths.iter().cloned().collect::<BTreeSet<_>>();
    expected.insert(workspace.join(".locks"));
    expected.insert(source_path.clone());
    if actual != expected {
        return Err(PyValueError::new_err(
            "unexpected original provider workspace members; retained",
        ));
    }
    let mut budget = 2048;
    directories(workspace, workspace, &mut budget, 0, &mut Vec::new())?;
    for path in paths {
        let members = std::fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<Vec<_>>>()?;
        let allowed: &[&str] = if inventory.contains(path) {
            &["inv.md", "out.md"]
        } else if maps.contains(path) {
            &[
                "ticker_to_id.json",
                "ticker_to_id.prebroken.json",
                "keywords.csv",
                "inv.md",
                "out.md",
                "emoji",
                "state.json",
                "cache.json",
            ]
        } else if replacements.contains(path) {
            &[
                "emoji",
                "png",
                "ticker_to_id.json",
                "other_map.json",
                "verify_logos_intent.json",
                ".incoming",
            ]
        } else if verify.contains(path) {
            &["emoji", "state.json", "map.json", "pack.lock"]
        } else if remap.contains(path) {
            &[
                "emoji",
                "cache.json",
                "state.json",
                "ticker_to_id.json",
                "ticker_to_id.candidate.json",
                "ticker_to_id.prebroken.json",
                "pack_cryptoemoji.lock",
                "cand.json",
            ]
        } else if primary.contains(path) {
            &[
                "emoji",
                "state.json",
                "ticker_to_id.json",
                "pack_cryptoemoji.lock",
                "pack.lock",
                ".incoming",
                "cache.json",
                "inv.md",
                "out.md",
            ]
        } else {
            &[]
        };
        if allowed.is_empty() {
            // Original replacement proof downloads one live.png per acknowledged context.
            if members == [std::ffi::OsString::from("live.png")] {
                continue;
            }
            if members.iter().any(|name| {
                !name
                    .to_str()
                    .and_then(|name| name.strip_prefix('u'))
                    .is_some_and(|suffix| {
                        !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
                    })
            }) {
                return Err(PyValueError::new_err(
                    "unexpected original provider download artifact; retained",
                ));
            }
        } else if members
            .iter()
            .any(|name| !allowed.contains(&name.to_str().unwrap_or("")))
        {
            return Err(PyValueError::new_err(
                "unexpected original provider test artifact; retained",
            ));
        }
        let required: &[&str] = if inventory.contains(path) {
            &["inv.md"]
        } else if maps.contains(path) {
            &["ticker_to_id.json"]
        } else if replacements.contains(path) {
            &["emoji", "png", "ticker_to_id.json"]
        } else if verify.contains(path) {
            &["emoji", "state.json", "map.json"]
        } else if remap.contains(path) {
            &[]
        } else if primary.contains(path) {
            &["emoji", "state.json", "ticker_to_id.json"]
        } else {
            &[]
        };
        if required.iter().any(|name| !path.join(name).exists()) {
            return Err(PyValueError::new_err(
                "missing original provider test artifact; retained",
            ));
        }
    }
    let locks = workspace.join(".locks");
    // A stable lock file is expected; an actual held lease after a test is not.
    let lease = required(bindings, "ps")?.call_method0("canonical_map_lock")?;
    lease.call_method0("__enter__")?;
    lease.call_method1("__exit__", (py.None(), py.None(), py.None()))?;
    let family = required(bindings, "ps")?
        .call_method1("exclusive_lock", (locks.join("pack_cryptoemoji.lock"),))?;
    family.call_method0("__enter__")?;
    family.call_method1("__exit__", (py.None(), py.None(), py.None()))?;
    let members = std::fs::read_dir(&locks)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    let expected = ["canonical_map.lock", "pack_cryptoemoji.lock"]
        .map(std::ffi::OsString::from)
        .into_iter()
        .collect::<BTreeSet<_>>();
    if members.into_iter().collect::<BTreeSet<_>>() != expected {
        return Err(PyValueError::new_err(
            "unexpected original provider lock artifact; retained",
        ));
    }
    for path in paths {
        std::fs::remove_dir_all(path)?;
    }
    std::fs::remove_dir_all(locks)?;
    std::fs::remove_file(source_path)?;
    Ok(())
}
