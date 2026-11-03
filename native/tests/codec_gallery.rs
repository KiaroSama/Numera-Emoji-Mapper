//! Gallery policy differs from panel previews: 88px/Lanczos/quality65 and source animation flags.
use pyo3::prelude::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[pyclass]
struct ReadFault {
    kind: &'static str,
    hits: Arc<AtomicUsize>,
}
#[pymethods]
impl ReadFault {
    #[pyo3(signature=(*_args, **_kwargs))]
    fn __call__(
        &self,
        _args: &Bound<'_, pyo3::types::PyTuple>,
        _kwargs: Option<&Bound<'_, pyo3::types::PyDict>>,
    ) -> PyResult<Vec<u8>> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        Err(match self.kind {
            "OSError" => pyo3::exceptions::PyOSError::new_err("controlled gallery read"),
            "TypeError" => pyo3::exceptions::PyTypeError::new_err("controlled gallery read"),
            "KeyboardInterrupt" => {
                pyo3::exceptions::PyKeyboardInterrupt::new_err("controlled gallery read")
            }
            "SystemExit" => pyo3::exceptions::PySystemExit::new_err("controlled gallery read"),
            _ => unreachable!(),
        })
    }
}
#[pyclass]
struct EncodeFault(ReadFault);
#[pymethods]
impl EncodeFault {
    #[pyo3(signature=(*_args, **options))]
    fn __call__(
        &self,
        _args: &Bound<'_, pyo3::types::PyTuple>,
        options: Option<&Bound<'_, pyo3::types::PyDict>>,
    ) -> PyResult<()> {
        let options = options.expect("source single-frame WebP options");
        assert!(options.get_item("lossless")?.unwrap().extract::<bool>()?);
        assert!(options.get_item("exact")?.unwrap().extract::<bool>()?);
        self.0.hits.fetch_add(1, Ordering::SeqCst);
        Err(match self.0.kind {
            "OSError" => pyo3::exceptions::PyOSError::new_err("controlled preview encode"),
            "TypeError" => pyo3::exceptions::PyTypeError::new_err("controlled preview encode"),
            "KeyboardInterrupt" => {
                pyo3::exceptions::PyKeyboardInterrupt::new_err("controlled preview encode")
            }
            _ => unreachable!(),
        })
    }
}
struct RestoreSave<'py>(Bound<'py, PyAny>, Bound<'py, PyAny>);
impl Drop for RestoreSave<'_> {
    fn drop(&mut self) {
        self.0
            .setattr("save", &self.1)
            .expect("restore external Pillow save");
    }
}

struct RestoreRead<'py>(Bound<'py, PyAny>, Bound<'py, PyAny>);
impl Drop for RestoreRead<'_> {
    fn drop(&mut self) {
        self.0
            .setattr("read_bytes", &self.1)
            .expect("restore external pathlib read");
    }
}

struct CloseState {
    fault: &'static str,
    created: AtomicUsize,
    events: std::sync::Mutex<Vec<String>>,
}
impl CloseState {
    fn event(&self, event: impl Into<String>) {
        self.events.lock().unwrap().push(event.into());
    }
}
#[pyclass]
struct CloseFrame {
    index: usize,
    state: Arc<CloseState>,
}
#[pymethods]
impl CloseFrame {
    #[pyo3(signature=(*_args, **options))]
    fn save(
        &self,
        _args: &Bound<'_, pyo3::types::PyTuple>,
        options: Option<&Bound<'_, pyo3::types::PyDict>>,
    ) -> PyResult<()> {
        self.state.event("save");
        let options = options.unwrap();
        assert!(options.get_item("save_all")?.unwrap().extract::<bool>()?);
        assert_eq!(options.get_item("quality")?.unwrap().extract::<u32>()?, 60);
        if self.state.fault.starts_with("save") {
            Err(pyo3::exceptions::PyValueError::new_err("body save"))
        } else {
            Ok(())
        }
    }
    fn close(&self) -> PyResult<()> {
        self.state.event(format!("close {}", self.index));
        if self.index == 0
            && matches!(
                self.state.fault,
                "close-first"
                    | "save-close-first"
                    | "getvalue-close-first"
                    | "close-both"
                    | "construct-close"
            )
        {
            Err(pyo3::exceptions::PyTypeError::new_err("close first"))
        } else if self.index == 1 && matches!(self.state.fault, "close-second" | "close-both") {
            Err(pyo3::exceptions::PyOSError::new_err("close second"))
        } else {
            Ok(())
        }
    }
}
#[pyclass]
struct CloseFrames(Arc<CloseState>);
#[pymethods]
impl CloseFrames {
    fn __call__(
        &self,
        py: Python<'_>,
        mode: &str,
        size: (u32, u32),
        data: &Bound<'_, pyo3::types::PyBytes>,
    ) -> PyResult<Py<CloseFrame>> {
        assert_eq!((mode, size), ("RGBA", (52, 52)));
        assert_eq!(data.as_bytes().len(), 52 * 52 * 4);
        let index = self.0.created.load(Ordering::SeqCst);
        if self.0.fault.starts_with("construct") && index == 1 {
            self.0.event("construct refused");
            return Err(pyo3::exceptions::PyValueError::new_err(
                "frame construction",
            ));
        }
        self.0.created.fetch_add(1, Ordering::SeqCst);
        self.0.event(format!("create {index}"));
        Py::new(
            py,
            CloseFrame {
                index,
                state: self.0.clone(),
            },
        )
    }
}
#[pyclass]
struct CloseBuffer(Arc<CloseState>);
#[pymethods]
impl CloseBuffer {
    fn getvalue<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
        self.0.event("getvalue");
        if self.0.fault.starts_with("getvalue") {
            Err(pyo3::exceptions::PyValueError::new_err("body getvalue"))
        } else {
            Ok(pyo3::types::PyBytes::new(py, b"owned external buffer"))
        }
    }
    fn close(&self) {
        self.0.event("close buffer");
    }
}
#[pyclass]
struct CloseBuffers(Arc<CloseState>);
#[pymethods]
impl CloseBuffers {
    fn __call__(&self, py: Python<'_>) -> PyResult<Py<CloseBuffer>> {
        self.0.event("create buffer");
        Py::new(py, CloseBuffer(self.0.clone()))
    }
}
struct RestoreClose<'py>(Vec<(Bound<'py, PyAny>, &'static str, Bound<'py, PyAny>)>);
impl Drop for RestoreClose<'_> {
    fn drop(&mut self) {
        for (object, name, original) in self.0.iter().rev() {
            object
                .setattr(*name, original)
                .expect("restore preview close seam");
        }
    }
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn gallery_artwork_and_animation_flags_match_source() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let committed: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/gallery-codec.json"
    ))
    .unwrap();
    let fixture: Value = std::env::var_os("NUMERA_GALLERY_CODEC_ORACLE")
        .map(PathBuf::from)
        .map(|path| serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap())
        .unwrap_or_else(|| committed.clone());
    let (preview, _) = _native::source_lottie_render::fixtures().unwrap();
    _native::source_lottie_render::fixture_identity(&preview, &fixture).unwrap();
    assert_eq!(fixture["source_sha256"], committed["source_sha256"]);
    assert_eq!(
        fixture["cases"].as_array().unwrap().len(),
        committed["cases"].as_array().unwrap().len()
    );
    for (source, original) in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(committed["cases"].as_array().unwrap())
    {
        for field in ["input", "input_sha256", "format", "still", "animation_info"] {
            assert_eq!(source[field], original[field]);
        }
    }
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"]["emojikit/media_bridge.py"],
        "8d7f86244ed498b32381ba78a6321622aa28d9b72c1a1adba9a48c39565dd5b7"
    );
    let scratch = Scratch(
        root.join("logs/tmp")
            .join(format!("gallery codec {}", std::process::id())),
    );
    std::fs::create_dir(&scratch.0).unwrap();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let source = root.join(case["input"].as_str().unwrap());
        let hash = |bytes: &[u8]| {
            Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
        assert_eq!(hash(&std::fs::read(&source).unwrap()), case["input_sha256"]);
        let output = scratch.0.join(format!("{index}.webp"));
        let request = json!({"operation":"gallery_thumb","source":source,"format":case["format"],"still":case["still"],"output":output});
        let result = _native::codec_worker::execute_typed_at(None, &request).unwrap();
        let bytes = std::fs::read(&output).unwrap();
        assert_eq!(hash(&bytes), case["sha256"], "{index}");
        assert_eq!(result["bytes"], case["bytes"]);
        assert!(_native::codec_worker::execute(&request).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), bytes);
        let request = json!({"operation":"animation_info","source":source,"format":case["format"]});
        assert_eq!(
            _native::codec_worker::execute(&request).unwrap(),
            case["animation_info"]
        );
        assert_eq!(
            _native::codec_worker::execute_typed_at(None, &request).unwrap(),
            case["animation_info"]
        );
    }
    let video_case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["format"] == "video")
        .unwrap();
    let video = root.join(video_case["input"].as_str().unwrap());
    let original = std::fs::read(&video).unwrap();
    let output = scratch.0.join("read-failed.webm");
    Python::initialize();
    // The same valid RGBA sample and fixed key as the controlled-tool fingerprint_once case.
    let raw = [20, 40, 60, 255].repeat(64 * 64);
    let normal = _native::codec_video::fingerprint_frames_typed(&video, &raw).unwrap();
    assert_eq!(
        normal,
        json!({"content_key":"v:769f9a9a414a35bdb6e3ee62419e9c1e","phash":0,"fmt":"video","extension":".webm"})
    );
    assert_eq!(
        _native::codec_image::frame_phash_typed(&[0; 16]).unwrap(),
        None
    );
    assert_eq!(
        _native::codec_video::fingerprint_frames_typed(&video, &[]).unwrap(),
        json!({"content_key":"v:a6062ced6f8609a0351c511282d589b5","phash":null,"fmt":"video","extension":".webm"})
    );
    let mut without_phash = normal.clone();
    without_phash["phash"] = Value::Null;
    Python::attach(|py| {
        let image = py.import("PIL.Image").unwrap();
        let path = py.import("pathlib").unwrap().getattr("Path").unwrap();
        let _restore = RestoreClose(vec![
            (
                image.clone().into_any(),
                "frombytes",
                image.getattr("frombytes").unwrap(),
            ),
            (
                path.clone(),
                "read_bytes",
                path.getattr("read_bytes").unwrap(),
            ),
        ]);
        for kind in ["OSError", "TypeError", "KeyboardInterrupt", "SystemExit"] {
            let hashes = Arc::new(AtomicUsize::new(0));
            let reads = Arc::new(AtomicUsize::new(0));
            image
                .setattr(
                    "frombytes",
                    Py::new(
                        py,
                        ReadFault {
                            kind,
                            hits: hashes.clone(),
                        },
                    )
                    .unwrap(),
                )
                .unwrap();
            path.setattr(
                "read_bytes",
                Py::new(
                    py,
                    ReadFault {
                        kind,
                        hits: reads.clone(),
                    },
                )
                .unwrap(),
            )
            .unwrap();
            let result = _native::codec_video::fingerprint_frames_typed(&video, &raw);
            if matches!(kind, "OSError" | "TypeError") {
                assert_eq!(result.unwrap(), without_phash);
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.kind, kind);
                assert_eq!(error.message, "controlled gallery read");
                assert!(!error.handled);
                assert!(_native::codec_error::CodecError::worker_reply(Err(error)).is_err());
            }
            assert_eq!(
                reads.load(Ordering::SeqCst),
                0,
                "nonempty samples never read source bytes"
            );
            assert_eq!(_native::codec_image::frame_phash(&raw), None);
            assert_eq!(hashes.load(Ordering::SeqCst), 2);
            let error = _native::codec_video::fingerprint_frames_typed(&video, &[]).unwrap_err();
            assert_eq!(error.kind, kind);
            assert_eq!(error.message, "controlled gallery read");
            assert_eq!(error.handled, kind == "OSError");
            assert_eq!(reads.load(Ordering::SeqCst), 1);
            assert_eq!(
                hashes.load(Ordering::SeqCst),
                2,
                "empty samples do not request a phash"
            );
            assert_eq!(std::fs::read(&video).unwrap(), original);
        }
    });
    Python::attach(|py| {
        let path = py.import("pathlib").unwrap().getattr("Path").unwrap();
        let _restore = RestoreRead(path.clone(), path.getattr("read_bytes").unwrap());
        for kind in ["OSError", "TypeError", "KeyboardInterrupt"] {
            let hits = Arc::new(AtomicUsize::new(0));
            path.setattr(
                "read_bytes",
                Py::new(
                    py,
                    ReadFault {
                        kind,
                        hits: hits.clone(),
                    },
                )
                .unwrap(),
            )
            .unwrap();
            let error = _native::codec_worker::execute_typed_at(None,
                &json!({"operation":"gallery_thumb","source":video,"format":"video","output":output})
            ).unwrap_err();
            assert_eq!(error.kind, kind);
            assert_eq!(error.message, "controlled gallery read");
            assert_eq!(error.handled, kind == "OSError");
            assert_eq!(hits.load(Ordering::SeqCst), 1);
            let reply = _native::codec_error::CodecError::worker_reply(Err(error));
            if kind == "OSError" {
                let reply = reply.unwrap();
                assert_eq!(reply.exit, 1);
                assert_eq!(
                    reply.envelope,
                    json!({"ok":false,"error":"controlled gallery read"})
                );
            } else {
                assert!(reply.is_err());
            }
            assert!(!output.exists());
            assert_eq!(std::fs::read(&video).unwrap(), original);
        }
    });
    assert_eq!(
        std::fs::read_dir(&scratch.0).unwrap().count(),
        fixture["cases"].as_array().unwrap().len()
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let decoder = _native::codec_video::VideoDecoder::new(&root).unwrap();
    Python::attach(|py| {
        let image = py.import("PIL.Image").unwrap().getattr("Image").unwrap();
        let _restore = RestoreSave(image.clone(), image.getattr("save").unwrap());
        for kind in ["OSError", "TypeError", "KeyboardInterrupt"] {
            let hits = Arc::new(AtomicUsize::new(0));
            image
                .setattr(
                    "save",
                    Py::new(
                        py,
                        EncodeFault(ReadFault {
                            kind,
                            hits: hits.clone(),
                        }),
                    )
                    .unwrap(),
                )
                .unwrap();
            let error = py
                .detach(|| {
                    _native::codec_worker::execute_typed_at(
                        Some(&scratch.0),
                        &json!({"operation":"preview","source":video,"format":"video",
                            "output":output,"key":"fixture","size":52,"fps":15,"still":true}),
                    )
                })
                .unwrap_err();
            assert_eq!(error.kind, kind);
            assert_eq!(error.message, "controlled preview encode");
            assert_eq!(error.handled, kind == "OSError");
            assert_eq!(hits.load(Ordering::SeqCst), 1);
            let reply = _native::codec_error::CodecError::worker_reply(Err(error));
            if kind == "OSError" {
                assert_eq!(
                    reply.unwrap().envelope,
                    json!({"ok":false,"error":"controlled preview encode"})
                );
            } else {
                assert!(reply.is_err());
            }
            assert!(!output.exists());
            assert_eq!(std::fs::read(&video).unwrap(), original);
        }
        let hits = Arc::new(AtomicUsize::new(0));
        image
            .setattr(
                "save",
                Py::new(
                    py,
                    EncodeFault(ReadFault {
                        kind: "TypeError",
                        hits: hits.clone(),
                    }),
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            py.detach(|| runtime.block_on(_native::codec_video_preview::preview(
                &decoder, &video, &output, 52, 15, true
            )))
            .unwrap_err(),
            "external video preview encoding failed"
        );
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert!(!output.exists());
    });
    Python::attach(|py| {
        let image = py.import("PIL.Image").unwrap();
        let io = py.import("io").unwrap();
        let _restore = RestoreClose(vec![
            (
                image.clone().into_any(),
                "frombytes",
                image.getattr("frombytes").unwrap(),
            ),
            (
                io.clone().into_any(),
                "BytesIO",
                io.getattr("BytesIO").unwrap(),
            ),
        ]);
        for fault in [
            "",
            "save",
            "getvalue",
            "close-first",
            "save-close-first",
            "getvalue-close-first",
            "close-second",
            "close-both",
            "construct",
            "construct-close",
        ] {
            let state = Arc::new(CloseState {
                fault,
                created: AtomicUsize::new(0),
                events: std::sync::Mutex::new(Vec::new()),
            });
            image
                .setattr(
                    "frombytes",
                    Py::new(py, CloseFrames(state.clone())).unwrap(),
                )
                .unwrap();
            io.setattr("BytesIO", Py::new(py, CloseBuffers(state.clone())).unwrap())
                .unwrap();
            let answer = py.detach(|| _native::codec_worker::execute_typed_at(Some(&scratch.0),
                &json!({"operation":"preview","source":video,"format":"video","output":output,"key":"fixture","size":52,"fps":15,"still":false})));
            let count = state.created.load(Ordering::SeqCst);
            let events = state.events.lock().unwrap();
            let close_events = events
                .iter()
                .filter(|event| event.starts_with("close ") && *event != "close buffer")
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(
                close_events,
                (0..count)
                    .map(|index| format!("close {index}"))
                    .collect::<Vec<_>>(),
                "{fault}"
            );
            if fault.starts_with("construct") {
                assert_eq!(count, 1);
                assert!(
                    !events
                        .iter()
                        .any(|event| event == "save" || event == "create buffer")
                );
            } else {
                assert!(count > 1);
                let buffer_closed = events
                    .iter()
                    .position(|event| event == "close buffer")
                    .unwrap();
                let first_frame_closed = events
                    .iter()
                    .position(|event| event.starts_with("close ") && event != "close buffer")
                    .unwrap();
                assert!(
                    buffer_closed < first_frame_closed,
                    "native buffer ownership adaptation"
                );
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| *event == "close buffer")
                        .count(),
                    1
                );
            }
            if fault.is_empty() {
                assert_eq!(answer.unwrap(), json!({"bytes":21}));
                assert_eq!(std::fs::read(&output).unwrap(), b"owned external buffer");
                std::fs::remove_file(&output).unwrap();
            } else {
                let error = answer.unwrap_err();
                let (kind, message) = if fault.contains("first") || fault == "close-both" {
                    ("TypeError", "close first")
                } else if fault == "close-second" {
                    ("OSError", "close second")
                } else if fault.starts_with("construct") {
                    ("ValueError", "frame construction")
                } else if fault == "save" {
                    ("ValueError", "body save")
                } else {
                    ("ValueError", "body getvalue")
                };
                assert_eq!(
                    (error.kind.as_str(), error.message.as_str()),
                    (kind, message),
                    "{fault}"
                );
                assert_eq!(error.handled, kind != "TypeError");
                assert!(!output.exists());
            }
            assert_eq!(std::fs::read(&video).unwrap(), original);
            assert_eq!(
                std::fs::read_dir(scratch.0.join("logs/tmp"))
                    .unwrap()
                    .count(),
                0
            );
        }
        let state = Arc::new(CloseState {
            fault: "save-close-first",
            created: AtomicUsize::new(0),
            events: std::sync::Mutex::new(Vec::new()),
        });
        image
            .setattr(
                "frombytes",
                Py::new(py, CloseFrames(state.clone())).unwrap(),
            )
            .unwrap();
        io.setattr("BytesIO", Py::new(py, CloseBuffers(state)).unwrap())
            .unwrap();
        assert_eq!(
            py.detach(|| runtime.block_on(_native::codec_video_preview::preview(
                &decoder, &video, &output, 52, 15, false
            )))
            .unwrap_err(),
            "external video preview encoding failed"
        );
        assert!(!output.exists());
    });
    let mut expected = (0..fixture["cases"].as_array().unwrap().len())
        .map(|index| std::ffi::OsString::from(format!("{index}.webp")))
        .collect::<Vec<_>>();
    expected.push("logs".into());
    expected.sort();
    let mut actual = std::fs::read_dir(&scratch.0)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    actual.sort();
    assert_eq!(actual, expected);
    assert_eq!(
        std::fs::read_dir(scratch.0.join("logs/tmp"))
            .unwrap()
            .count(),
        0
    );
    assert!(!_native::process_owner::cleanup_pending(&scratch.0));
    assert!(!_native::process_owner::cleanup_pending(&root));
    let error =
        _native::codec_video_preview::preview_at(None, &video, &output, 52, 15, true).unwrap_err();
    assert_eq!(error.kind, "ValueError");
    assert_eq!(
        error.message,
        "video worker requires an explicit project root"
    );
    assert!(!output.exists());
    let occupied = scratch.0.join("0.webp");
    let before = std::fs::read(&occupied).unwrap();
    assert!(_native::codec_video_preview::gallery_typed(&video, &occupied).is_err());
    assert_eq!(std::fs::read(&occupied).unwrap(), before);
    let corrupt = scratch.0.join("corrupt.png");
    let output = scratch.0.join("corrupt.webp");
    std::fs::write(&corrupt, b"not an image at all").unwrap();
    let error = _native::codec_worker::execute_typed_at(
        None,
        &json!({"operation":"gallery_thumb","source":corrupt,"format":"static","output":output}),
    )
    .unwrap_err();
    assert_eq!(error.kind, "UnidentifiedImageError");
    assert!(error.handled);
    let reply = _native::codec_error::CodecError::worker_reply(Err(error)).unwrap();
    assert_eq!(reply.exit, 1);
    assert_eq!(reply.envelope["ok"], false);
    assert!(!output.exists());
    assert_eq!(std::fs::read(corrupt).unwrap(), b"not an image at all");
    let bad = json!({"operation":"gallery_thumb","source":root.join("tests/fixtures/native/static-codec/opaque.png"),"format":"static","still":1,"output":scratch.0.join("bad.webp")});
    assert!(_native::codec_worker::execute(&bad).is_err());
    assert!(!scratch.0.join("bad.webp").exists());
    std::fs::remove_dir_all(&scratch.0).unwrap();
}
