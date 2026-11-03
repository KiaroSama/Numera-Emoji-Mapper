//! Eighteen pinned source observations, with faults injected only into unchanged external APIs.
use _native::codec_error::CodecError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct FaultState {
    kind: String,
    message: String,
    hits: AtomicUsize,
}
impl FaultState {
    fn raise(&self) -> PyErr {
        self.hits.fetch_add(1, Ordering::SeqCst);
        match self.kind.as_str() {
            "OSError" => pyo3::exceptions::PyOSError::new_err(self.message.clone()),
            "ValueError" => pyo3::exceptions::PyValueError::new_err(self.message.clone()),
            "TypeError" => pyo3::exceptions::PyTypeError::new_err(self.message.clone()),
            "KeyboardInterrupt" => {
                pyo3::exceptions::PyKeyboardInterrupt::new_err(self.message.clone())
            }
            "SystemExit" => pyo3::exceptions::PySystemExit::new_err(self.message.clone()),
            _ => panic!("uninventoried source fault"),
        }
    }
}
#[pyclass]
struct Fault(Arc<FaultState>);
#[pymethods]
impl Fault {
    #[pyo3(signature=(*_args, **_kwargs))]
    fn __call__(
        &self,
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        Err(self.0.raise())
    }
}
#[pyclass]
struct Buffer {
    original: Py<PyAny>,
    state: Arc<FaultState>,
}
#[pymethods]
impl Buffer {
    fn getvalue(&self) -> PyResult<()> {
        Err(self.state.raise())
    }
    fn __getattr__(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        Ok(self.original.bind(py).getattr(name)?.unbind())
    }
}
#[pyclass]
struct Buffers {
    original: Py<PyAny>,
    state: Arc<FaultState>,
}
#[pymethods]
impl Buffers {
    #[pyo3(signature=(*args, **kwargs))]
    fn __call__(
        &self,
        py: Python<'_>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<Buffer>> {
        let original = self.original.bind(py).call(args, kwargs)?.unbind();
        Py::new(
            py,
            Buffer {
                original,
                state: self.state.clone(),
            },
        )
    }
}
struct Restore<'py>(Vec<(Bound<'py, PyAny>, String, Bound<'py, PyAny>)>);
impl Drop for Restore<'_> {
    fn drop(&mut self) {
        for (target, name, original) in self.0.iter().rev() {
            target
                .setattr(name, original)
                .expect("restore external source fault seam");
        }
    }
}
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

fn initial_read_errors(py: Python<'_>, source: &std::path::Path) {
    let class = py.import("pathlib").unwrap().getattr("Path").unwrap();
    let image = py.import("PIL.Image").unwrap();
    let mut restore = Restore(Vec::new());
    for (target, name) in [(&class, "read_bytes"), (image.as_any(), "open")] {
        restore
            .0
            .push((target.clone(), name.into(), target.getattr(name).unwrap()));
    }
    for kind in ["OSError", "TypeError", "KeyboardInterrupt", "SystemExit"] {
        let state = Arc::new(FaultState {
            kind: kind.into(),
            message: "controlled initial read".into(),
            hits: AtomicUsize::new(0),
        });
        let image_state = Arc::new(FaultState {
            kind: "TypeError".into(),
            message: "encoding must follow the initial read".into(),
            hits: AtomicUsize::new(0),
        });
        class
            .setattr("read_bytes", Py::new(py, Fault(state.clone())).unwrap())
            .unwrap();
        image
            .setattr("open", Py::new(py, Fault(image_state.clone())).unwrap())
            .unwrap();
        let request = json!({"operation":"prepare","source":source,"format":"static",
            "reencode":true,"tint":"bad"});
        let mut warnings = vec!["prior warning".to_owned()];
        for result in [
            _native::codec_image::reencode_static_typed(source).map(|_| Value::Null),
            _native::codec_static_prepare::prepare(source, &request, &mut warnings),
            _native::codec_worker::execute_typed_at(None, &request),
        ] {
            let error = result.unwrap_err();
            assert_eq!(error.kind, kind);
            assert_eq!(error.message, "controlled initial read");
            assert_eq!(error.handled, kind == "OSError");
        }
        assert_eq!(warnings, ["prior warning"]);
        assert_eq!(state.hits.load(Ordering::SeqCst), 3);
        assert_eq!(image_state.hits.load(Ordering::SeqCst), 0);
        // Legacy keeps its native byte read; this controlled codec refusal performs no encode.
        assert!(!_native::codec_image::reencode_static(source).unwrap());
        assert_eq!(state.hits.load(Ordering::SeqCst), 3);
        assert_eq!(image_state.hits.load(Ordering::SeqCst), 1);
    }
    drop(restore);
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn normalize(text: &str, source: &std::path::Path) -> String {
    let text = text.replace(
        &_native::python_repr::quoted(&source.to_string_lossy()),
        "'$source'",
    );
    if let Some(start) = text.find("<_io.BytesIO object at 0x") {
        let end = start + text[start..].find('>').expect("complete BytesIO repr") + 1;
        assert!(
            text[start + "<_io.BytesIO object at 0x".len()..end - 1]
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
        format!("{}$buffer{}", &text[..start], &text[end..])
    } else {
        text
    }
}

#[test]
fn original_static_fault_results_keep_classes_warnings_and_unchanged_inputs() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/media-continuation/static-boundary.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(fixture["source_runtime"], "CPython3.11.9");
    assert_eq!(fixture["Pillow"], "12.3.0");
    assert_eq!(
        fixture["source_sha256"]["emojikit/media.py"],
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 18);
    let scratch = tooling_fixture::Root::new("static source boundary");
    Python::initialize();
    Python::attach(|py| {
        let image = py.import("PIL.Image").unwrap();
        let io = py.import("io").unwrap();
        for (index, case) in cases.iter().enumerate() {
            let original_path = root.join(case["input"].as_str().unwrap());
            let original = std::fs::read(&original_path).unwrap();
            assert_eq!(hash(&original), case["input_sha256"]);
            let directory = scratch.0.join(index.to_string());
            std::fs::create_dir(&directory).unwrap();
            let source = directory.join(original_path.file_name().unwrap());
            std::fs::write(&source, &original).unwrap();
            let output = directory.join("preview.webp");
            let operation = case["operation"].as_str().unwrap();
            let fault = case["fault"].as_str();
            let mut restore = Restore(Vec::new());
            let state = case["injected_kind"].as_str().map(|kind| {
                Arc::new(FaultState {
                    kind: kind.into(),
                    message: format!("controlled {} {kind}", fault.unwrap()),
                    hits: AtomicUsize::new(0),
                })
            });
            if let Some(state) = &state {
                let fault = fault.unwrap();
                assert!(case["injected_calls"].as_u64().unwrap() > 0);
                let (target, attribute, replacement) = if fault == "getvalue" {
                    (
                        io.clone().into_any(),
                        "BytesIO",
                        Py::new(
                            py,
                            Buffers {
                                original: io.getattr("BytesIO").unwrap().unbind(),
                                state: state.clone(),
                            },
                        )
                        .unwrap()
                        .into_bound(py)
                        .into_any(),
                    )
                } else {
                    let target = if fault == "open" {
                        image.clone().into_any()
                    } else {
                        image.getattr("Image").unwrap()
                    };
                    (
                        target,
                        fault,
                        Py::new(py, Fault(state.clone()))
                            .unwrap()
                            .into_bound(py)
                            .into_any(),
                    )
                };
                restore.0.push((
                    target.clone(),
                    attribute.into(),
                    target.getattr(attribute).unwrap(),
                ));
                target.setattr(attribute, &replacement).unwrap();
            }
            let mut warnings = Vec::new();
            let result: Result<Value, CodecError> = if operation == "preview" {
                _native::codec_worker::execute_typed_at(
                    None,
                    &json!({"operation":"preview","source":source,"format":"static","key":"fixture","size":52,"output":output}),
                )
            } else {
                _native::codec_image::reencode_static_typed(&source).map(|outcome| {
                    warnings.extend(outcome.warning);
                    json!({"rewritten":outcome.rewritten})
                })
            };
            if operation == "reencode" && case["injected_kind"] == "KeyboardInterrupt" {
                let tint_fixture: Value = serde_json::from_str(include_str!(
                    "../../tests/fixtures/native/static-tint.json"
                ))
                .unwrap();
                let mut request = json!({"operation":"prepare","source":source,"format":"static","reencode":true});
                // Reencode precedes tint validation/application in the original worker.
                for tint in [
                    None,
                    Some(tint_fixture["cases"][1]["tint"].clone()),
                    Some(Value::Null),
                ] {
                    if let Some(tint) = tint {
                        request["tint"] = tint;
                    }
                    let worker_error =
                        _native::codec_worker::execute_typed_at(None, &request).unwrap_err();
                    assert_eq!(worker_error.kind, case["error"]["kind"].as_str().unwrap());
                    assert_eq!(
                        worker_error.message,
                        case["error"]["message"].as_str().unwrap()
                    );
                    assert!(!worker_error.handled);
                    assert_eq!(std::fs::read(&source).unwrap(), original);
                }
            }
            drop(restore);
            if let Some(state) = &state {
                assert!(
                    state.hits.load(Ordering::SeqCst) > 0,
                    "fault was not exercised: {}",
                    case["name"]
                );
            }
            if let Some(expected) = case.get("expected") {
                let result = result.unwrap();
                let bytes = std::fs::read(if operation == "preview" {
                    &output
                } else {
                    &source
                })
                .unwrap();
                assert_eq!(bytes.len() as u64, expected["bytes"].as_u64().unwrap());
                assert_eq!(hash(&bytes), expected["sha256"]);
                if operation == "preview" {
                    assert_eq!(result, json!({"bytes":expected["bytes"]}));
                } else {
                    assert_eq!(result["rewritten"], expected["rewritten"]);
                }
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.kind, case["error"]["kind"].as_str().unwrap());
                assert_eq!(
                    normalize(&error.message, &source),
                    case["error"]["message"].as_str().unwrap()
                );
                assert_eq!(
                    error.handled,
                    !matches!(error.kind.as_str(), "KeyboardInterrupt" | "TypeError")
                );
                assert!(!output.exists());
            }
            let warnings = warnings
                .iter()
                .map(|warning| normalize(warning, &source))
                .collect::<Vec<_>>();
            assert_eq!(json!(warnings), case["warnings"]);
            assert_eq!(
                std::fs::read(&source).unwrap() == original,
                case["input_unchanged"].as_bool().unwrap()
            );
            assert_eq!(std::fs::read(&original_path).unwrap(), original);
        }
        let original_path = root.join("tests/fixtures/native/static-codec/opaque.png");
        let original = std::fs::read(&original_path).unwrap();
        let source = scratch.0.join("initial read.png");
        std::fs::write(&source, &original).unwrap();
        initial_read_errors(py, &source);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert_eq!(std::fs::read(&original_path).unwrap(), original);
    });
    scratch.clean().unwrap();
}
