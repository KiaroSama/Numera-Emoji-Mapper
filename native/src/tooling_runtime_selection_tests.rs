//! Explicit selection is a read-only admission boundary, not evidence that an interpreter executed.
use crate::tooling_runtime::selected;
use pyo3::prelude::*;
use std::collections::BTreeMap;
use std::path::Path;

mod tooling_fixture {
    use crate as _native;
    include!("../tests/support/tooling_fixture.rs");
}

#[test]
fn explicit_paths_and_borrowed_path_select_the_same_venv_without_default_or_side_effects() {
    let fixture = tooling_fixture::Root::new("check interpreter selection café");
    let root = &fixture.0;
    let prefix = root.join("selected venv café");
    let scripts = prefix.join(if cfg!(windows) { "Scripts" } else { "bin" });
    std::fs::create_dir_all(&scripts).unwrap();
    let executable = scripts.join(if cfg!(windows) {
        "python.exe"
    } else {
        "python"
    });
    let original = b"selection-only fixture; never executed";
    std::fs::write(&executable, original).unwrap();
    let config = format!(
        "home = {}\nversion = 3.11.9\ninclude-system-site-packages = true\n",
        root.display()
    );
    let configuration = prefix.join("pyvenv.cfg");
    std::fs::write(&configuration, &config).unwrap();
    let environment = BTreeMap::from([("PATH".into(), scripts.to_str().unwrap().into())]);
    let before = environment.clone();
    let mixed = BTreeMap::from([("Path".into(), scripts.to_str().unwrap().into())]);
    let by_name = selected(root, Some(Path::new("python")), &mixed);
    if cfg!(windows) {
        assert_eq!(by_name.unwrap().executable, executable);
    } else {
        assert!(by_name.is_err(), "POSIX PATH names remain case-sensitive");
    }
    let relative = executable.strip_prefix(root).unwrap();
    for requested in [executable.as_path(), relative, Path::new("python")] {
        let selected = selected(root, Some(requested), &environment).unwrap();
        assert_eq!(selected.executable, executable);
        assert_eq!(selected.prefix, prefix);
        assert_eq!(selected.version, [3, 11, 9]);
        assert!(selected.shared_sites);
    }
    #[cfg(windows)]
    assert_eq!(
        selected(root, Some(&prefix.join("sCrIpTs/python.exe")), &environment)
            .unwrap()
            .prefix,
        prefix
    );
    assert!(!root.join(".venv").exists());
    assert_eq!(environment, before);
    assert_eq!(std::fs::read(&executable).unwrap(), original);
    assert_eq!(std::fs::read_to_string(&configuration).unwrap(), config);
    for requested in [
        Path::new(""),
        Path::new("--option"),
        Path::new("python\n"),
        Path::new("absent-python"),
    ] {
        assert!(selected(root, Some(requested), &environment).is_err());
    }
    for bad in [
        "home = nowhere\nversion = 3.11.9\ninclude-system-site-packages = true\n".to_owned(),
        config.replace("3.11.9", "3.12.0"),
        config.replace("true", "unknown"),
        format!("{config}version = 3.11.9\n"),
    ] {
        std::fs::write(&configuration, &bad).unwrap();
        assert!(selected(root, Some(&executable), &environment).is_err());
        assert_eq!(std::fs::read_to_string(&configuration).unwrap(), bad);
    }
    std::fs::write(&configuration, &config).unwrap();
    #[cfg(unix)]
    {
        let alias = root.join("linked-venv");
        std::os::unix::fs::symlink(&prefix, &alias).unwrap();
        assert!(selected(root, Some(&alias.join("bin/python")), &environment).is_err());
        std::fs::remove_file(&alias).unwrap();
        // Final interpreter links are normal POSIX venvs and must retain their lexical path.
        let base = root.join("base-python");
        std::fs::rename(&executable, &base).unwrap();
        std::os::unix::fs::symlink(&base, &executable).unwrap();
        assert_eq!(
            selected(root, Some(&executable), &environment)
                .unwrap()
                .executable,
            executable
        );
        std::fs::remove_file(&executable).unwrap();
        std::fs::rename(base, &executable).unwrap();
    }
    assert!(!root.join("logs").exists() && !root.join(".cache").exists());
    fixture.clean().unwrap();
}

#[test]
fn runtime_environment_preserves_machine_casing_without_import_override_aliases() {
    let original = BTreeMap::from([
        ("Path".into(), "selected runtime path".into()),
        ("sYsTeMrOoT".into(), "machine root".into()),
        ("pYtHoNhOmE".into(), "untrusted home".into()),
        ("BOT_TOKEN".into(), "synthetic-secret".into()),
    ]);
    let before = original.clone();
    let child = crate::tooling_runtime::environment(&original);
    if cfg!(windows) {
        assert_eq!(child["PATH"], "selected runtime path");
        assert_eq!(child["SystemRoot"], "machine root");
        assert_eq!(child.len(), 7);
    } else {
        assert_eq!(child.len(), 5, "POSIX casing is not normalized");
    }
    assert!(!child.contains_key("Path") && !child.contains_key("sYsTeMrOoT"));
    assert!(!child.contains_key("pYtHoNhOmE") && !child.contains_key("BOT_TOKEN"));
    assert_eq!(original, before);
}

#[pyclass]
struct FoundSpec {
    mode: u8,
    truth_calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

#[pymethods]
impl FoundSpec {
    fn __bool__(&self) -> pyo3::PyResult<bool> {
        self.truth_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.mode == 2 {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "controlled spec truth",
            ));
        }
        Ok(self.mode != 1)
    }
}

#[pyclass]
struct FindSpec {
    stop: usize,
    mode: u8,
    calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    truth_calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

#[pymethods]
impl FindSpec {
    fn __call__(
        &self,
        py: pyo3::Python<'_>,
        name: String,
    ) -> pyo3::PyResult<pyo3::Py<pyo3::PyAny>> {
        let mut calls = self.calls.lock().unwrap();
        calls.push(name);
        if calls.len() == self.stop {
            match self.mode {
                0 => return Ok(py.None()),
                3 => return Err(pyo3::exceptions::PyValueError::new_err("controlled finder")),
                4 => {
                    return Err(pyo3::exceptions::PyKeyboardInterrupt::new_err(
                        "controlled finder",
                    ));
                }
                5 => return Err(pyo3::exceptions::PySystemExit::new_err("controlled finder")),
                _ => {}
            }
        }
        pyo3::Py::new(
            py,
            FoundSpec {
                mode: if calls.len() == self.stop {
                    self.mode
                } else {
                    0
                },
                truth_calls: self.truth_calls.clone(),
            },
        )
        .map(pyo3::Py::into_any)
    }
}

#[test]
fn check_prerequisites_keep_original_order_truthiness_and_finder_errors_without_imports() {
    use pyo3::prelude::*;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    Python::initialize();
    Python::attach(|py| {
        // Each control is an isolated external module object; no shared import machinery is patched.
        let util = pyo3::types::PyModule::new(py, "controlled_importlib_util").unwrap();
        // Literals are the original scripts/check.ps1 prerequisite order, not an implementation-derived list.
        let names = ["numpy", "ruff", "emojikit._native", "playwright"];
        for (no_browser, stop, mode) in [
            (false, usize::MAX, 0),
            (true, usize::MAX, 0),
            (false, 1, 0),
            (false, 2, 0),
            (false, 3, 0),
            (false, 4, 0),
            (true, 4, 3),
            (false, 2, 1),
            (false, 2, 2),
            (false, 3, 3),
            (false, 3, 4),
            (false, 3, 5),
        ] {
            let calls = Arc::new(Mutex::new(Vec::new()));
            let truth_calls = Arc::new(AtomicUsize::new(0));
            let finder = Py::new(
                py,
                FindSpec {
                    stop,
                    mode,
                    calls: calls.clone(),
                    truth_calls: truth_calls.clone(),
                },
            )
            .unwrap();
            util.setattr("find_spec", finder).unwrap();
            let result = crate::tooling_check::prerequisites(util.as_any(), no_browser);
            let count = stop.min(if no_browser { 3 } else { 4 });
            assert_eq!(*calls.lock().unwrap(), names[..count]);
            let reached = stop <= count;
            assert_eq!(
                truth_calls.load(Ordering::SeqCst),
                if reached && matches!(mode, 0 | 3..=5) {
                    count - 1
                } else {
                    count
                }
            );
            if reached && mode >= 2 {
                let error = result.unwrap_err();
                let expected = match mode {
                    2 => "TypeError",
                    3 => "ValueError",
                    4 => "KeyboardInterrupt",
                    _ => "SystemExit",
                };
                assert_eq!(error.get_type(py).name().unwrap(), expected);
                assert_eq!(
                    error.value(py).str().unwrap().to_str().unwrap(),
                    if mode == 2 {
                        "controlled spec truth"
                    } else {
                        "controlled finder"
                    }
                );
            } else {
                assert_eq!(result.unwrap(), !reached || mode > 1);
            }
        }
    });
}

#[test]
fn independent_sysconfig_binds_the_selected_patch_not_only_minor_and_prefix() {
    use crate::tooling_runtime::{Selection, sysconfig_paths};
    let fixture = tooling_fixture::Root::new("independent selected patch");
    let prefix = &fixture.0;
    let selected = Selection {
        prefix: prefix.clone(),
        executable: prefix.join("python"),
        shared_sites: false,
        version: [3, 11, 9],
    };
    let source = format!(
        "Python version: \"3.11\"\nPaths: \n\tdata = \"{0}\"\n\tscripts = \"{0}\"\n\tplatlib = \"{0}\"\nVariables: \n\tpy_version = \"3.11.9\"\n",
        prefix.display()
    );
    assert_eq!(
        sysconfig_paths(&selected, &source).unwrap()["data"],
        *prefix
    );
    for bad in [
        source.replace("py_version = \"3.11.9\"", "py_version = \"3.11.10\""),
        source.replace("py_version = \"3.11.9\"", "py_version = \"3.12.0\""),
        source.replace("\tpy_version = \"3.11.9\"\n", ""),
        format!("{source}\tpy_version = \"3.11.9\"\n"),
        source
            .replace("Variables: ", "\tpy_version = \"3.11.9\"\nVariables: ")
            .replace("Variables: \n\tpy_version = \"3.11.9\"", "Variables: "),
        source.replace("\tscripts =", "\tother ="),
        source.replace("Python version: \"3.11\"", "Python version: \"3.12\""),
    ] {
        assert!(sysconfig_paths(&selected, &bad).is_err(), "accepted {bad}");
    }
    fixture.clean().unwrap();
}
