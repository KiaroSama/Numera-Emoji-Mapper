//! Historical media ownership syntax; this never imports modules or certifies runtime identity.
use crate::source_binding_policy::{assignment, definition, imported, names};
use crate::source_git_inventory::Source;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyList;
use std::collections::BTreeMap;

const PATHS: [&str; 5] = [
    "emojikit/media.py",
    "emojikit/repaint.py",
    "tests/reference/make_emoji_pngs.py",
    "coins/_paprika_api.py",
    "tests/reference/collection_state.py",
];

pub fn check(sources: &[Source]) -> Result<(), String> {
    let mut admitted = BTreeMap::new();
    let mut total = 0usize;
    for source in sources {
        crate::source_test_inventory::source_path(&source.path)?;
        total = total
            .checked_add(source.text.len())
            .ok_or("media source byte limit")?;
        if source.text.len() > 2 * 1024 * 1024
            || total > 32 * 1024 * 1024
            || source.sha256 != crate::codec_raster::digest_hex(source.text.as_bytes())
            || source.baseline_revision != sources[0].baseline_revision
            || admitted.insert(source.path.as_str(), source).is_some()
        {
            return Err("media ownership requires unique admitted source bytes".into());
        }
    }
    if PATHS.iter().any(|path| !admitted.contains_key(path)) {
        return Err("required media ownership source missing".into());
    }
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let ast = py.import("ast")?;
        let mut trees = BTreeMap::new();
        let mut bindings = BTreeMap::new();
        for path in PATHS {
            let source = admitted
                .get(path)
                .ok_or_else(|| PyValueError::new_err("required media ownership source missing"))?;
            let tree = ast.call_method1("parse", (&source.text, path))?;
            bindings.insert(path, names(&ast, &tree)?);
            trees.insert(path, tree);
        }
        for (path, name) in [
            ("emojikit/media.py", "fit_100"),
            ("emojikit/media.py", "_trim"),
            ("emojikit/repaint.py", "parse_tint"),
            ("emojikit/repaint.py", "repaint_in_place"),
        ] {
            let function = definition(&trees[path], name)?
                .ok_or_else(|| PyValueError::new_err("shared media definition missing"))?;
            if bindings[path].get(name) != Some(&1)
                || !function
                    .getattr("decorator_list")?
                    .cast_into::<PyList>()?
                    .is_empty()
            {
                return Err(PyValueError::new_err(
                    "shared media callable ownership drift",
                ));
            }
        }
        let converter = &trees["tests/reference/make_emoji_pngs.py"];
        if !imported(&ast, converter, "media", "emojikit", "media", 0)?
            || !assignment(&ast, converter, "_fit_100", "media.fit_100")?
            || !assignment(&ast, converter, "_trim", "media._trim")?
            || !imported(
                &ast,
                &trees["coins/_paprika_api.py"],
                "_fit_100",
                "emojikit.media",
                "fit_100",
                0,
            )?
        {
            return Err(PyValueError::new_err("shared fitting aliases drifted"));
        }
        for (path, forbidden) in [
            ("emojikit/media.py", &["parse_tint", "repaint_in_place"][..]),
            (
                "tests/reference/collection_state.py",
                &["BRAND_LOGO_DEFAULT", "BRAND_LOGO_BOTS", "BRAND_LOGO_KW"][..],
            ),
        ] {
            if forbidden
                .iter()
                .any(|name| bindings[path].contains_key(*name))
            {
                return Err(PyValueError::new_err("forbidden historical media export"));
            }
        }
        Ok(())
    })
    .map_err(|error| format!("historical media ownership contract failed: {error}"))
}
