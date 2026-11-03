//! Shared external Pillow resources keep ownership, context and alpha-call observations in one fixture.
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyTuple};
use std::sync::{Arc, Mutex};

pub struct State {
    pub fault: &'static str,
    pub snapshot: bool,
    pub events: Mutex<Vec<String>>,
}
impl State {
    pub fn event(&self, action: &str, role: &str) {
        self.events.lock().unwrap().push(format!("{action} {role}"));
    }
    pub fn check(&self, action: &str) -> PyResult<()> {
        if self.fault == action || (self.fault == "convert-exit" && action == "convert") {
            Err(match action {
                "convert" | "getvalue" => {
                    pyo3::exceptions::PyValueError::new_err(action.to_owned())
                }
                "thumbnail" => pyo3::exceptions::PyTypeError::new_err(action.to_owned()),
                _ => pyo3::exceptions::PyOSError::new_err(action.to_owned()),
            })
        } else {
            Ok(())
        }
    }
}
#[pyclass]
pub struct Resource {
    #[pyo3(get)]
    pub role: &'static str,
    pub state: Arc<State>,
}
#[pymethods]
impl Resource {
    fn __enter__(slf: PyRef<'_, Self>) -> PyResult<PyRef<'_, Self>> {
        slf.state.event("enter", slf.role);
        slf.state.check("enter")?;
        Ok(slf)
    }
    fn __exit__(
        &self,
        kind: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
        _traceback: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.state.event("exit", self.role);
        let events = self.state.events.lock().unwrap();
        let body_failed = matches!(self.state.fault, "convert" | "thumbnail" | "convert-exit")
            || (self.state.fault == "load" && events.iter().any(|event| event == "load image"))
            || (self.state.fault == "mode" && events.iter().any(|event| event == "mode image"));
        drop(events);
        if body_failed {
            assert!(!kind.is_none() && !value.is_none());
        } else {
            assert!(kind.is_none() && value.is_none());
        }
        match self.state.fault {
            "exit-os" | "convert-exit" => {
                Err(pyo3::exceptions::PyOSError::new_err("controlled exit"))
            }
            "exit-type" => Err(pyo3::exceptions::PyTypeError::new_err("controlled exit")),
            "exit-interrupt" => Err(pyo3::exceptions::PyKeyboardInterrupt::new_err(
                "controlled exit",
            )),
            "exit-system" => Err(pyo3::exceptions::PySystemExit::new_err("controlled exit")),
            _ => Ok(()),
        }
    }
    fn load(&self) -> PyResult<()> {
        self.state.event("load", self.role);
        self.state.check("load")
    }
    #[getter]
    fn size(&self) -> (u32, u32) {
        self.state.event("size", self.role);
        (16, 12)
    }
    #[getter]
    fn mode(&self) -> PyResult<&'static str> {
        self.state.event("mode", self.role);
        if self.state.fault == "mode" {
            return Err(pyo3::exceptions::PyTypeError::new_err("mode"));
        }
        Ok("RGBA")
    }
    fn close(&self) {
        self.state.event("close", self.role);
    }
    fn convert(&self, py: Python<'_>, mode: &str) -> PyResult<Py<Resource>> {
        assert_eq!(mode, "RGBA");
        self.state.event("convert", self.role);
        self.state.check("convert")?;
        self.state.event("create", "rgba");
        Py::new(
            py,
            Self {
                role: "rgba",
                state: self.state.clone(),
            },
        )
    }
    fn split<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        self.state.event("split", self.role);
        let bands = (0..4)
            .map(|index| {
                let role = if index == 3 { "alpha" } else { "channel" };
                self.state.event("create", role);
                Py::new(
                    py,
                    Self {
                        role,
                        state: self.state.clone(),
                    },
                )
            })
            .collect::<PyResult<Vec<_>>>()?;
        PyTuple::new(py, bands)
    }
    fn getbbox(&self) -> (u32, u32, u32, u32) {
        self.state.event("getbbox", self.role);
        (0, 0, 16, 12)
    }
    fn histogram(&self) -> Vec<u64> {
        self.state.event("histogram", self.role);
        let mut bins = vec![0; 256];
        bins[255] = 16 * 12;
        bins
    }
    #[pyo3(signature=(*_args, **_kwargs))]
    fn thumbnail(
        &self,
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        self.state.check("thumbnail")
    }
    #[pyo3(signature=(*_args, **kwargs))]
    fn save(&self, _args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        let kwargs = kwargs.expect("explicit encoding options");
        let gallery = if let Some(lossless) = kwargs.get_item("lossless")? {
            assert!(lossless.extract::<bool>()?);
            false
        } else {
            assert_eq!(kwargs.get_item("quality")?.unwrap().extract::<u32>()?, 65);
            true
        };
        assert!(kwargs.get_item("exact")?.unwrap().extract::<bool>()?);
        let events = self.state.events.lock().unwrap();
        assert_eq!(
            events.iter().any(|event| event == "close image"),
            !gallery,
            "only gallery encoding stays inside the source context"
        );
        drop(events);
        self.state.check("save")
    }
    fn getvalue<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.state.check("getvalue")?;
        Ok(PyBytes::new(py, b"external encoded fixture"))
    }
    fn tobytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &[255; 16 * 12 * 4])
    }
}
#[pyclass]
pub struct Factory {
    pub role: &'static str,
    pub state: Arc<State>,
}
#[pymethods]
impl Factory {
    #[pyo3(signature=(*_args, **_kwargs))]
    fn __call__(
        &self,
        py: Python<'_>,
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<Resource>> {
        if self.role == "image" {
            self.state.check("open")?;
            let input = _args.get_item(0)?;
            assert_eq!(!input.hasattr("__fspath__")?, self.state.snapshot);
            if self.state.snapshot {
                assert_eq!(input.getattr("role")?.extract::<String>()?, "buffer");
            }
        }
        self.state.event("create", self.role);
        Py::new(
            py,
            Resource {
                role: self.role,
                state: self.state.clone(),
            },
        )
    }
}
