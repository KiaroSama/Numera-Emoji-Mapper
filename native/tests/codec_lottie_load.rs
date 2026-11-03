//! Real compressed source fixtures exercise the bound, decoder errors and root shape.
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

// Both tests decode with process-global gzip/json APIs in the same interpreter.
static GZIP_API: Mutex<()> = Mutex::new(());

#[test]
fn pinned_tgs_load_preserves_decoded_document_and_bounded_source_errors() {
    let _gzip_api = GZIP_API.lock().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/tgs-load.json")).unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 7);
    let directory = root
        .join("logs")
        .join(format!("tgs load {}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for case in cases {
        let path = if let Some(encoded) = case["input_base64"].as_str() {
            use base64::Engine;
            let text = std::fs::read_to_string(root.join(encoded)).unwrap();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(text.trim())
                .unwrap();
            let path = directory.join(case["name"].as_str().unwrap());
            std::fs::write(&path, bytes).unwrap();
            path
        } else {
            root.join(case["input"].as_str().unwrap())
        };
        assert_eq!(
            Sha256::digest(std::fs::read(&path).unwrap())
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>(),
            case["input_sha256"]
        );
        let actual = _native::codec_lottie_load::load(&path);
        if let Some(expected) = case.get("expected") {
            assert_eq!(actual.unwrap(), *expected, "{}", case["name"]);
        } else {
            let error = actual.unwrap_err();
            assert_eq!(error.kind, case["error_kind"].as_str().unwrap());
            assert_eq!(error.message, case["error"].as_str().unwrap());
        }
        if case["input_base64"].is_string() {
            std::fs::remove_file(path).unwrap();
        }
    }
    std::fs::remove_dir(directory).unwrap();
}

struct GzipState {
    mode: &'static str,
    phase: &'static str,
    kind: &'static str,
    close_kind: Option<&'static str>,
    suppress: bool,
    events: Mutex<Vec<&'static str>>,
    triples: Mutex<Vec<(String, String, bool)>>,
    buffers: Mutex<Vec<Arc<std::sync::atomic::AtomicUsize>>>,
    loads: std::sync::atomic::AtomicUsize,
    values: std::sync::atomic::AtomicUsize,
}
impl GzipState {
    fn event(&self, event: &'static str, active: bool) {
        if active {
            self.events.lock().unwrap().push(event);
        }
    }
    fn fail(&self, phase: &str, active: bool) -> PyResult<()> {
        if active && self.phase == phase {
            Err(gzip_failure(self.kind))
        } else {
            Ok(())
        }
    }
}
fn gzip_failure(kind: &str) -> PyErr {
    match kind {
        "OSError" => pyo3::exceptions::PyOSError::new_err("controlled gzip fault"),
        "ValueError" => pyo3::exceptions::PyValueError::new_err("controlled gzip fault"),
        "TypeError" => pyo3::exceptions::PyTypeError::new_err("controlled gzip fault"),
        "KeyboardInterrupt" => {
            pyo3::exceptions::PyKeyboardInterrupt::new_err("controlled gzip fault")
        }
        "SystemExit" => pyo3::exceptions::PySystemExit::new_err("controlled gzip fault"),
        _ => panic!("unexpected gzip fixture error class"),
    }
}

#[pyclass]
struct GzipBody {
    raw: Py<PyAny>,
    state: Arc<GzipState>,
    active: bool,
}
#[pymethods]
impl GzipBody {
    fn read<'py>(&self, py: Python<'py>, length: usize) -> PyResult<Bound<'py, PyAny>> {
        self.state.event("read", self.active);
        self.state.fail("body", self.active)?;
        self.raw.bind(py).call_method1("read", (length,))
    }
    fn write<'py>(&self, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.state.event("write", self.active);
        self.state.fail("body", self.active)?;
        self.raw.bind(value.py()).call_method1("write", (value,))
    }
}
#[pyclass]
struct GzipManager {
    raw: Py<PyAny>,
    state: Arc<GzipState>,
    active: bool,
}
#[pymethods]
impl GzipManager {
    fn __enter__(&self, py: Python<'_>) -> PyResult<Py<GzipBody>> {
        self.state.event("enter", self.active);
        self.state.fail("enter", self.active)?;
        let entered = self.raw.bind(py).call_method0("__enter__")?;
        Py::new(
            py,
            GzipBody {
                raw: entered.unbind(),
                state: self.state.clone(),
                active: self.active,
            },
        )
    }
    fn __exit__(
        &self,
        py: Python<'_>,
        kind: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
        traceback: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.state.event("exit", self.active);
        if self.active {
            self.state.triples.lock().unwrap().push((
                if kind.is_none() {
                    "None".into()
                } else {
                    kind.getattr("__name__")?.extract()?
                },
                if value.is_none() {
                    "None".into()
                } else {
                    value.str()?.extract()?
                },
                traceback.is_none(),
            ));
        }
        self.close(py)?;
        Ok(self.state.suppress && self.active)
    }
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        self.state.event("close", self.active);
        self.raw.bind(py).call_method0("close")?;
        if self.active
            && let Some(kind) = self.state.close_kind
        {
            return Err(gzip_failure(kind));
        }
        Ok(())
    }
    // A distinct entered value catches accidentally operating on the manager itself.
    fn read(&self, _length: usize) -> PyResult<()> {
        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "gzip manager used instead of entered value",
        ))
    }
    fn write(&self, _value: &Bound<'_, PyAny>) -> PyResult<()> {
        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "gzip manager used instead of entered value",
        ))
    }
}
#[pyclass]
struct GzipFactory(Py<PyAny>, Arc<GzipState>);
#[pymethods]
impl GzipFactory {
    #[pyo3(signature=(*args, **kwargs))]
    fn __call__(
        &self,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<GzipManager>> {
        let mode = kwargs
            .map(|options| options.get_item("mode"))
            .transpose()?
            .flatten()
            .map(|value| value.extract::<String>())
            .transpose()?
            .unwrap_or_else(|| "rb".into());
        let raw = self.0.bind(args.py()).call(args, kwargs)?;
        Py::new(
            args.py(),
            GzipManager {
                raw: raw.unbind(),
                state: self.1.clone(),
                active: mode == self.1.mode,
            },
        )
    }
}

#[pyclass]
struct GzipBuffer {
    raw: Py<PyAny>,
    state: Arc<GzipState>,
    closes: Arc<std::sync::atomic::AtomicUsize>,
}
#[pymethods]
impl GzipBuffer {
    fn __getattr__<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
        self.raw.bind(py).getattr(name)
    }
    fn getvalue<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.state
            .values
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.raw.bind(py).call_method0("getvalue")
    }
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        self.closes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.raw.bind(py).call_method0("close")?;
        Ok(())
    }
}
struct RestoreGzip<'py>(Vec<(Bound<'py, PyModule>, &'static str, Bound<'py, PyAny>)>);
impl Drop for RestoreGzip<'_> {
    fn drop(&mut self) {
        for (module, name, value) in self.0.iter().rev() {
            module
                .setattr(*name, value)
                .expect("restore gzip fixture API");
        }
    }
}

#[test]
fn gzip_context_errors_preserve_identity_validation_and_rewrite_boundaries() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let _gzip_api = GZIP_API.lock().unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let valid = root.join("tests/fixtures/native/tgs-load/valid.tgs");
    let original = std::fs::read(&valid).unwrap();
    assert_eq!(
        _native::tooling_manifest::digest(&valid).unwrap(),
        "7eab5f062ab6ff2b69178b0086356f9fa361ac0f755b4448e05c97c713376135"
    );
    let plain = root.join("tests/fixtures/native/tgs-package/valid.json");
    let plain_before = std::fs::read(&plain).unwrap();
    assert_eq!(
        _native::tooling_manifest::digest(&plain).unwrap(),
        "98c87aa14eff37f85f1bb7193e244f5ddecb424a00d77838e8f7375fc056dce5"
    );
    let fixtures: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/tgs-load.json")).unwrap();
    let expected_document = fixtures["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["input"] == "tests/fixtures/native/tgs-load/valid.tgs")
        .unwrap()["expected"]
        .clone();
    let scratch = tooling_fixture::Root::new("gzip contexts café");
    let source = scratch.0.join("input.tgs");
    std::fs::write(&source, &original).unwrap();
    Python::initialize();
    Python::attach(|py| {
        let gzip = py.import("gzip").unwrap();
        let io = py.import("io").unwrap();
        let json = py.import("json").unwrap();
        for (index, (route, mode, phase, kind, close_kind, suppress, expected)) in [
            ("load", "rb", "", "", None, false, ""),
            ("load", "rb", "enter", "OSError", None, false, "OSError"),
            (
                "load",
                "rb",
                "enter",
                "TypeError",
                Some("SystemExit"),
                false,
                "TypeError",
            ),
            ("load", "rb", "body", "TypeError", None, false, "TypeError"),
            ("load", "rb", "", "", Some("OSError"), false, "OSError"),
            (
                "load",
                "rb",
                "body",
                "TypeError",
                Some("SystemExit"),
                false,
                "SystemExit",
            ),
            ("load", "rb", "body", "TypeError", None, true, "TypeError"),
            (
                "validate",
                "rb",
                "",
                "",
                Some("OSError"),
                false,
                "MediaError",
            ),
            ("identity", "rb", "", "", Some("ValueError"), false, ""),
            (
                "identity",
                "rb",
                "",
                "",
                Some("KeyboardInterrupt"),
                false,
                "KeyboardInterrupt",
            ),
            ("compare", "rb", "", "", Some("TypeError"), false, ""),
            ("reencode", "rb", "", "", Some("OSError"), false, ""),
            ("repaint", "rb", "", "", Some("TypeError"), false, ""),
            ("reencode", "wb", "", "", Some("OSError"), false, ""),
            (
                "repaint",
                "wb",
                "",
                "",
                Some("KeyboardInterrupt"),
                false,
                "KeyboardInterrupt",
            ),
            ("package", "wb", "enter", "OSError", None, false, "OSError"),
            (
                "package",
                "wb",
                "body",
                "TypeError",
                None,
                false,
                "TypeError",
            ),
            ("package", "wb", "", "", Some("OSError"), false, "OSError"),
            (
                "package",
                "wb",
                "body",
                "TypeError",
                Some("SystemExit"),
                false,
                "SystemExit",
            ),
            ("package", "wb", "", "", None, false, ""),
        ]
        .into_iter()
        .enumerate()
        {
            let state = Arc::new(GzipState {
                mode,
                phase,
                kind,
                close_kind,
                suppress,
                events: Mutex::new(Vec::new()),
                triples: Mutex::new(Vec::new()),
                buffers: Mutex::new(Vec::new()),
                loads: AtomicUsize::new(0),
                values: AtomicUsize::new(0),
            });
            let old_gzip = gzip.getattr("GzipFile").unwrap();
            let old_buffer = io.getattr("BytesIO").unwrap();
            let old_loads = json.getattr("loads").unwrap();
            let restore = RestoreGzip(vec![
                (gzip.clone(), "GzipFile", old_gzip.clone()),
                (io.clone(), "BytesIO", old_buffer.clone()),
                (json.clone(), "loads", old_loads.clone()),
            ]);
            gzip.setattr(
                "GzipFile",
                Py::new(py, GzipFactory(old_gzip.unbind(), state.clone())).unwrap(),
            )
            .unwrap();
            let buffer_class = old_buffer.unbind();
            let observed = state.clone();
            let buffer = pyo3::types::PyCFunction::new_closure(
                py,
                None,
                None,
                move |args, kwargs| -> PyResult<Py<GzipBuffer>> {
                    let raw = buffer_class.bind(args.py()).call(args, kwargs)?;
                    let closes = Arc::new(AtomicUsize::new(0));
                    observed.buffers.lock().unwrap().push(closes.clone());
                    Py::new(
                        args.py(),
                        GzipBuffer {
                            raw: raw.unbind(),
                            state: observed.clone(),
                            closes,
                        },
                    )
                },
            )
            .unwrap();
            io.setattr("BytesIO", buffer).unwrap();
            let actual_loads = old_loads.unbind();
            let observed = state.clone();
            let loads = pyo3::types::PyCFunction::new_closure(
                py,
                None,
                None,
                move |args, kwargs| -> PyResult<Py<PyAny>> {
                    observed.loads.fetch_add(1, Ordering::SeqCst);
                    actual_loads
                        .bind(args.py())
                        .call(args, kwargs)
                        .map(Bound::unbind)
                },
            )
            .unwrap();
            json.setattr("loads", loads).unwrap();
            let output = scratch.0.join(format!("output-{index}.tgs"));
            let result = match route {
                "load" => _native::codec_lottie_load::load(&source).map_err(|error| {
                    _native::codec_error::CodecError {
                        kind: error.kind,
                        message: error.message,
                        handled: error.handled,
                    }
                }),
                "validate" => _native::codec_lottie_validate::validate_file_typed(&source)
                    .map(|()| Value::Null),
                "identity" => _native::codec_lottie_identity::fingerprint_typed(&source),
                "compare" => _native::codec_lottie_identity::same_image_typed(&source, &source)
                    .map(|value| serde_json::json!(value)),
                "reencode" | "repaint" => _native::codec_lottie_prepare::rewrite_typed(
                    &source,
                    (route == "repaint").then_some([26, 128, 255]),
                )
                .map(|outcome| {
                    serde_json::json!({
                        "rewritten":outcome.rewritten,"warning":outcome.warning})
                }),
                "package" => _native::codec_lottie_package::package_typed(&plain, &output)
                    .map(|()| Value::Null),
                _ => unreachable!(),
            };
            drop(restore);
            let events = state.events.lock().unwrap().clone();
            assert_eq!(
                events,
                if phase == "enter" {
                    vec!["enter", "close"]
                } else if mode == "rb" {
                    vec!["enter", "read", "exit", "close"]
                } else {
                    vec!["enter", "write", "exit", "close"]
                },
                "{route}/{index}"
            );
            let triples = state.triples.lock().unwrap().clone();
            assert_eq!(
                triples,
                if phase == "enter" {
                    vec![]
                } else if phase == "body" {
                    vec![(kind.into(), "controlled gzip fault".into(), true)]
                } else {
                    vec![("None".into(), "None".into(), true)]
                },
                "{route}/{index}"
            );
            if !expected.is_empty() {
                let error = result.unwrap_err();
                assert_eq!(error.kind, expected, "{route}/{index}");
                assert_eq!(
                    error.message,
                    if expected == "MediaError" {
                        "input.tgs: not a valid TGS/Lottie (controlled gzip fault)"
                    } else {
                        "controlled gzip fault"
                    }
                );
                assert_eq!(
                    error.handled,
                    matches!(expected, "OSError" | "ValueError" | "MediaError")
                );
            } else {
                let value = result.unwrap();
                match route {
                    "load" => assert_eq!(value, expected_document),
                    "identity" => {
                        assert_eq!(value["content_key"], "a:7eab5f062ab6ff2b69178b0086356f9f")
                    }
                    "compare" => assert_eq!(value, Value::Null),
                    "reencode" | "repaint" => {
                        assert_eq!(value["rewritten"], false);
                        assert_eq!(
                            value["warning"],
                            format!(
                                "{} skipped for input.tgs (animated): controlled gzip fault",
                                if route == "reencode" {
                                    "re-encode"
                                } else {
                                    "repaint"
                                }
                            )
                        );
                    }
                    "package" => {
                        assert_eq!(
                            _native::tooling_manifest::digest(&output).unwrap(),
                            "8b83d1e5effe7f27100badeeba5fc39883fd2835986305ef6f7e8c736b0040eb"
                        );
                        assert_eq!(std::fs::metadata(&output).unwrap().len(), 100);
                        std::fs::remove_file(&output).unwrap();
                    }
                    _ => unreachable!(),
                }
            }
            let normal = phase.is_empty() && close_kind.is_none();
            assert_eq!(
                state.loads.load(Ordering::SeqCst),
                if mode == "rb" {
                    usize::from(normal)
                } else if normal {
                    2
                } else {
                    1
                },
                "JSON must wait for successful gzip exit"
            );
            assert_eq!(
                state.values.load(Ordering::SeqCst),
                usize::from(mode == "wb" && normal),
                "getvalue must wait for successful gzip exit"
            );
            assert!(!state.buffers.lock().unwrap().is_empty());
            assert!(
                state
                    .buffers
                    .lock()
                    .unwrap()
                    .iter()
                    .all(|closes| closes.load(Ordering::SeqCst) == 1)
            );
            assert!(!output.exists());
            assert_eq!(std::fs::read(&source).unwrap(), original);
            assert_eq!(std::fs::read(&plain).unwrap(), plain_before);
            assert_eq!(std::fs::read_dir(&scratch.0).unwrap().count(), 1);
            assert_eq!(
                _native::codec_lottie_load::load(&valid).unwrap(),
                expected_document,
                "the real external APIs must be restored after every variant"
            );
        }
    });
    assert_eq!(std::fs::read(valid).unwrap(), original);
    scratch.clean().unwrap();
}
