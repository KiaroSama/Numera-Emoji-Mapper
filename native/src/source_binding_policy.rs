//! Shared syntax-only binding checks; callers admit source bytes before parsing.
use crate::source_test_inventory::{body, children, kind, walk};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyList;
use std::collections::BTreeMap;

pub(crate) fn imported(
    ast: &Bound<'_, PyModule>,
    tree: &Bound<'_, PyAny>,
    local: &str,
    module: &str,
    symbol: &str,
    level: usize,
) -> PyResult<bool> {
    let mut matches = 0;
    for statement in body(tree)? {
        let statement_kind = kind(&statement)?;
        if statement_kind == "ImportFrom" {
            for alias in statement.getattr("names")?.cast_into::<PyList>()?.iter() {
                let name: String = alias.getattr("name")?.extract()?;
                let alias_name: Option<String> = alias.getattr("asname")?.extract()?;
                if name == "*" {
                    return Ok(false);
                }
                if alias_name.as_deref().unwrap_or(&name) == local {
                    if statement.getattr("level")?.extract::<usize>()? != level
                        || statement
                            .getattr("module")?
                            .extract::<Option<String>>()?
                            .as_deref()
                            != Some(module)
                        || name != symbol
                    {
                        return Ok(false);
                    }
                    matches += 1;
                }
            }
            continue;
        }
        if matches!(
            statement_kind.as_str(),
            "FunctionDef" | "AsyncFunctionDef" | "ClassDef"
        ) {
            if statement.getattr("name")?.extract::<String>()? == local {
                return Ok(false);
            }
            continue;
        }
        for node in walk(ast, &statement)? {
            let node_kind = kind(&node)?;
            if node_kind == "Name"
                && node.getattr("id")?.extract::<String>()? == local
                && matches!(kind(&node.getattr("ctx")?)?.as_str(), "Store" | "Del")
            {
                return Ok(false);
            }
            if node_kind == "ImportFrom" {
                for alias in node.getattr("names")?.cast_into::<PyList>()?.iter() {
                    let name: String = alias.getattr("name")?.extract()?;
                    let alias_name: Option<String> = alias.getattr("asname")?.extract()?;
                    if name == "*" || alias_name.as_deref().unwrap_or(&name) == local {
                        return Ok(false);
                    }
                }
            }
            if node_kind == "Import" {
                for alias in node.getattr("names")?.cast_into::<PyList>()?.iter() {
                    let name: String = alias.getattr("name")?.extract()?;
                    let alias_name: Option<String> = alias.getattr("asname")?.extract()?;
                    if alias_name
                        .as_deref()
                        .unwrap_or_else(|| name.split('.').next().unwrap())
                        == local
                    {
                        return Ok(false);
                    }
                }
            }
        }
    }
    Ok(matches == 1)
}

pub(crate) fn names(
    ast: &Bound<'_, PyModule>,
    tree: &Bound<'_, PyAny>,
) -> PyResult<BTreeMap<String, usize>> {
    let mut pending = body(tree)?;
    let mut found = BTreeMap::new();
    let mut visited = 0;
    while let Some(node) = pending.pop() {
        visited += 1;
        if visited > 100_000 {
            return Err(PyValueError::new_err(
                "binding inventory exceeds node limit",
            ));
        }
        let tag = kind(&node)?;
        if matches!(
            tag.as_str(),
            "FunctionDef" | "AsyncFunctionDef" | "ClassDef"
        ) {
            let name: String = node.getattr("name")?.extract()?;
            if name == "__getattr__" {
                return Err(PyValueError::new_err(
                    "dynamic exports require runtime evidence",
                ));
            }
            *found.entry(name).or_insert(0) += 1;
            continue;
        }
        if matches!(tag.as_str(), "Import" | "ImportFrom") {
            for alias in node.getattr("names")?.cast_into::<PyList>()?.iter() {
                let name: String = alias.getattr("name")?.extract()?;
                if name == "*" {
                    return Err(PyValueError::new_err(
                        "star exports require runtime evidence",
                    ));
                }
                let renamed: Option<String> = alias.getattr("asname")?.extract()?;
                let local = renamed.unwrap_or_else(|| {
                    if tag == "Import" {
                        name.split('.').next().unwrap().into()
                    } else {
                        name
                    }
                });
                *found.entry(local).or_insert(0) += 1;
            }
            continue;
        }
        if tag == "Name" && matches!(kind(&node.getattr("ctx")?)?.as_str(), "Store" | "Del") {
            *found
                .entry(node.getattr("id")?.extract::<String>()?)
                .or_insert(0) += 1;
        }
        if tag == "Call" {
            let function = node.getattr("func")?;
            if kind(&function)? == "Name"
                && matches!(
                    function.getattr("id")?.extract::<String>()?.as_str(),
                    "globals" | "locals" | "exec" | "eval"
                )
            {
                return Err(PyValueError::new_err(
                    "dynamic namespace requires runtime evidence",
                ));
            }
        }
        pending.extend(children(ast, &node)?);
    }
    Ok(found)
}

pub(crate) fn definition<'py>(
    tree: &Bound<'py, PyAny>,
    name: &str,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let mut found = None;
    for node in body(tree)? {
        if kind(&node)? == "FunctionDef" && node.getattr("name")?.extract::<String>()? == name {
            if found.is_some() {
                return Ok(None);
            }
            found = Some(node);
        }
    }
    Ok(found)
}

pub(crate) fn assignment(
    ast: &Bound<'_, PyModule>,
    tree: &Bound<'_, PyAny>,
    name: &str,
    expected: &str,
) -> PyResult<bool> {
    let mut matched = 0;
    let mut writes = 0;
    for statement in body(tree)? {
        let tag = kind(&statement)?;
        if matches!(
            tag.as_str(),
            "FunctionDef" | "AsyncFunctionDef" | "ClassDef"
        ) {
            if statement.getattr("name")?.extract::<String>()? == name {
                return Ok(false);
            }
            continue;
        }
        if tag == "Assign" {
            let targets = statement.getattr("targets")?.cast_into::<PyList>()?;
            if targets.len() == 1 {
                let target = targets.get_item(0)?;
                if kind(&target)? == "Name" && target.getattr("id")?.extract::<String>()? == name {
                    let value: String = ast
                        .call_method1("unparse", (statement.getattr("value")?,))?
                        .extract()?;
                    matched += usize::from(value == expected);
                }
            }
        }
        for node in walk(ast, &statement)? {
            let tag = kind(&node)?;
            if tag == "Name"
                && node.getattr("id")?.extract::<String>()? == name
                && matches!(kind(&node.getattr("ctx")?)?.as_str(), "Store" | "Del")
            {
                writes += 1;
            }
            if matches!(tag.as_str(), "Import" | "ImportFrom") {
                for alias in node.getattr("names")?.cast_into::<PyList>()?.iter() {
                    let imported: String = alias.getattr("name")?.extract()?;
                    let renamed: Option<String> = alias.getattr("asname")?.extract()?;
                    if imported == "*" || renamed.as_deref().unwrap_or(&imported) == name {
                        return Ok(false);
                    }
                }
            }
        }
    }
    Ok(matched == 1 && writes == 1)
}
