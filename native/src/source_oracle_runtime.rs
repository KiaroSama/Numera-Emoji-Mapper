//! Shared historical-oracle mechanics; callers admit only their fixed source identity first.
use crate::source_git_inventory::Source;
use crate::source_test_inventory::{body, kind};
use pyo3::prelude::*;
use pyo3::types::{PyCode, PyCodeMethods, PyDict, PyList, PyModule};

const MODULE: &str = "pinned_source_oracle";

/// Rust-side fixture ports import through importlib, outside an original
/// function's deliberately restricted __import__ facade. Cache the importer
/// before calling original code so a callback cannot recursively import itself.
pub(crate) fn host_import<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyModule>> {
    static IMPORTER: pyo3::sync::PyOnceLock<Py<PyAny>> = pyo3::sync::PyOnceLock::new();
    let importer = IMPORTER.get_or_try_init(py, || -> PyResult<_> {
        Ok(py.import("importlib")?.getattr("import_module")?.unbind())
    })?;
    Ok(importer.bind(py).call1((name,))?.cast_into::<PyModule>()?)
}

pub(crate) struct ModuleRegistration<'py> {
    modules: Bound<'py, PyDict>,
    previous: Option<Bound<'py, PyAny>>,
    restored: bool,
}
impl<'py> ModuleRegistration<'py> {
    pub(crate) fn new(py: Python<'py>, bindings: &Bound<'py, PyDict>) -> PyResult<Self> {
        let modules = py
            .import("sys")?
            .getattr("modules")?
            .cast_into::<PyDict>()?;
        let previous = modules.get_item(MODULE)?;
        let module = PyModule::new(py, MODULE)?;
        module.dict().update(bindings.as_mapping())?;
        modules.set_item(MODULE, module)?;
        Ok(Self {
            modules,
            previous,
            restored: false,
        })
    }
    pub(crate) fn restore(&mut self) -> PyResult<()> {
        if !self.restored {
            match &self.previous {
                Some(value) => self.modules.set_item(MODULE, value)?,
                None => self.modules.del_item(MODULE)?,
            }
            self.restored = true;
        }
        Ok(())
    }
}
impl Drop for ModuleRegistration<'_> {
    fn drop(&mut self) {
        if self.restore().is_err() {
            eprintln!("ERROR: historical oracle module registration could not be restored");
        }
    }
}

pub(crate) fn require_runtime(py: Python<'_>) -> Result<(), String> {
    let sys = py
        .import("sys")
        .map_err(|_| "cannot inspect oracle runtime")?;
    let implementation: String = sys
        .getattr("implementation")
        .and_then(|value| value.getattr("name"))
        .and_then(|value| value.extract())
        .map_err(|_| "cannot inspect oracle implementation")?;
    let version = sys
        .getattr("version_info")
        .map_err(|_| "cannot inspect oracle version")?;
    let mut actual = [0_u8; 3];
    for (index, part) in actual.iter_mut().enumerate() {
        *part = version
            .get_item(index)
            .and_then(|value| value.extract::<u8>())
            .map_err(|_| "invalid oracle version")?;
    }
    if implementation != "cpython" || actual != [3, 11, 9] {
        return Err("source oracle requires exactly CPython3.11.9".into());
    }
    Ok(())
}

pub(crate) fn without_applications(
    observe: impl FnOnce() -> Result<serde_json::Value, String>,
) -> Result<serde_json::Value, String> {
    const BLOCKED: [&str; 8] = [
        "emojikit._native",
        "emojikit.catalog",
        "emojikit.operator_config",
        "emojikit.packstate",
        "emojikit.panel_plan",
        "emojikit.panel_view",
        "emojikit.plan_apply",
        "emojikit.collection_state",
    ];
    Python::initialize();
    Python::attach(|py| -> Result<serde_json::Value, String> {
        require_runtime(py)?;
        let setup = (|| -> PyResult<_> {
            let modules = py
                .import("sys")?
                .getattr("modules")?
                .cast_into::<PyDict>()?;
            let previous = BLOCKED
                .iter()
                .map(|name| modules.get_item(*name))
                .collect::<PyResult<Vec<_>>>()?;
            Ok((modules, previous))
        })()
        .map_err(|error| format!("cannot inspect application imports: {error}"))?;
        let (modules, previous) = setup;
        let blocked = BLOCKED
            .iter()
            .try_for_each(|name| modules.set_item(*name, py.None()))
            .map_err(|error| format!("cannot block application imports: {error}"));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            blocked?;
            observe()
        }));
        // patch.dict restores the whole dictionary, discarding unrelated imports. Own eight entries only.
        let mut errors = Vec::new();
        for (name, previous) in BLOCKED.iter().zip(previous) {
            let restore = (|| -> PyResult<()> {
                match &previous {
                    Some(value) => modules.set_item(*name, value)?,
                    None if modules.contains(*name)? => modules.del_item(*name)?,
                    None => {}
                }
                let current = modules.get_item(*name)?;
                if !match (&previous, current) {
                    (Some(previous), Some(current)) => previous.is(&current),
                    (None, None) => true,
                    _ => false,
                } {
                    return Err(pyo3::exceptions::PyRuntimeError::new_err(
                        "application module identity was not restored",
                    ));
                }
                Ok(())
            })();
            if let Err(error) = restore {
                errors.push(format!("{name}: {error}"));
            }
        }
        let restored = if errors.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "application import restoration failed: {}",
                errors.join("; ")
            ))
        };
        let result = match result {
            Ok(result) => result,
            Err(panic) => {
                if let Err(error) = restored {
                    eprintln!("ERROR: {error}");
                }
                std::panic::resume_unwind(panic)
            }
        };
        if let Err(error) = restored {
            return Err(match result {
                Err(original) => format!("{original}; {error}"),
                Ok(_) => error,
            });
        }
        let mut result = result?;
        result["blocked_application_modules"] = serde_json::json!(BLOCKED.len());
        Ok(result)
    })
}

pub(crate) fn namespace<'py>(
    py: Python<'py>,
    source: &Source,
    definitions: &[&str],
    builtins: &[&str],
    bindings: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyDict>> {
    populate(
        py,
        source,
        definitions,
        builtins,
        bindings,
        PyDict::new(py),
        MODULE,
    )
}

pub(crate) fn module<'py>(
    py: Python<'py>,
    source: &Source,
    definitions: &[&str],
    builtins: &[&str],
    bindings: &Bound<'py, PyDict>,
    name: &str,
) -> PyResult<Bound<'py, PyModule>> {
    let module = PyModule::new(py, name)?;
    // Decorated generators close over their original globals; build in the real
    // module dictionary so patch.object reaches them without rewriting closures.
    populate(
        py,
        source,
        definitions,
        builtins,
        bindings,
        module.dict(),
        name,
    )?;
    Ok(module)
}

fn populate<'py>(
    py: Python<'py>,
    source: &Source,
    definitions: &[&str],
    builtins: &[&str],
    bindings: &Bound<'py, PyDict>,
    globals: Bound<'py, PyDict>,
    name: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let ast = py.import("ast")?;
    let tree = ast.call_method1("parse", (&source.text, &source.path))?;
    let selected = PyList::empty(py);
    let mut found = std::collections::BTreeSet::new();
    for node in body(&tree)? {
        let node_kind = kind(&node)?;
        if node_kind == "ImportFrom"
            && node
                .getattr("module")?
                .extract::<Option<String>>()?
                .as_deref()
                == Some("__future__")
        {
            selected.append(node)?;
        } else if matches!(node_kind.as_str(), "FunctionDef" | "ClassDef") {
            let name = node.getattr("name")?.extract::<String>()?;
            if definitions.contains(&name.as_str()) {
                if !found.insert(name) {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "duplicate oracle definition",
                    ));
                }
                selected.append(node)?;
            }
        }
    }
    if found.len() != definitions.len() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "missing oracle definition",
        ));
    }
    let library = py.import("builtins")?;
    let allowed = PyDict::new(py);
    for &name in builtins {
        allowed.set_item(name, library.getattr(name)?)?;
    }
    // The hash-verified future statement imports stdlib only, never application modules.
    allowed.set_item("__import__", library.getattr("__import__")?)?;
    globals.set_item("__builtins__", allowed)?;
    globals.set_item("__name__", name)?;
    for (key, value) in bindings.iter() {
        globals.set_item(key, value)?;
    }
    execute_nodes(py, source, &selected, &globals)?;
    Ok(globals)
}

fn execute_nodes(
    py: Python<'_>,
    source: &Source,
    selected: &Bound<'_, PyList>,
    globals: &Bound<'_, PyDict>,
) -> PyResult<()> {
    let options = PyDict::new(py);
    options.set_item("body", selected)?;
    options.set_item("type_ignores", PyList::empty(py))?;
    let module = py
        .import("ast")?
        .getattr("Module")?
        .call((), Some(&options))?;
    let flags = PyDict::new(py);
    flags.set_item("dont_inherit", true)?;
    py.import("builtins")?
        .getattr("compile")?
        .call((module, &source.path, "exec"), Some(&flags))?
        .cast_into::<PyCode>()?
        .run(Some(globals), Some(globals))?;
    Ok(())
}

/// Execute one admitted original assignment in an already private module dictionary.
pub(crate) fn assignment(
    py: Python<'_>,
    source: &Source,
    name: &str,
    globals: &Bound<'_, PyDict>,
) -> PyResult<()> {
    let tree = py
        .import("ast")?
        .call_method1("parse", (&source.text, &source.path))?;
    let selected = PyList::empty(py);
    for node in body(&tree)? {
        if kind(&node)? == "Assign" {
            let targets = node.getattr("targets")?;
            if targets.len()? == 1 {
                let target = targets.get_item(0)?;
                if kind(&target)? == "Name" && target.getattr("id")?.extract::<String>()? == name {
                    selected.append(node)?;
                }
            }
        }
    }
    if selected.len() != 1 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "original assignment identity drift",
        ));
    }
    execute_nodes(py, source, &selected, globals)
}
