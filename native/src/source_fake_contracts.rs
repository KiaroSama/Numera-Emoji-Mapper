//! Original fake-client signature policy over external AST nodes, never imported source modules.
use crate::source_git_inventory::Source;
use crate::source_test_inventory::{body, kind, walk};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyList;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub real_methods: usize,
    pub fake_methods: usize,
    pub keyword_errors: Vec<String>,
    pub positional_errors: Vec<String>,
}

#[derive(Clone)]
struct Arguments {
    names: BTreeSet<String>,
    keywords: BTreeSet<String>,
    positional: usize,
    required: usize,
    vararg: bool,
    kwarg: bool,
}
fn arguments(node: &Bound<'_, PyAny>) -> PyResult<Arguments> {
    let args = node.getattr("args")?;
    let mut names = BTreeSet::new();
    let mut keywords = BTreeSet::new();
    let mut positional = 0;
    for field in ["posonlyargs", "args", "kwonlyargs"] {
        for arg in args.getattr(field)?.cast_into::<PyList>()?.iter() {
            let name = arg.getattr("arg")?.extract::<String>()?;
            if field == "kwonlyargs" {
                keywords.insert(name.clone());
            } else {
                positional += 1;
            }
            names.insert(name);
        }
    }
    let defaults = args.getattr("defaults")?.cast_into::<PyList>()?.len();
    Ok(Arguments {
        names,
        keywords,
        positional,
        required: positional
            .checked_sub(defaults)
            .ok_or_else(|| PyValueError::new_err("invalid positional defaults"))?,
        vararg: !args.getattr("vararg")?.is_none(),
        kwarg: !args.getattr("kwarg")?.is_none(),
    })
}
fn surface(ast: &Bound<'_, PyModule>, text: &str) -> PyResult<BTreeMap<String, Arguments>> {
    let tree = ast.call_method1("parse", (text, "emojikit/telegram_api.py"))?;
    let mut classes = Vec::new();
    for node in body(&tree)? {
        if kind(&node)? == "ClassDef" && node.getattr("name")?.extract::<String>()? == "Telegram" {
            classes.push(node);
        }
    }
    if classes.len() != 1 {
        return Err(PyValueError::new_err(
            "fake contract requires one direct Telegram class",
        ));
    }
    let class = &classes[0];
    if !class.getattr("bases")?.cast_into::<PyList>()?.is_empty()
        || !class
            .getattr("decorator_list")?
            .cast_into::<PyList>()?
            .is_empty()
        || !class.getattr("keywords")?.cast_into::<PyList>()?.is_empty()
    {
        return Err(PyValueError::new_err(
            "dynamic Telegram class surface is unverified",
        ));
    }
    let mut methods = BTreeMap::new();
    for node in body(class)? {
        match kind(&node)?.as_str() {
            "FunctionDef" | "AsyncFunctionDef" => {
                if !node
                    .getattr("decorator_list")?
                    .cast_into::<PyList>()?
                    .is_empty()
                {
                    return Err(PyValueError::new_err(
                        "decorated Telegram signature is unverified",
                    ));
                }
                methods.insert(
                    node.getattr("name")?.extract::<String>()?,
                    arguments(&node)?,
                );
            }
            "Assign" => {
                let value = node.getattr("value")?;
                if kind(&value)? != "Name" {
                    return Err(PyValueError::new_err(
                        "dynamic Telegram method assignment is unverified",
                    ));
                }
                let name = value.getattr("id")?.extract::<String>()?;
                let signature = methods
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| PyValueError::new_err("unknown Telegram method alias"))?;
                for target in node.getattr("targets")?.cast_into::<PyList>()?.iter() {
                    if kind(&target)? != "Name" {
                        return Err(PyValueError::new_err(
                            "dynamic Telegram alias target is unverified",
                        ));
                    }
                    methods.insert(
                        target.getattr("id")?.extract::<String>()?,
                        signature.clone(),
                    );
                }
            }
            "Pass" => {}
            "Expr"
                if kind(&node.getattr("value")?)? == "Constant"
                    && node
                        .getattr("value")?
                        .getattr("value")?
                        .extract::<String>()
                        .is_ok() => {}
            _ => {
                return Err(PyValueError::new_err(
                    "dynamic Telegram class body is unverified",
                ));
            }
        }
    }
    methods.retain(|name, _| !name.starts_with('_') || matches!(name.as_str(), "_call" | "_safe"));
    if methods.is_empty() {
        return Err(PyValueError::new_err("Telegram method surface is empty"));
    }
    Ok(methods)
}

pub fn inspect(sources: &[Source]) -> Result<Report, String> {
    let mut seen = BTreeSet::new();
    for source in sources {
        crate::source_test_inventory::source_path(&source.path)?;
        if source.text.len() > 2 * 1024 * 1024
            || source.sha256 != crate::codec_raster::digest_hex(source.text.as_bytes())
            || !seen.insert(source.path.as_str())
        {
            return Err("fake-client inventory requires unique admitted source bytes".into());
        }
    }
    let real = sources
        .iter()
        .find(|source| source.path == "emojikit/telegram_api.py")
        .ok_or("fake-client inventory is missing Telegram source")?;
    Python::initialize();
    Python::attach(|py| -> PyResult<Report> {
        let ast = py.import("ast")?;
        let methods = surface(&ast, &real.text)?;
        let mut report = Report { real_methods: methods.len(), ..Report::default() };
        for source in sources {
            let Some(file) = source.path.strip_prefix("tests/") else { continue; };
            if file.contains('/') { continue; }
            let tree = ast.call_method1("parse", (&source.text, &source.path))?;
            for class in walk(&ast, &tree)? {
                if kind(&class)? != "ClassDef" { continue; }
                let mut test_case = false;
                for base in class.getattr("bases")?.cast_into::<PyList>()?.iter() {
                    let text: String = ast.call_method1("unparse", (base,))?.extract()?;
                    test_case |= text.contains("TestCase");
                }
                if test_case { continue; }
                let class_name = class.getattr("name")?.extract::<String>()?;
                for method in body(&class)? {
                    if kind(&method)? != "FunctionDef" { continue; }
                    let name = method.getattr("name")?.extract::<String>()?;
                    let Some(real) = methods.get(&name) else { continue; };
                    let fake = arguments(&method)?;
                    report.fake_methods += 1;
                    let location = format!("{file}:{class_name}.{name}");
                    if !fake.kwarg {
                        for keyword in real.keywords.difference(&fake.names) {
                            report.keyword_errors.push(format!("{location} does not accept {keyword:?}, which Telegram.{name} defines"));
                        }
                    }
                    if !fake.vararg && fake.positional < real.required {
                        report.positional_errors.push(format!("{location} takes {} positional argument(s); Telegram.{name} passes {}", fake.positional, real.required));
                    }
                }
            }
        }
        report.keyword_errors.sort();
        report.positional_errors.sort();
        Ok(report)
    }).map_err(|error| format!("fake-client signature inventory failed: {error}"))
}

pub fn check(report: &Report) -> Result<(), String> {
    if report.real_methods == 0 || report.fake_methods < 5 {
        return Err(
            "fake-client signature scan must find the real surface and at least five fakes".into(),
        );
    }
    if !report.keyword_errors.is_empty() || !report.positional_errors.is_empty() {
        return Err(format!(
            "original fake-client signature violations: {report:?}"
        ));
    }
    Ok(())
}
