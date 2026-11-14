//! Rust owns bounded matching; unchanged NumPy APIs supply source float32 BLAS operations.
use pyo3::prelude::*;
use pyo3::types::PyDict;

pub use crate::coin_remap_policy::Nearest;

fn numpy(py: Python<'_>) -> Result<Bound<'_, pyo3::types::PyModule>, String> {
    py.import("numpy").map_err(|error| {
        if error.is_instance_of::<pyo3::exceptions::PyImportError>(py) {
            "coin-remap-ids needs numpy: pip install -r requirements-coins.txt".into()
        } else {
            "external remap numerical dependency import failed".into()
        }
    })
}

pub fn require_available() -> Result<(), String> {
    Python::initialize();
    Python::attach(|py| numpy(py).map(|_| ()))
}

pub fn nearest(local: &[Vec<f32>], live: &[Vec<f32>]) -> Result<Nearest, String> {
    let width = live
        .first()
        .map(Vec::len)
        .filter(|n| *n > 0)
        .ok_or("remap needs nonempty live signatures")?;
    if local.iter().chain(live).any(|row| {
        row.len() != width
            || row
                .iter()
                .any(|value| !value.is_finite() || !(0.0..=255.0).contains(value))
    }) {
        return Err("remap signatures must have equal width and finite byte-range samples".into());
    }
    if width > 4096 || live.len() > 20000 || local.len() > 100000 {
        return Err("remap numerical working set exceeds bounded signature limits".into());
    }
    Python::initialize();
    Python::attach(|py| -> Result<Nearest, String> {
        let np = numpy(py)?;
        (|| -> PyResult<Nearest> {
            let options = PyDict::new(py);
            options.set_item("dtype", np.getattr("float32")?)?;
            let live = np.call_method("array", (live.to_vec(),), Some(&options))?;
            let vn = live
                .call_method1("__mul__", (&live,))?
                .call_method1("sum", (1,))?;
            let transposed = live.getattr("T")?;
            let mut result = Nearest {
                indices: Vec::new(),
                squared: Vec::new(),
                second: Vec::new(),
            };
            for block in local.chunks(512) {
                let block = np.call_method("array", (block.to_vec(),), Some(&options))?;
                let ln = block
                    .call_method1("__mul__", (&block,))?
                    .call_method1("sum", (1,))?
                    .call_method1("reshape", ((-1, 1),))?;
                let gram = np.call_method1("matmul", (&block, &transposed))?;
                let doubled = gram.call_method1("__rmul__", (2,))?;
                let squared = ln
                    .call_method1("__sub__", (doubled,))?
                    .call_method1("__add__", (&vn,))?;
                let squared = np.call_method1("maximum", (squared, 0.0))?;
                let best = squared
                    .call_method1("argmin", (1,))?
                    .call_method0("tolist")?
                    .extract::<Vec<usize>>()?;
                for (row, index) in best.into_iter().enumerate() {
                    result.indices.push(index);
                    result
                        .squared
                        .push(squared.get_item((row, index))?.extract::<f64>()?);
                    squared.set_item((row, index), f64::INFINITY)?;
                }
                result.second.extend(
                    squared
                        .call_method1("min", (1,))?
                        .call_method0("tolist")?
                        .extract::<Vec<f64>>()?,
                );
            }
            Ok(result)
        })()
        .map_err(|_| "external remap numerical operation failed".into())
    })
}
