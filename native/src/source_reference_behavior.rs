//! Original relocated-reference fingerprint policy; external AST/JSON APIs, no source execution.
use pyo3::exceptions::{PyAttributeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyComplex, PyDict, PyList};

struct Normalize<'py> {
    ast: Bound<'py, PyAny>,
    ellipsis: Bound<'py, PyAny>,
    remaining: usize,
}
impl<'py> Normalize<'py> {
    fn stable(
        &mut self,
        py: Python<'py>,
        value: Bound<'py, PyAny>,
        depth: usize,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        if depth > 256 || self.remaining == 0 {
            return Err(PyValueError::new_err(
                "reference behavior AST exceeds normalization bound",
            ));
        }
        self.remaining -= 1;
        if value.is_instance(&self.ast)? {
            let name = crate::source_test_inventory::kind(&value)?;
            if matches!(name.as_str(), "Import" | "ImportFrom") {
                return Ok(None);
            }
            if name == "Assign" {
                for target in value.getattr("targets")?.cast_into::<PyList>()?.iter() {
                    if crate::source_test_inventory::kind(&target)? == "Name"
                        && matches!(
                            target.getattr("id")?.extract::<String>()?.as_str(),
                            "ROOT" | "OFFSET_FILE"
                        )
                    {
                        return Ok(None);
                    }
                }
            }
            let fields = PyList::empty(py);
            for field in value.getattr("_fields")?.extract::<Vec<String>>()? {
                let item = match value.getattr(field.as_str()) {
                    Ok(item) => item,
                    Err(error) if error.is_instance_of::<PyAttributeError>(py) => {
                        py.None().into_bound(py)
                    }
                    Err(error) => return Err(error),
                };
                if field == "type_params" && !item.is_truthy()? {
                    continue;
                }
                let pair = PyList::empty(py);
                pair.append(field)?;
                pair.append(
                    self.stable(py, item, depth + 1)?
                        .unwrap_or_else(|| py.None().into_bound(py)),
                )?;
                fields.append(pair)?;
            }
            let node = PyList::empty(py);
            node.append(name)?;
            node.append(fields)?;
            return Ok(Some(node.into_any()));
        }
        if let Ok(values) = value.cast::<PyList>() {
            let result = PyList::empty(py);
            for value in values.iter() {
                if let Some(value) = self.stable(py, value, depth + 1)? {
                    result.append(value)?;
                }
            }
            return Ok(Some(result.into_any()));
        }
        if value.is_instance_of::<PyBytes>() {
            let result = PyDict::new(py);
            result.set_item("bytes", value.call_method0("hex")?)?;
            return Ok(Some(result.into_any()));
        }
        if value.is_instance_of::<PyComplex>() {
            let result = PyDict::new(py);
            let parts = PyList::empty(py);
            parts.append(value.getattr("real")?)?;
            parts.append(value.getattr("imag")?)?;
            result.set_item("complex", parts)?;
            return Ok(Some(result.into_any()));
        }
        if value.is(&self.ellipsis) {
            let result = PyDict::new(py);
            result.set_item("ellipsis", true)?;
            return Ok(Some(result.into_any()));
        }
        Ok(Some(value))
    }
}

pub fn digest(source: &str) -> Result<String, String> {
    if source.len() > 2 * 1024 * 1024 {
        return Err("reference behavior source exceeds2MiB".into());
    }
    Python::initialize();
    Python::attach(|py| -> PyResult<String> {
        let ast = py.import("ast")?;
        let tree = ast.call_method1("parse", (source,))?;
        let mut normalizer = Normalize {
            ast: ast.getattr("AST")?,
            ellipsis: py.import("builtins")?.getattr("Ellipsis")?,
            remaining: 500_000,
        };
        let stable = normalizer
            .stable(py, tree, 0)?
            .ok_or_else(|| PyValueError::new_err("reference behavior root was removed"))?;
        let options = PyDict::new(py);
        options.set_item("ensure_ascii", true)?;
        options.set_item("separators", (",", ":"))?;
        let text: String = py
            .import("json")?
            .call_method("dumps", (stable,), Some(&options))?
            .extract()?;
        Ok(crate::codec_raster::digest_hex(text.as_bytes()))
    })
    .map_err(|error| format!("reference behavior normalization failed: {error}"))
}
