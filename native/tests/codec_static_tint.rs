//! Source-derived colour flattening retains every alpha value and invisible RGB.
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

static PILLOW_API: Mutex<()> = Mutex::new(());

#[pyclass]
struct PutAlphaFault {
    kind: &'static str,
    hits: Arc<AtomicUsize>,
}
#[pymethods]
impl PutAlphaFault {
    fn __get__(&self, _instance: &Bound<'_, PyAny>, _owner: &Bound<'_, PyAny>) -> PyResult<()> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        Err(match self.kind {
            "OSError" => pyo3::exceptions::PyOSError::new_err("controlled putalpha lookup"),
            "TypeError" => pyo3::exceptions::PyTypeError::new_err("controlled putalpha lookup"),
            "KeyboardInterrupt" => {
                pyo3::exceptions::PyKeyboardInterrupt::new_err("controlled putalpha lookup")
            }
            "SystemExit" => pyo3::exceptions::PySystemExit::new_err("controlled putalpha lookup"),
            _ => panic!("unlisted tint lookup fault"),
        })
    }
}
#[pyclass]
struct ChannelMustNotRun(Arc<AtomicUsize>);
#[pymethods]
impl ChannelMustNotRun {
    #[pyo3(signature=(*_args))]
    fn __call__(&self, _args: &Bound<'_, pyo3::types::PyTuple>) -> PyResult<()> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(pyo3::exceptions::PyTypeError::new_err(
            "channel must follow putalpha lookup",
        ))
    }
}
struct RestoreTint<'py> {
    class: Bound<'py, PyAny>,
    putalpha: Bound<'py, PyAny>,
    getchannel: Bound<'py, PyAny>,
}
impl Drop for RestoreTint<'_> {
    fn drop(&mut self) {
        self.class.setattr("putalpha", &self.putalpha).unwrap();
        self.class.setattr("getchannel", &self.getchannel).unwrap();
    }
}

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

#[test]
fn static_tint_preserves_source_bytes_alpha_and_corrupt_inputs() {
    let _serial = PILLOW_API.lock().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/static-tint.json")).unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"],
        "845d1443956499a93413316342f06d819e9ef89a9a8bb789dcaa53638cdbfc59"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    let directory = tooling_fixture::Root::new("static tint");
    let hash = |bytes: &[u8]| {
        Sha256::digest(bytes)
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>()
    };
    for case in cases {
        let source = root.join(case["input"].as_str().unwrap());
        let bytes = std::fs::read(&source).unwrap();
        assert_eq!(hash(&bytes), case["input_sha256"]);
        let copy = directory
            .0
            .join(format!("{}.png", case["name"].as_str().unwrap()));
        std::fs::write(&copy, bytes).unwrap();
        let rgb = _native::codec_tint::parse_tint(case["tint"].as_str().unwrap()).unwrap();
        assert_eq!(
            _native::codec_image::repaint_static(&copy, rgb).unwrap(),
            case["expected"]["changed"].as_bool().unwrap()
        );
        let output = std::fs::read(&copy).unwrap();
        assert_eq!(
            hash(&output),
            case["expected"]["sha256"],
            "{}",
            case["name"]
        );
        assert_eq!(
            output.len() as u64,
            case["expected"]["bytes"].as_u64().unwrap()
        );
        if case["expected"]["changed"] == true {
            Python::attach(|py| {
                let image = py
                    .import("PIL.Image")
                    .unwrap()
                    .call_method1("open", (copy.clone(),))
                    .unwrap();
                let rgba = image.call_method1("convert", ("RGBA",)).unwrap();
                assert_eq!(
                    hash(
                        rgba.call_method0("tobytes")
                            .unwrap()
                            .cast::<PyBytes>()
                            .unwrap()
                            .as_bytes()
                    ),
                    case["expected"]["rgba_sha256"]
                );
                let alpha = rgba.call_method1("getchannel", ("A",)).unwrap();
                assert_eq!(
                    hash(
                        alpha
                            .call_method0("tobytes")
                            .unwrap()
                            .cast::<PyBytes>()
                            .unwrap()
                            .as_bytes()
                    ),
                    case["expected"]["alpha_sha256"]
                );
                alpha.call_method0("close").unwrap();
                rgba.call_method0("close").unwrap();
                image.call_method0("close").unwrap();
            });
        }
        assert_eq!(
            _native::codec_image::repaint_static(&copy, rgb).unwrap(),
            case["expected"]["repeat"].as_bool().unwrap()
        );
        assert_eq!(std::fs::read(&copy).unwrap(), output);
        let request = serde_json::json!({"operation":"prepare","source":copy,"format":"static",
            "tint":case["tint"]});
        let result = _native::codec_worker::execute(&request);
        if case["expected"]["changed"] == true {
            assert_eq!(result.unwrap()["fmt"], "static");
        } else {
            assert_eq!(result.unwrap_err(), "requested tint could not be applied");
        }
        assert_eq!(std::fs::read(&copy).unwrap(), output);
        std::fs::remove_file(copy).unwrap();
    }
    assert!(std::fs::read_dir(&directory.0).unwrap().next().is_none());
    directory.clean().unwrap();
}

#[test]
fn tint_resolves_putalpha_before_observing_the_original_alpha_channel() {
    let _serial = PILLOW_API.lock().unwrap();
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let original =
        std::fs::read(project.join("tests/fixtures/native/static-codec/opaque.png")).unwrap();
    let directory = tooling_fixture::Root::new("tint method order");
    let source = directory.0.join("source.png");
    std::fs::write(&source, &original).unwrap();
    Python::initialize();
    Python::attach(|py| {
        let class = py.import("PIL.Image").unwrap().getattr("Image").unwrap();
        for kind in ["OSError", "TypeError", "KeyboardInterrupt", "SystemExit"] {
            let restore = RestoreTint {
                class: class.clone(),
                putalpha: class.getattr("putalpha").unwrap(),
                getchannel: class.getattr("getchannel").unwrap(),
            };
            let method_hits = Arc::new(AtomicUsize::new(0));
            let channel_hits = Arc::new(AtomicUsize::new(0));
            class
                .setattr(
                    "putalpha",
                    Py::new(
                        py,
                        PutAlphaFault {
                            kind,
                            hits: method_hits.clone(),
                        },
                    )
                    .unwrap(),
                )
                .unwrap();
            class
                .setattr(
                    "getchannel",
                    Py::new(py, ChannelMustNotRun(channel_hits.clone())).unwrap(),
                )
                .unwrap();
            let direct = _native::codec_static_prepare::repaint_static_typed(&source, [10, 20, 30]);
            let mut warnings = Vec::new();
            let prepared = _native::codec_static_prepare::prepare(
                &source,
                &serde_json::json!({"tint":"#0a141e"}),
                &mut warnings,
            );
            drop(restore);
            assert_eq!(method_hits.load(Ordering::SeqCst), 2);
            assert_eq!(channel_hits.load(Ordering::SeqCst), 0);
            if matches!(kind, "OSError" | "TypeError") {
                let result = direct.unwrap();
                assert!(!result.rewritten);
                assert_eq!(
                    result.warning.as_deref(),
                    Some("repaint skipped for source.png (static): controlled putalpha lookup"),
                );
                assert_eq!(warnings, [result.warning.unwrap()]);
                let error = prepared.unwrap_err();
                assert_eq!(error.kind, "ValueError");
                assert_eq!(error.message, "requested tint could not be applied");
                assert!(error.handled);
            } else {
                for error in [direct.err().unwrap(), prepared.unwrap_err()] {
                    assert_eq!(error.kind, kind);
                    assert_eq!(error.message, "controlled putalpha lookup");
                    assert!(!error.handled);
                }
                assert!(warnings.is_empty());
            }
            assert_eq!(std::fs::read(&source).unwrap(), original);
            assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
        }
    });
    directory.clean().unwrap();
}
