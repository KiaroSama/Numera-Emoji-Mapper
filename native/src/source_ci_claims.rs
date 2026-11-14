//! Original CI suite ownership: Rust policy over external AST nodes, never imported test modules.
use crate::source_git_inventory::Source;
use crate::source_test_inventory::{body, kind, walk};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyList};
use std::collections::BTreeSet;

const HARNESS: &str = "_panel_browser_fixtures";
const WINDOWS: &str = "RUNS_ON_NATIVE_WINDOWS";
const OFF_WINDOWS: [&str; 2] = ["!= 'win32'", "!= 'nt'"];
const ON_WINDOWS: [&str; 2] = ["== 'win32'", "== 'nt'"];

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Claims {
    pub test_modules: usize,
    pub direct_runs: BTreeSet<String>,
    pub browser: BTreeSet<String>,
    pub windows: BTreeSet<String>,
    pub unmarked_windows: BTreeSet<String>,
}

fn imports_harness(node: &Bound<'_, PyAny>) -> PyResult<bool> {
    let from = kind(node)? == "ImportFrom";
    if !from && kind(node)? != "Import" {
        return Ok(false);
    }
    let module = if from {
        node.getattr("module")?.extract::<Option<String>>()?
    } else {
        None
    };
    if module
        .as_deref()
        .is_some_and(|module| module.contains(HARNESS))
    {
        return Ok(true);
    }
    for alias in node.getattr("names")?.cast_into::<PyList>()?.iter() {
        let name: String = alias.getattr("name")?.extract()?;
        if (!from && name.contains(HARNESS))
            || (module.as_deref() == Some("tests") && name == HARNESS)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn declares_windows(tree: &Bound<'_, PyAny>) -> PyResult<bool> {
    for node in body(tree)? {
        if kind(&node)? != "Assign" {
            continue;
        }
        for target in node.getattr("targets")?.cast_into::<PyList>()?.iter() {
            if kind(&target)? == "Name" && target.getattr("id")?.extract::<String>()? == WINDOWS {
                let value = node.getattr("value")?;
                if kind(&value)? != "Constant" {
                    return Ok(false);
                }
                let value = value.getattr("value")?;
                return Ok(value.is_instance_of::<PyBool>() && value.extract::<bool>()?);
            }
        }
    }
    Ok(false)
}

fn attribute_call(node: &Bound<'_, PyAny>, name: &str) -> PyResult<bool> {
    Ok(kind(node)? == "Call"
        && kind(&node.getattr("func")?)? == "Attribute"
        && node.getattr("func")?.getattr("attr")?.extract::<String>()? == name)
}

fn direct_unittest_main(ast: &Bound<'_, PyModule>, nodes: &[Bound<'_, PyAny>]) -> PyResult<bool> {
    for node in nodes {
        if kind(node)? != "If" {
            continue;
        }
        let condition = node.getattr("test")?;
        if kind(&condition)? != "Compare" {
            continue;
        }
        let left = condition.getattr("left")?;
        if kind(&left)? != "Name" || left.getattr("id")?.extract::<String>()? != "__name__" {
            continue;
        }
        // The original guard includes every descendant, including else blocks and nested definitions.
        for child in walk(ast, node)? {
            if attribute_call(&child, "main")? {
                let receiver = child.getattr("func")?.getattr("value")?;
                if kind(&receiver)? == "Name"
                    && receiver.getattr("id")?.extract::<String>()? == "unittest"
                {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

fn skips_unless_windows(ast: &Bound<'_, PyModule>, nodes: &[Bound<'_, PyAny>]) -> PyResult<bool> {
    for node in nodes {
        if kind(node)? == "If" {
            let condition: String = ast
                .call_method1("unparse", (node.getattr("test")?,))?
                .extract()?;
            if OFF_WINDOWS.iter().any(|text| condition.contains(text)) {
                for statement in body(node)? {
                    for child in walk(ast, &statement)? {
                        if attribute_call(&child, "skipTest")? {
                            return Ok(true);
                        }
                    }
                }
            }
        }
        if kind(node)? == "Call" && kind(&node.getattr("func")?)? == "Attribute" {
            let arguments = node.getattr("args")?.cast_into::<PyList>()?;
            if arguments.is_empty() {
                continue;
            }
            let condition: String = ast
                .call_method1("unparse", (arguments.get_item(0)?,))?
                .extract()?;
            let name: String = node.getattr("func")?.getattr("attr")?.extract()?;
            if (name == "skipUnless" && ON_WINDOWS.iter().any(|text| condition.contains(text)))
                || (name == "skipIf" && OFF_WINDOWS.iter().any(|text| condition.contains(text)))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub fn step_run(workflow: &str, name: &str) -> Result<String, &'static str> {
    let heading = format!("- name: {name}");
    let lines = workflow.lines().collect::<Vec<_>>();
    let starts = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim() == heading)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if starts.len() != 1 {
        return Err("CI step must exist exactly once");
    }
    let start = starts[0];
    let indentation = lines[start].len() - lines[start].trim_start().len();
    let end = (start + 1..lines.len())
        .find(|index| {
            let line = lines[*index];
            !line.trim().is_empty()
                && !line.trim().starts_with('#')
                && line.len() - line.trim_start().len() <= indentation
        })
        .unwrap_or(lines.len());
    let block = &lines[start + 1..end];
    let run = block
        .iter()
        .position(|line| line.trim_start().starts_with("run:"))
        .ok_or("CI step has no run command")?;
    let line = block[run];
    let value = line.trim_start().strip_prefix("run:").unwrap().trim();
    if !matches!(value, "|" | ">-" | ">") {
        return Ok(value.into());
    }
    let run_indent = line.len() - line.trim_start().len();
    Ok(block[run + 1..]
        .iter()
        .take_while(|line| {
            line.trim().is_empty() || line.len() - line.trim_start().len() > run_indent
        })
        .map(|line| line.trim())
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn check(claims: &Claims, workflow: &str) -> Result<(), String> {
    if claims.test_modules == 0 {
        return Err("original suite guard must inspect nonempty test modules".into());
    }
    if !claims.direct_runs.is_empty() {
        return Err(format!(
            "test modules call unittest.main() outside the hermetic guard: {:?}",
            claims.direct_runs
        ));
    }
    if claims.browser.len() < 2 || claims.windows.len() < 5 {
        return Err(
            "CI suite discovery must find nonempty original browser/Windows cohorts".into(),
        );
    }
    if !claims.unmarked_windows.is_empty() {
        return Err(format!(
            "Windows-only suites lack their marker: {:?}",
            claims.unmarked_windows
        ));
    }
    let browser = workflow
        .split_once("\n  panel-browser:")
        .and_then(|(_, rest)| rest.split_once("\n  build:"))
        .map(|(job, _)| job)
        .ok_or("original browser job is missing")?;
    let browser_run = step_run(browser, "Panel browser tests")?;
    let windows = workflow
        .split_once("\n  windows-safety:")
        .map(|(_, job)| job)
        .ok_or("original Windows job is missing")?;
    let windows_run = step_run(
        windows,
        "Native lock, migration, HTTP persistence, identity and CLI regressions",
    )?;
    let names = |text: &str| {
        text.split_whitespace()
            .filter(|word| word.starts_with("tests.test_"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let browser_names = names(&browser_run);
    let windows_names = names(&windows_run);
    let all_names = names(workflow);
    for module in &claims.browser {
        if browser_names.iter().filter(|name| *name == module).count() != 1
            || all_names.iter().filter(|name| *name == module).count() != 1
        {
            return Err(format!(
                "original browser suite must execute exactly once: {module}"
            ));
        }
    }
    for module in &claims.windows {
        if windows_names.iter().filter(|name| *name == module).count() != 1 {
            return Err(format!(
                "original Windows suite must execute exactly once: {module}"
            ));
        }
    }
    if windows_names
        .iter()
        .any(|name| !claims.windows.contains(name))
    {
        return Err("original Windows job names a stale or unmarked suite".into());
    }
    if !workflow.contains("NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS") {
        return Err("original matrix must explicitly opt out of browser tests".into());
    }
    Ok(())
}

pub fn inspect(sources: &[Source]) -> Result<Claims, String> {
    let mut admitted = BTreeSet::new();
    for source in sources {
        crate::source_test_inventory::source_path(&source.path)?;
        if source.text.len() > 2 * 1024 * 1024
            || source.sha256 != crate::codec_raster::digest_hex(source.text.as_bytes())
            || !admitted.insert(source.path.as_str())
        {
            return Err("CI suite inventory requires unique admitted source bytes".into());
        }
    }
    Python::initialize();
    Python::attach(|py| -> PyResult<Claims> {
        let ast = py.import("ast")?;
        let mut claims = Claims::default();
        for source in sources {
            let Some(file) = source.path.strip_prefix("tests/") else {
                continue;
            };
            if file.contains('/') || !file.starts_with("test_") {
                continue;
            }
            let tree = ast.call_method1("parse", (&source.text, &source.path))?;
            let nodes = walk(&ast, &tree)?;
            claims.test_modules += 1;
            if direct_unittest_main(&ast, &nodes)? {
                claims.direct_runs.insert(file.into());
            }
            let module = format!("tests.{}", file.strip_suffix(".py").unwrap());
            let mut browser = false;
            for node in &nodes {
                browser |= imports_harness(node)?;
            }
            if browser {
                claims.browser.insert(module.clone());
            }
            if declares_windows(&tree)? {
                claims.windows.insert(module.clone());
            } else if skips_unless_windows(&ast, &nodes)? {
                claims.unmarked_windows.insert(module);
            }
        }
        Ok(claims)
    })
    .map_err(|error| format!("CI suite AST inventory failed: {error}"))
}
