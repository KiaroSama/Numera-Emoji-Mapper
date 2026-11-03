//! External Pillow faults keep typed preview errors, unchanged inputs and deterministic ownership.
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use serde_json::json;
use std::sync::{Arc, Mutex};

#[path = "support/static_image_fixture.rs"]
mod static_image_fixture;
use static_image_fixture::{Factory, State};
#[pyclass]
struct AnimationResource(Arc<State>);
#[pymethods]
impl AnimationResource {
    fn close(&self) {
        self.0.event("close", "animation");
    }
    fn __enter__(slf: PyRef<'_, Self>) -> PyResult<PyRef<'_, Self>> {
        slf.0.event("enter", "animation");
        match slf.0.fault {
            "enter-os" => Err(pyo3::exceptions::PyOSError::new_err("controlled enter")),
            "enter-type" => Err(pyo3::exceptions::PyTypeError::new_err("controlled enter")),
            _ => Ok(slf),
        }
    }
    fn __exit__(
        &self,
        kind: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
        traceback: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.0.event("exit", "animation");
        if matches!(
            self.0.fault,
            "getter-type" | "frames-type" | "frames-missing" | "body-type-exit-os"
        ) {
            assert_eq!(
                kind.getattr("__name__")?.extract::<String>()?,
                if self.0.fault == "frames-missing" {
                    "AttributeError"
                } else {
                    "TypeError"
                }
            );
            assert!(!value.is_none());
        } else {
            assert!(kind.is_none() && value.is_none() && traceback.is_none());
        }
        match self.0.fault {
            "exit-os" | "body-type-exit-os" => {
                Err(pyo3::exceptions::PyOSError::new_err("controlled exit"))
            }
            "exit-type" => Err(pyo3::exceptions::PyTypeError::new_err("controlled exit")),
            "exit-interrupt" => Err(pyo3::exceptions::PyKeyboardInterrupt::new_err(
                "controlled exit",
            )),
            _ => Ok(()),
        }
    }
    fn __getattr__<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
        match (name, self.0.fault) {
            ("is_animated", "missing") | ("n_frames", "frames-missing") => {
                return Err(pyo3::exceptions::PyAttributeError::new_err(name.to_owned()));
            }
            ("is_animated", "getter-type" | "body-type-exit-os") => {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "controlled animation getter",
                ));
            }
            ("n_frames", "frames-type") => {
                return Ok(pyo3::types::PyString::new(py, "two").into_any());
            }
            _ => {}
        }
        let value = match name {
            "is_animated" => i64::from(self.0.fault != "false"),
            "n_frames" if self.0.fault == "large" => 1_i64 << 32,
            "n_frames" => 2,
            _ => return Err(pyo3::exceptions::PyAttributeError::new_err(name.to_owned())),
        };
        py.import("builtins")?.getattr("int")?.call1((value,))
    }
}
#[pyclass]
struct AnimationFactory(Arc<State>);
#[pymethods]
impl AnimationFactory {
    fn __call__(
        &self,
        py: Python<'_>,
        _path: &Bound<'_, PyAny>,
    ) -> PyResult<Py<AnimationResource>> {
        match self.0.fault {
            "open-os" => return Err(pyo3::exceptions::PyOSError::new_err("controlled open")),
            "open-value" => return Err(pyo3::exceptions::PyValueError::new_err("controlled open")),
            "open-runtime" => {
                return Err(pyo3::exceptions::PyRuntimeError::new_err("controlled open"));
            }
            "open-type" => return Err(pyo3::exceptions::PyTypeError::new_err("controlled open")),
            "open-interrupt" => {
                return Err(pyo3::exceptions::PyKeyboardInterrupt::new_err(
                    "controlled open",
                ));
            }
            _ => {}
        }
        self.0.event("create", "animation");
        Py::new(py, AnimationResource(self.0.clone()))
    }
}

fn animation_contract(py: Python<'_>, image: &Bound<'_, PyModule>, source: &std::path::Path) {
    for (fault, expected, kind) in [
        ("", Some(true), ""),
        ("large", Some(true), ""),
        ("false", Some(false), ""),
        ("missing", Some(false), ""),
        ("open-os", Some(false), ""),
        ("open-value", Some(false), ""),
        ("open-runtime", Some(false), ""),
        ("open-type", None, "TypeError"),
        ("open-interrupt", None, "KeyboardInterrupt"),
        ("getter-type", None, "TypeError"),
        ("frames-type", None, "TypeError"),
        ("frames-missing", None, "AttributeError"),
        ("enter-os", Some(false), ""),
        ("enter-type", None, "TypeError"),
        ("exit-os", Some(false), ""),
        ("exit-type", None, "TypeError"),
        ("exit-interrupt", None, "KeyboardInterrupt"),
        ("body-type-exit-os", Some(false), ""),
    ] {
        let state = Arc::new(State {
            fault,
            snapshot: false,
            events: Mutex::new(Vec::new()),
        });
        image
            .setattr(
                "open",
                Py::new(py, AnimationFactory(state.clone())).unwrap(),
            )
            .unwrap();
        let request = json!({"operation":"animation_info","source":source,"format":"static"});
        let typed = _native::codec_worker::execute_typed_at(None, &request);
        let legacy = _native::codec_worker::execute(&request);
        if let Some(expected) = expected {
            assert_eq!(typed.unwrap(), json!(expected), "{fault}");
            assert_eq!(legacy.unwrap(), json!(expected), "{fault}");
        } else {
            let error = typed.unwrap_err();
            assert_eq!(error.kind, kind, "{fault}");
            assert!(!error.handled, "{fault}");
            assert_eq!(legacy.unwrap_err(), error.message, "{fault}");
        }
        let events = state.events.lock().unwrap();
        let count = if fault.starts_with("open-") { 0 } else { 2 };
        assert_eq!(
            events
                .iter()
                .filter(|event| *event == "create animation")
                .count(),
            count,
            "{fault}"
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| *event == "close animation")
                .count(),
            count,
            "{fault}"
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| *event == "enter animation")
                .count(),
            count,
            "{fault}"
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| *event == "exit animation")
                .count(),
            if fault.starts_with("enter-") {
                0
            } else {
                count
            },
            "{fault}"
        );
    }
}

struct Restore<'py>(Vec<(Bound<'py, PyModule>, &'static str, Bound<'py, PyAny>)>);
impl Drop for Restore<'_> {
    fn drop(&mut self) {
        for (module, name, value) in &self.0 {
            module
                .setattr(*name, value)
                .expect("restore external codec API");
        }
    }
}
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

fn metadata_order(py: Python<'_>, image: &Bound<'_, PyModule>, source: &std::path::Path) {
    for (operation, fault, expected, order) in [
        (
            "dimensions",
            "mode",
            Some(json!([16, 12])),
            vec!["load image", "size image"],
        ),
        (
            "image_info",
            "mode",
            None,
            vec!["load image", "size image", "mode image"],
        ),
        ("dimensions", "load", None, vec!["load image"]),
        ("usable", "load", Some(json!(true)), vec!["convert image"]),
        ("usable", "mode", Some(json!(true)), vec!["convert image"]),
    ] {
        let state = Arc::new(State {
            fault,
            snapshot: false,
            events: Mutex::new(Vec::new()),
        });
        image
            .setattr(
                "open",
                Py::new(
                    py,
                    Factory {
                        role: "image",
                        state: state.clone(),
                    },
                )
                .unwrap(),
            )
            .unwrap();
        let result = _native::codec_worker::execute_typed_at(
            None,
            &json!({"operation":operation,"source":source,"format":"static"}),
        );
        if let Some(expected) = expected {
            assert_eq!(result.unwrap(), expected);
        } else {
            let error = result.unwrap_err();
            assert_eq!(
                error.kind,
                if fault == "mode" {
                    "TypeError"
                } else {
                    "OSError"
                }
            );
            assert_eq!(error.message, fault);
        }
        let events = state.events.lock().unwrap();
        let observed = events
            .iter()
            .filter(|event| {
                ["load image", "size image", "mode image", "convert image"]
                    .contains(&event.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(observed, order);
        assert_eq!(
            events.iter().filter(|event| *event == "exit image").count(),
            1
        );
    }
}

fn static_context_errors(
    py: Python<'_>,
    image: &Bound<'_, PyModule>,
    io: &Bound<'_, PyModule>,
    source: &std::path::Path,
) {
    for operation in [
        "image_info",
        "dimensions",
        "preview",
        "gallery_thumb",
        "prepare",
    ] {
        for fault in [
            "enter",
            "exit-os",
            "exit-type",
            "exit-interrupt",
            "exit-system",
            "convert-exit",
        ] {
            if fault == "convert-exit" && matches!(operation, "image_info" | "dimensions") {
                continue;
            }
            let state = Arc::new(State {
                fault,
                snapshot: operation == "prepare",
                events: Mutex::new(Vec::new()),
            });
            image
                .setattr(
                    "open",
                    Py::new(
                        py,
                        Factory {
                            role: "image",
                            state: state.clone(),
                        },
                    )
                    .unwrap(),
                )
                .unwrap();
            io.setattr(
                "BytesIO",
                Py::new(
                    py,
                    Factory {
                        role: "buffer",
                        state: state.clone(),
                    },
                )
                .unwrap(),
            )
            .unwrap();
            let output = source.parent().unwrap().join("context output.webp");
            let request = json!({"operation":operation,"source":source,"format":"static",
                "output":output,"key":"fixture","size":52,"reencode":true,"tint":"bad"});
            let mut warnings = Vec::new();
            let error = if operation == "prepare" {
                _native::codec_static_prepare::prepare(source, &request, &mut warnings).unwrap_err()
            } else {
                _native::codec_worker::execute_typed_at(None, &request).unwrap_err()
            };
            let (kind, message) = match fault {
                "enter" => ("OSError", "enter"),
                "exit-type" => ("TypeError", "controlled exit"),
                "exit-interrupt" => ("KeyboardInterrupt", "controlled exit"),
                "exit-system" => ("SystemExit", "controlled exit"),
                _ => ("OSError", "controlled exit"),
            };
            if operation == "prepare" && matches!(kind, "OSError" | "TypeError") {
                assert_eq!(error.kind, "MediaError");
                assert_eq!(error.message, "tint must be #RRGGBB, got 'bad'");
                assert_eq!(
                    warnings,
                    [format!(
                        "re-encode skipped for context.png (static): {message}"
                    )]
                );
            } else {
                assert_eq!(error.kind, kind, "{operation}/{fault}");
                assert_eq!(error.message, message);
                assert_eq!(error.handled, kind == "OSError");
                assert!(warnings.is_empty());
            }
            let events = state.events.lock().unwrap();
            assert_eq!(
                events
                    .iter()
                    .filter(|event| *event == "enter image")
                    .count(),
                1
            );
            assert_eq!(
                events.iter().filter(|event| *event == "exit image").count(),
                usize::from(fault != "enter")
            );
            assert_eq!(
                events
                    .iter()
                    .filter(|event| *event == "close image")
                    .count(),
                1
            );
            assert!(!output.exists());
        }
    }
}

#[test]
fn static_preview_and_reencode_close_all_external_objects_after_codec_faults() {
    let scratch = tooling_fixture::Root::new("static lifetime");
    Python::initialize();
    Python::attach(|py| {
        let image = py.import("PIL.Image").unwrap();
        let io = py.import("io").unwrap();
        let _restore = Restore(vec![
            (image.clone(), "open", image.getattr("open").unwrap()),
            (io.clone(), "BytesIO", io.getattr("BytesIO").unwrap()),
        ]);
        for operation in ["preview", "reencode"] {
            for fault in ["", "open", "convert", "thumbnail", "save", "getvalue"] {
                if operation == "reencode" && fault == "thumbnail" {
                    continue;
                }
                let state = Arc::new(State {
                    fault,
                    snapshot: operation == "reencode",
                    events: Mutex::new(Vec::new()),
                });
                image
                    .setattr(
                        "open",
                        Py::new(
                            py,
                            Factory {
                                role: "image",
                                state: state.clone(),
                            },
                        )
                        .unwrap(),
                    )
                    .unwrap();
                io.setattr(
                    "BytesIO",
                    Py::new(
                        py,
                        Factory {
                            role: "buffer",
                            state: state.clone(),
                        },
                    )
                    .unwrap(),
                )
                .unwrap();
                let source = scratch.0.join(format!("{operation}-{fault}.png"));
                let output = scratch.0.join(format!("{operation}-{fault}.webp"));
                let original = b"immutable fixture input";
                std::fs::write(&source, original).unwrap();
                if operation == "preview" {
                    let request = json!({"operation":"preview","source":source,"format":"static","key":"fixture","size":52,"output":output});
                    let result = _native::codec_worker::execute_typed_at(None, &request);
                    if fault.is_empty() {
                        assert_eq!(result.unwrap(), json!({"bytes":24}));
                        assert_eq!(std::fs::read(&output).unwrap(), b"external encoded fixture");
                    } else {
                        let error = result.unwrap_err();
                        let kind = match fault {
                            "convert" | "getvalue" => "ValueError",
                            "thumbnail" => "TypeError",
                            _ => "OSError",
                        };
                        assert_eq!(error.kind, kind, "{fault}");
                        assert_eq!(error.message, fault);
                        assert_eq!(error.handled, fault != "thumbnail");
                        assert!(!output.exists());
                    }
                    assert_eq!(std::fs::read(&source).unwrap(), original);
                } else {
                    assert_eq!(
                        _native::codec_image::reencode_static(&source).unwrap(),
                        fault.is_empty()
                    );
                    assert_eq!(
                        std::fs::read(&source).unwrap(),
                        if fault.is_empty() {
                            b"external encoded fixture".as_slice()
                        } else {
                            original.as_slice()
                        }
                    );
                }
                let events = state.events.lock().unwrap();
                for role in ["image", "rgba", "buffer"] {
                    let created = events
                        .iter()
                        .filter(|event| **event == format!("create {role}"))
                        .count();
                    let closed = events
                        .iter()
                        .filter(|event| **event == format!("close {role}"))
                        .count();
                    assert_eq!(closed, created, "{operation}/{fault}/{role}: {events:?}");
                    if role == "image" {
                        assert_eq!(created, usize::from(fault != "open"));
                    }
                }
            }
        }
        let source = scratch.0.join("context.png");
        let original = b"immutable context input";
        std::fs::write(&source, original).unwrap();
        static_context_errors(py, &image, &io, &source);
        metadata_order(py, &image, &source);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        let source = scratch.0.join("animation.png");
        let original = b"immutable animation input";
        std::fs::write(&source, original).unwrap();
        animation_contract(py, &image, &source);
        assert_eq!(std::fs::read(&source).unwrap(), original);
    });
}
