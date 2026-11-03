//! Public conversion/provider faults exercise the pinned source fit, never a replacement encoder.
use pyo3::prelude::*;
use pyo3::types::{PyCFunction, PyTuple};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Fault {
    Mode,
    Convert,
    Split,
    Index,
    Bbox,
    FalseBounds,
    Truth,
    Crop,
    Size,
    Zero,
    ResizeLookup,
    Lanczos,
    Canvas,
    Paste,
    Resize,
}
struct State {
    fault: Fault,
    provider: bool,
    events: Mutex<Vec<String>>,
}
impl State {
    fn event(&self, action: &str, role: &str) {
        self.events.lock().unwrap().push(format!("{action} {role}"));
    }
    fn image(self: &Arc<Self>, py: Python<'_>, role: &'static str) -> PyResult<Py<Raster>> {
        self.event("create", role);
        Py::new(
            py,
            Raster {
                state: self.clone(),
                role,
            },
        )
    }
    fn error(&self, stage: &str) -> PyErr {
        let message = format!("controlled fit {stage}");
        match stage {
            "split" | "new" => pyo3::exceptions::PyOSError::new_err(message),
            "convert" if !self.provider => pyo3::exceptions::PyValueError::new_err(message),
            "bbox" | "legacy crop" | "zero canvas" => {
                pyo3::exceptions::PyValueError::new_err(message)
            }
            "truth" | "resize lookup" | "convert" => {
                pyo3::exceptions::PyKeyboardInterrupt::new_err(message)
            }
            "crop" | "lanczos" | "close" => pyo3::exceptions::PySystemExit::new_err(message),
            _ => pyo3::exceptions::PyTypeError::new_err(message),
        }
    }
}
#[pyclass]
struct Raster {
    state: Arc<State>,
    role: &'static str,
}
#[pymethods]
impl Raster {
    fn convert(&self, py: Python<'_>, mode: &str) -> PyResult<Py<Raster>> {
        self.state.event("convert", self.role);
        if mode != "RGBA" {
            return Err(pyo3::exceptions::PyAssertionError::new_err(
                "conversion mode",
            ));
        }
        if self.role == "opened" {
            return self.state.image(py, "rgba");
        }
        if self.state.provider && self.role == "rgba" {
            let calls = self
                .state
                .events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| *e == "convert rgba")
                .count();
            if calls == 1 {
                return self.state.image(py, "blank");
            }
        }
        if self.state.fault == Fault::Convert {
            return Err(self.state.error("convert"));
        }
        self.state.image(py, "normalized")
    }
    #[getter]
    fn mode(&self) -> PyResult<&'static str> {
        self.state.event("mode", self.role);
        if self.state.fault == Fault::Mode {
            return Err(self.state.error("mode"));
        }
        Ok(if self.state.fault == Fault::Convert {
            "RGB"
        } else {
            "RGBA"
        })
    }
    fn split<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        self.state.event("split", self.role);
        if self.role != "blank" && self.state.fault == Fault::Split {
            return Err(self.state.error("split"));
        }
        let blank = self.role == "blank";
        let mut bands = (0..3)
            .map(|_| {
                self.state
                    .image(py, if blank { "blank channel" } else { "channel" })
            })
            .collect::<PyResult<Vec<_>>>()?;
        if self.state.fault != Fault::Index || blank {
            bands.push(
                self.state
                    .image(py, if blank { "blank alpha" } else { "alpha" })?,
            );
        }
        PyTuple::new(py, bands)
    }
    fn getchannel(&self, py: Python<'_>, channel: &str) -> PyResult<Py<Raster>> {
        self.state.event("getchannel", self.role);
        if channel != "A" {
            return Err(pyo3::exceptions::PyAssertionError::new_err("alpha channel"));
        }
        self.state.image(py, "alpha")
    }
    fn getbbox<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.state.event("getbbox", self.role);
        if self.role != "blank alpha" {
            match self.state.fault {
                Fault::Bbox => return Err(self.state.error("bbox")),
                Fault::FalseBounds => return Ok(false.into_pyobject(py)?.to_owned().into_any()),
                Fault::Truth => {
                    return Ok(Py::new(py, Truth(self.state.clone()))?
                        .into_bound(py)
                        .into_any());
                }
                Fault::Zero => return Ok(py.None().into_bound(py)),
                _ => {}
            }
        }
        Ok((0, 0, 20, 10).into_pyobject(py)?.into_any())
    }
    fn histogram(&self) -> Vec<u64> {
        self.state.event("histogram", self.role);
        let mut bins = vec![0; 256];
        bins[255] = 200;
        bins
    }
    fn crop(&self, py: Python<'_>, _bbox: &Bound<'_, PyAny>) -> PyResult<Py<Raster>> {
        self.state.event("crop", self.role);
        match self.state.fault {
            Fault::Crop => Err(self.state.error("crop")),
            Fault::FalseBounds => Err(self.state.error("legacy crop")),
            _ => self.state.image(py, "cropped"),
        }
    }
    #[getter]
    fn size(&self) -> PyResult<(u32, u32)> {
        self.state.event("size", self.role);
        if self.state.fault == Fault::Size {
            return Err(self.state.error("size"));
        }
        Ok(if self.state.fault == Fault::Zero {
            (0, 10)
        } else {
            (20, 10)
        })
    }
    #[getter]
    fn resize(&self, py: Python<'_>) -> PyResult<Py<Resize>> {
        self.state.event("resize lookup", self.role);
        if self.state.fault == Fault::ResizeLookup {
            return Err(self.state.error("resize lookup"));
        }
        Py::new(
            py,
            Resize {
                state: self.state.clone(),
                role: self.role,
            },
        )
    }
    fn paste(&self, image: &Bound<'_, PyAny>, position: (u32, u32)) -> PyResult<()> {
        self.state.event("paste", self.role);
        if image.extract::<PyRef<'_, Raster>>()?.role != "resized" || position != (0, 25) {
            return Err(pyo3::exceptions::PyAssertionError::new_err(
                "unmasked centered paste",
            ));
        }
        Err(self.state.error("paste"))
    }
    fn close(&self) -> PyResult<()> {
        self.state.event("close", self.role);
        if self.state.fault == Fault::Paste {
            return Err(self.state.error("close"));
        }
        Ok(())
    }
}
#[pyclass]
struct Truth(Arc<State>);
#[pymethods]
impl Truth {
    fn __bool__(&self) -> PyResult<bool> {
        self.0.event("truth", "bbox");
        Err(self.0.error("truth"))
    }
}
#[pyclass]
struct Resize {
    state: Arc<State>,
    role: &'static str,
}
#[pymethods]
impl Resize {
    fn __call__(&self, py: Python<'_>, size: (u32, u32), filter: u32) -> PyResult<Py<Raster>> {
        self.state.event("resize", self.role);
        if size != (100, 50) || filter != 1 {
            return Err(pyo3::exceptions::PyAssertionError::new_err(
                "LANCZOS fit size",
            ));
        }
        if matches!(self.state.fault, Fault::FalseBounds | Fault::Resize) {
            return Err(self.state.error("resize"));
        }
        self.state.image(py, "resized")
    }
}
struct Restore<'py> {
    module: Bound<'py, PyModule>,
    slots: Vec<(&'static str, Option<Bound<'py, PyAny>>)>,
}
impl Drop for Restore<'_> {
    fn drop(&mut self) {
        for (name, value) in self.slots.iter().rev() {
            if let Some(value) = value {
                self.module
                    .setattr(*name, value)
                    .expect("restore fitting API");
            } else {
                self.module.delattr(*name).expect("remove fitting API");
            }
        }
    }
}
fn observe<T>(
    py: Python<'_>,
    fault: Fault,
    provider: bool,
    body: impl FnOnce() -> T,
) -> (T, Vec<String>) {
    let module = py.import("PIL.Image").unwrap();
    let slots = ["open", "new", "__getattr__", "LANCZOS"]
        .into_iter()
        .map(|name| (name, module.getattr_opt(name).unwrap()))
        .collect();
    let restore = Restore {
        module: module.clone(),
        slots,
    };
    let restored = restore
        .slots
        .iter()
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    let state = Arc::new(State {
        fault,
        provider,
        events: Mutex::new(Vec::new()),
    });
    let open_state = state.clone();
    module
        .setattr(
            "open",
            PyCFunction::new_closure(py, None, None, move |args, _| {
                open_state.image(args.py(), "opened")
            })
            .unwrap(),
        )
        .unwrap();
    let new_state = state.clone();
    module
        .setattr(
            "new",
            PyCFunction::new_closure(
                py,
                None,
                None,
                move |args, kwargs| -> PyResult<Py<Raster>> {
                    new_state.event("new", "canvas");
                    let (mode, size, color) =
                        args.extract::<(String, (u32, u32), (u8, u8, u8, u8))>()?;
                    if mode != "RGBA"
                        || size != (100, 100)
                        || color != (0, 0, 0, 0)
                        || kwargs.is_some_and(|kw| !kw.is_empty())
                    {
                        return Err(pyo3::exceptions::PyAssertionError::new_err(
                            "transparent canvas",
                        ));
                    }
                    if fault == Fault::Zero {
                        return Err(new_state.error("zero canvas"));
                    }
                    if fault == Fault::Canvas {
                        return Err(new_state.error("new"));
                    }
                    new_state.image(args.py(), "canvas")
                },
            )
            .unwrap(),
        )
        .unwrap();
    // The source reads the module alias only after resolving the image's resize descriptor.
    module.delattr("LANCZOS").unwrap();
    let alias_state = state.clone();
    module
        .setattr(
            "__getattr__",
            PyCFunction::new_closure(py, None, None, move |args, _| -> PyResult<u32> {
                let name: String = args.get_item(0)?.extract()?;
                if name != "LANCZOS" {
                    return Err(pyo3::exceptions::PyAttributeError::new_err(name));
                }
                alias_state.event("LANCZOS", "alias");
                if matches!(fault, Fault::Lanczos | Fault::ResizeLookup) {
                    return Err(alias_state.error("lanczos"));
                }
                Ok(1)
            })
            .unwrap(),
        )
        .unwrap();
    let result = body();
    drop(restore);
    for (name, original) in ["open", "new", "__getattr__", "LANCZOS"]
        .into_iter()
        .zip(restored.iter())
    {
        let actual = module.getattr_opt(name).unwrap();
        assert!(
            match (original, actual) {
                (Some(original), Some(actual)) => original.is(&actual),
                (None, None) => true,
                _ => false,
            },
            "{name} was not restored"
        );
    }
    let events = state.events.lock().unwrap().clone();
    (result, events)
}
fn resources(events: &[String]) {
    for role in [
        "opened",
        "rgba",
        "blank",
        "normalized",
        "channel",
        "alpha",
        "blank channel",
        "blank alpha",
        "cropped",
        "resized",
        "canvas",
    ] {
        assert_eq!(
            events
                .iter()
                .filter(|e| **e == format!("create {role}"))
                .count(),
            events
                .iter()
                .filter(|e| **e == format!("close {role}"))
                .count(),
            "{role}: {events:?}"
        );
    }
}
fn trace(events: &[String]) -> String {
    events
        .iter()
        .filter(|e| !e.starts_with("create ") && !e.starts_with("close "))
        .cloned()
        .collect::<Vec<_>>()
        .join("|")
}

pub fn check(source: &Path, is_provider: bool) {
    if is_provider {
        let before = std::fs::read(source).unwrap();
        provider(&before);
        assert_eq!(std::fs::read(source).unwrap(), before);
    } else {
        conversion(source);
    }
}

fn conversion(source: &Path) {
    let scratch = crate::tooling_fixture::Root::new("static fitting café");
    let before = std::fs::read(source).unwrap();
    Python::initialize();
    Python::attach(|py| {
        // Literal prefixes come from media._trim/fit_100, not the native fitter's output.
        for (fault, kind, stage, expected) in [
            (Fault::Mode, "TypeError", "mode", "convert opened|mode rgba"),
            (
                Fault::Convert,
                "ValueError",
                "convert",
                "convert opened|mode rgba|convert rgba",
            ),
            (
                Fault::Split,
                "OSError",
                "split",
                "convert opened|mode rgba|split rgba",
            ),
            (
                Fault::Index,
                "IndexError",
                "",
                "convert opened|mode rgba|split rgba",
            ),
            (
                Fault::Bbox,
                "ValueError",
                "bbox",
                "convert opened|mode rgba|split rgba|getbbox alpha",
            ),
            (
                Fault::FalseBounds,
                "TypeError",
                "resize",
                "convert opened|mode rgba|split rgba|getbbox alpha|size rgba|resize lookup rgba|LANCZOS alias|resize rgba",
            ),
            (
                Fault::Truth,
                "KeyboardInterrupt",
                "truth",
                "convert opened|mode rgba|split rgba|getbbox alpha|truth bbox",
            ),
            (
                Fault::Crop,
                "SystemExit",
                "crop",
                "convert opened|mode rgba|split rgba|getbbox alpha|crop rgba",
            ),
            (
                Fault::Size,
                "TypeError",
                "size",
                "convert opened|mode rgba|split rgba|getbbox alpha|crop rgba|size cropped",
            ),
            (
                Fault::Zero,
                "ValueError",
                "zero canvas",
                "convert opened|mode rgba|split rgba|getbbox alpha|size rgba|new canvas",
            ),
            (
                Fault::ResizeLookup,
                "KeyboardInterrupt",
                "resize lookup",
                "convert opened|mode rgba|split rgba|getbbox alpha|crop rgba|size cropped|resize lookup cropped",
            ),
            (
                Fault::Lanczos,
                "SystemExit",
                "lanczos",
                "convert opened|mode rgba|split rgba|getbbox alpha|crop rgba|size cropped|resize lookup cropped|LANCZOS alias",
            ),
            (
                Fault::Canvas,
                "OSError",
                "new",
                "convert opened|mode rgba|split rgba|getbbox alpha|crop rgba|size cropped|resize lookup cropped|LANCZOS alias|resize cropped|new canvas",
            ),
            (
                Fault::Paste,
                "TypeError",
                "paste",
                "convert opened|mode rgba|split rgba|getbbox alpha|crop rgba|size cropped|resize lookup cropped|LANCZOS alias|resize cropped|new canvas|paste canvas",
            ),
        ] {
            let output = scratch.0.join("not-created.png");
            let (result, events) = observe(py, fault, false, || {
                _native::codec_image::to_static_png_typed(source, &output)
            });
            let error = result.unwrap_err();
            assert_eq!(error.kind, kind, "{fault:?}");
            assert_eq!(
                error.message,
                if fault == Fault::Index {
                    "tuple index out of range".into()
                } else {
                    format!("controlled fit {stage}")
                }
            );
            assert_eq!(error.handled, matches!(kind, "OSError" | "ValueError"));
            assert_eq!(trace(&events), expected, "{fault:?}");
            resources(&events);
            assert!(!output.exists());
            assert_eq!(std::fs::read_dir(&scratch.0).unwrap().count(), 0);
            assert_eq!(std::fs::read(source).unwrap(), before);
            if matches!(fault, Fault::FalseBounds | Fault::Canvas) {
                let (result, events) = observe(py, fault, false, || {
                    _native::codec_image::to_static_png(source, &output)
                });
                assert_eq!(
                    result.unwrap_err(),
                    if fault == Fault::Canvas {
                        "external static conversion failed"
                    } else {
                        "controlled fit legacy crop"
                    }
                );
                assert_eq!(
                    trace(&events),
                    if fault == Fault::Canvas {
                        "convert opened|getchannel rgba|getbbox alpha|crop rgba|size cropped|new canvas"
                    } else {
                        "convert opened|getchannel rgba|getbbox alpha|crop rgba"
                    }
                );
                resources(&events);
                assert!(!output.exists());
            }
        }
    });
    scratch.clean().unwrap();
}

fn provider(data: &[u8]) {
    let scratch = crate::tooling_fixture::Root::new("provider fitting café");
    let before = data.to_vec();
    Python::initialize();
    Python::attach(|py| {
        for (fault, tail) in [
            (Fault::Split, "mode rgba|split rgba"),
            (Fault::Convert, "mode rgba|convert rgba"),
            (
                Fault::Resize,
                "mode rgba|split rgba|getbbox alpha|crop rgba|size cropped|resize lookup cropped|LANCZOS alias|resize cropped",
            ),
        ] {
            let output = scratch.0.join("not-created.png");
            let (result, events) = observe(py, fault, true, || {
                _native::codec_image::provider_png(data, &output)
            });
            assert_eq!(
                result.unwrap_err(),
                "external provider image conversion failed"
            );
            assert_eq!(
                trace(&events),
                format!(
                    "convert opened|convert rgba|split blank|getbbox blank alpha|histogram blank alpha|{tail}"
                )
            );
            resources(&events);
            assert!(!output.exists());
            assert_eq!(std::fs::read_dir(&scratch.0).unwrap().count(), 0);
            assert_eq!(data, before);
        }
    });
    scratch.clean().unwrap();
}
