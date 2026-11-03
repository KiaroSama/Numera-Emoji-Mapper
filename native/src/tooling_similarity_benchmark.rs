//! Contributor benchmark worker body; callers must use the existing owned, bounded process route.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::path::Path;

const SAMPLES: usize = 5;
const QUERY: &str =
    "SELECT phash FROM items WHERE phash IS NOT NULL ORDER BY position, content_key";

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Report {
    pub hashes: usize,
    pub python_median: f64,
    pub rust_median: f64,
}

impl Report {
    /// Recheck untrusted worker responses before logging a parity observation.
    pub fn validate(&self) -> Result<(), String> {
        summarize(
            self.hashes,
            [self.python_median; SAMPLES],
            [self.rust_median; SAMPLES],
        )?;
        Ok(())
    }

    /// The original logs exact parity before checking the size-sensitive regression policy.
    pub fn line(&self) -> String {
        format!(
            "Exact parity: hashes={} python_median={:.6}s rust_median={:.6}s speedup={:.2}x",
            self.hashes,
            self.python_median,
            self.rust_median,
            self.python_median / self.rust_median.max(1e-9)
        )
    }

    pub fn check_regression(&self) -> Result<(), String> {
        if self.hashes >= 100 && self.rust_median > self.python_median {
            return Err("Native median regressed against the same-input source baseline".into());
        }
        Ok(())
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Failure {
    Verification(String),
    Uncaught(String),
}
impl std::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Verification(message) | Self::Uncaught(message) => formatter.write_str(message),
        }
    }
}

fn failure(py: Python<'_>, error: PyErr) -> Failure {
    let caught = error.is_instance_of::<pyo3::exceptions::PyOSError>(py)
        || error.is_instance_of::<pyo3::exceptions::PyRuntimeError>(py)
        || error.is_instance_of::<pyo3::exceptions::PyValueError>(py)
        || py
            .import("sqlite3")
            .and_then(|module| module.getattr("Error"))
            .is_ok_and(|class| error.is_instance(py, &class));
    if caught {
        match error.value(py).str().and_then(|text| text.extract()) {
            Ok(message) => Failure::Verification(message),
            Err(_) => Failure::Uncaught(error.to_string()),
        }
    } else {
        Failure::Uncaught(error.to_string())
    }
}

#[derive(Debug, PartialEq)]
pub enum Arguments {
    Catalog(Option<std::path::PathBuf>),
    Exit(u8),
}

/// Keep argparse's original abbreviation, negative-filename, duplicate, help and error policy.
pub fn arguments(args: &[String]) -> Result<Arguments, String> {
    Python::initialize();
    Python::attach(|py| -> Result<Arguments, String> {
        let parsed = (|| -> PyResult<Arguments> {
            let options = PyDict::new(py);
            options.set_item("prog", "numera-emoji benchmark-similarity")?;
            options.set_item("description", "Bounded native/source parity and repeated compute timings; optional read-only replay.")?;
            let parser = py
                .import("argparse")?
                .getattr("ArgumentParser")?
                .call((), Some(&options))?;
            let option = PyDict::new(py);
            option.set_item("type", py.import("pathlib")?.getattr("Path")?)?;
            option.set_item("help", "Optional local catalog, opened read-only")?;
            parser.call_method("add_argument", ("--catalog",), Some(&option))?;
            match parser.call_method1("parse_args", (PyList::new(py, args)?,)) {
                Ok(parsed) => Ok(Arguments::Catalog(parsed.getattr("catalog")?.extract()?)),
                Err(error) if error.is_instance_of::<pyo3::exceptions::PySystemExit>(py) => {
                    let code: u8 = error.value(py).getattr("code")?.extract()?;
                    Ok(Arguments::Exit(code))
                }
                Err(error) => Err(error),
            }
        })();
        // Embedded Python is not finalized on native exit; argparse only writes, never flushes.
        let sys = py.import("sys").map_err(|error| error.to_string())?;
        for stream in ["stdout", "stderr"] {
            sys.getattr(stream)
                .and_then(|stream| stream.call_method0("flush"))
                .map_err(|error| error.to_string())?;
        }
        parsed.map_err(|error| error.to_string())
    })
}

/// The caller supplies its existing run log and performs the benchmark through the process owner.
pub fn finish(result: Result<Report, Failure>, log: &crate::logging::RunLog) -> u8 {
    let result = result.and_then(|report| {
        report.validate().map_err(Failure::Uncaught)?;
        log.event("INFO", "benchmark_similarity", &report.line());
        report.check_regression().map_err(Failure::Verification)
    });
    match result {
        Ok(()) => 0,
        Err(Failure::Verification(error)) => {
            log.event(
                "ERROR",
                "benchmark_similarity",
                &format!("Verification failed: {error}"),
            );
            1
        }
        Err(Failure::Uncaught(error)) => {
            log.event(
                "ERROR",
                "benchmark_similarity",
                &format!("Benchmark runtime failed: {error}"),
            );
            1
        }
    }
}

fn cardinality(count: usize) -> Result<(), String> {
    if !(1..=10000).contains(&count) {
        return Err("Benchmark requires 1..10000 hashes".into());
    }
    Ok(())
}

pub fn summarize(
    hashes: usize,
    mut python: [f64; SAMPLES],
    mut native: [f64; SAMPLES],
) -> Result<Report, String> {
    cardinality(hashes)?;
    if python
        .iter()
        .chain(&native)
        .any(|sample| !sample.is_finite() || *sample < 0.0)
    {
        return Err("Benchmark clock returned an invalid elapsed time".into());
    }
    python.sort_by(f64::total_cmp);
    native.sort_by(f64::total_cmp);
    Ok(Report {
        hashes,
        python_median: python[SAMPLES / 2],
        rust_median: native[SAMPLES / 2],
    })
}

struct Database<'py> {
    value: Bound<'py, PyAny>,
    closed: bool,
}
impl Database<'_> {
    fn close(&mut self) -> PyResult<()> {
        if !self.closed {
            self.value.call_method0("close")?;
            self.closed = true;
        }
        Ok(())
    }
}
impl Drop for Database<'_> {
    fn drop(&mut self) {
        if self.close().is_err() {
            eprintln!("ERROR: benchmark read-only SQLite connection could not be closed");
        }
    }
}

fn input(py: Python<'_>, catalog: Option<&Path>) -> PyResult<Vec<u64>> {
    if let Some(path) = catalog {
        // External pathlib owns URI quoting and Windows/UNC spelling, exactly as in the source.
        let uri: String = py
            .import("pathlib")?
            .getattr("Path")?
            .call1((path,))?
            .call_method0("resolve")?
            .call_method0("as_uri")?
            .extract()?;
        let options = PyDict::new(py);
        options.set_item("uri", true)?;
        let mut database = Database {
            value: py.import("sqlite3")?.call_method(
                "connect",
                (format!("{uri}?mode=ro"),),
                Some(&options),
            )?,
            closed: false,
        };
        let result = (|| {
            let integer = py.import("builtins")?.getattr("int")?;
            let mut hashes = Vec::new();
            for row in database
                .value
                .call_method1("execute", (QUERY,))?
                .try_iter()?
            {
                // int accepts the original text/real/blob values; u64 masking follows conversion.
                let hash: u64 = integer
                    .call1((row?.get_item(0)?,))?
                    .call_method1("__and__", (u64::MAX,))?
                    .extract()?;
                // Continue validating rows to preserve conversion failures, but cap retained memory.
                if hashes.len() <= 10000 {
                    hashes.push(hash);
                }
            }
            Ok(hashes)
        })();
        database.close()?;
        result
    } else {
        let rng = py.import("random")?.getattr("Random")?.call1((321,))?;
        (0..1500)
            .map(|_| rng.call_method1("getrandbits", (64,))?.extract())
            .collect()
    }
}

/// No catalog creation or application imports. The full worker must run under an outer wall/idle owner.
pub fn load_hashes(catalog: Option<&Path>) -> Result<Vec<u64>, Failure> {
    Python::initialize();
    let hashes = Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py).map_err(Failure::Uncaught)?;
        input(py, catalog).map_err(|error| failure(py, error))
    })?;
    cardinality(hashes.len()).map_err(Failure::Verification)?;
    Ok(hashes)
}

/// Executes only the already-admitted historical reference, never target-derived expectations.
/// Returns the parity report even for a slower median so the caller can log before refusing.
pub fn run(source: &Source, catalog: Option<&Path>) -> Result<Report, Failure> {
    crate::source_similarity_oracle::source_identity(source).map_err(Failure::Uncaught)?;
    let hashes = load_hashes(catalog)?;
    Python::attach(|py| -> Result<Report, Failure> {
        let observed = (|| -> PyResult<Report> {
            let globals = crate::source_oracle_runtime::namespace(
                py,
                source,
                &["greedy_indices", "near_indices"],
                &["list", "range", "len", "enumerate"],
                &PyDict::new(py),
            )?;
            let missing = || pyo3::exceptions::PyValueError::new_err("missing benchmark reference");
            let greedy = globals.get_item("greedy_indices")?.ok_or_else(missing)?;
            let near = globals.get_item("near_indices")?.ok_or_else(missing)?;
            let values = PyList::new(py, hashes.iter().copied())?;
            // Reuse the actual PyO3 entry points: Vec extraction, detach and list marshalling belong
            // inside the native measurement, just as in the source's extension call.
            let native_greedy = pyo3::wrap_pyfunction!(crate::greedy_indices, py)?;
            let native_near = pyo3::wrap_pyfunction!(crate::near_indices, py)?;
            let clock = py.import("time")?.getattr("perf_counter")?;
            let now = || clock.call0()?.extract::<f64>();
            let mut python = [0.0; SAMPLES];
            let mut native = [0.0; SAMPLES];
            for sample in 0..SAMPLES {
                let start = now()?;
                let expected = greedy.call1((&values,))?;
                python[sample] = now()? - start;
                let start = now()?;
                let actual = native_greedy.call1((&values,))?;
                native[sample] = now()? - start;
                if !expected.eq(&actual)? {
                    return Err(pyo3::exceptions::PyRuntimeError::new_err(
                        "Similarity order differs from the source oracle",
                    ));
                }
            }
            for &query in hashes.iter().take(20) {
                for threshold in [-1, 0, 16] {
                    let actual = native_near.call1((&values, query, threshold))?;
                    let expected = near.call1((&values, query, threshold))?;
                    if !actual.eq(&expected)? {
                        return Err(pyo3::exceptions::PyRuntimeError::new_err(
                            "Candidate shortlist differs from the source oracle",
                        ));
                    }
                }
            }
            summarize(hashes.len(), python, native).map_err(pyo3::exceptions::PyValueError::new_err)
        })();
        observed.map_err(|error| failure(py, error))
    })
}
