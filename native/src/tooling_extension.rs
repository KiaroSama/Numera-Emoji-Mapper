//! Explicit on-disk extension admission for the fresh selected-runtime probe.
use crate::{migration_bundle, tooling_environment, tooling_manifest};
use pyo3::prelude::*;
use serde_json::{Value, json};
use std::path::Path;

pub(crate) fn inspect(py: Python<'_>, root: &Path, path: &Path) -> Result<Value, String> {
    let root = tooling_environment::physical_root(root)?;
    if !path.is_absolute() {
        return Err("native extension path must be absolute".into());
    }
    tooling_environment::reject_links(path, "native extension cannot follow links")?;
    let path = migration_bundle::resolved(path)?;
    if !path.starts_with(&root) {
        return Err("native extension must remain inside its owning project".into());
    }
    let metadata = std::fs::metadata(&path).map_err(|_| "native extension is unreadable")?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err("native extension must be a nonempty regular file".into());
    }
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("native extension filename must be UTF-8")?;
    let text = path.to_str().ok_or("native extension path must be UTF-8")?;
    let before = tooling_manifest::digest(&path)?;
    let inspect = || -> PyResult<Value> {
        let machinery = py.import("importlib.machinery")?;
        let suffixes: Vec<String> = machinery.getattr("EXTENSION_SUFFIXES")?.extract()?;
        if !suffixes
            .iter()
            .any(|suffix| filename == format!("_native{suffix}"))
        {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "native extension suffix does not match the selected interpreter",
            ));
        }
        let modules = py.import("sys")?.getattr("modules")?;
        if modules.contains("_native")? || modules.contains("emojikit")? {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "native extension probe requires an uncached module and package",
            ));
        }
        // Force the external binary loader: never execute the package initializer
        // or satisfy this check with the module compiled into the probe executable.
        let loader = machinery
            .getattr("ExtensionFileLoader")?
            .call1(("_native", text))?;
        let util = py.import("importlib.util")?;
        let spec = util.call_method1("spec_from_loader", ("_native", &loader))?;
        let module = util.call_method1("module_from_spec", (&spec,))?;
        loader.call_method1("exec_module", (&module,))?;
        let origin: String = module.getattr("__file__")?.extract()?;
        let origin = migration_bundle::resolved(Path::new(&origin))
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
        let version: i32 = module.getattr("API_VERSION")?.extract()?;
        let greedy = module.getattr("greedy_indices")?.is_callable();
        let near = module.getattr("near_indices")?.is_callable();
        if origin != path || version != 1 || !greedy || !near || modules.contains("emojikit")? {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "native extension origin or required API is incompatible",
            ));
        }
        Ok(
            json!({"path":origin,"api_version":version,"greedy_callable":greedy,
            "near_callable":near,"package_imported":false,"sha256":before}),
        )
    };
    let report = inspect().map_err(|error| {
        format!(
            "native extension verification failed: {}",
            crate::logging::redact(&error.to_string(), &[])
        )
    })?;
    if tooling_manifest::digest(&path)? != before {
        return Err("native extension changed while being verified".into());
    }
    Ok(report)
}
