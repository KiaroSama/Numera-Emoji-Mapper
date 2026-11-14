//! Historical coin-source architecture policy; syntax evidence never certifies runtime import identity.
use crate::source_binding_policy::{assignment, definition, imported};
use crate::source_git_inventory::Source;
use crate::source_test_inventory::{body, kind};
use pyo3::prelude::*;
use pyo3::types::PyList;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default)]
pub struct Report {
    pub files: BTreeSet<String>,
    pub urlopen: BTreeSet<String>,
    pub inventory_owners: BTreeSet<String>,
    pub bindings: usize,
    pub shared_contracts: usize,
    pub numpy_users: BTreeSet<String>,
}

fn wrapper(ast: &Bound<'_, PyModule>, tree: &Bound<'_, PyAny>) -> PyResult<bool> {
    let Some(function) = definition(tree, "refill_inventory")? else {
        return Ok(false);
    };
    if !function
        .getattr("decorator_list")?
        .cast_into::<PyList>()?
        .is_empty()
    {
        return Ok(false);
    }
    let args = function.getattr("args")?;
    for field in ["posonlyargs", "kwonlyargs", "defaults", "kw_defaults"] {
        if !args.getattr(field)?.cast_into::<PyList>()?.is_empty() {
            return Ok(false);
        }
    }
    if !args.getattr("vararg")?.is_none() || !args.getattr("kwarg")?.is_none() {
        return Ok(false);
    }
    let positional = args.getattr("args")?.cast_into::<PyList>()?;
    if positional.len() != 1
        || positional
            .get_item(0)?
            .getattr("arg")?
            .extract::<String>()?
            != "ticker_to_id"
    {
        return Ok(false);
    }
    let statements = body(&function)?;
    if statements.len() != 1 || kind(&statements[0])? != "Return" {
        return Ok(false);
    }
    let text: String = ast.call_method1("unparse", (&statements[0],))?.extract()?;
    Ok(text == "return _inventory.refill_inventory(ticker_to_id, INV, OUT_INV)")
}

pub fn inspect(sources: &[Source]) -> Result<Report, String> {
    if sources.is_empty() || sources.len() > 4096 {
        return Err("coin contracts require an admitted nonempty corpus".into());
    }
    let mut seen = BTreeSet::new();
    let mut coins = BTreeMap::new();
    let mut total = 0usize;
    let mut report = Report::default();
    for source in sources {
        crate::source_test_inventory::source_path(&source.path)?;
        total = total
            .checked_add(source.text.len())
            .ok_or("coin source byte limit")?;
        if source.text.len() > 2 * 1024 * 1024
            || total > 32 * 1024 * 1024
            || source.sha256 != crate::codec_raster::digest_hex(source.text.as_bytes())
            || !seen.insert(source.path.as_str())
            || source.baseline_revision != sources[0].baseline_revision
        {
            return Err("coin contracts require unique admitted source bytes".into());
        }
        let direct = source
            .path
            .strip_prefix("coins/")
            .or_else(|| source.path.strip_prefix("emojikit/"))
            .unwrap_or(&source.path);
        if !direct.contains('/')
            && source.text.split('\n').any(|line| {
                line.trim_start_matches(crate::emoji_ids::whitespace)
                    .starts_with("import numpy")
            })
        {
            report.numpy_users.insert(source.path.clone());
        }
        if let Some(file) = source.path.strip_prefix("coins/")
            && !file.contains('/')
        {
            coins.insert(file, source);
        }
    }
    for (file, source) in &coins {
        report.files.insert((*file).into());
        if source.text.contains("premium-id: {eid}") {
            report.inventory_owners.insert((*file).into());
        }
        if source.text.match_indices("urlopen").any(|(start, _)| {
            source.text[start + 7..]
                .trim_start_matches(crate::emoji_ids::whitespace)
                .starts_with('(')
        }) {
            report.urlopen.insert((*file).into());
        }
    }
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let ast = py.import("ast")?;
        let mut trees = BTreeMap::new();
        for file in [
            "_inventory.py",
            "_paprika_api.py",
            "_dedup_map.py",
            "fetch_cmc.py",
            "fetch_paprika.py",
            "_provider_publish.py",
            "_dedup_plan.py",
            "verify_logos.py",
            "remap_ids.py",
        ] {
            let source = coins.get(file).ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "coin binding source missing: {file}"
                ))
            })?;
            trees.insert(
                file,
                ast.call_method1("parse", (&source.text, &source.path))?,
            );
        }
        for (file, local, module, symbol) in [
            (
                "_paprika_api.py",
                "base_ticker",
                "coins._inventory",
                "base_ticker",
            ),
            (
                "fetch_cmc.py",
                "base_ticker",
                "coins._inventory",
                "base_ticker",
            ),
            (
                "_dedup_map.py",
                "refill_inventory",
                "coins._inventory",
                "refill_inventory",
            ),
            (
                "fetch_cmc.py",
                "refill_inventory",
                "coins.fetch_paprika",
                "refill_inventory",
            ),
            ("fetch_paprika.py", "_inventory", "coins", "_inventory"),
        ] {
            if imported(&ast, &trees[file], local, module, symbol, 0)? {
                report.bindings += 1;
            }
        }
        if wrapper(&ast, &trees["fetch_paprika.py"])?
            && definition(&trees["_inventory.py"], "base_ticker")?.is_some()
            && definition(&trees["_inventory.py"], "refill_inventory")?.is_some()
        {
            report.bindings += 1;
        }
        for file in ["fetch_cmc.py", "fetch_paprika.py"] {
            if imported(
                &ast,
                &trees[file],
                "publish_logos",
                "coins._provider_publish",
                "publish_logos",
                0,
            )? {
                report.shared_contracts += 1;
            }
        }
        for (file, base, lease) in [
            ("_provider_publish.py", "SET_BASE", "PACK_LOCK"),
            ("_dedup_plan.py", "BASE", "LOCK"),
            ("verify_logos.py", "SET_BASE", "PACK_LOCK"),
            ("remap_ids.py", "SET_BASE", "PACK_LOCK"),
        ] {
            let tree = &trees[file];
            if imported(
                &ast,
                tree,
                "pack_family_lock_path",
                "emojikit.packstate",
                "pack_family_lock_path",
                0,
            )? && assignment(&ast, tree, base, "operator_config.value('COIN_PACK_BASE')")?
                && assignment(&ast, tree, lease, &format!("pack_family_lock_path({base})"))?
            {
                report.shared_contracts += 1;
            }
        }
        if assignment(
            &ast,
            &trees["_provider_publish.py"],
            "STATE",
            "ROOT / 'rebuild_dedup_state.json'",
        )? && assignment(
            &ast,
            &trees["_dedup_plan.py"],
            "STATE",
            "ROOT / 'rebuild_dedup_state.json'",
        )? && assignment(
            &ast,
            &trees["_dedup_plan.py"],
            "OLD_STATE",
            "ROOT / 'rebuild_state.json'",
        )? {
            report.shared_contracts += 1;
        }
        Ok(())
    })
    .map_err(|error| format!("coin source contract parse failed: {error}"))?;
    Ok(report)
}

pub fn check(report: &Report) -> Result<(), String> {
    if report.files.len() < 5 {
        return Err("coin source contracts must find their real shared tools".into());
    }
    if !report.urlopen.is_empty() {
        return Err(format!(
            "coin source urlopen bypasses: {:?}",
            report.urlopen
        ));
    }
    if report.inventory_owners != BTreeSet::from(["_inventory.py".into()]) {
        return Err(format!(
            "coin inventory definition owners drifted: {:?}",
            report.inventory_owners
        ));
    }
    if report.bindings != 6 {
        return Err("coin inventory shared binding contracts drifted".into());
    }
    if report.shared_contracts != 7 {
        return Err("coin shared publisher/state/family contracts drifted".into());
    }
    if report.numpy_users != BTreeSet::from(["coins/remap_ids.py".into()]) {
        return Err(format!(
            "historical NumPy import scope drifted: {:?}",
            report.numpy_users
        ));
    }
    Ok(())
}
