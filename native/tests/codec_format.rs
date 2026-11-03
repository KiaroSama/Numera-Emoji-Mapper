//! Independent source header/extension and collision-key observations.
use pyo3::prelude::*;
use serde_json::{Value, json};
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
            "OSError" => pyo3::exceptions::PyOSError::new_err("controlled collision read"),
            "TypeError" => pyo3::exceptions::PyTypeError::new_err("controlled collision read"),
            "KeyboardInterrupt" => {
                pyo3::exceptions::PyKeyboardInterrupt::new_err("controlled collision read")
            }
            _ => unreachable!(),
        })
    }
}
struct RestoreRead<'py>(Bound<'py, PyAny>, Bound<'py, PyAny>);
impl Drop for RestoreRead<'_> {
    fn drop(&mut self) {
        self.0
            .setattr("read_bytes", &self.1)
            .expect("restore external pathlib byte read");
    }
}

struct HeaderState {
    phase: &'static str,
    kind: &'static str,
    events: std::sync::Mutex<Vec<String>>,
}
impl HeaderState {
    fn fail(&self) -> PyErr {
        match self.kind {
            "OSError" => pyo3::exceptions::PyOSError::new_err("controlled header failure"),
            "TypeError" => pyo3::exceptions::PyTypeError::new_err("controlled header failure"),
            "KeyboardInterrupt" => {
                pyo3::exceptions::PyKeyboardInterrupt::new_err("controlled header failure")
            }
            _ => pyo3::exceptions::PySystemExit::new_err("controlled header failure"),
        }
    }
    fn event(&self, text: impl Into<String>) {
        self.events.lock().unwrap().push(text.into());
    }
}
#[pyclass]
struct HeaderFile {
    state: Arc<HeaderState>,
}
#[pymethods]
impl HeaderFile {
    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf.state.event("enter");
        slf
    }
    fn read(&self, py: Python<'_>, size: usize) -> PyResult<Py<pyo3::types::PyBytes>> {
        self.state.event(format!("read:{size}"));
        assert_eq!(size, 16);
        if matches!(self.state.phase, "read" | "read-exit") {
            return Err(self.state.fail());
        }
        Ok(pyo3::types::PyBytes::new(py, b"GIF89a01234567890").unbind())
    }
    fn __exit__(
        &self,
        kind: Bound<'_, PyAny>,
        _value: Bound<'_, PyAny>,
        _traceback: Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.state.event(if kind.is_none() {
            "exit:None".to_owned()
        } else {
            format!("exit:{}", kind.getattr("__name__")?.extract::<String>()?)
        });
        if self.state.phase == "exit-kind" {
            return Err(self.state.fail());
        }
        if matches!(self.state.phase, "exit" | "read-exit") {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "controlled header close",
            ));
        }
        Ok(())
    }
    fn close(&self) {
        self.state.event("fallback-close");
    }
}
#[pyclass]
struct HeaderOpen {
    state: Arc<HeaderState>,
}
#[pymethods]
impl HeaderOpen {
    fn __call__(&self, py: Python<'_>, _path: PathBuf, mode: &str) -> PyResult<Py<HeaderFile>> {
        assert_eq!(mode, "rb");
        self.state.event("open");
        if self.state.phase == "open" {
            return Err(self.state.fail());
        }
        Py::new(
            py,
            HeaderFile {
                state: self.state.clone(),
            },
        )
    }
}
struct RestoreOpen<'py>(Bound<'py, PyAny>, Bound<'py, PyAny>);
impl Drop for RestoreOpen<'_> {
    fn drop(&mut self) {
        self.0
            .setattr("open", &self.1)
            .expect("restore external binary open");
    }
}

fn header_errors(source: &std::path::Path, root: &std::path::Path) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let decoder = _native::codec_video::VideoDecoder::new(root).unwrap();
    Python::initialize();
    Python::attach(|py| {
        let builtins = py.import("builtins").unwrap();
        let _restore = RestoreOpen(
            builtins.clone().into_any(),
            builtins.getattr("open").unwrap(),
        );
        for phase in ["open", "read", "exit", "read-exit", "exit-kind", "ok"] {
            for kind in ["OSError", "TypeError", "KeyboardInterrupt", "SystemExit"] {
                let state = Arc::new(HeaderState {
                    phase,
                    kind,
                    events: std::sync::Mutex::new(Vec::new()),
                });
                builtins
                    .setattr(
                        "open",
                        Py::new(
                            py,
                            HeaderOpen {
                                state: state.clone(),
                            },
                        )
                        .unwrap(),
                    )
                    .unwrap();
                assert_eq!(
                    _native::codec_format::extension_typed(source, "video").unwrap(),
                    ".webm"
                );
                assert_eq!(
                    _native::codec_format::extension_typed(source, "animated").unwrap(),
                    ".tgs"
                );
                assert!(state.events.lock().unwrap().is_empty());
                assert!(
                    runtime
                        .block_on(decoder.decoder_args(source))
                        .unwrap()
                        .is_empty()
                );
                assert_eq!(_native::codec_format::detect(source).unwrap(), "static");
                assert!(
                    state.events.lock().unwrap().is_empty(),
                    "legacy header reads stay native"
                );
                for operation in ["detect", "extension", "worker", "decoder", "frames"] {
                    if phase == "ok" && matches!(operation, "worker" | "frames") {
                        continue;
                    }
                    state.events.lock().unwrap().clear();
                    let result = match operation {
                        "extension" => _native::codec_format::extension_typed(source, "static"),
                        "worker" => _native::codec_worker::execute_typed_at(
                            None,
                            &json!({"operation":"image_info","source":source}),
                        )
                        .map(|_| "unexpected metadata success"),
                        "decoder" => {
                            runtime
                                .block_on(decoder.decoder_args_typed(source))
                                .map(|args| {
                                    assert!(args.is_empty());
                                    "static"
                                })
                        }
                        "frames" => runtime
                            .block_on(decoder.frames_rgba_typed(source, 10))
                            .map(|_| "unexpected frames success"),
                        _ => _native::codec_format::detect_typed(source),
                    };
                    let extension = operation == "extension";
                    if phase == "ok" {
                        assert_eq!(result.unwrap(), if extension { ".gif" } else { "static" });
                    } else {
                        let error = result.unwrap_err();
                        let exit_failed = matches!(phase, "exit" | "read-exit");
                        assert_eq!(error.kind, if exit_failed { "ValueError" } else { kind });
                        assert_eq!(
                            error.message,
                            if exit_failed {
                                "controlled header close"
                            } else {
                                "controlled header failure"
                            }
                        );
                        assert_eq!(error.handled, exit_failed || kind == "OSError");
                    }
                    let expected = if phase == "open" {
                        vec!["open".to_owned()]
                    } else {
                        vec![
                            "open".into(),
                            "enter".into(),
                            "read:16".into(),
                            format!(
                                "exit:{}",
                                if matches!(phase, "read" | "read-exit") {
                                    kind
                                } else {
                                    "None"
                                }
                            ),
                        ]
                    };
                    assert_eq!(*state.events.lock().unwrap(), expected);
                }
            }
        }
    });
}

fn video_header_errors(project: &std::path::Path) {
    let workspace = tooling_fixture::Root::new("video header faults");
    let work = &workspace.0;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/video-timeline.json"
    ))
    .unwrap();
    let case = &fixture["cases"][0];
    let source = project.join(case["input"].as_str().unwrap());
    let original = std::fs::read(&source).unwrap();
    assert_eq!(
        _native::tooling_manifest::digest(&source).unwrap(),
        case["input_sha256"]
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let decoder = _native::codec_video::VideoDecoder::new(work).unwrap();
    Python::initialize();
    Python::attach(|py| {
        // Probe parsing must not import modules under the controlled binary-open patch.
        py.import("json").unwrap();
        let builtins = py.import("builtins").unwrap();
        for (phase, kind, expected) in [
            ("open", "OSError", vec!["open"]),
            (
                "read",
                "TypeError",
                vec!["open", "enter", "read:16", "exit:TypeError"],
            ),
            (
                "exit-kind",
                "KeyboardInterrupt",
                vec!["open", "enter", "read:16", "exit:None"],
            ),
            (
                "exit-kind",
                "SystemExit",
                vec!["open", "enter", "read:16", "exit:None"],
            ),
        ] {
            let state = Arc::new(HeaderState {
                phase,
                kind,
                events: std::sync::Mutex::new(Vec::new()),
            });
            let restore = RestoreOpen(
                builtins.clone().into_any(),
                builtins.getattr("open").unwrap(),
            );
            builtins
                .setattr(
                    "open",
                    Py::new(
                        py,
                        HeaderOpen {
                            state: state.clone(),
                        },
                    )
                    .unwrap(),
                )
                .unwrap();
            let result = runtime.block_on(async {
                tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    decoder.same_image_typed(&source, &source),
                )
                .await
                .expect("bounded video header failure")
            });
            if matches!(kind, "OSError" | "TypeError") {
                assert_eq!(result.unwrap(), None);
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.kind, kind);
                assert_eq!(error.message, "controlled header failure");
                assert!(!error.handled);
            }
            assert_eq!(*state.events.lock().unwrap(), expected);
            drop(restore);
            assert_eq!(
                _native::codec_format::detect_typed(&source).unwrap(),
                "video"
            );
            assert_eq!(std::fs::read(&source).unwrap(), original);
        }
    });
    assert_eq!(std::fs::read_dir(work.join("logs/tmp")).unwrap().count(), 0);
    assert!(!_native::process_owner::cleanup_pending(work));
    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
    workspace.clean().unwrap();
}

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

#[test]
fn source_detection_and_collision_lookup_contract_is_preserved() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/media-format.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"]["emojikit/media.py"],
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53"
    );
    assert_eq!(
        fixture["source_sha256"]["emojikit/identity.py"],
        "8cbefd550e306e671df1e52af2dd9fe6b4e3647c3aa1b13dc98bf35080d36e89"
    );
    let decode = |value: &Value| {
        value
            .as_str()
            .unwrap()
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>()
    };
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 22);
    let directory = tooling_fixture::Root::new("codec format");
    for case in cases {
        let bytes = decode(&case["header_hex"]);
        let path = directory.0.join(case["name"].as_str().unwrap());
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(
            _native::codec_format::detect_bytes(&bytes),
            case["expected"]["header_format"]
        );
        assert_eq!(
            _native::codec_format::detect(&path).unwrap(),
            case["expected"]["format"]
        );
        assert_eq!(
            _native::codec_format::extension(&path, "static").unwrap(),
            case["expected"]["extension"]
        );
        assert_eq!(
            _native::codec_format::detect_typed(&path).unwrap(),
            case["expected"]["format"]
        );
        assert_eq!(
            _native::codec_format::extension_typed(&path, "static").unwrap(),
            case["expected"]["extension"]
        );
        assert_eq!(
            _native::codec_format::extension(&path, "video").unwrap(),
            ".webm"
        );
        assert_eq!(
            _native::codec_format::extension(&path, "animated").unwrap(),
            ".tgs"
        );
        std::fs::remove_file(path).unwrap();
    }
    let path = directory.0.join("collision.bin");
    std::fs::write(&path, decode(&fixture["collision_input_hex"])).unwrap();
    let collisions = fixture["collisions"].as_array().unwrap();
    assert_eq!(collisions.len(), 8);
    for case in collisions {
        let key = case["key"].as_str().unwrap();
        assert_eq!(
            _native::codec_format::collision_key(&path, key).unwrap(),
            case["expected"]
        );
        assert_eq!(
            _native::codec_format::collision_key_typed(&path, key).unwrap(),
            case["expected"]
        );
        if !key.is_empty() {
            assert_eq!(
                _native::codec_worker::execute_typed_at(
                    None,
                    &json!({"operation":"collision",
                    "source":path,"format":"video","key":key})
                )
                .unwrap(),
                case["expected"]
            );
            assert_eq!(
                _native::codec_worker::execute(&json!({"operation":"collision","source":path,
                "format":"video","key":key}))
                .unwrap(),
                case["expected"]
            );
        }
    }
    let original = std::fs::read(&path).unwrap();
    Python::attach(|py| {
        let class = py.import("pathlib").unwrap().getattr("Path").unwrap();
        let _restore = RestoreRead(class.clone(), class.getattr("read_bytes").unwrap());
        for kind in ["OSError", "TypeError", "KeyboardInterrupt"] {
            let hits = Arc::new(AtomicUsize::new(0));
            class
                .setattr(
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
            for key in [Value::Null, json!(""), json!(false), json!(7)] {
                let error = _native::codec_worker::execute_typed_at(
                    None,
                    &json!({"operation":"collision","source":path,"format":"video","key":key}),
                )
                .unwrap_err();
                assert_eq!(error.kind, "ValueError");
                assert_eq!(error.message, "collision requires a content key");
                assert!(error.handled);
            }
            assert_eq!(hits.load(Ordering::SeqCst), 0);
            let error = _native::codec_worker::execute_typed_at(
                None,
                &json!({"operation":"collision","source":path,"format":"video","key":"v:lookup"}),
            )
            .unwrap_err();
            assert_eq!(error.kind, kind);
            assert_eq!(error.message, "controlled collision read");
            assert_eq!(error.handled, kind == "OSError");
            assert_eq!(hits.load(Ordering::SeqCst), 1);
            let reply = _native::codec_error::CodecError::worker_reply(Err(error));
            if kind == "OSError" {
                let reply = reply.unwrap();
                assert_eq!(reply.exit, 1);
                assert_eq!(
                    reply.envelope,
                    json!({"ok":false,"error":"controlled collision read"})
                );
            } else {
                assert!(reply.is_err());
            }
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
    });
    assert_eq!(std::fs::read(&path).unwrap(), original);
    std::fs::remove_file(path).unwrap();
    let source = root.join("tests/fixtures/native/static-codec/opaque.png");
    let source_before = std::fs::read(&source).unwrap();
    assert_eq!(
        _native::codec_format::detect(&directory.0.join("missing.png")).unwrap_err(),
        "cannot read media header"
    );
    header_errors(&source, &directory.0);
    video_header_errors(&root);
    assert_eq!(std::fs::read(&source).unwrap(), source_before);
    let expected = _native::codec_worker::execute(
        &json!({"operation":"fingerprint","source":source,"format":"static"}),
    )
    .unwrap();
    for format in [Value::Null, json!(false), json!("")] {
        assert_eq!(
            _native::codec_worker::execute(&json!({"operation":"fingerprint","source":source,
            "format":format}))
            .unwrap(),
            expected
        );
    }
    assert_eq!(
        _native::codec_worker::execute(&json!({"operation":"fingerprint","source":source}))
            .unwrap(),
        expected
    );
    assert!(
        _native::codec_worker::execute(&json!({"operation":"collision","source":source,"key":""}))
            .is_err()
    );
    assert!(std::fs::read_dir(&directory.0).unwrap().next().is_none());
    directory.clean().unwrap();
}
