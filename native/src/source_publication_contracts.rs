//! Historical announcement import ownership; not Python import or object-identity execution.
use crate::source_binding_policy::{imported, names};
use crate::source_git_inventory::Source;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::collections::BTreeMap;

const PATHS: [&str; 4] = [
    "tests/reference/build_pack.py",
    "tests/reference/collection_notify.py",
    "tests/reference/build_collection.py",
    "coins/rebuild_dedup.py",
];

pub fn check(sources: &[Source]) -> Result<(), String> {
    if sources.is_empty() || sources.len() > 4096 {
        return Err("publication ownership requires an admitted nonempty corpus".into());
    }
    let mut admitted = BTreeMap::new();
    let mut total = 0usize;
    for source in sources {
        crate::source_test_inventory::source_path(&source.path)?;
        total = total
            .checked_add(source.text.len())
            .ok_or("publication source byte limit")?;
        if source.text.len() > 2 * 1024 * 1024
            || total > 32 * 1024 * 1024
            || source.sha256 != crate::codec_raster::digest_hex(source.text.as_bytes())
            || source.baseline_revision != sources[0].baseline_revision
            || admitted.insert(source.path.as_str(), source).is_some()
        {
            return Err("publication ownership requires unique admitted source bytes".into());
        }
    }
    if PATHS.iter().any(|path| !admitted.contains_key(path)) {
        return Err("required publication ownership source missing".into());
    }
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let ast = py.import("ast")?;
        let mut trees = BTreeMap::new();
        let mut bindings = BTreeMap::new();
        for path in PATHS {
            let source = admitted
                .get(path)
                .ok_or_else(|| PyValueError::new_err("publication source missing"))?;
            let tree = ast.call_method1("parse", (&source.text, path))?;
            bindings.insert(path, names(&ast, &tree)?);
            trees.insert(path, tree);
        }
        for (path, module, level) in [
            ("tests/reference/build_pack.py", "announce", 1),
            ("tests/reference/collection_notify.py", "announce", 1),
            ("coins/rebuild_dedup.py", "emojikit.announce", 0),
        ] {
            if bindings[path].get("announce_packs") != Some(&1)
                || !imported(
                    &ast,
                    &trees[path],
                    "announce_packs",
                    module,
                    "announce_packs",
                    level,
                )?
            {
                return Err(PyValueError::new_err(
                    "shared announcer import ownership drift",
                ));
            }
        }
        for path in [
            "tests/reference/build_collection.py",
            "coins/rebuild_dedup.py",
        ] {
            if bindings[path].contains_key("links_chat_id") {
                return Err(PyValueError::new_err(
                    "publisher exposes a separate destination resolver",
                ));
            }
        }
        Ok(())
    })
    .map_err(|error| format!("historical publication ownership contract failed: {error}"))
}
