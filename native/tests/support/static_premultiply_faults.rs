//! Public identity/comparison faults observe external Pillow mode, lookup and ownership boundaries.
use pyo3::prelude::*;
use pyo3::types::{PyCFunction, PyTuple};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Case {
    ModeTrue,
    ModeFalse,
    EqType,
    EqExit,
    MergeLookup,
    RedLookup,
    GreenLookup,
    BlueLookup,
    MergeCall,
    NoComparisonMode,
}
struct State {
    case: Case,
    events: Mutex<Vec<String>>,
    lookups: AtomicUsize,
    bands: Mutex<Vec<usize>>,
    owned: Mutex<Vec<Py<PyAny>>>,
    closed: Mutex<Vec<usize>>,
}
impl State {
    fn event(&self, event: &str) {
        self.events.lock().unwrap().push(event.into());
    }
    fn own(&self, image: &Bound<'_, PyAny>) {
        self.owned.lock().unwrap().push(image.clone().unbind());
    }
    fn wrap(
        self: &Arc<Self>,
        py: Python<'_>,
        image: Bound<'_, PyAny>,
        normalized: bool,
    ) -> PyResult<Py<PyAny>> {
        self.own(&image);
        Ok(Py::new(
            py,
            Image {
                actual: image.unbind(),
                normalized,
                reads: AtomicUsize::new(0),
                state: self.clone(),
            },
        )?
        .into_any())
    }
}
#[pyclass]
struct Image {
    actual: Py<PyAny>,
    normalized: bool,
    reads: AtomicUsize,
    state: Arc<State>,
}
#[pymethods]
impl Image {
    #[getter]
    fn mode(&self, py: Python<'_>) -> PyResult<Py<Mode>> {
        self.state.event(if self.normalized {
            "mode normalized"
        } else {
            "mode source"
        });
        if self.normalized || self.reads.fetch_add(1, Ordering::SeqCst) != 0 {
            return Err(pyo3::exceptions::PyKeyboardInterrupt::new_err(
                "unexpected extra mode read",
            ));
        }
        Py::new(py, Mode(self.state.clone()))
    }
    fn split(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.state.event(if self.normalized {
            "split normalized"
        } else {
            "split source"
        });
        let bands = self.actual.bind(py).call_method0("split")?;
        let mut ids = Vec::new();
        for band in bands.cast::<PyTuple>()?.iter() {
            ids.push(band.as_ptr() as usize);
            self.state.own(&band);
        }
        *self.state.bands.lock().unwrap() = ids;
        Ok(bands.unbind())
    }
    fn convert(&self, py: Python<'_>, mode: &str) -> PyResult<Py<PyAny>> {
        if mode == "L" && self.state.case == Case::ModeFalse {
            self.state.event("convert L");
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "controlled non-RGBA conversion",
            ));
        }
        let converted = self.actual.bind(py).call_method1("convert", (mode,))?;
        self.state.own(&converted);
        Ok(converted.unbind())
    }
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        self.actual.bind(py).call_method0("close").map(|_| ())
    }
}
#[pyclass]
struct Mode(Arc<State>);
#[pymethods]
impl Mode {
    fn __eq__(&self, other: &str) -> PyResult<bool> {
        self.0.event("mode eq");
        if other != "RGBA" {
            return Err(pyo3::exceptions::PyAssertionError::new_err(
                "mode comparison operand",
            ));
        }
        match self.0.case {
            Case::EqType => Err(pyo3::exceptions::PyTypeError::new_err(
                "controlled mode equality",
            )),
            Case::EqExit => Err(pyo3::exceptions::PySystemExit::new_err(
                "controlled mode equality",
            )),
            Case::ModeFalse => Ok(false),
            _ => Ok(true),
        }
    }
}
type SavedAttribute<'py> = (Bound<'py, PyAny>, &'static str, Option<Bound<'py, PyAny>>);
struct Restore<'py>(Vec<SavedAttribute<'py>>);
impl Drop for Restore<'_> {
    fn drop(&mut self) {
        for (object, name, value) in self.0.iter().rev() {
            if let Some(value) = value {
                object
                    .setattr(*name, value)
                    .expect("restore premultiply API");
            } else {
                object.delattr(*name).expect("remove premultiply API");
            }
        }
    }
}
fn observe<T>(py: Python<'_>, case: Case, body: impl FnOnce() -> T) -> (T, Arc<State>) {
    let module = py.import("PIL.Image").unwrap();
    let chops = py.import("PIL.ImageChops").unwrap();
    let class = module.getattr("Image").unwrap();
    let slots = [
        (module.as_any(), "open"),
        (module.as_any(), "frombytes"),
        (module.as_any(), "merge"),
        (module.as_any(), "__getattr__"),
        (chops.as_any(), "multiply"),
        (chops.as_any(), "__getattr__"),
        (&class, "resize"),
        (&class, "close"),
    ]
    .into_iter()
    .map(|(object, name)| (object.clone(), name, object.getattr_opt(name).unwrap()))
    .collect::<Vec<_>>();
    let readback = slots.clone();
    let restore = Restore(slots);
    let state = Arc::new(State {
        case,
        events: Mutex::new(Vec::new()),
        lookups: AtomicUsize::new(0),
        bands: Mutex::new(Vec::new()),
        owned: Mutex::new(Vec::new()),
        closed: Mutex::new(Vec::new()),
    });
    for name in ["open", "frombytes"] {
        let original = module.getattr(name).unwrap().unbind();
        let images = state.clone();
        let replacement =
            PyCFunction::new_closure(py, None, None, move |args, kwargs| -> PyResult<Py<PyAny>> {
                let py = args.py();
                images.wrap(py, original.bind(py).call(args, kwargs)?, false)
            })
            .unwrap();
        module.setattr(name, replacement).unwrap();
    }
    let close = class.getattr("close").unwrap().unbind();
    let closes = state.clone();
    let tracer =
        PyCFunction::new_closure(py, None, None, move |args, kwargs| -> PyResult<Py<PyAny>> {
            let image = args.get_item(0)?;
            // Pillow also closes unregistered internal temporaries. Their addresses can
            // be reused later; record only objects already owned by this observation.
            if closes
                .owned
                .lock()
                .unwrap()
                .iter()
                .any(|owned| owned.bind(args.py()).is(&image))
            {
                closes.closed.lock().unwrap().push(image.as_ptr() as usize);
            }
            close.bind(args.py()).call(args, kwargs).map(Bound::unbind)
        })
        .unwrap();
    let partial = py
        .import("functools")
        .unwrap()
        .getattr("partialmethod")
        .unwrap();
    class
        .setattr("close", partial.call1((tracer,)).unwrap())
        .unwrap();
    if case == Case::NoComparisonMode {
        let resize = class.getattr("resize").unwrap().unbind();
        let images = state.clone();
        let tracer =
            PyCFunction::new_closure(py, None, None, move |args, kwargs| -> PyResult<Py<PyAny>> {
                let py = args.py();
                let result = resize.bind(py).call(args, kwargs)?;
                if args.get_item(1)?.extract::<(u32, u32)>()? == (64, 64)
                    && result.getattr("mode")?.extract::<String>()? == "RGBA"
                {
                    images.wrap(py, result, true)
                } else {
                    Ok(result.unbind())
                }
            })
            .unwrap();
        class
            .setattr("resize", partial.call1((tracer,)).unwrap())
            .unwrap();
    }
    let merge = module.getattr("merge").unwrap().unbind();
    module.delattr("merge").unwrap();
    let merges = state.clone();
    let lookup = PyCFunction::new_closure(py, None, None, move |args, _| -> PyResult<Py<PyAny>> {
        let name: String = args.get_item(0)?.extract()?;
        if name != "merge" {
            return Err(pyo3::exceptions::PyAttributeError::new_err(name));
        }
        merges.event("merge lookup");
        if case == Case::MergeLookup {
            return Err(pyo3::exceptions::PyOSError::new_err(
                "controlled merge lookup",
            ));
        }
        let callable = merge.clone_ref(args.py());
        let calls = merges.clone();
        Ok(PyCFunction::new_closure(
            args.py(),
            None,
            None,
            move |args, kwargs| -> PyResult<Py<PyAny>> {
                calls.event("merge call");
                let bands = args.get_item(1)?;
                if args.get_item(0)?.extract::<String>()? != "RGBA"
                    || bands.get_item(3)?.as_ptr() as usize != calls.bands.lock().unwrap()[3]
                {
                    return Err(pyo3::exceptions::PyAssertionError::new_err(
                        "merge RGBA/alpha arguments",
                    ));
                }
                if case == Case::MergeCall {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "controlled merge call",
                    ));
                }
                let result = callable.bind(args.py()).call(args, kwargs)?;
                calls.own(&result);
                Ok(result.unbind())
            },
        )?
        .into_any()
        .unbind())
    })
    .unwrap();
    module.setattr("__getattr__", lookup).unwrap();
    let multiply = chops.getattr("multiply").unwrap().unbind();
    chops.delattr("multiply").unwrap();
    let multiplies = state.clone();
    let lookup = PyCFunction::new_closure(py, None, None, move |args, _| -> PyResult<Py<PyAny>> {
        let name: String = args.get_item(0)?.extract()?;
        if name != "multiply" {
            return Err(pyo3::exceptions::PyAttributeError::new_err(name));
        }
        multiplies.event("multiply lookup");
        let index = multiplies.lookups.fetch_add(1, Ordering::SeqCst);
        match (case, index) {
            (Case::MergeLookup, _) => {
                return Err(pyo3::exceptions::PySystemExit::new_err(
                    "multiply must follow merge lookup",
                ));
            }
            (Case::RedLookup, 0) => {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "controlled red multiply lookup",
                ));
            }
            (Case::GreenLookup, 1) => {
                return Err(pyo3::exceptions::PyKeyboardInterrupt::new_err(
                    "controlled green multiply lookup",
                ));
            }
            (Case::BlueLookup, 2) => {
                return Err(pyo3::exceptions::PySystemExit::new_err(
                    "controlled blue multiply lookup",
                ));
            }
            _ => {}
        }
        let callable = multiply.clone_ref(args.py());
        let calls = multiplies.clone();
        Ok(PyCFunction::new_closure(
            args.py(),
            None,
            None,
            move |args, kwargs| -> PyResult<Py<PyAny>> {
                calls.event("multiply call");
                let bands = calls.bands.lock().unwrap();
                if args.get_item(0)?.as_ptr() as usize != bands[index % 3]
                    || args.get_item(1)?.as_ptr() as usize != bands[3]
                {
                    return Err(pyo3::exceptions::PyAssertionError::new_err(
                        "multiply RGB/alpha arguments",
                    ));
                }
                drop(bands);
                let result = callable.bind(args.py()).call(args, kwargs)?;
                calls.own(&result);
                Ok(result.unbind())
            },
        )?
        .into_any()
        .unbind())
    })
    .unwrap();
    chops.setattr("__getattr__", lookup).unwrap();
    let result = body();
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
    (result, state)
}
fn resources(py: Python<'_>, state: &State) {
    for image in state.owned.lock().unwrap().iter() {
        let id = image.bind(py).as_ptr() as usize;
        assert_eq!(
            state
                .closed
                .lock()
                .unwrap()
                .iter()
                .filter(|closed| **closed == id)
                .count(),
            1,
            "owned Pillow image close: {:?}, identity={id}",
            state.case
        );
    }
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn check(comparison: bool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    let case = &fixture["cases"][3];
    let source = root.join(case["input"].as_str().unwrap());
    let before = std::fs::read(&source).unwrap();
    assert_eq!(digest(&before), case["input_sha256"]);
    let compare: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/static-compare.json"
    ))
    .unwrap();
    let pair = &compare["cases"][1];
    assert_eq!(pair["name"], "red-near-red");
    let left = root.join(pair["a"].as_str().unwrap());
    let right = root.join(pair["b"].as_str().unwrap());
    let pair_before = [&left, &right].map(|p| std::fs::read(p).unwrap());
    for (bytes, key) in pair_before.iter().zip(["a_sha256", "b_sha256"]) {
        assert_eq!(digest(bytes), pair[key]);
    }
    let raw = [20, 40, 60, 255].repeat(64 * 64);
    let cases: &[Case] = if comparison {
        &[
            Case::MergeLookup,
            Case::BlueLookup,
            Case::MergeCall,
            Case::NoComparisonMode,
        ]
    } else {
        &[
            Case::ModeTrue,
            Case::ModeFalse,
            Case::EqType,
            Case::EqExit,
            Case::RedLookup,
            Case::GreenLookup,
        ]
    };
    Python::initialize();
    Python::attach(|py| {
        for &variant in cases {
            let (result, state) = observe(
                py,
                variant,
                || -> Result<Value, _native::codec_error::CodecError> {
                    match variant {
                        Case::MergeLookup | Case::NoComparisonMode => {
                            _native::codec_image::same_frame_typed(&raw, &raw)
                                .map(|same| json!(same))
                        }
                        Case::BlueLookup | Case::MergeCall => {
                            _native::codec_image::same_image_typed(&left, &right)
                                .map(|same| json!(same))
                        }
                        Case::ModeFalse | Case::EqExit | Case::GreenLookup => {
                            _native::codec_image::frame_phash_typed(&raw).map(|hash| json!(hash))
                        }
                        _ => _native::codec_static_identity::fingerprint_typed(&source),
                    }
                },
            );
            let prefix = "mode source|mode eq|";
            let channels = "split source|merge lookup|";
            let once = "multiply lookup|multiply call|";
            let events = state.events.lock().unwrap().join("|");
            match variant {
                Case::ModeTrue => {
                    assert_eq!(
                        result.unwrap(),
                        json!({"content_key":case["expected"]["content_key"],"phash":case["expected"]["phash"]})
                    );
                    assert_eq!(
                        events,
                        format!("{prefix}{channels}{}merge call", once.repeat(3))
                    );
                }
                Case::ModeFalse => {
                    assert_eq!(result.unwrap(), Value::Null);
                    assert_eq!(events, format!("{prefix}convert L"));
                }
                Case::EqType => {
                    let actual = result.unwrap();
                    assert_eq!(actual["content_key"], case["expected"]["content_key"]);
                    assert_eq!(actual["phash"], Value::Null);
                    assert_eq!(events, "mode source|mode eq");
                }
                Case::RedLookup => {
                    let actual = result.unwrap();
                    assert_eq!(actual["content_key"], case["expected"]["content_key"]);
                    assert_eq!(actual["phash"], Value::Null);
                    assert_eq!(events, format!("{prefix}{channels}multiply lookup"));
                }
                Case::MergeCall => {
                    assert_eq!(result.unwrap(), Value::Null);
                    assert_eq!(
                        events,
                        format!("{prefix}{channels}{}merge call", once.repeat(3))
                    );
                }
                Case::NoComparisonMode => {
                    assert_eq!(result.unwrap(), true);
                    let hash_group = format!("{prefix}{channels}{}merge call", once.repeat(3));
                    let color_group =
                        format!("split normalized|merge lookup|{}merge call", once.repeat(3));
                    assert_eq!(
                        events,
                        [&hash_group, &hash_group, &color_group, &color_group]
                            .into_iter()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("|")
                    );
                }
                _ => {
                    let (kind, message, expected) = match variant {
                        Case::EqExit => (
                            "SystemExit",
                            "controlled mode equality",
                            "mode source|mode eq".into(),
                        ),
                        Case::MergeLookup => (
                            "OSError",
                            "controlled merge lookup",
                            format!("{prefix}split source|merge lookup"),
                        ),
                        Case::GreenLookup => (
                            "KeyboardInterrupt",
                            "controlled green multiply lookup",
                            format!("{prefix}{channels}{once}multiply lookup"),
                        ),
                        Case::BlueLookup => (
                            "SystemExit",
                            "controlled blue multiply lookup",
                            format!("{prefix}{channels}{}multiply lookup", once.repeat(2)),
                        ),
                        _ => unreachable!(),
                    };
                    let error = result.unwrap_err();
                    assert_eq!(error.kind, kind);
                    assert_eq!(error.message, message);
                    assert_eq!(error.handled, kind == "OSError");
                    assert_eq!(events, expected);
                }
            }
            resources(py, &state);
            assert_eq!(std::fs::read(&source).unwrap(), before);
            for (path, bytes) in [&left, &right].into_iter().zip(&pair_before) {
                assert_eq!(&std::fs::read(path).unwrap(), bytes);
            }
        }
    });
    assert_eq!(
        _native::codec_image::frame_phash_typed(&raw).unwrap(),
        Some(0)
    );
    assert_eq!(
        _native::codec_image::same_image_typed(&left, &right).unwrap(),
        Some(true)
    );
}
