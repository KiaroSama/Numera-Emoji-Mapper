//! Regenerate the seven admitted original audit-policy observations without native expectations.
use crate::source_git_inventory::Source;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde_json::{Value, json};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 2] = [
    (
        "coins/check_all_packs.py",
        "f62da85bd939464a2673a758740cab9001ae0920a000df32f7827d42485b5272",
    ),
    (
        "emojikit/cli_env.py",
        "9e4c0a045f031bffd23fe5ff9e51a132363e9952a1cdde47d071bfc81234e887",
    ),
];
// Independently read from c80aa79: remove only each case.expected, retain object order.
const RECIPE: &str = "9a001c55123b24e6b559042a7e7c6c4f14d2b91107cf0863c1ac41773e173522";

pub fn source_identity(source: &Source, environment: &Source) -> Result<(), String> {
    for (source, (path, hash)) in [source, environment].into_iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("audit oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

pub fn fixture_identity(fixture: &Value) -> Result<(), String> {
    let mut recipe = fixture.clone();
    let cases = recipe
        .get_mut("cases")
        .and_then(Value::as_array_mut)
        .filter(|cases| cases.len() == 7)
        .ok_or("audit oracle requires7 original observations")?;
    for case in cases {
        let case = case.as_object_mut().ok_or("invalid original audit case")?;
        if !case.get("input").is_some_and(Value::is_object)
            || !case.get("expected").is_some_and(Value::is_object)
        {
            return Err("audit oracle requires input and expected objects".into());
        }
        case.remove("expected");
    }
    let bytes = serde_json::to_vec(&recipe).map_err(|_| "cannot encode audit oracle recipe")?;
    if crate::codec_raster::digest_hex(&bytes) != RECIPE {
        return Err("audit oracle recipe drift; refusing execution".into());
    }
    Ok(())
}

pub fn image_source_identity(source: &Source, media: &Source) -> Result<(), String> {
    for (source, path, hash) in [
        (source, IDENTITIES[0].0, IDENTITIES[0].1),
        (
            media,
            "emojikit/media.py",
            "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53",
        ),
    ] {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("audit image oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

pub fn image_fixture_identity(fixture: &Value) -> Result<(), String> {
    let mut recipe = fixture.clone();
    let cases = recipe
        .get_mut("cases")
        .and_then(Value::as_array_mut)
        .filter(|cases| cases.len() == 7)
        .ok_or("audit image oracle requires7 original observations")?;
    for case in cases {
        let case = case
            .as_object_mut()
            .ok_or("invalid original audit image case")?;
        if !case
            .remove("expected")
            .is_some_and(|value| value.is_object())
        {
            return Err("audit image oracle requires expected objects".into());
        }
    }
    let bytes = serde_json::to_vec(&recipe).map_err(|_| "cannot encode audit image recipe")?;
    if crate::codec_raster::digest_hex(&bytes)
        != "34d71d3aefed0de9113778d35da964cf27b41b0c75f6d7da82bcec1fbdf2e3a2"
    {
        return Err("audit image oracle recipe drift; refusing execution".into());
    }
    Ok(())
}

pub fn check_images(
    root: &std::path::Path,
    source: &Source,
    media: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    image_source_identity(source, media)?;
    image_fixture_identity(fixture)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("missing audit image cases")?;
    let inputs = cases
        .iter()
        .map(|case| crate::source_static_oracle::input(&root, case))
        .collect::<Result<Vec<_>, _>>()?;
    Python::initialize();
    Python::attach(|py| -> Result<(), String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let result = (|| -> PyResult<()> {
            let image = py.import("PIL.Image")?;
            if image.getattr("__version__")?.extract::<String>()? != "12.3.0" {
                return Err(PyValueError::new_err(
                    "audit image oracle requires Pillow12.3.0",
                ));
            }
            let bindings = PyDict::new(py);
            bindings.set_item("Image", image)?;
            bindings.set_item("io", py.import("io")?)?;
            bindings.set_item("hashlib", py.import("hashlib")?)?;
            for name in ["VISIBLE_ALPHA", "BLANK_MAX_VISIBLE"] {
                bindings.set_item(
                    name,
                    crate::source_logging_oracle::literal(py, media, name, false)?,
                )?;
            }
            let original_media = crate::source_oracle_runtime::namespace(
                py,
                media,
                &["is_blank_image"],
                &["sum"],
                &bindings,
            )?;
            bindings.set_item(
                "is_blank_image",
                required(&original_media, "is_blank_image")?,
            )?;
            let original =
                crate::source_oracle_runtime::namespace(py, source, &["analyze"], &[], &bindings)?;
            let analyze = required(&original, "analyze")?;
            for (index, (case, input)) in cases.iter().zip(&inputs).enumerate() {
                let observed = match analyze.call1((pyo3::types::PyBytes::new(py, &input.bytes),)) {
                    Ok(value) => {
                        let (hash, blank): (String, bool) = value.extract()?;
                        json!({"hash":hash,"blank":blank})
                    }
                    Err(error) if error.is_instance_of::<pyo3::exceptions::PyException>(py) => {
                        json!({"error_type":error.get_type(py).name()?.to_string()})
                    }
                    Err(error) => return Err(error),
                };
                if observed != case["expected"] {
                    return Err(PyValueError::new_err(format!(
                        "original audit image drift: case{index}"
                    )));
                }
            }
            Ok(())
        })();
        result.map_err(|error| format!("original audit image observation failed: {error}"))?;
        Ok(())
    })?;
    for input in inputs {
        if let Some(path) = input.path
            && std::fs::read(path).map_err(|_| "cannot verify audit image preservation")?
                != input.bytes
        {
            return Err("original audit image observation changed its input".into());
        }
    }
    Ok(
        json!({"cases_checked":cases.len(),"source_revision":BASELINE,
        "source_hash":source.sha256,"media_hash":media.sha256,
        "source_runtime":"CPython3.11.9","pillow":"12.3.0","fixture_drift":false,
        "ports":"original analyze/shared blank rule and immutable inputs; not original unittest or native execution proof"}),
    )
}

fn required<'py>(values: &Bound<'py, PyDict>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    values
        .get_item(name)?
        .ok_or_else(|| PyValueError::new_err(format!("missing original audit port {name}")))
}

#[pyclass]
struct Io {
    input: Py<PyDict>,
    cache: Py<PyDict>,
    calls: Py<PyList>,
    downloads: Py<PyList>,
    writes: Py<PyList>,
    waits: Py<PyList>,
    reports: Py<PyList>,
}
#[pymethods]
impl Io {
    fn load(&self, py: Python<'_>) -> Py<PyDict> {
        self.cache.clone_ref(py)
    }
    fn save(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let json = crate::source_oracle_runtime::host_import(py, "json")?;
        let snapshot = json.call_method1("loads", (json.call_method1("dumps", (value,))?,))?;
        self.writes.bind(py).append(snapshot)
    }
    fn read_text(&self, py: Python<'_>, encoding: &str) -> PyResult<String> {
        if encoding != "utf-8" {
            return Err(PyValueError::new_err("unexpected audit state encoding"));
        }
        let sets = required(self.input.bind(py), "sets")?.cast_into::<PyDict>()?;
        let rows = PyList::empty(py);
        for (index, (name, _)) in sets.iter().enumerate() {
            let row = PyDict::new(py);
            row.set_item("name", name)?;
            row.set_item("index", index + 1)?;
            rows.append(row)?;
        }
        let state = PyDict::new(py);
        state.set_item("sets", rows)?;
        crate::source_oracle_runtime::host_import(py, "json")?
            .call_method1("dumps", (state,))?
            .extract()
    }
    fn write_text(&self, py: Python<'_>, text: &str, encoding: &str) -> PyResult<()> {
        if encoding != "utf-8" {
            return Err(PyValueError::new_err("unexpected audit report encoding"));
        }
        self.reports.bind(py).append(text)
    }
    fn get_sticker_set(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyDict>> {
        let calls = self.calls.bind(py);
        calls.append(name)?;
        let count = calls
            .iter()
            .map(|value| value.extract::<String>())
            .collect::<PyResult<Vec<_>>>()?
            .iter()
            .filter(|called| called.as_str() == name)
            .count();
        if let Some(failures) = self.input.bind(py).get_item("failures")?
            && let Some(limit) = failures.cast_into::<PyDict>()?.get_item(name)?
            && count <= limit.extract::<usize>()?
        {
            return Err(PyRuntimeError::new_err("listing refused"));
        }
        let ids = required(self.input.bind(py), "sets")?.get_item(name)?;
        let stickers = PyList::empty(py);
        for id in ids.try_iter()? {
            let id = id?;
            let sticker = PyDict::new(py);
            sticker.set_item("custom_emoji_id", &id)?;
            sticker.set_item("file_id", id)?;
            stickers.append(sticker)?;
        }
        let result = PyDict::new(py);
        result.set_item("stickers", stickers)?;
        Ok(result.unbind())
    }
    fn download_bytes(&self, py: Python<'_>, cid: &str) -> PyResult<String> {
        self.downloads.bind(py).append(cid)?;
        Ok(cid.into())
    }
    fn analyze(&self, py: Python<'_>, cid: &str) -> PyResult<(String, bool)> {
        let value = required(self.input.bind(py), "analysis")?.get_item(cid)?;
        if let Ok(error) = value.extract::<String>() {
            return Err(PyValueError::new_err(error));
        }
        Ok((value.get_item(0)?.extract()?, value.get_item(1)?.extract()?))
    }
    fn safe(&self, error: &Bound<'_, PyAny>) -> PyResult<String> {
        error.str()?.extract()
    }
    fn sleep(&self, py: Python<'_>, seconds: &Bound<'_, PyAny>) -> PyResult<()> {
        self.waits.bind(py).append(seconds)
    }
    fn telegram<'py>(slf: PyRef<'py, Self>, token: &str) -> PyResult<PyRef<'py, Self>> {
        if token != "synthetic" {
            return Err(PyValueError::new_err("unexpected audit fixture token"));
        }
        Ok(slf)
    }
    fn noop(&self) {}
    fn setup(&self, name: &str) -> PyResult<()> {
        if name != "check_all_packs" {
            return Err(PyValueError::new_err("unexpected original audit log name"));
        }
        Ok(())
    }
}

fn observe(py: Python<'_>, source: &Source, environment: &Source, case: &Value) -> PyResult<Value> {
    let json_module = py.import("json")?;
    // Only inputs reach callbacks; expected observations remain outside the source namespace.
    let input = json_module
        .call_method1("loads", (case["input"].to_string(),))?
        .cast_into::<PyDict>()?;
    let unchanged: String = json_module.call_method1("dumps", (&input,))?.extract()?;
    let cache = json_module
        .call_method1(
            "loads",
            (json_module.call_method1("dumps", (required(&input, "cache")?,))?,),
        )?
        .cast_into::<PyDict>()?;
    let ports = Py::new(
        py,
        Io {
            input: input.clone().unbind(),
            cache: cache.unbind(),
            calls: PyList::empty(py).unbind(),
            downloads: PyList::empty(py).unbind(),
            writes: PyList::empty(py).unbind(),
            waits: PyList::empty(py).unbind(),
            reports: PyList::empty(py).unbind(),
        },
    )?;
    let port = ports.bind(py);
    let bindings = PyDict::new(py);
    for (name, method) in [
        ("load_env", "noop"),
        ("setup_logging", "setup"),
        ("Telegram", "telegram"),
        ("load_audit", "load"),
        ("save_audit", "save"),
        ("analyze", "analyze"),
    ] {
        bindings.set_item(name, port.getattr(method)?)?;
    }
    for name in ["STATE", "REPORT", "time"] {
        bindings.set_item(name, port)?;
    }
    bindings.set_item("json", &json_module)?;
    bindings.set_item(
        "defaultdict",
        py.import("collections")?.getattr("defaultdict")?,
    )?;
    let fake_os = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    let values = PyDict::new(py);
    values.set_item("TELEGRAM_BOT_TOKEN", "synthetic")?;
    fake_os.setattr("environ", values)?;
    bindings.set_item("os", fake_os)?;
    let codes = PyDict::new(py);
    for (name, value) in [
        ("EXIT_OK", 0),
        ("EXIT_USAGE", 2),
        ("EXIT_PARTIAL", 3),
        ("EXIT_FAILED", 4),
    ] {
        codes.set_item(name, value)?;
        bindings.set_item(name, value)?;
    }
    let exit = crate::source_oracle_runtime::namespace(
        py,
        environment,
        &["ingest_exit_code"],
        &[],
        &codes,
    )?;
    bindings.set_item("ingest_exit_code", required(&exit, "ingest_exit_code")?)?;
    let capture = py.import("io")?.getattr("StringIO")?.call0()?;
    let result = (|| -> PyResult<Value> {
        let options = PyDict::new(py);
        options.set_item("file", &capture)?;
        bindings.set_item(
            "print",
            py.import("functools")?
                .getattr("partial")?
                .call((py.import("builtins")?.getattr("print")?,), Some(&options))?,
        )?;
        let globals = crate::source_oracle_runtime::namespace(
            py,
            source,
            &["main"],
            &[
                "sorted",
                "range",
                "Exception",
                "enumerate",
                "str",
                "len",
                "list",
                "sum",
            ],
            &bindings,
        )?;
        let code: u8 = required(&globals, "main")?.call0()?.extract()?;
        if json_module
            .call_method1("dumps", (&input,))?
            .extract::<String>()?
            != unchanged
        {
            return Err(PyValueError::new_err(
                "original audit mutated fixture inputs",
            ));
        }
        let ports = port.borrow();
        let observations = PyDict::new(py);
        observations.set_item("code", code)?;
        for (name, value) in [
            ("calls", &ports.calls),
            ("downloads", &ports.downloads),
            ("writes", &ports.writes),
            ("waits", &ports.waits),
            ("reports", &ports.reports),
        ] {
            observations.set_item(name, value.bind(py))?;
        }
        let encoded: String = json_module
            .call_method1("dumps", (observations,))?
            .extract()?;
        serde_json::from_str(&encoded)
            .map_err(|_| PyValueError::new_err("invalid original audit observations"))
    })();
    let closed = capture.call_method0("close");
    match (result, closed) {
        (Err(error), _) | (Ok(_), Err(error)) => Err(error),
        (Ok(value), Ok(_)) => Ok(value),
    }
}

pub fn check(source: &Source, environment: &Source, fixture: &Value) -> Result<Value, String> {
    source_identity(source, environment)?;
    fixture_identity(fixture)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let cases = fixture["cases"].as_array().ok_or("missing audit cases")?;
        for (index, case) in cases.iter().enumerate() {
            let observed = observe(py, source, environment, case)
                .map_err(|error| format!("original audit observation {index} failed: {error}"))?;
            if observed != case["expected"] {
                return Err(format!("original audit fixture drift: case{index}"));
            }
        }
        Ok(
            json!({"cases_checked":cases.len(),"source_revision":BASELINE,
            "source_hash":source.sha256,"environment_hash":environment.sha256,
            "source_runtime":"CPython3.11.9","fixture_drift":false,
            "ports":"virtual listing/download/analysis/clock/cache/report and private stdout; no original unittest, pixel, durable-file, redaction or interruption equivalence"}),
        )
    })
}
