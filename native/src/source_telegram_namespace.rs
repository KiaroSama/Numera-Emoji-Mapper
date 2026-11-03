//! Shared historical Telegram client assembly; callers admit the exact source before use.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

pub(crate) fn namespace<'py>(
    py: Python<'py>,
    source: &Source,
    bindings: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyDict>> {
    let globals = bindings.copy()?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let mock = py.import("unittest.mock")?.getattr("Mock")?;
    // Original fixtures replace the session before calling it. An omitted patch
    // must refuse, never open a real connection or manufacture a successful reply.
    let blocked = PyDict::new(py);
    blocked.set_item(
        "side_effect",
        PyRuntimeError::new_err("unpatched original publication effect").value(py),
    )?;
    let session = ns.call0()?;
    session.setattr("post", mock.call((), Some(&blocked))?)?;
    session.setattr("get", mock.call((), Some(&blocked))?)?;
    let session_options = PyDict::new(py);
    session_options.set_item("return_value", session)?;
    let requests = ns.call0()?;
    requests.setattr("Session", mock.call((), Some(&session_options))?)?;
    requests.setattr(
        "RequestException",
        py.import("requests")?.getattr("RequestException")?,
    )?;
    requests.setattr("exceptions", py.import("requests.exceptions")?)?;
    globals.set_item("requests", requests)?;
    globals.set_item("json", py.import("json")?)?;
    globals.set_item(
        "NAME_LOCK_TIMEOUT",
        crate::source_logging_oracle::literal(py, source, "NAME_LOCK_TIMEOUT", false)?,
    )?;
    for name in ["COIN_DEFAULT_EMOJI", "_MIME", "_MIME_BY_EXT"] {
        globals.set_item(
            name,
            crate::source_logging_oracle::literal(py, source, name, false)?,
        )?;
    }
    for name in [
        "SetState",
        "LiveStateUnknown",
        "BotApiError",
        "AmbiguousUploadError",
    ] {
        required(&globals, name)?;
    }
    crate::source_oracle_runtime::namespace(
        py,
        source,
        &[
            "api_base",
            "_usable_fuids",
            "_mime_for_path",
            "_trim_keywords",
            "_input_sticker",
            "_sticker_json",
            "Telegram",
        ],
        &[
            "__build_class__",
            "range",
            "isinstance",
            "dict",
            "str",
            "int",
            "print",
            "ValueError",
            "min",
            "RuntimeError",
            "Exception",
            "any",
            "len",
            "set",
            "next",
        ],
        &globals,
    )
}
