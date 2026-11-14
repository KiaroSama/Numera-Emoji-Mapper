//! Original converter watchdog CLI; native ownership and deadlines stay in tooling_convert.
use crate::logging::RunLog;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq)]
pub enum Arguments {
    Convert {
        source: PathBuf,
        output: PathBuf,
        stdout: PathBuf,
        stderr: PathBuf,
    },
    Exit(i32),
}

/// External argparse retains required paths, unambiguous abbreviations and its help/error exits.
pub fn arguments(args: &[String]) -> Result<Arguments, String> {
    Python::initialize();
    Python::attach(|py| -> Result<Arguments, String> {
        let parsed = (|| -> PyResult<Arguments> {
            let options = PyDict::new(py);
            options.set_item("prog", "numera-emoji native-convert-owner")?;
            options.set_item(
                "description",
                "Own one coin-watchdog native conversion attempt, including closed-stdio descendants.",
            )?;
            let parser = py
                .import("argparse")?
                .getattr("ArgumentParser")?
                .call((), Some(&options))?;
            let path = py.import("pathlib")?.getattr("Path")?;
            for (name, destination) in [
                ("--in", "source"),
                ("--out", "output"),
                ("--stdout", "stdout"),
                ("--stderr", "stderr"),
            ] {
                let option = PyDict::new(py);
                option.set_item("type", &path)?;
                option.set_item("dest", destination)?;
                option.set_item("required", true)?;
                parser.call_method("add_argument", (name,), Some(&option))?;
            }
            match parser.call_method1("parse_args", (PyList::new(py, args)?,)) {
                Ok(parsed) => Ok(Arguments::Convert {
                    source: parsed.getattr("source")?.extract()?,
                    output: parsed.getattr("output")?.extract()?,
                    stdout: parsed.getattr("stdout")?.extract()?,
                    stderr: parsed.getattr("stderr")?.extract()?,
                }),
                Err(error) if error.is_instance_of::<pyo3::exceptions::PySystemExit>(py) => {
                    Ok(Arguments::Exit(error.value(py).getattr("code")?.extract()?))
                }
                Err(error) => Err(error),
            }
        })();
        // Native exit does not finalize embedded CPython; argparse's buffered help must be flushed.
        let sys = py.import("sys").map_err(|error| error.to_string())?;
        for stream in ["stdout", "stderr"] {
            sys.getattr(stream)
                .and_then(|stream| stream.call_method0("flush"))
                .map_err(|error| error.to_string())?;
        }
        parsed.map_err(|error| error.to_string())
    })
}

/// Config-independent contributor entry point. No builds, prerequisite installs or dotenv loading.
/// The caller owns the run log's final status and the platform's process-exit-width conversion.
pub fn run(root: &Path, args: &[String], log: &RunLog) -> i32 {
    let result = (|| -> Result<i32, String> {
        match arguments(args)? {
            Arguments::Exit(code) => Ok(code),
            Arguments::Convert {
                source,
                output,
                stdout,
                stderr,
            } => {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| "Cannot initialize converter process monitor")?;
                runtime.block_on(crate::tooling_convert::run(
                    root, &source, &output, &stdout, &stderr, log,
                ))
            }
        }
    })();
    match result {
        Ok(code) => code,
        Err(error) => {
            log.event(
                "ERROR",
                "native_convert_owner",
                &format!("Native converter ownership failed: {error}"),
            );
            1
        }
    }
}
