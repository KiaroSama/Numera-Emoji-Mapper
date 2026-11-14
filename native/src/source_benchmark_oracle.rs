//! Fixed original benchmark policy observations; supplied clock/target/database ports are not timings.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const PATH: &str = "scripts/benchmark_similarity.py";
const HASH: &str = "9240f9b4e83c72db4a20910b8618564e198fddd8f8033f92ca4d6b4fcd6a65ab";
const QUERY: &str =
    "SELECT phash FROM items WHERE phash IS NOT NULL ORDER BY position, content_key";

pub fn source_identity(source: &Source, reference: &Source) -> Result<(), String> {
    let actual = Sha256::digest(source.text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if source.baseline_revision != BASELINE
        || source.path != PATH
        || source.sha256 != HASH
        || actual != HASH
    {
        return Err("benchmark oracle source identity drift; refusing execution".into());
    }
    crate::source_similarity_oracle::source_identity(reference)
}

fn event(py: Python<'_>, events: &Py<PyList>, value: Value) -> PyResult<()> {
    let text = serde_json::to_string(&value).map_err(|_| {
        pyo3::exceptions::PyAssertionError::new_err("cannot encode benchmark event")
    })?;
    events
        .bind(py)
        .append(py.import("json")?.call_method1("loads", (text,))?)
}
fn encode(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Value> {
    let text: String = py
        .import("json")?
        .call_method1("dumps", (value,))?
        .extract()?;
    serde_json::from_str(&text)
        .map_err(|_| pyo3::exceptions::PyAssertionError::new_err("invalid benchmark observation"))
}
fn outcome(
    py: Python<'_>,
    result: PyResult<Bound<'_, PyAny>>,
    exception: Option<&str>,
) -> PyResult<Value> {
    match result {
        Ok(value) if exception.is_none() => Ok(json!({"returned":encode(py, &value)?})),
        Ok(_) => Err(pyo3::exceptions::PyAssertionError::new_err(
            "benchmark supplied error was not reached",
        )),
        Err(error) => {
            let name: String = error.get_type(py).getattr("__name__")?.extract()?;
            // Namespace/builtin/callback mistakes are harness failures, never expected observations.
            if exception != Some(name.as_str()) {
                return Err(error);
            }
            Ok(
                json!({"raised":{"type":name,"message":error.value(py).str()?.extract::<String>()?}}),
            )
        }
    }
}
fn required<'py>(globals: &Bound<'py, PyDict>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    globals.get_item(name)?.ok_or_else(|| {
        pyo3::exceptions::PyAssertionError::new_err(format!("missing benchmark definition {name}"))
    })
}

#[pyclass]
struct Clock {
    ticks: Vec<f64>,
    next: Mutex<usize>,
    events: Py<PyList>,
}
#[pymethods]
impl Clock {
    fn __call__(&self, py: Python<'_>) -> PyResult<f64> {
        let mut index = self.next.lock().map_err(|_| {
            pyo3::exceptions::PyAssertionError::new_err("benchmark clock recording failed")
        })?;
        let value = *self.ticks.get(*index).ok_or_else(|| {
            pyo3::exceptions::PyAssertionError::new_err("benchmark exhausted supplied clock")
        })?;
        *index += 1;
        event(py, &self.events, json!(["clock", value]))?;
        Ok(value)
    }
}
#[pyclass]
struct Greedy {
    reference: Py<PyAny>,
    role: &'static str,
    mismatch: bool,
    events: Py<PyList>,
}
#[pymethods]
impl Greedy {
    fn __call__(&self, py: Python<'_>, hashes: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let result = self.reference.bind(py).call1((hashes,))?;
        if self.mismatch {
            result.call_method1("append", (-1,))?;
        }
        self.events.bind(py).append((self.role, hashes, &result))?;
        Ok(result.unbind())
    }
}
#[pyclass]
struct Near {
    reference: Py<PyAny>,
    role: &'static str,
    mismatch: bool,
    events: Py<PyList>,
}
#[pymethods]
impl Near {
    fn __call__(
        &self,
        py: Python<'_>,
        hashes: &Bound<'_, PyAny>,
        query: u64,
        threshold: i32,
    ) -> PyResult<Py<PyAny>> {
        let result = self.reference.bind(py).call1((hashes, query, threshold))?;
        if self.mismatch {
            result.call_method1("append", (-1,))?;
        }
        self.events
            .bind(py)
            .append((self.role, query, threshold, &result))?;
        Ok(result.unbind())
    }
}

fn compare_case(
    py: Python<'_>,
    source: &Source,
    reference: &Bound<'_, PyDict>,
    mismatch: bool,
) -> PyResult<Value> {
    let events = PyList::empty(py).unbind();
    // These durations are harness inputs, never measured performance or expected medians.
    let source_samples = [9.0, 1.0, 2.0, 8.0, 3.0];
    let target_samples = [5.0, 4.0, 9.0, 1.0, 6.0];
    let mut ticks = Vec::new();
    let mut start = 0.0;
    for (old, new) in source_samples.into_iter().zip(target_samples) {
        for elapsed in [old, new] {
            ticks.extend([start, start + elapsed]);
            start += elapsed + 1.0;
        }
    }
    let bindings = PyDict::new(py);
    let clock = Py::new(
        py,
        Clock {
            ticks: ticks.clone(),
            next: Mutex::new(0),
            events: events.clone_ref(py),
        },
    )?;
    let time = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    time.setattr("perf_counter", clock)?;
    bindings.set_item("time", time)?;
    bindings.set_item("statistics", py.import("statistics")?)?;
    for (name, role, mutate) in [
        ("reference", "source", false),
        ("greedy_indices", "target-port", mismatch),
    ] {
        bindings.set_item(
            name,
            Py::new(
                py,
                Greedy {
                    reference: required(reference, "greedy_indices")?.unbind(),
                    role,
                    mismatch: mutate,
                    events: events.clone_ref(py),
                },
            )?,
        )?;
    }
    let globals = crate::source_oracle_runtime::namespace(
        py,
        source,
        &["compare"],
        &["range", "zip", "RuntimeError"],
        &bindings,
    )?;
    let hashes = PyList::new(py, [7_u64, 7, 3, 5, 7, 0, u64::MAX])?;
    let observed = outcome(
        py,
        required(&globals, "compare")?.call1((&hashes,)),
        if mismatch { Some("RuntimeError") } else { None },
    )?;
    Ok(
        json!({"name":if mismatch {"greedy-mismatch"} else {"paired-medians"},
        "input_hashes":encode(py, hashes.as_any())?,"clock_ticks":ticks,
        "observation":observed,"events":encode(py, events.bind(py).as_any())?}),
    )
}

#[derive(Clone, Copy)]
enum Fault {
    None,
    Near,
    Os,
    Sqlite,
    Runtime,
    Value,
    Type,
    Overflow,
    Interrupt,
}
#[pyclass]
struct Database {
    rows: Py<PyList>,
    events: Py<PyList>,
}
#[pymethods]
impl Database {
    fn __enter__(slf: PyRef<'_, Self>) -> PyResult<PyRef<'_, Self>> {
        event(slf.py(), &slf.events, json!(["database-enter"]))?;
        Ok(slf)
    }
    fn __exit__(
        &self,
        py: Python<'_>,
        _kind: &Bound<'_, PyAny>,
        _error: &Bound<'_, PyAny>,
        _trace: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        event(py, &self.events, json!(["database-exit"]))
    }
    fn execute(&self, py: Python<'_>, sql: &str) -> PyResult<Py<PyList>> {
        if sql != QUERY {
            return Err(pyo3::exceptions::PyAssertionError::new_err(
                "benchmark query recipe drift",
            ));
        }
        event(py, &self.events, json!(["query", sql]))?;
        Ok(self.rows.clone_ref(py))
    }
}
#[pyclass]
struct Ports {
    database: Py<Database>,
    durations: (f64, f64),
    fault: Fault,
    events: Py<PyList>,
}
#[pymethods]
impl Ports {
    fn setup_logging(&self, py: Python<'_>, name: &str) -> PyResult<()> {
        event(py, &self.events, json!(["setup-logging", name]))
    }
    #[pyo3(signature = (database, *, uri=false))]
    fn connect(&self, py: Python<'_>, database: &str, uri: bool) -> PyResult<Py<Database>> {
        if !uri || !database.starts_with("file:") || !database.ends_with("?mode=ro") {
            return Err(pyo3::exceptions::PyAssertionError::new_err(
                "benchmark read-only URI drift",
            ));
        }
        event(
            py,
            &self.events,
            json!(["connect",{"uri":true,"mode":"ro"}]),
        )?;
        match self.fault {
            Fault::Os => Err(pyo3::exceptions::PyOSError::new_err(
                "controlled open failure",
            )),
            Fault::Sqlite => Err(PyErr::from_value(
                crate::source_oracle_runtime::host_import(py, "sqlite3")?
                    .getattr("OperationalError")?
                    .call1(("controlled sqlite failure",))?,
            )),
            _ => Ok(self.database.clone_ref(py)),
        }
    }
    fn compare(&self, py: Python<'_>, hashes: &Bound<'_, PyList>) -> PyResult<(f64, f64)> {
        let values: Vec<u64> = hashes.extract()?;
        let bytes = serde_json::to_vec(&values).map_err(|_| {
            pyo3::exceptions::PyAssertionError::new_err("cannot record benchmark inputs")
        })?;
        let digest = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        event(
            py,
            &self.events,
            json!(["compare-port",{"hashes":values.len(),"sha256":digest,"head":values.iter().take(4).collect::<Vec<_>>()}]),
        )?;
        match self.fault {
            Fault::Runtime => Err(pyo3::exceptions::PyRuntimeError::new_err(
                "controlled compare failure",
            )),
            Fault::Interrupt => Err(pyo3::exceptions::PyKeyboardInterrupt::new_err(
                "controlled interrupt",
            )),
            _ => Ok(self.durations),
        }
    }
    #[pyo3(signature = (message, *args))]
    fn info(&self, py: Python<'_>, message: &str, args: &Bound<'_, PyTuple>) -> PyResult<()> {
        self.log(py, "INFO", 20, message, args)
    }
    #[pyo3(signature = (message, *args))]
    fn error(&self, py: Python<'_>, message: &str, args: &Bound<'_, PyTuple>) -> PyResult<()> {
        self.log(py, "ERROR", 40, message, args)
    }
}
impl Ports {
    fn log(
        &self,
        py: Python<'_>,
        level: &str,
        number: i32,
        message: &str,
        args: &Bound<'_, PyTuple>,
    ) -> PyResult<()> {
        let text: String = py
            .import("logging")?
            .getattr("LogRecord")?
            .call1((
                "benchmark_similarity",
                number,
                "",
                0,
                message,
                args,
                py.None(),
            ))?
            .call_method0("getMessage")?
            .extract()?;
        event(py, &self.events, json!(["log", level, text]))
    }
}

fn main_case(
    py: Python<'_>,
    source: &Source,
    reference: &Bound<'_, PyDict>,
    recipe: (&str, usize, (f64, f64), Fault, bool),
) -> PyResult<Value> {
    let (name, count, durations, fault, random) = recipe;
    let events = PyList::empty(py).unbind();
    let rows = PyList::new(py, (0..count).map(|value| (value as u64,)))?;
    match fault {
        Fault::Value => rows.set_item(0, ("not an integer",))?,
        Fault::Type => rows.set_item(0, (py.None(),))?,
        Fault::Overflow => rows.set_item(0, (f64::INFINITY,))?,
        _ => {}
    }
    let ports = Py::new(
        py,
        Ports {
            database: Py::new(
                py,
                Database {
                    rows: rows.unbind(),
                    events: events.clone_ref(py),
                },
            )?,
            durations,
            fault,
            events: events.clone_ref(py),
        },
    )?;
    let bindings = PyDict::new(py);
    let library = py.import("types")?.getattr("SimpleNamespace")?;
    let sqlite = library.call0()?;
    sqlite.setattr("Error", py.import("sqlite3")?.getattr("Error")?)?;
    sqlite.setattr("connect", ports.bind(py).getattr("connect")?)?;
    bindings.set_item("sqlite3", sqlite)?;
    bindings.set_item("argparse", py.import("argparse")?)?;
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item("random", py.import("random")?)?;
    bindings.set_item("compare", ports.bind(py).getattr("compare")?)?;
    bindings.set_item("setup_logging", ports.bind(py).getattr("setup_logging")?)?;
    bindings.set_item("log", &ports)?;
    bindings.set_item(
        "__doc__",
        "Bounded native/source parity and repeated compute timings; optional read-only replay.",
    )?;
    for (binding, role, mismatch) in [
        (
            "near_indices",
            "target-near-port",
            matches!(fault, Fault::Near),
        ),
        ("reference_near", "source-near", false),
    ] {
        bindings.set_item(
            binding,
            Py::new(
                py,
                Near {
                    reference: required(reference, "near_indices")?.unbind(),
                    role,
                    mismatch,
                    events: events.clone_ref(py),
                },
            )?,
        )?;
    }
    let globals = crate::source_oracle_runtime::namespace(
        py,
        source,
        &["main"],
        &[
            "int",
            "range",
            "len",
            "max",
            "RuntimeError",
            "ValueError",
            "OSError",
        ],
        &bindings,
    )?;
    let argv = if random {
        vec![]
    } else {
        vec!["--catalog", "benchmark-policy-catalog.db"]
    };
    let exception = match fault {
        Fault::Type => Some("TypeError"),
        Fault::Overflow => Some("OverflowError"),
        Fault::Interrupt => Some("KeyboardInterrupt"),
        _ => None,
    };
    let observed = outcome(
        py,
        required(&globals, "main")?.call1((PyList::new(py, &argv)?,)),
        exception,
    )?;
    let events = encode(py, events.bind(py).as_any())?;
    // A callback signature failure must not masquerade as the supplied int-conversion failure.
    if matches!(fault, Fault::Value | Fault::Type | Fault::Overflow)
        && !events.as_array().is_some_and(|events| {
            events.iter().any(|entry| entry[0] == "query")
                && events.iter().any(|entry| entry[0] == "database-exit")
                && !events.iter().any(|entry| entry[0] == "compare-port")
        })
    {
        return Err(pyo3::exceptions::PyAssertionError::new_err(
            "benchmark supplied row-conversion fault was not reached",
        ));
    }
    Ok(
        json!({"name":name,"supplied_count":if random {Value::Null} else {json!(count)},
        "supplied_medians":[durations.0,durations.1],"observation":observed,"events":events}),
    )
}

pub fn generate(source: &Source, reference: &Source) -> Result<Value, String> {
    source_identity(source, reference)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let observed = (|| -> PyResult<Value> {
            let reference_hash = reference.sha256.clone();
            let reference = crate::source_oracle_runtime::namespace(
                py,
                reference,
                &["greedy_indices", "near_indices"],
                &["list", "range", "len", "enumerate"],
                &PyDict::new(py),
            )?;
            let comparisons = [false, true]
                .into_iter()
                .map(|mismatch| compare_case(py, source, &reference, mismatch))
                .collect::<PyResult<Vec<_>>>()?;
            let mut main = Vec::new();
            for (name, count, durations, fault, random) in [
                (
                    "below-regression-boundary",
                    99,
                    (1.0, 2.0),
                    Fault::None,
                    false,
                ),
                ("regression-boundary", 100, (1.0, 2.0), Fault::None, false),
                ("equal-medians", 100, (1.0, 1.0), Fault::None, false),
                ("zero-native-floor", 1, (1e-9, 0.0), Fault::None, false),
                ("empty", 0, (1.0, 1.0), Fault::None, false),
                ("maximum-input", 10000, (1.0, 1.0), Fault::Runtime, false),
                ("oversized", 10001, (1.0, 1.0), Fault::None, false),
                ("near-mismatch", 3, (1.0, 1.0), Fault::Near, false),
                ("caught-os", 1, (1.0, 1.0), Fault::Os, false),
                ("caught-sqlite", 1, (1.0, 1.0), Fault::Sqlite, false),
                ("caught-runtime", 1, (1.0, 1.0), Fault::Runtime, false),
                ("caught-int-value", 1, (1.0, 1.0), Fault::Value, false),
                ("uncaught-int-type", 1, (1.0, 1.0), Fault::Type, false),
                (
                    "uncaught-int-overflow",
                    1,
                    (1.0, 1.0),
                    Fault::Overflow,
                    false,
                ),
                ("uncaught-interrupt", 1, (1.0, 1.0), Fault::Interrupt, false),
                ("default-random-input", 0, (2.0, 1.0), Fault::None, true),
            ] {
                main.push(main_case(
                    py,
                    source,
                    &reference,
                    (name, count, durations, fault, random),
                )?);
            }
            Ok(
                json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9","source_hash":HASH,
                "reference_hash":reference_hash,
                "compare":comparisons,"main":main,
                "coverage":{"policy":"original compare/main; source reference owns lists",
                "clock":"supplied ticks; NOT wall-clock performance",
                "target":"supplied historical reference callback; NOT native parity",
                "database":"supplied read-only URI/rows context; NOT real SQLite acceptance",
                "main_compare":"supplied median/error port; NOT repeated compute timing"}}),
            )
        })();
        observed.map_err(|error| format!("benchmark oracle source failed: {error}"))
    })
}
