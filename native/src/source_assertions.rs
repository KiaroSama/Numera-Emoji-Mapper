//! Parse-only assertion/variant evidence. Rust owns traversal; CPython supplies syntax and spans.
use crate::source_test_inventory::{body, kind, source_path, walk};
use pyo3::prelude::*;
use pyo3::types::PyList;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn segment(ast: &Bound<'_, PyModule>, node: &Bound<'_, PyAny>, source: &str) -> PyResult<String> {
    ast.call_method1("get_source_segment", (source, node))?
        .extract()
}

fn assertion(
    ast: &Bound<'_, PyModule>,
    node: &Bound<'_, PyAny>,
    source: &str,
) -> PyResult<Option<Value>> {
    let label = if kind(node)? == "Assert" {
        Some("assert".to_owned())
    } else if kind(node)? == "Call" {
        let callable = node.getattr("func")?;
        if kind(&callable)? == "Attribute" {
            let owner = callable.getattr("value")?;
            let name: String = callable.getattr("attr")?.extract()?;
            let mock_assertion = matches!(
                name.as_str(),
                "assert_called"
                    | "assert_called_once"
                    | "assert_called_with"
                    | "assert_called_once_with"
                    | "assert_any_call"
                    | "assert_has_calls"
                    | "assert_not_called"
                    | "assert_awaited"
                    | "assert_awaited_once"
                    | "assert_awaited_with"
                    | "assert_awaited_once_with"
                    | "assert_any_await"
                    | "assert_has_awaits"
                    | "assert_not_awaited"
                    | "wait_for_function"
            );
            (mock_assertion
                || (kind(&owner)? == "Name"
                    && owner.getattr("id")?.extract::<String>()? == "self"
                    && (name.starts_with("assert") || name == "fail")))
                .then_some(name)
        } else {
            None
        }
    } else {
        None
    };
    label
        .map(|label| {
            Ok(json!({
                "kind":label,"source":segment(ast,node,source)?,
                "line":node.getattr("lineno")?.extract::<u64>()?,
                "column":node.getattr("col_offset")?.extract::<u64>()?,
                "end_line":node.getattr("end_lineno")?.extract::<u64>()?,
                "end_column":node.getattr("end_col_offset")?.extract::<u64>()?
            }))
        })
        .transpose()
}

fn lexical_nodes<'py>(
    ast: &Bound<'py, PyModule>,
    method: &Bound<'py, PyAny>,
) -> PyResult<Vec<Bound<'py, PyAny>>> {
    let mut queue = std::collections::VecDeque::from(body(method)?);
    let mut nodes = Vec::new();
    while let Some(node) = queue.pop_front() {
        if nodes.len() >= 100_000 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "source AST exceeds inventory node limit",
            ));
        }
        // Definitions are bindings in this scope, but their bodies cannot
        // supply assertions or calls executed by the enclosing method.
        if !matches!(
            kind(&node)?.as_str(),
            "FunctionDef" | "AsyncFunctionDef" | "ClassDef" | "Lambda"
        ) {
            queue.extend(crate::source_test_inventory::children(ast, &node)?);
        }
        nodes.push(node);
    }
    Ok(nodes)
}

fn method(
    ast: &Bound<'_, PyModule>,
    node: &Bound<'_, PyAny>,
    source: &str,
    class: &str,
) -> PyResult<Value> {
    let mut assertions = Vec::new();
    let mut contexts = Vec::new();
    for item in walk(ast, node)? {
        let node_kind = kind(&item)?;
        if let Some(assertion) = assertion(ast, &item, source)? {
            assertions.push(assertion);
        }
        let expressions = match node_kind.as_str() {
            "For" | "AsyncFor" => Some(vec![item.getattr("target")?, item.getattr("iter")?]),
            "While" | "If" => Some(vec![item.getattr("test")?]),
            "With" | "AsyncWith" => Some(
                item.getattr("items")?
                    .cast_into::<PyList>()?
                    .iter()
                    .map(|entry| entry.getattr("context_expr"))
                    .collect::<PyResult<Vec<_>>>()?,
            ),
            "Try" => Some(Vec::new()),
            _ => None,
        };
        if let Some(expressions) = expressions {
            let expressions = expressions
                .iter()
                .map(|expression| segment(ast, expression, source))
                .collect::<PyResult<Vec<_>>>()?;
            contexts.push(json!({
                "kind":node_kind,"line":item.getattr("lineno")?.extract::<u64>()?,
                "end_line":item.getattr("end_lineno")?.extract::<u64>()?,"expressions":expressions
            }));
        }
    }
    Ok(
        json!({"class":class,"method":node.getattr("name")?.extract::<String>()?,
        "assertions":assertions,"contexts":contexts}),
    )
}

pub fn parse(path: &str, source: &str) -> Result<Value, String> {
    source_path(path)?;
    if source.len() > 2 * 1024 * 1024 {
        return Err("source assertion input exceeds2MiB".into());
    }
    Python::initialize();
    let (methods, origins) = Python::attach(|py| -> PyResult<(Vec<Value>, Vec<Value>)> {
        let ast = py.import("ast")?;
        let tree = ast.call_method1("parse", (source, path))?;
        // Bound the whole AST, not only class methods; malformed oversized helpers also refuse.
        walk(&ast, &tree)?;
        let mut methods = Vec::new();
        let mut origins = Vec::new();
        for class in body(&tree)? {
            if kind(&class)? != "ClassDef" {
                continue;
            }
            let name: String = class.getattr("name")?.extract()?;
            let members = body(&class)?;
            // Only plain local classes justify a direct helper binding. Assignments,
            // decorators or conditional bodies can replace a method without a def.
            let mut plain = class.getattr("decorator_list")?.len()? == 0
                && class.getattr("keywords")?.len()? == 0;
            for child in &members {
                plain &= match kind(child)?.as_str() {
                    "FunctionDef" | "AsyncFunctionDef" => child.getattr("decorator_list")?.len()? == 0,
                    "Pass" => true,
                    "Expr" => kind(&child.getattr("value")?)? == "Constant",
                    _ => false,
                };
            }
            for child in members {
                if matches!(kind(&child)?.as_str(), "FunctionDef" | "AsyncFunctionDef") {
                    let method_name: String = child.getattr("name")?.extract()?;
                    let method_line = child.getattr("lineno")?.extract::<u64>()?;
                    // CPython 3.11.9 compile.c uses the first method decorator expression,
                    // not its @ token or a class decorator, for the original code object's span.
                    let decorators = child.getattr("decorator_list")?.cast_into::<PyList>()?;
                    let code_first_line = if let Some(decorator) = decorators.iter().next() {
                        decorator.getattr("lineno")?.extract::<u64>()?
                    } else {
                        method_line
                    };
                    let nodes = lexical_nodes(&ast, &child)?;
                    let mut direct_scope = true;
                    for node in walk(&ast, &child)? {
                        if matches!(kind(&node)?.as_str(), "Global" | "Nonlocal")
                            && node.getattr("names")?.extract::<Vec<String>>()?
                                .iter().any(|name| name == "self")
                        {
                            direct_scope = false;
                        }
                    }
                    let mut direct_assertions = Vec::new();
                    for node in &nodes {
                        let node_kind = kind(node)?;
                        if matches!(node_kind.as_str(), "FunctionDef" | "AsyncFunctionDef" | "ClassDef") {
                            let bound_name = node.getattr("name")?.extract::<String>()?;
                            if bound_name == "self" || bound_name == method_name {
                                direct_scope = false;
                            }
                        }
                        if matches!(node_kind.as_str(), "Import" | "ImportFrom") {
                            for alias in node.getattr("names")?.cast_into::<PyList>()?.iter() {
                                let name = alias.getattr("name")?.extract::<String>()?;
                                let bound = alias.getattr("asname")?.extract::<Option<String>>()?
                                    .unwrap_or_else(|| name.split('.').next().unwrap_or("").to_owned());
                                if bound == "self" || bound == method_name {
                                    direct_scope = false;
                                }
                            }
                        }
                        if node_kind == "ExceptHandler"
                            && node.getattr("name")?.extract::<Option<String>>()?.as_deref() == Some("self")
                        {
                            direct_scope = false;
                        }
                        if let Some(assertion) = assertion(&ast, node, source)? {
                            direct_assertions.push(assertion);
                        }
                        if node_kind == "Name"
                            && node.getattr("id")?.extract::<String>()? == "self"
                            && kind(&node.getattr("ctx")?)? != "Load"
                        {
                            direct_scope = false;
                        }
                        if node_kind == "Attribute" && kind(&node.getattr("ctx")?)? != "Load" {
                            let receiver = node.getattr("value")?;
                            if kind(&receiver)? == "Name"
                                && receiver.getattr("id")?.extract::<String>()? == "self"
                            {
                                direct_scope = false;
                            }
                        }
                    }
                    let mut calls = Vec::new();
                    for node in nodes {
                        if !direct_scope || kind(&node)? != "Call" { continue; }
                        let callable = node.getattr("func")?;
                        if kind(&callable)? != "Attribute" { continue; }
                        let owner = callable.getattr("value")?;
                        let called: String = callable.getattr("attr")?.extract()?;
                        if kind(&owner)? == "Name"
                            && owner.getattr("id")?.extract::<String>()? == "self"
                            && !called.starts_with("test") && !called.starts_with("__")
                        {
                            calls.push(json!({"name":called,"source":segment(&ast,&node,source)?,
                                "line":node.getattr("lineno")?.extract::<u64>()?,
                                "column":node.getattr("col_offset")?.extract::<u64>()?,
                                "end_line":node.getattr("end_lineno")?.extract::<u64>()?,
                                "end_column":node.getattr("end_col_offset")?.extract::<u64>()?}));
                        }
                    }
                    origins.push(json!({
                        "class_identity":format!("{path}:{}",class.getattr("lineno")?.extract::<u64>()?),
                        "method":method_name,"method_line":method_line,"code_first_line":code_first_line,
                        "plain_class":plain,"helper_callable":plain && direct_scope && kind(&child)? == "FunctionDef"
                            && !calls.iter().any(|call| call["name"] == method_name),
                        "self_calls":calls,"direct_assertions":direct_assertions
                    }));
                    methods.push(method(&ast, &child, source, &name)?);
                }
            }
        }
        Ok((methods, origins))
    })
    .map_err(|error| format!("source assertion parse failed: {error}"))?;
    let hash = Sha256::digest(source.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(
        json!({"path":path,"sha256":hash,"methods":methods,"method_origins":origins,"runtime_variants_complete":false}),
    )
}

/// Attach original method evidence to syntax-discovered cases, never to a same-named class.
/// Missing definitions remain unresolved; this join proves neither runtime collection nor parity.
pub fn resolve_cases(files: &[Value]) -> Result<Value, String> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut definitions = BTreeMap::new();
    let mut paths = BTreeSet::new();
    for file in files {
        let path = file["source"]
            .as_str()
            .ok_or("assertion evidence requires source path")?;
        source_path(path)?;
        if !paths.insert(path) {
            return Err("assertion evidence repeats a source path".into());
        }
        let methods = file["assertions"]["methods"]
            .as_array()
            .ok_or("assertion evidence requires methods")?;
        let origins = file["assertions"]["method_origins"]
            .as_array()
            .ok_or("assertion evidence requires method origins")?;
        if methods.len() != origins.len() {
            return Err("assertion method/origin evidence length differs".into());
        }
        for (method, origin) in methods.iter().zip(origins) {
            let identity = origin["class_identity"]
                .as_str()
                .ok_or("assertion origin requires class identity")?;
            let name = origin["method"]
                .as_str()
                .ok_or("assertion origin requires method")?;
            let line = origin["method_line"]
                .as_u64()
                .filter(|line| *line > 0)
                .ok_or("assertion origin requires method line")?;
            origin["code_first_line"]
                .as_u64()
                .filter(|first| *first > 0 && *first <= line)
                .ok_or("assertion origin requires a positive code first line no later than method line; regenerate source assertion evidence")?;
            let class_line = identity
                .strip_prefix(path)
                .and_then(|tail| tail.strip_prefix(':'))
                .and_then(|line| line.parse::<u64>().ok())
                .filter(|line| *line > 0);
            if class_line.is_none() || method["method"] != name {
                return Err("assertion origin differs from method evidence".into());
            }
            // A later function definition in this same class replaces the earlier function.
            // Discovery has already handled noncallable/decorated shadows conservatively.
            definitions.insert((identity, name), (file, method, line, origin));
        }
    }
    let mut resolved = serde_json::Map::new();
    for file in files {
        let path = file["source"]
            .as_str()
            .ok_or("assertion evidence requires source path")?;
        let origins = file["discovery"]["case_origins"]
            .as_array()
            .ok_or("assertion evidence requires discovered case origins")?;
        let mut cases = Vec::new();
        let mut unresolved = Vec::new();
        let mut seen = BTreeSet::new();
        for origin in origins {
            let field = |name: &str| {
                origin[name]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| format!("case assertion origin requires {name}"))
            };
            let binding = field("binding")?;
            let name = field("method")?;
            let identity = field("class_identity")?;
            let owner = field("defined_in_identity")?;
            let source_case = format!("{binding}.{name}");
            if !seen.insert(source_case.clone()) {
                return Err("case assertion evidence repeats a case".into());
            }
            let matching = definitions
                .get(&(owner, name))
                .filter(|(_, _, line, _)| origin["method_line"].as_u64() == Some(*line));
            let Some((definition, method, line, method_origin)) = matching else {
                unresolved.push(
                    json!({"source_case":source_case,"defined_in_identity":owner,
                    "reason":"original method assertion evidence is unavailable"}),
                );
                continue;
            };
            let mut helpers = Vec::new();
            if identity == owner && method_origin["plain_class"] == true {
                for call in method_origin["self_calls"].as_array().into_iter().flatten() {
                    let Some(called) = call["name"].as_str().filter(|called| *called != name)
                    else {
                        continue;
                    };
                    let Some((helper_file, helper, helper_line, helper_origin)) =
                        definitions.get(&(owner, called))
                    else {
                        continue;
                    };
                    let Some(direct) = helper_origin["direct_assertions"]
                        .as_array()
                        .filter(|assertions| !assertions.is_empty())
                    else {
                        continue;
                    };
                    if helper_origin["helper_callable"] != true
                        || direct.iter().any(|assertion| {
                            !helper["assertions"]
                                .as_array()
                                .is_some_and(|assertions| assertions.contains(assertion))
                        })
                    {
                        continue;
                    }
                    helpers.push(json!({"defined_in_identity":owner,
                        "defined_in_file":helper_file["source"],"defined_in_hash":helper_file["sha256"],
                        "method_line":helper_line,"method":helper,"call":call,"direct_assertions":direct}));
                }
            }
            cases.push(json!({"source_case":source_case,"class_identity":identity,
                "defined_in_identity":owner,"defined_in_file":definition["source"],
                "defined_in_hash":definition["sha256"],"method_line":line,
                "code_first_line":method_origin["code_first_line"],"method":method,"helper_assertions":helpers}));
        }
        // A decorator can hide the binding without erasing the admitted original body.
        // Keep every ambiguous declaration; consumers must not pick one by name alone.
        let declarations = file["assertions"]["methods"].as_array().unwrap().iter()
            .zip(file["assertions"]["method_origins"].as_array().unwrap())
            .filter(|(method, _)| method["method"].as_str().is_some_and(|name|
                name.starts_with("test") || name == "runTest"))
            .filter_map(|(method, origin)| {
                let name = format!("{}.{}", method["class"].as_str()?, method["method"].as_str()?);
                (!cases.iter().any(|case| case["source_case"] == name
                    && case["defined_in_identity"] == origin["class_identity"]
                    && case["method_line"] == origin["method_line"]
                    && case["code_first_line"] == origin["code_first_line"])).then(|| json!({
                    "source_case":name,"class_identity":origin["class_identity"],
                    "defined_in_identity":origin["class_identity"],"defined_in_file":path,
                    "defined_in_hash":file["sha256"],"method_line":origin["method_line"],
                    "code_first_line":origin["code_first_line"],"method":method,
                    "helper_assertions":[],"binding_resolved":false,"evidence_kind":"declaration-only"
                }))
            }).collect::<Vec<_>>();
        resolved.insert(
            path.into(),
            json!({"cases":cases,"unresolved":unresolved,"unresolved_declarations":declarations,
            "runtime_variants_complete":false}),
        );
    }
    Ok(Value::Object(resolved))
}
