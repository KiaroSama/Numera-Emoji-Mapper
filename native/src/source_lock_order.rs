//! Retained original lock-order syntax policy, distinct from native runtime lock acceptance.
use crate::source_git_inventory::Source;
use crate::source_test_inventory::{children, kind, walk};
use pyo3::prelude::*;
use pyo3::types::PyList;
use std::collections::{BTreeMap, BTreeSet};

pub const MAP: &str = "canonical_map_lock";
pub const PACK: &str = "exclusive_lock";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub line: u64,
    pub acquired: BTreeSet<String>,
    pub pairs: Vec<(String, String)>,
    pub calls: BTreeSet<String>,
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub files: BTreeSet<String>,
    pub inversions: Vec<String>,
    pub reentrant: Vec<String>,
    pub helper_reentry: Vec<String>,
}

fn function(node: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(matches!(
        kind(node)?.as_str(),
        "FunctionDef" | "AsyncFunctionDef"
    ))
}
fn contexts(node: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    if !matches!(kind(node)?.as_str(), "With" | "AsyncWith") {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for item in node.getattr("items")?.cast_into::<PyList>()?.iter() {
        let call = item.getattr("context_expr")?;
        if kind(&call)? != "Call" {
            continue;
        }
        let callable = call.getattr("func")?;
        let name: Option<String> = match kind(&callable)?.as_str() {
            "Name" => Some(callable.getattr("id")?.extract()?),
            "Attribute" => Some(callable.getattr("attr")?.extract()?),
            _ => None,
        };
        if let Some(name) = name.filter(|name| matches!(name.as_str(), MAP | PACK)) {
            result.push(name);
        }
    }
    Ok(result)
}

fn ordered(ast: &Bound<'_, PyModule>, node: &Bound<'_, PyAny>) -> PyResult<Vec<(String, String)>> {
    let mut output = Vec::new();
    let mut pending = children(ast, node)?
        .into_iter()
        .rev()
        .map(|child| (child, Vec::<String>::new()))
        .collect::<Vec<_>>();
    let mut count = 0;
    while let Some((node, held)) = pending.pop() {
        count += 1;
        if count > 100_000 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "lock-order AST exceeds node limit",
            ));
        }
        if function(&node)? {
            continue;
        }
        let mine = contexts(&node)?;
        for outer in &held {
            for inner in &mine {
                output.push((outer.clone(), inner.clone()));
            }
        }
        for (index, outer) in mine.iter().enumerate() {
            for inner in &mine[index + 1..] {
                output.push((outer.clone(), inner.clone()));
            }
        }
        let mut held = held;
        held.extend(mine);
        pending.extend(
            children(ast, &node)?
                .into_iter()
                .rev()
                .map(|node| (node, held.clone())),
        );
    }
    Ok(output)
}

fn functions(py: Python<'_>, source: &str) -> PyResult<Vec<Function>> {
    let ast = py.import("ast")?;
    let tree = ast.call_method1("parse", (source,))?;
    let mut output = Vec::new();
    for node in walk(&ast, &tree)? {
        if !function(&node)? {
            continue;
        }
        let mut value = Function {
            name: node.getattr("name")?.extract()?,
            line: node.getattr("lineno")?.extract()?,
            pairs: ordered(&ast, &node)?,
            ..Function::default()
        };
        for child in walk(&ast, &node)? {
            value.acquired.extend(contexts(&child)?);
            if kind(&child)? == "Call" && kind(&child.getattr("func")?)? == "Name" {
                value
                    .calls
                    .insert(child.getattr("func")?.getattr("id")?.extract()?);
            }
        }
        output.push(value);
    }
    Ok(output)
}

pub fn inspect_functions(source: &str) -> Result<Vec<Function>, String> {
    if source.len() > 2 * 1024 * 1024 {
        return Err("lock-order source exceeds2MiB".into());
    }
    Python::initialize();
    Python::attach(|py| functions(py, source))
        .map_err(|error| format!("lock-order AST parse failed: {error}"))
}

pub fn inspect(sources: &[Source]) -> Result<Report, String> {
    let mut admitted = BTreeSet::new();
    for source in sources {
        crate::source_test_inventory::source_path(&source.path)?;
        if source.text.len() > 2 * 1024 * 1024
            || source.sha256 != crate::codec_raster::digest_hex(source.text.as_bytes())
            || !admitted.insert(source.path.as_str())
        {
            return Err("lock-order inventory requires unique admitted source bytes".into());
        }
    }
    Python::initialize();
    Python::attach(|py| -> PyResult<Report> {
        let mut report = Report::default();
        for source in sources {
            let Some((directory, name)) = source.path.rsplit_once('/') else {
                continue;
            };
            let executable = source.text.contains("\nif __name__ == \"__main__\":");
            if directory != "coins"
                && !(executable && matches!(directory, "emojikit" | "tests/reference"))
            {
                continue;
            }
            report.files.insert(name.into());
            let functions = functions(py, &source.text)?;
            // The original dict comprehension's final same-name definition wins.
            let mut takes = BTreeMap::new();
            for function in &functions {
                takes.insert(function.name.as_str(), &function.acquired);
            }
            for function in &functions {
                for (outer, inner) in &function.pairs {
                    if outer == MAP && inner == PACK {
                        report
                            .inversions
                            .push(format!("{name}:{} {}()", function.line, function.name));
                    }
                    if outer == inner {
                        report.reentrant.push(format!(
                            "{name}:{} {}() nests {inner}",
                            function.line, function.name
                        ));
                    }
                }
                for call in &function.calls {
                    if call == &function.name {
                        continue;
                    }
                    if let Some(acquired) = takes.get(call.as_str()) {
                        let clash = function
                            .acquired
                            .intersection(acquired)
                            .cloned()
                            .collect::<Vec<_>>();
                        if !clash.is_empty() {
                            report.helper_reentry.push(format!(
                                "{name}:{} {}() holds {} and calls {call}(), which takes it again",
                                function.line,
                                function.name,
                                clash.join("/")
                            ));
                        }
                    }
                }
            }
        }
        Ok(report)
    })
    .map_err(|error| format!("lock-order corpus AST parse failed: {error}"))
}

pub fn check(report: &Report) -> Result<(), String> {
    for expected in [
        "build_pack.py",
        "fetch_paprika.py",
        "verify_logos.py",
        "rebuild_dedup.py",
        "remap_ids.py",
    ] {
        if !report.files.contains(expected) {
            return Err(format!(
                "lock-order source scan missed required tool: {expected}"
            ));
        }
    }
    if !report.inversions.is_empty()
        || !report.reentrant.is_empty()
        || !report.helper_reentry.is_empty()
    {
        return Err(format!("original source lock-order violations: {report:?}"));
    }
    Ok(())
}
