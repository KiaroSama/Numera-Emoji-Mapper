//! Source-inspected renderer order and exception boundaries; these are not captured source outputs.
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::{Arc, Mutex};

struct State {
    fault: &'static str,
    events: Mutex<Vec<&'static str>>,
}
impl State {
    fn event(&self, event: &'static str) {
        self.events.lock().unwrap().push(event);
    }
}
#[pyclass]
struct Factory(Arc<State>);
#[pymethods]
impl Factory {
    #[pyo3(name = "from_data")]
    fn create_from_data(&self, py: Python<'_>, text: &str) -> PyResult<Py<Animation>> {
        assert!(text.contains("layers"));
        self.0.event("create");
        Py::new(py, Animation(self.0.clone()))
    }
}
#[pyclass]
struct Animation(Arc<State>);
#[pymethods]
impl Animation {
    #[pyo3(signature=(**options))]
    fn render_pillow_frame(
        &self,
        py: Python<'_>,
        options: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<Frame>> {
        let options = options.unwrap();
        assert_eq!(options.get_item("frame_num")?.unwrap().extract::<u32>()?, 0);
        self.0.event("render");
        if matches!(self.0.fault, "render" | "render-destroy") {
            return Err(pyo3::exceptions::PyValueError::new_err("render"));
        }
        Py::new(py, Frame(self.0.clone()))
    }
    fn lottie_animation_get_duration<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.0.event("duration");
        match self.0.fault {
            "duration-interrupt" => Err(pyo3::exceptions::PyKeyboardInterrupt::new_err("duration")),
            "duration-error" => Err(pyo3::exceptions::PyValueError::new_err("duration")),
            "short" => Ok(pyo3::types::PyFloat::new(py, 0.001).into_any()),
            // Source calls builtin float(), not the C API's numeric-only extraction.
            _ => Ok(pyo3::types::PyString::new(py, "1.0").into_any()),
        }
    }
    #[pyo3(signature=(path, **options))]
    fn save_animation(&self, path: &str, options: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        self.0.event("save animation");
        let options = options.unwrap();
        let expected = !matches!(self.0.fault, "short" | "duration-error");
        assert_eq!(options.contains("fps")?, expected);
        if expected {
            assert_eq!(options.get_item("fps")?.unwrap().extract::<u32>()?, 15);
        }
        assert!(!options.get_item("lossless")?.unwrap().extract::<bool>()?);
        if self.0.fault == "save" {
            return Err(pyo3::exceptions::PyOSError::new_err("save"));
        }
        std::fs::write(path, b"typed external animation").unwrap();
        Ok(())
    }
    fn lottie_animation_destroy(&self) -> PyResult<()> {
        self.0.event("destroy");
        if matches!(self.0.fault, "destroy" | "render-destroy") {
            Err(pyo3::exceptions::PyOSError::new_err("destroy"))
        } else {
            Ok(())
        }
    }
}
#[pyclass]
struct Frame(Arc<State>);
#[pymethods]
impl Frame {
    #[pyo3(signature=(path, **options))]
    fn save(&self, path: std::path::PathBuf, options: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        self.0.event("save frame");
        assert!(self.0.events.lock().unwrap().contains(&"destroy"));
        let options = options.unwrap();
        assert_eq!(options.get_item("quality")?.unwrap().extract::<u32>()?, 60);
        assert!(!options.get_item("lossless")?.unwrap().extract::<bool>()?);
        if self.0.fault == "save" {
            return Err(pyo3::exceptions::PyTypeError::new_err("save"));
        }
        std::fs::write(path, b"typed external frame").unwrap();
        Ok(())
    }
    fn close(&self) {
        self.0.event("close frame");
    }
}
struct Restore<'py>(Bound<'py, PyModule>, Bound<'py, PyAny>);
impl Drop for Restore<'_> {
    fn drop(&mut self) {
        self.0.setattr("LottieAnimation", &self.1).unwrap();
    }
}

#[test]
fn preview_destroys_before_still_encoding_and_preserves_duration_catch_scope() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let source = root.join("tests/fixtures/native/tgs-load/plain.json");
    let original = std::fs::read(&source).unwrap();
    assert_eq!(
        _native::tooling_manifest::digest(&source).unwrap(),
        "655c333bb0235dee13ab05ba6e7cf6f6d2921939ec9b10cb13b358701361bfbc"
    );
    let directory = root
        .join("logs/tmp")
        .join(format!("lottie lifetime {}", std::process::id()));
    std::fs::create_dir_all(directory.parent().unwrap()).unwrap();
    std::fs::create_dir(&directory).unwrap();
    Python::initialize();
    Python::attach(|py| {
        let module = py.import("rlottie_python").unwrap();
        let _restore = Restore(module.clone(), module.getattr("LottieAnimation").unwrap());
        for (still, fault, kind, expected) in [
            (
                true,
                "",
                "",
                vec!["create", "render", "destroy", "save frame", "close frame"],
            ),
            (
                true,
                "render",
                "ValueError",
                vec!["create", "render", "destroy"],
            ),
            (
                true,
                "render-destroy",
                "OSError",
                vec!["create", "render", "destroy"],
            ),
            (
                true,
                "destroy",
                "OSError",
                vec!["create", "render", "destroy", "close frame"],
            ),
            (
                true,
                "save",
                "TypeError",
                vec!["create", "render", "destroy", "save frame", "close frame"],
            ),
            (
                false,
                "",
                "",
                vec!["create", "duration", "save animation", "destroy"],
            ),
            (
                false,
                "short",
                "",
                vec!["create", "duration", "save animation", "destroy"],
            ),
            (
                false,
                "duration-error",
                "",
                vec!["create", "duration", "save animation", "destroy"],
            ),
            (
                false,
                "duration-interrupt",
                "KeyboardInterrupt",
                vec!["create", "duration", "destroy"],
            ),
            (
                false,
                "save",
                "OSError",
                vec!["create", "duration", "save animation", "destroy"],
            ),
            (
                false,
                "destroy",
                "OSError",
                vec!["create", "duration", "save animation", "destroy"],
            ),
        ] {
            let state = Arc::new(State {
                fault,
                events: Mutex::new(Vec::new()),
            });
            module
                .setattr(
                    "LottieAnimation",
                    Py::new(py, Factory(state.clone())).unwrap(),
                )
                .unwrap();
            let output = directory.join("preview.webp");
            let result =
                _native::codec_lottie_preview::preview_typed(&source, &output, 52, 15, still);
            if kind.is_empty() {
                result.unwrap();
                assert_eq!(
                    std::fs::read(&output).unwrap(),
                    if still {
                        b"typed external frame".as_slice()
                    } else {
                        b"typed external animation".as_slice()
                    }
                );
                std::fs::remove_file(output).unwrap();
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.kind, kind);
                assert_eq!(
                    error.message,
                    match fault {
                        "render-destroy" => "destroy",
                        "duration-interrupt" => "duration",
                        _ => fault,
                    }
                );
                assert_eq!(
                    error.handled,
                    !matches!(kind, "KeyboardInterrupt" | "TypeError")
                );
                assert!(!output.exists());
            }
            assert_eq!(*state.events.lock().unwrap(), expected, "{fault}/{still}");
            assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
            assert_eq!(std::fs::read(&source).unwrap(), original);
        }
    });
    std::fs::remove_dir(directory).unwrap();
}
