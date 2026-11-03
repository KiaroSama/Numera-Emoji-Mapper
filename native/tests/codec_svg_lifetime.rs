//! Typed third-party doubles exercise cleanup faults without embedded first-party Python.
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::{Arc, Mutex};

#[pyclass]
struct Resource {
    name: &'static str,
    events: Arc<Mutex<Vec<String>>>,
    fail_convert: Option<&'static str>,
    fail_close: bool,
}
#[pymethods]
impl Resource {
    fn close(&self) -> PyResult<()> {
        self.events
            .lock()
            .unwrap()
            .push(format!("close {}", self.name));
        if self.fail_close {
            Err(pyo3::exceptions::PyOSError::new_err(
                "controlled close failure",
            ))
        } else {
            Ok(())
        }
    }
    fn convert(&self, py: Python<'_>, _mode: &str) -> PyResult<Py<Resource>> {
        if let Some(kind) = self.fail_convert {
            Err(fault(kind, "controlled convert failure"))
        } else {
            Py::new(
                py,
                Resource {
                    name: "rgba",
                    events: self.events.clone(),
                    fail_convert: None,
                    fail_close: false,
                },
            )
        }
    }
}
#[pyclass]
struct Factory {
    events: Arc<Mutex<Vec<String>>>,
    fail_open: bool,
    fail_convert: Option<&'static str>,
    fail_close: bool,
}
#[pymethods]
impl Factory {
    #[pyo3(signature=(*args, **_kwargs))]
    fn __call__(
        &self,
        py: Python<'_>,
        args: &Bound<'_, pyo3::types::PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<Resource>> {
        assert_eq!(args.len(), 1);
        let name = "image";
        if self.fail_open {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "controlled open failure",
            ));
        }
        Py::new(
            py,
            Resource {
                name,
                events: self.events.clone(),
                fail_convert: self.fail_convert,
                fail_close: self.fail_close,
            },
        )
    }
}
#[pyclass]
struct StreamFactory(Arc<Mutex<Vec<String>>>);
#[pymethods]
impl StreamFactory {
    fn __call__(&self, py: Python<'_>, _data: &Bound<'_, PyAny>) -> PyResult<Py<Resource>> {
        Py::new(
            py,
            Resource {
                name: "stream",
                events: self.0.clone(),
                fail_convert: None,
                fail_close: false,
            },
        )
    }
}
#[pyfunction]
#[pyo3(signature=(*_args, **_kwargs))]
fn render(_args: &Bound<'_, pyo3::types::PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) -> Vec<u8> {
    Vec::new()
}

#[pyclass]
struct Rendered {
    kind: Option<&'static str>,
    events: Arc<Mutex<Vec<String>>>,
}
#[pymethods]
impl Rendered {
    fn __bytes__(&self, py: Python<'_>) -> PyResult<Py<pyo3::types::PyBytes>> {
        self.events.lock().unwrap().push("bytes".into());
        if let Some(kind) = self.kind {
            return Err(fault(kind, "controlled bytes failure"));
        }
        Ok(pyo3::types::PyBytes::new(py, b"external PNG bytes").unbind())
    }
}

fn fault(kind: &str, message: &str) -> PyErr {
    match kind {
        "OSError" => pyo3::exceptions::PyOSError::new_err(message.to_owned()),
        "ValueError" => pyo3::exceptions::PyValueError::new_err(message.to_owned()),
        "TypeError" => pyo3::exceptions::PyTypeError::new_err(message.to_owned()),
        "KeyboardInterrupt" => pyo3::exceptions::PyKeyboardInterrupt::new_err(message.to_owned()),
        _ => pyo3::exceptions::PySystemExit::new_err(message.to_owned()),
    }
}

#[test]
fn svg_closes_stream_and_image_independently_on_codec_errors() {
    Python::initialize();
    Python::attach(|py| {
        let io = py.import("io").unwrap();
        let image = py.import("PIL.Image").unwrap();
        let renderer = py.import("resvg_py").unwrap();
        let original = [
            (io.clone(), "BytesIO", io.getattr("BytesIO").unwrap()),
            (image.clone(), "open", image.getattr("open").unwrap()),
            (
                renderer.clone(),
                "svg_to_bytes",
                renderer.getattr("svg_to_bytes").unwrap(),
            ),
        ];
        struct Restore<'py>(
            Vec<(
                Bound<'py, pyo3::types::PyModule>,
                &'static str,
                Bound<'py, PyAny>,
            )>,
        );
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                for (module, name, value) in &self.0 {
                    module
                        .setattr(*name, value)
                        .expect("restore typed external API");
                }
            }
        }
        let _restore = Restore(original.into());
        renderer
            .setattr("svg_to_bytes", wrap_pyfunction!(render, py).unwrap())
            .unwrap();
        for (fail_open, fail_convert, fail_close, wanted) in [
            (true, false, false, "controlled open failure"),
            (false, true, false, "controlled convert failure"),
            (false, true, true, "controlled convert failure"),
            (false, false, true, "controlled close failure"),
            (false, false, false, ""),
        ] {
            let events = Arc::new(Mutex::new(Vec::new()));
            io.setattr(
                "BytesIO",
                Py::new(py, StreamFactory(events.clone())).unwrap(),
            )
            .unwrap();
            image
                .setattr(
                    "open",
                    Py::new(
                        py,
                        Factory {
                            events: events.clone(),
                            fail_open,
                            fail_convert: fail_convert.then_some("ValueError"),
                            fail_close,
                        },
                    )
                    .unwrap(),
                )
                .unwrap();
            let result = _native::codec_svg::load_rgba(py, std::path::Path::new("fixture.svg"));
            if wanted.is_empty() {
                result.unwrap().call_method0("close").unwrap();
            } else {
                assert_eq!(result.unwrap_err().value(py).to_string(), wanted);
            }
            let typed =
                _native::codec_svg::load_rgba_typed(py, std::path::Path::new("fixture.svg"));
            if wanted.is_empty() {
                typed.unwrap().call_method0("close").unwrap();
            } else {
                let error = typed.unwrap_err();
                assert_eq!(error.message, wanted);
                assert_eq!(
                    error.kind,
                    if fail_close && !fail_convert {
                        "OSError"
                    } else {
                        "ValueError"
                    }
                );
                assert!(error.handled);
            }
            let observed = events.lock().unwrap();
            assert_eq!(
                observed
                    .iter()
                    .filter(|event| *event == "close stream")
                    .count(),
                2
            );
            assert_eq!(
                observed
                    .iter()
                    .filter(|event| *event == "close image")
                    .count(),
                2 * usize::from(!fail_open)
            );
            assert_eq!(
                observed
                    .iter()
                    .filter(|event| *event == "close rgba")
                    .count(),
                2 * usize::from(!fail_open && !fail_convert)
            );
        }
        for kind in [
            "OSError",
            "ValueError",
            "TypeError",
            "KeyboardInterrupt",
            "SystemExit",
        ] {
            let decode_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            for (module, name) in [(&io, "BytesIO"), (&image, "open")] {
                let hits = decode_calls.clone();
                let refused = pyo3::types::PyCFunction::new_closure(
                    py,
                    None,
                    None,
                    move |_, _| -> PyResult<()> {
                        hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        Err(pyo3::exceptions::PyRuntimeError::new_err(
                            "decode must not follow renderer failure",
                        ))
                    },
                )
                .unwrap();
                module.setattr(name, refused).unwrap();
            }
            let replacement = pyo3::types::PyCFunction::new_closure(
                py,
                None,
                None,
                move |_, _| -> PyResult<()> {
                    Err(match kind {
                        "OSError" => {
                            pyo3::exceptions::PyOSError::new_err("controlled render failure")
                        }
                        "ValueError" => {
                            pyo3::exceptions::PyValueError::new_err("controlled render failure")
                        }
                        "TypeError" => {
                            pyo3::exceptions::PyTypeError::new_err("controlled render failure")
                        }
                        "KeyboardInterrupt" => pyo3::exceptions::PyKeyboardInterrupt::new_err(
                            "controlled render failure",
                        ),
                        _ => pyo3::exceptions::PySystemExit::new_err("controlled render failure"),
                    })
                },
            )
            .unwrap();
            renderer.setattr("svg_to_bytes", replacement).unwrap();
            let error =
                _native::codec_svg::load_rgba_typed(py, std::path::Path::new("fixture.svg"))
                    .unwrap_err();
            if matches!(kind, "KeyboardInterrupt" | "SystemExit") {
                assert_eq!(error.kind, kind);
                assert_eq!(error.message, "controlled render failure");
                assert!(!error.handled);
            } else {
                assert_eq!(error.kind, "MediaError");
                assert_eq!(error.message, "failed to render SVG: fixture.svg");
                assert!(error.handled);
            }
            let legacy =
                _native::codec_svg::load_rgba(py, std::path::Path::new("fixture.svg")).unwrap_err();
            assert_eq!(legacy.get_type(py).name().unwrap(), kind);
            assert_eq!(legacy.value(py).to_string(), "controlled render failure");
            assert_eq!(
                decode_calls.load(std::sync::atomic::Ordering::SeqCst),
                0,
                "renderer failure must not allocate decode resources"
            );
        }
        renderer
            .setattr("svg_to_bytes", wrap_pyfunction!(render, py).unwrap())
            .unwrap();
        for phase in ["BytesIO", "Pillowopen", "convert"] {
            for kind in ["OSError", "TypeError", "KeyboardInterrupt", "SystemExit"] {
                let events = Arc::new(Mutex::new(Vec::new()));
                if phase == "BytesIO" {
                    let refused = pyo3::types::PyCFunction::new_closure(
                        py,
                        None,
                        None,
                        move |_, _| -> PyResult<()> {
                            Err(fault(kind, "controlled stream failure"))
                        },
                    )
                    .unwrap();
                    io.setattr("BytesIO", refused).unwrap();
                } else {
                    io.setattr(
                        "BytesIO",
                        Py::new(py, StreamFactory(events.clone())).unwrap(),
                    )
                    .unwrap();
                    if phase == "Pillowopen" {
                        let refused = pyo3::types::PyCFunction::new_closure(
                            py,
                            None,
                            None,
                            move |_, _| -> PyResult<()> {
                                Err(fault(kind, "controlled Pillow open failure"))
                            },
                        )
                        .unwrap();
                        image.setattr("open", refused).unwrap();
                    } else {
                        image
                            .setattr(
                                "open",
                                Py::new(
                                    py,
                                    Factory {
                                        events: events.clone(),
                                        fail_open: false,
                                        fail_convert: Some(kind),
                                        fail_close: false,
                                    },
                                )
                                .unwrap(),
                            )
                            .unwrap();
                    }
                }
                let error =
                    _native::codec_svg::load_rgba_typed(py, std::path::Path::new("fixture.svg"))
                        .unwrap_err();
                assert_eq!(error.kind, kind);
                assert_eq!(
                    error.message,
                    match phase {
                        "BytesIO" => "controlled stream failure",
                        "Pillowopen" => "controlled Pillow open failure",
                        _ => "controlled convert failure",
                    }
                );
                assert_eq!(error.handled, kind == "OSError");
                assert_eq!(
                    *events.lock().unwrap(),
                    match phase {
                        "BytesIO" => Vec::<String>::new(),
                        "Pillowopen" => vec!["close stream".into()],
                        _ => vec!["close image".into(), "close stream".into()],
                    }
                );
            }
        }
        let modules = py.import("sys").unwrap().getattr("modules").unwrap();
        let original = modules.get_item("resvg_py").unwrap();
        modules.set_item("resvg_py", py.None()).unwrap();
        let imported = _native::codec_svg::load_rgba_typed(py, std::path::Path::new("fixture.svg"));
        modules.set_item("resvg_py", original).unwrap();
        let error = imported.unwrap_err();
        assert_eq!(error.kind, "ModuleNotFoundError");
        assert!(!error.handled);
        for kind in [
            None,
            Some("TypeError"),
            Some("KeyboardInterrupt"),
            Some("SystemExit"),
        ] {
            let events = Arc::new(Mutex::new(Vec::new()));
            let observed = events.clone();
            let replacement = pyo3::types::PyCFunction::new_closure(
                py,
                None,
                None,
                move |args, kwargs| -> PyResult<Py<Rendered>> {
                    assert!(args.is_empty());
                    let kwargs = kwargs.unwrap();
                    assert_eq!(kwargs.len(), 2);
                    assert_eq!(
                        kwargs.get_item("svg_path")?.unwrap().extract::<String>()?,
                        "fixture.svg"
                    );
                    assert_eq!(kwargs.get_item("width")?.unwrap().extract::<u32>()?, 256);
                    Py::new(
                        args.py(),
                        Rendered {
                            kind,
                            events: observed.clone(),
                        },
                    )
                },
            )
            .unwrap();
            renderer.setattr("svg_to_bytes", replacement).unwrap();
            io.setattr(
                "BytesIO",
                Py::new(py, StreamFactory(events.clone())).unwrap(),
            )
            .unwrap();
            image
                .setattr(
                    "open",
                    Py::new(
                        py,
                        Factory {
                            events: events.clone(),
                            fail_open: false,
                            fail_convert: None,
                            fail_close: false,
                        },
                    )
                    .unwrap(),
                )
                .unwrap();
            let result =
                _native::codec_svg::load_rgba_typed(py, std::path::Path::new("fixture.svg"));
            if let Some(kind) = kind {
                let error = result.unwrap_err();
                assert_eq!(error.kind, kind);
                assert_eq!(error.message, "controlled bytes failure");
                assert!(!error.handled);
                assert_eq!(*events.lock().unwrap(), ["bytes"]);
            } else {
                result.unwrap().call_method0("close").unwrap();
                assert_eq!(
                    *events.lock().unwrap(),
                    ["bytes", "close image", "close stream", "close rgba"]
                );
            }
        }
    });
}
