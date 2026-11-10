//! Actual pinned prepare/tint outcomes keep class, warnings, byte integrity and prefix ordering.
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[pyclass]
struct SaveFault {
    kind: String,
    hits: Arc<AtomicUsize>,
}
#[pymethods]
impl SaveFault {
    #[pyo3(signature=(*_args, **_kwargs))]
    fn __call__(
        &self,
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        let message = format!("controlled save {}", self.kind);
        Err(match self.kind.as_str() {
            "OSError" => pyo3::exceptions::PyOSError::new_err(message),
            "TypeError" => pyo3::exceptions::PyTypeError::new_err(message),
            "KeyboardInterrupt" => pyo3::exceptions::PyKeyboardInterrupt::new_err(message),
            _ => panic!("uninventoried source fault"),
        })
    }
}
struct Restore<'py>(Bound<'py, PyAny>, Bound<'py, PyAny>);
impl Drop for Restore<'_> {
    fn drop(&mut self) {
        self.0
            .setattr("save", &self.1)
            .expect("restore external save");
    }
}
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("clean tint source scratch");
    }
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn static_prepare_matches_all_nine_source_results_errors_and_warning_observations() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/media-continuation/prepare-tint.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"]["emojikit/repaint.py"],
        "845d1443956499a93413316342f06d819e9ef89a9a8bb789dcaa53638cdbfc59"
    );
    assert_eq!(fixture["source_runtime"], "CPython3.11.9");
    assert_eq!(fixture["Pillow"], "12.3.0");
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 9);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let directory = root
        .join("logs/tmp")
        .join(format!("prepare tint source {}", std::process::id()));
    std::fs::create_dir_all(directory.parent().unwrap()).unwrap();
    std::fs::create_dir(&directory).unwrap();
    let scratch = Scratch(directory);
    Python::initialize();
    Python::attach(|py| {
        let class = py.import("PIL.Image").unwrap().getattr("Image").unwrap();
        for (index, case) in cases.iter().enumerate() {
            let original_path = root.join(case["input"].as_str().unwrap());
            let original = std::fs::read(&original_path).unwrap();
            assert_eq!(hash(&original), case["input_sha256"]);
            let folder = scratch.0.join(index.to_string());
            std::fs::create_dir(&folder).unwrap();
            let source = folder.join(original_path.file_name().unwrap());
            // Pillow reports repr(os.fspath(path)), including native Windows separators.
            let source_repr = py
                .import("os")
                .unwrap()
                .call_method1("fspath", (source.clone(),))
                .unwrap()
                .repr()
                .unwrap()
                .extract::<String>()
                .unwrap();
            let hits = Arc::new(AtomicUsize::new(0));
            let _restore = if let Some(kind) = case["injected_kind"].as_str() {
                assert_eq!(case["fault"], "save");
                assert_eq!(case["injected_calls"], 1);
                let old = class.getattr("save").unwrap();
                class
                    .setattr(
                        "save",
                        Py::new(
                            py,
                            SaveFault {
                                kind: kind.into(),
                                hits: hits.clone(),
                            },
                        )
                        .unwrap(),
                    )
                    .unwrap();
                Some(Restore(class.clone(), old))
            } else {
                None
            };
            for boundary in ["prepare", "worker"] {
                std::fs::write(&source, &original).unwrap();
                let mut request = case["request"].clone();
                request["source"] = serde_json::json!(source);
                let mut warnings = Vec::new();
                let before_calls = hits.load(Ordering::SeqCst);
                let actual = if boundary == "prepare" {
                    _native::codec_static_prepare::prepare(&source, &request, &mut warnings)
                } else {
                    _native::codec_worker::execute_typed_at(None, &request)
                };
                assert_eq!(
                    hits.load(Ordering::SeqCst) - before_calls,
                    case["injected_calls"].as_u64().unwrap() as usize,
                    "{boundary}: {}",
                    case["name"]
                );
                if let Some(expected) = case.get("expected") {
                    assert_eq!(actual.unwrap(), *expected);
                } else {
                    let error = actual.unwrap_err();
                    assert_eq!(
                        error.kind,
                        case["error"]["kind"].as_str().unwrap(),
                        "{boundary}: {}",
                        case["name"]
                    );
                    assert_eq!(error.message, case["error"]["message"].as_str().unwrap());
                    assert_eq!(error.handled, error.kind != "KeyboardInterrupt");
                }
                if boundary == "prepare" {
                    let warnings = warnings
                        .iter()
                        .map(|warning| warning.replace(&source_repr, "'$source'"))
                        .collect::<Vec<_>>();
                    assert_eq!(serde_json::json!(warnings), case["warnings"]);
                }
                let bytes = std::fs::read(&source).unwrap();
                assert_eq!(
                    bytes.len() as u64,
                    case["output"]["bytes"].as_u64().unwrap()
                );
                assert_eq!(hash(&bytes), case["output"]["sha256"]);
                assert_eq!(
                    bytes == original,
                    case["output"]["input_unchanged"].as_bool().unwrap()
                );
            }
            if case["injected_kind"].is_string() {
                assert_eq!(hits.load(Ordering::SeqCst), 2);
            }
            assert_eq!(std::fs::read(&original_path).unwrap(), original);
        }
    });
}
