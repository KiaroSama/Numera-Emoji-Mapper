//! Rust owns inventory policy; unchanged CPython AST APIs parse source without executing it.
use pyo3::prelude::*;
use pyo3::types::PyList;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;

pub fn source_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || !path.ends_with(".py")
        || path.contains(['\\', ':', '\0'])
        || path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err("source inventory path must be a relative nontraversing Python path".into());
    }
    Ok(())
}
pub(crate) fn kind(node: &Bound<'_, PyAny>) -> PyResult<String> {
    node.get_type().getattr("__name__")?.extract()
}
pub(crate) fn children<'py>(
    ast: &Bound<'py, PyModule>,
    node: &Bound<'py, PyAny>,
) -> PyResult<Vec<Bound<'py, PyAny>>> {
    let list = node
        .py()
        .import("builtins")?
        .getattr("list")?
        .call1((ast.call_method1("iter_child_nodes", (node,))?,))?;
    Ok(list.cast_into::<PyList>()?.iter().collect())
}
pub(crate) fn walk<'py>(
    ast: &Bound<'py, PyModule>,
    node: &Bound<'py, PyAny>,
) -> PyResult<Vec<Bound<'py, PyAny>>> {
    let mut queue = VecDeque::from([node.clone()]);
    let mut nodes = Vec::new();
    while let Some(node) = queue.pop_front() {
        if nodes.len() >= 100_000 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "source AST exceeds inventory node limit",
            ));
        }
        queue.extend(children(ast, &node)?);
        nodes.push(node);
    }
    Ok(nodes)
}
fn expression(ast: &Bound<'_, PyModule>, node: &Bound<'_, PyAny>) -> PyResult<String> {
    ast.call_method1("unparse", (node,))?.extract()
}
pub(crate) fn body<'py>(node: &Bound<'py, PyAny>) -> PyResult<Vec<Bound<'py, PyAny>>> {
    Ok(node
        .getattr("body")?
        .cast_into::<PyList>()?
        .iter()
        .collect())
}
fn function(node: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(matches!(
        kind(node)?.as_str(),
        "FunctionDef" | "AsyncFunctionDef"
    ))
}
fn method(ast: &Bound<'_, PyModule>, node: &Bound<'_, PyAny>, source: &str) -> PyResult<Value> {
    let mut assertions = Vec::new();
    for item in walk(ast, node)? {
        if kind(&item)? != "Call" {
            continue;
        }
        let callable = item.getattr("func")?;
        if kind(&callable)? != "Attribute" {
            continue;
        }
        let owner = callable.getattr("value")?;
        if kind(&owner)? == "Name"
            && owner.getattr("id")?.extract::<String>()? == "self"
            && callable
                .getattr("attr")?
                .extract::<String>()?
                .starts_with("assert")
        {
            assertions.push(
                ast.call_method1("get_source_segment", (source, &item))?
                    .extract::<String>()?,
            );
        }
    }
    let decorators = node
        .getattr("decorator_list")?
        .cast_into::<PyList>()?
        .iter()
        .map(|node| expression(ast, &node))
        .collect::<PyResult<Vec<_>>>()?;
    Ok(json!({"name":node.getattr("name")?.extract::<String>()?,
        "line":node.getattr("lineno")?.extract::<u64>()?,"end_line":node.getattr("end_lineno")?.extract::<u64>()?,
        "async":kind(node)? == "AsyncFunctionDef","decorators":decorators,"assertions":assertions}))
}
fn parse_attached(py: Python<'_>, path: &str, source: &str) -> PyResult<Value> {
    let ast = py.import("ast")?;
    let tree = ast.call_method1("parse", (source, path))?;
    let nodes = walk(&ast, &tree)?;
    let mut imports = Vec::new();
    let mut commands = Vec::new();
    for node in &nodes {
        match kind(node)?.as_str() {
            "Import" | "ImportFrom" => {
                let from = kind(node)? == "ImportFrom";
                let module = if from {
                    let level = node.getattr("level")?.extract::<usize>()?;
                    if level > 256 {
                        return Err(pyo3::exceptions::PyValueError::new_err(
                            "source relative import depth exceeds limit",
                        ));
                    }
                    Some(format!(
                        "{}{}",
                        ".".repeat(level),
                        node.getattr("module")?
                            .extract::<Option<String>>()?
                            .unwrap_or_default()
                    ))
                } else {
                    None
                };
                for alias in node.getattr("names")?.cast_into::<PyList>()?.iter() {
                    let name = alias.getattr("name")?.extract::<String>()?;
                    let local = alias
                        .getattr("asname")?
                        .extract::<Option<String>>()?
                        .unwrap_or_else(|| {
                            if from {
                                name.clone()
                            } else {
                                name.split('.').next().unwrap_or("").into()
                            }
                        });
                    imports.push(json!({"module":module.as_ref().unwrap_or(&name),"symbol":if from {Some(&name)} else {None},"local":local}));
                }
            }
            "Call" => {
                let callable = node.getattr("func")?;
                if kind(&callable)? == "Attribute"
                    && callable.getattr("attr")?.extract::<String>()? == "add_argument"
                {
                    let mut args = Vec::new();
                    for arg in node.getattr("args")?.cast_into::<PyList>()?.iter() {
                        if kind(&arg)? == "Constant"
                            && let Ok(value) = arg.getattr("value")?.extract::<String>()
                        {
                            args.push(value);
                        }
                    }
                    commands.push(args);
                }
            }
            _ => {}
        }
    }
    let mut classes = Vec::new();
    let mut load_tests = false;
    let mut declared = 0;
    for node in body(&tree)? {
        if function(&node)? && node.getattr("name")?.extract::<String>()? == "load_tests" {
            load_tests = true;
        }
        if kind(&node)? != "ClassDef" {
            continue;
        }
        let bases = node
            .getattr("bases")?
            .cast_into::<PyList>()?
            .iter()
            .map(|base| expression(&ast, &base))
            .collect::<PyResult<Vec<_>>>()?;
        let mut methods = Vec::new();
        for child in body(&node)? {
            if function(&child)? {
                let value = method(&ast, &child, source)?;
                if value["name"]
                    .as_str()
                    .is_some_and(|name| name.starts_with("test"))
                {
                    declared += 1;
                }
                methods.push(value);
            }
        }
        classes.push(json!({"name":node.getattr("name")?.extract::<String>()?,"line":node.getattr("lineno")?.extract::<u64>()?,"bases":bases,"methods":methods}));
    }
    let hash = Sha256::digest(source.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(
        json!({"path":path,"sha256":hash,"classes":classes,"imports":imports,"cli_arguments":commands,"load_tests":load_tests,"declared_test_methods":declared}),
    )
}
pub fn parse(path: &str, source: &str) -> Result<Value, String> {
    source_path(path)?;
    if source.len() > 2 * 1024 * 1024 {
        return Err("source inventory input exceeds 2MiB".into());
    }
    Python::initialize();
    Python::attach(|py| parse_attached(py, path, source))
        .map_err(|error| format!("source inventory parse failed: {error}"))
}
