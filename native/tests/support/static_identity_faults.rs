//! Static identity retry/catch and shared frame resampling faults, through public codec seams.
use super::hash;
use pyo3::prelude::*;
use pyo3::types::{PyCFunction, PyDict, PyTuple};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[pyclass]
struct HashFault {
    kind: &'static str,
    hits: Arc<AtomicUsize>,
}
fn fault(kind: &str, message: &str) -> PyErr {
    match kind {
        "OSError" => pyo3::exceptions::PyOSError::new_err(message.to_owned()),
        "TypeError" => pyo3::exceptions::PyTypeError::new_err(message.to_owned()),
        "KeyboardInterrupt" => pyo3::exceptions::PyKeyboardInterrupt::new_err(message.to_owned()),
        "SystemExit" => pyo3::exceptions::PySystemExit::new_err(message.to_owned()),
        _ => panic!("uninventoried identity fault"),
    }
}
#[pymethods]
impl HashFault {
    #[pyo3(signature=(*_args, **_kwargs))]
    fn __call__(
        &self,
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        Err(fault(self.kind, "controlled static hash"))
    }
}
struct Restore<'py>(Vec<(Bound<'py, PyAny>, &'static str, Option<Bound<'py, PyAny>>)>);
impl Drop for Restore<'_> {
    fn drop(&mut self) {
        for (target, name, value) in self.0.iter().rev() {
            if let Some(value) = value {
                target.setattr(*name, value).expect("restore identity API");
            } else {
                target.delattr(*name).expect("remove identity API");
            }
        }
    }
}
fn fixture() -> (PathBuf, Value) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    (root, fixture)
}
pub fn hash_exceptions() {
    let (root, fixture) = fixture();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "alpha-edge")
        .unwrap();
    let source = root.join(case["input"].as_str().unwrap());
    let before = std::fs::read(&source).unwrap();
    assert_eq!(hash(&before), case["input_sha256"]);
    let request = json!({"operation":"fingerprint","source":source,"format":"static"});
    Python::initialize();
    Python::attach(|py| {
        let class = py.import("PIL.Image").unwrap().getattr("Image").unwrap();
        for kind in ["OSError", "TypeError", "KeyboardInterrupt", "SystemExit"] {
            let original = class.getattr("split").unwrap();
            let restore = Restore(vec![(class.clone(), "split", Some(original.clone()))]);
            let hits = Arc::new(AtomicUsize::new(0));
            class
                .setattr(
                    "split",
                    Py::new(
                        py,
                        HashFault {
                            kind,
                            hits: hits.clone(),
                        },
                    )
                    .unwrap(),
                )
                .unwrap();
            let result = _native::codec_worker::execute_typed_at(None, &request);
            drop(restore);
            assert!(class.getattr("split").unwrap().is(&original));
            if matches!(kind, "OSError" | "TypeError") {
                assert_eq!(
                    result.unwrap(),
                    json!({"content_key":case["expected"]["content_key"], "phash":null,"fmt":"static","extension":".png"})
                );
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.kind, kind);
                assert_eq!(error.message, "controlled static hash");
                assert!(!error.handled);
                assert!(_native::codec_error::CodecError::worker_reply(Err(error)).is_err());
            }
            assert_eq!(hits.load(Ordering::SeqCst), 1);
        }
    });
    let result = _native::codec_worker::execute_typed_at(None, &request).unwrap();
    assert_eq!(result["content_key"], case["expected"]["content_key"]);
    assert_eq!(result["phash"], case["expected"]["phash"]);
    assert_eq!(std::fs::read(source).unwrap(), before);
}
pub fn initial_key_retry() {
    let (root, fixture) = fixture();
    let case = &fixture["cases"][3];
    let source = root.join(case["input"].as_str().unwrap());
    let before = std::fs::read(&source).unwrap();
    assert_eq!(hash(&before), case["input_sha256"]);
    Python::initialize();
    Python::attach(|py| {
        let module = py.import("PIL.Image").unwrap();
        let original = module.getattr("open").unwrap();
        for (kind, persistent, expected_hits) in [
            ("OSError", false, 2),
            ("TypeError", false, 2),
            ("OSError", true, 2),
            ("KeyboardInterrupt", false, 1),
            ("SystemExit", false, 1),
        ] {
            let restore = Restore(vec![(
                module.clone().into_any(),
                "open",
                Some(original.clone()),
            )]);
            let calls = Arc::new(AtomicUsize::new(0));
            let hits = calls.clone();
            let open = original.clone().unbind();
            let replacement = PyCFunction::new_closure(
                py,
                None,
                None,
                move |args, kwargs| -> PyResult<Py<PyAny>> {
                    let count = hits.fetch_add(1, Ordering::SeqCst);
                    if count == 0 || persistent {
                        return Err(fault(kind, "controlled initial key"));
                    }
                    open.bind(args.py()).call(args, kwargs).map(Bound::unbind)
                },
            )
            .unwrap();
            module.setattr("open", replacement).unwrap();
            let result = _native::codec_static_identity::fingerprint_typed(&source);
            drop(restore);
            assert!(module.getattr("open").unwrap().is(&original));
            assert_eq!(calls.load(Ordering::SeqCst), expected_hits);
            if !persistent && matches!(kind, "OSError" | "TypeError") {
                assert_eq!(
                    result.unwrap(),
                    json!({"content_key":case["expected"]["content_key"],"phash":null})
                );
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.kind, kind);
                assert_eq!(error.message, "controlled initial key");
                assert_eq!(error.handled, kind == "OSError");
            }
        }
    });
    Python::attach(|py| {
        let class = py.import("PIL.Image").unwrap().getattr("Image").unwrap();
        let original = class.getattr("tobytes").unwrap();
        let restore = Restore(vec![(class.clone(), "tobytes", Some(original.clone()))]);
        let hits = Arc::new(AtomicUsize::new(0));
        let count = hits.clone();
        let malformed = PyCFunction::new_closure(
            py,
            None,
            None,
            move |args, _| -> PyResult<Py<pyo3::types::PyBytes>> {
                let index = count.fetch_add(1, Ordering::SeqCst);
                let bytes = if index == 0 {
                    vec![1; 64 * 64 * 4]
                } else {
                    Vec::new()
                };
                Ok(pyo3::types::PyBytes::new(args.py(), &bytes).unbind())
            },
        )
        .unwrap();
        class.setattr("tobytes", malformed).unwrap();
        let result = _native::codec_static_identity::fingerprint_typed(&source);
        drop(restore);
        assert!(class.getattr("tobytes").unwrap().is(&original));
        assert_eq!(result.unwrap_err().kind, "UnverifiedCodecError");
        assert_eq!(hits.load(Ordering::SeqCst), 2);
        let count = Arc::new(AtomicUsize::new(0));
        let hits = count.clone();
        let original = class.getattr("tobytes").unwrap();
        let restore = Restore(vec![(class.clone(), "tobytes", Some(original.clone()))]);
        let malformed = PyCFunction::new_closure(
            py,
            None,
            None,
            move |args, _| -> PyResult<Py<pyo3::types::PyBytes>> {
                hits.fetch_add(1, Ordering::SeqCst);
                Ok(pyo3::types::PyBytes::new(args.py(), b"").unbind())
            },
        )
        .unwrap();
        class.setattr("tobytes", malformed).unwrap();
        let result = _native::codec_static_identity::fingerprint_typed(&source);
        drop(restore);
        assert!(class.getattr("tobytes").unwrap().is(&original));
        assert_eq!(result.unwrap_err().kind, "UnverifiedCodecError");
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "native malformed pixels must not retry or manufacture a key"
        );
    });
    let native =
        _native::codec_static_identity::fingerprint_typed(std::path::Path::new("relative.png"))
            .unwrap_err();
    assert_eq!(native.kind, "UnverifiedCodecError");
    assert!(!native.handled);
    let info = _native::codec_static_identity::fingerprint_typed(&source).unwrap();
    assert_eq!(info["content_key"], case["expected"]["content_key"]);
    assert_eq!(info["phash"], case["expected"]["phash"]);
    assert_eq!(std::fs::read(source).unwrap(), before);
    resampling();
}

struct Resampling {
    mode: &'static str,
    getter: bool,
    kind: &'static str,
    phase: Mutex<String>,
    events: Mutex<Vec<String>>,
    created: Mutex<Vec<Py<PyAny>>>,
    closed: Mutex<Vec<usize>>,
}
impl Resampling {
    fn event(&self, stage: &str, mode: &str) {
        self.events.lock().unwrap().push(format!("{stage} {mode}"));
    }
}
#[pyclass]
struct ResizeDescriptor {
    original: Py<PyAny>,
    state: Arc<Resampling>,
}
#[pymethods]
impl ResizeDescriptor {
    fn __get__(
        &self,
        py: Python<'_>,
        image: &Bound<'_, PyAny>,
        owner: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        if image.is_none() {
            return Ok(self.original.clone_ref(py));
        }
        let mode = image.getattr("mode")?.extract::<String>()?;
        let bound = self
            .original
            .bind(py)
            .call_method1("__get__", (image, owner))?;
        if mode != "RGBA" && mode != "L" {
            return Ok(bound.unbind());
        }
        self.state.event("lookup", &mode);
        *self.state.phase.lock().unwrap() = mode.clone();
        if self.state.getter && mode == self.state.mode {
            return Err(fault(self.state.kind, "controlled resize getter"));
        }
        Ok(Py::new(
            py,
            ResizeCall {
                bound: bound.unbind(),
                state: self.state.clone(),
                mode,
            },
        )?
        .into_any())
    }
}
#[pyclass]
struct ResizeCall {
    bound: Py<PyAny>,
    state: Arc<Resampling>,
    mode: String,
}
#[pymethods]
impl ResizeCall {
    #[pyo3(signature=(*args, **kwargs))]
    fn __call__(
        &self,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        self.state.event("resize", &self.mode);
        let result = self.bound.bind(args.py()).call(args, kwargs)?;
        self.state
            .created
            .lock()
            .unwrap()
            .push(result.clone().unbind());
        Ok(result.unbind())
    }
}
fn resampling() {
    let (root, fixture) = fixture();
    let case = &fixture["cases"][3];
    let source = root.join(case["input"].as_str().unwrap());
    let before = std::fs::read(&source).unwrap();
    assert_eq!(hash(&before), case["input_sha256"]);
    let raw = [20, 40, 60, 255].repeat(64 * 64);
    Python::attach(|py| {
        // Independent identity.py expressions require lookup before the direct alias, not Resampling.
        for (frame, mode, getter, kind) in [
            (false, "RGBA", true, "TypeError"),
            (false, "RGBA", false, "OSError"),
            (false, "RGBA", false, "KeyboardInterrupt"),
            (false, "L", true, "TypeError"),
            (false, "L", false, "TypeError"),
            (false, "L", false, "SystemExit"),
            (true, "L", true, "TypeError"),
            (true, "L", true, "KeyboardInterrupt"),
            (true, "L", false, "TypeError"),
            (true, "L", false, "SystemExit"),
        ] {
            let module = py.import("PIL.Image").unwrap();
            let class = module.getattr("Image").unwrap();
            let slots = [
                (&class, "resize"),
                (&class, "close"),
                (module.as_any(), "LANCZOS"),
                (module.as_any(), "__getattr__"),
            ]
            .into_iter()
            .map(|(object, name)| (object.clone(), name, object.getattr_opt(name).unwrap()))
            .collect::<Vec<_>>();
            let readback = slots.clone();
            let restore = Restore(slots);
            let state = Arc::new(Resampling {
                mode,
                getter,
                kind,
                phase: Mutex::new(String::new()),
                events: Mutex::new(Vec::new()),
                created: Mutex::new(Vec::new()),
                closed: Mutex::new(Vec::new()),
            });
            let resize = class.getattr("resize").unwrap();
            class
                .setattr(
                    "resize",
                    Py::new(
                        py,
                        ResizeDescriptor {
                            original: resize.unbind(),
                            state: state.clone(),
                        },
                    )
                    .unwrap(),
                )
                .unwrap();
            let original_close = class.getattr("close").unwrap().unbind();
            let closes = state.clone();
            let close = PyCFunction::new_closure(
                py,
                None,
                None,
                move |args, kwargs| -> PyResult<Py<PyAny>> {
                    closes
                        .closed
                        .lock()
                        .unwrap()
                        .push(args.get_item(0)?.as_ptr() as usize);
                    original_close
                        .bind(args.py())
                        .call(args, kwargs)
                        .map(Bound::unbind)
                },
            )
            .unwrap();
            class
                .setattr(
                    "close",
                    py.import("functools")
                        .unwrap()
                        .getattr("partialmethod")
                        .unwrap()
                        .call1((close,))
                        .unwrap(),
                )
                .unwrap();
            module.delattr("LANCZOS").unwrap();
            let aliases = state.clone();
            let alias = PyCFunction::new_closure(py, None, None, move |args, _| -> PyResult<u32> {
                let name: String = args.get_item(0)?.extract()?;
                if name != "LANCZOS" {
                    return Err(pyo3::exceptions::PyAttributeError::new_err(name));
                }
                let mode = aliases.phase.lock().unwrap().clone();
                aliases.event("alias", &mode);
                if mode.is_empty() || (aliases.getter && mode == aliases.mode) {
                    return Err(fault("SystemExit", "alias must follow resize lookup"));
                }
                if mode == aliases.mode {
                    return Err(fault(aliases.kind, "controlled direct LANCZOS"));
                }
                Ok(1)
            })
            .unwrap();
            module.setattr("__getattr__", alias).unwrap();
            let result = if frame {
                _native::codec_image::frame_phash_typed(&raw).map(|hash| json!({"phash":hash}))
            } else {
                _native::codec_static_identity::fingerprint_typed(&source)
            };
            let legacy = frame.then(|| _native::codec_image::frame_phash(&raw));
            drop(restore);
            for (object, name, original) in readback {
                let actual = object.getattr_opt(name).unwrap();
                assert!(
                    match (original, actual) {
                        (Some(a), Some(b)) => a.is(&b),
                        (None, None) => true,
                        _ => false,
                    },
                    "{name} restore"
                );
            }
            if frame {
                assert_eq!(legacy, Some(None));
            }
            let ordinary = matches!(kind, "OSError" | "TypeError");
            if ordinary && (frame || mode == "L") {
                let actual = result.unwrap();
                assert_eq!(actual["phash"], Value::Null);
                if !frame {
                    assert_eq!(actual["content_key"], case["expected"]["content_key"]);
                }
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.kind, kind);
                assert_eq!(
                    error.message,
                    if getter {
                        "controlled resize getter"
                    } else {
                        "controlled direct LANCZOS"
                    }
                );
                assert_eq!(error.handled, kind == "OSError");
            }
            let prefix = if !frame && mode == "L" {
                "lookup RGBA|alias RGBA|resize RGBA|"
            } else {
                ""
            };
            let suffix = if getter {
                format!("lookup {mode}")
            } else {
                format!("lookup {mode}|alias {mode}")
            };
            let expected = if frame || (mode == "RGBA" && ordinary) {
                format!("{suffix}|{suffix}")
            } else {
                format!("{prefix}{suffix}")
            };
            assert_eq!(
                state.events.lock().unwrap().join("|"),
                expected,
                "{frame}/{mode}/{getter}/{kind}"
            );
            for image in state.created.lock().unwrap().iter() {
                let id = image.as_ptr() as usize;
                assert_eq!(
                    state
                        .closed
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|closed| **closed == id)
                        .count(),
                    1,
                    "resized image close"
                );
            }
            assert_eq!(std::fs::read(&source).unwrap(), before);
        }
    });
    for case in fixture["cases"].as_array().unwrap() {
        let source = root.join(case["input"].as_str().unwrap());
        let before = std::fs::read(&source).unwrap();
        assert_eq!(hash(&before), case["input_sha256"]);
        let actual = _native::codec_static_identity::fingerprint_typed(&source).unwrap();
        assert_eq!(actual["content_key"], case["expected"]["content_key"]);
        assert_eq!(actual["phash"], case["expected"]["phash"]);
        assert_eq!(std::fs::read(source).unwrap(), before);
    }
    assert_eq!(
        _native::codec_image::frame_phash_typed(&raw).unwrap(),
        Some(0)
    );
    assert_eq!(
        _native::codec_video::fingerprint_frames_typed(&source, &raw).unwrap(),
        json!({"content_key":"v:769f9a9a414a35bdb6e3ee62419e9c1e","phash":0,"fmt":"video","extension":".webm"})
    );
    assert_eq!(std::fs::read(source).unwrap(), before);
}
