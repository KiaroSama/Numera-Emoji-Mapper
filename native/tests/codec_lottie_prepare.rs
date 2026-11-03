//! Actual source bytes verify deterministic rewrite, tint and cap/corrupt preservation.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule, PyTuple};

// One case replaces process-global json APIs; no sibling may decode while it owns them.
static JSON_API: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[pyclass]
struct JsonFault(&'static str);
#[pymethods]
impl JsonFault {
    #[pyo3(signature=(*_args, **_kwargs))]
    fn __call__(
        &self,
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        Err(match self.0 {
            "ordinary" => pyo3::exceptions::PyTypeError::new_err("controlled codec fault"),
            "interrupt" => pyo3::exceptions::PyKeyboardInterrupt::new_err("controlled codec fault"),
            _ => panic!("unknown codec fault"),
        })
    }
}
struct Restore<'py>(Bound<'py, PyModule>, &'static str, Bound<'py, PyAny>);
impl Drop for Restore<'_> {
    fn drop(&mut self) {
        self.0
            .setattr(self.1, &self.2)
            .expect("restore external JSON API");
    }
}

#[test]
fn codec_failures_warn_but_interrupts_escape_without_rewriting() {
    let _json = JSON_API.lock().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let directory = root
        .join("logs/tmp")
        .join(format!("animated faults {}", std::process::id()));
    std::fs::create_dir_all(directory.parent().unwrap()).unwrap();
    std::fs::create_dir(&directory).unwrap();
    let source = directory.join("fault.tgs");
    let original = br#"{"v":"5.5","w":512,"h":512,"fr":30,"ip":0,"op":60,"layers":[]}"#;
    std::fs::write(&source, original).unwrap();
    Python::initialize();
    Python::attach(|py| {
        let json = py.import("json").unwrap();
        for operation in ["loads", "dumps"] {
            let old = json.getattr(operation).unwrap();
            let _restore = Restore(json.clone(), operation, old);
            for fault in ["ordinary", "interrupt"] {
                json.setattr(operation, Py::new(py, JsonFault(fault)).unwrap())
                    .unwrap();
                for tint in [None, Some([26, 128, 255])] {
                    let result = _native::codec_lottie_prepare::rewrite_typed(&source, tint);
                    if fault == "ordinary" {
                        let outcome = result.unwrap();
                        assert!(!outcome.rewritten);
                        let action = if tint.is_some() {
                            "repaint"
                        } else {
                            "re-encode"
                        };
                        assert_eq!(
                            outcome.warning.unwrap(),
                            format!(
                                "{action} skipped for fault.tgs (animated): controlled codec fault"
                            )
                        );
                    } else {
                        let error = result.unwrap_err();
                        assert_eq!(error.kind, "KeyboardInterrupt");
                        assert_eq!(error.message, "controlled codec fault");
                        assert!(!error.handled);
                    }
                    assert_eq!(std::fs::read(&source).unwrap(), original);
                    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
                }
            }
        }
    });
    std::fs::remove_file(source).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn animated_preparation_matches_source_rewrite_and_refusal_bytes() {
    let _json = JSON_API.lock().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/tgs-prepare.json")).unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"]["emojikit/repaint.py"],
        "845d1443956499a93413316342f06d819e9ef89a9a8bb789dcaa53638cdbfc59"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    let directory = root
        .join("logs/tmp")
        .join(format!("tgs prepare {}", std::process::id()));
    std::fs::create_dir_all(directory.parent().unwrap()).unwrap();
    std::fs::create_dir(&directory).unwrap();
    for case in cases {
        let source = root.join(case["input"].as_str().unwrap());
        let bytes = std::fs::read(&source).unwrap();
        let hash = |bytes: &[u8]| {
            Sha256::digest(bytes)
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>()
        };
        assert_eq!(hash(&bytes), case["input_sha256"]);
        let copy = directory.join(format!("{}.tgs", case["name"].as_str().unwrap()));
        std::fs::write(&copy, bytes).unwrap();
        let tint = case["tint"]
            .as_str()
            .map(|text| _native::codec_tint::parse_tint(text).unwrap());
        let outcome = _native::codec_lottie_prepare::rewrite_typed(&copy, tint).unwrap();
        assert_eq!(
            outcome.rewritten,
            case["expected"]["changed"].as_bool().unwrap()
        );
        match case["name"].as_str().unwrap() {
            "invalid" => assert_eq!(
                outcome.warning.as_deref(),
                Some(
                    "re-encode skipped for invalid.tgs (animated): Expecting property name enclosed in double quotes: line 1 column 2 (char 1)"
                )
            ),
            "over-cap" | "tint-over-cap" => {
                let warning = outcome.warning.unwrap();
                let (prefix, suffix) = if tint.is_some() {
                    (
                        "repaint of tint-over-cap.tgs would be ",
                        " bytes, over the 65536 cap; keeping the original",
                    )
                } else {
                    (
                        "re-encode of over-cap.tgs would be ",
                        " bytes, over the animated cap of 65536; keeping the original",
                    )
                };
                let length: usize = warning
                    .strip_prefix(prefix)
                    .unwrap()
                    .strip_suffix(suffix)
                    .unwrap()
                    .parse()
                    .unwrap();
                assert!(length > 65536);
            }
            _ => assert!(outcome.warning.is_none()),
        }
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
        assert_eq!(
            _native::codec_lottie_prepare::rewrite(&copy, tint).unwrap(),
            case["expected"]["repeat"].as_bool().unwrap()
        );
        assert_eq!(std::fs::read(&copy).unwrap(), output);
        std::fs::remove_file(copy).unwrap();
    }
    let source = root.join("tests/fixtures/lottie/red_circle_512.json");
    let copy = directory.join("worker.tgs");
    std::fs::copy(&source, &copy).unwrap();
    let request = serde_json::json!({"operation":"prepare","source":copy,"format":"animated",
        "reencode":true,"tint":"#1A80FF"});
    let expected = _native::codec_worker::execute_typed_at(None, &request).unwrap();
    assert_eq!(expected["fmt"], "animated");
    let bytes = std::fs::read(&copy).unwrap();
    assert_eq!(_native::codec_worker::execute(&request).unwrap(), expected);
    assert_eq!(std::fs::read(&copy).unwrap(), bytes);
    let mut invalid = request;
    invalid["reencode"] = serde_json::json!("yes");
    assert_eq!(
        _native::codec_worker::execute(&invalid).unwrap_err(),
        "reencode must be boolean"
    );
    assert_eq!(std::fs::read(&copy).unwrap(), bytes);
    std::fs::remove_file(copy).unwrap();
    assert!(std::fs::read_dir(&directory).unwrap().next().is_none());
    std::fs::remove_dir(directory).unwrap();
}

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

#[test]
fn original_white_repaint_keeps_fill_stroke_gradient_offsets_and_animation() {
    use pyo3::types::PyBytes;
    let _json = JSON_API.lock().unwrap();
    let directory = tooling_fixture::Root::new("original white Lottie café");
    let path = directory.0.join("a.tgs");
    let original = serde_json::json!({
        "v":"5.5", "fr":30, "ip":0, "op":30, "w":512, "h":512,
        "layers":[{"ty":4,"ks":{},"shapes":[{"ty":"gr","it":[
            {"ty":"fl","c":{"a":0,"k":[1.0,0.0,0.0,1.0]}},
            {"ty":"st","c":{"a":0,"k":[0.0,1.0,0.0,1.0]}},
            {"ty":"gf","g":{"p":2,"k":{"a":0,"k":[0.0,0.0,0.0,1.0,1.0,1.0,1.0,0.0]}}}
        ]}]}]
    });
    Python::initialize();
    let bytes = Python::attach(|py| -> PyResult<Vec<u8>> {
        let options = PyDict::new(py);
        options.set_item("mtime", 0)?;
        py.import("gzip")?
            .call_method(
                "compress",
                (PyBytes::new(py, original.to_string().as_bytes()),),
                Some(&options),
            )?
            .extract()
    })
    .unwrap();
    std::fs::write(&path, bytes).unwrap();
    let result =
        _native::codec_lottie_prepare::rewrite_typed(&path, Some([255, 255, 255])).unwrap();
    assert!(result.rewritten);
    assert!(result.warning.is_none());
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[4..8], &[0, 0, 0, 0]);
    let document = Python::attach(|py| -> PyResult<Value> {
        let decoded = py
            .import("gzip")?
            .call_method1("decompress", (PyBytes::new(py, &bytes),))?;
        let text = decoded
            .call_method1("decode", ("utf-8",))?
            .extract::<String>()?;
        Ok(serde_json::from_str(&text).unwrap())
    })
    .unwrap();
    let items = &document["layers"][0]["shapes"][0]["it"];
    assert_eq!(items[0]["c"]["k"], serde_json::json!([1.0, 1.0, 1.0, 1.0]));
    assert_eq!(items[1]["c"]["k"], serde_json::json!([1.0, 1.0, 1.0, 1.0]));
    assert_eq!(
        items[2]["g"]["k"]["k"],
        serde_json::json!([0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0])
    );
    for key in ["v", "fr", "ip", "op", "w", "h"] {
        assert_eq!(document[key], original[key]);
    }
    directory.clean().unwrap();
}

#[test]
fn original_empty_animation_reencoding_is_deterministic_and_preserves_the_document() {
    use pyo3::types::PyBytes;
    let _json = JSON_API.lock().unwrap();
    let directory = tooling_fixture::Root::new("original empty animation café");
    let raw = br#"{"v":"5.5","fr":60,"ip":0,"op":60,"w":512,"h":512,"layers":[]}"#;
    Python::initialize();
    let mut outputs = Vec::new();
    for (name, level) in [("default", 9), ("first", 1), ("second", 1)] {
        let path = directory.0.join(format!("{name}.tgs"));
        let before = Python::attach(|py| -> PyResult<Vec<u8>> {
            let document = py
                .import("json")?
                .call_method1("loads", (PyBytes::new(py, raw),))?;
            let text = py.import("json")?.call_method1("dumps", (&document,))?;
            let utf8 = text.call_method1("encode", ("utf-8",))?;
            py.import("gzip")?
                .call_method1("compress", (utf8, level))?
                .extract()
        })
        .unwrap();
        std::fs::write(&path, &before).unwrap();
        assert!(_native::codec_lottie_prepare::rewrite(&path, None).unwrap());
        let output = std::fs::read(&path).unwrap();
        assert_ne!(
            output, before,
            "the original compression must actually change"
        );
        assert_eq!(&output[..3], &[0x1f, 0x8b, 8]);
        assert_eq!(
            &output[4..8],
            &[0, 0, 0, 0],
            "gzip timestamps must be deterministic"
        );
        Python::attach(|py| -> PyResult<()> {
            let decoded = py
                .import("gzip")?
                .call_method1("decompress", (PyBytes::new(py, &output),))?;
            let json = py.import("json")?;
            assert!(
                json.call_method1("loads", (decoded,))?
                    .eq(&json.call_method1("loads", (PyBytes::new(py, raw),))?)?
            );
            Ok(())
        })
        .unwrap();
        assert!(!_native::codec_lottie_prepare::rewrite(&path, None).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), output);
        outputs.push(output);
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[1], outputs[2]);
    directory.clean().unwrap();
}

#[test]
fn typed_prepare_keeps_nonfinite_metadata_and_reencode_before_tint_refusal() {
    let _json = JSON_API.lock().unwrap();
    use pyo3::types::PyBytes;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let directory = root
        .join("logs/tmp")
        .join(format!("animated object {}", std::process::id()));
    std::fs::create_dir_all(directory.parent().unwrap()).unwrap();
    std::fs::create_dir(&directory).unwrap();
    let source = directory.join("colour.tgs");
    let original = br#"{"v":"5.5","w":512,"h":512,"fr":30,"ip":0,"op":60,"layers":[{"c":{"k":[NaN,Infinity,-Infinity,0.25]}}],"metadata":NaN}"#;
    Python::initialize();
    Python::attach(|py| {
        for (tint, error) in [
            (serde_json::json!(null), Some("tint must be #RRGGBB")),
            (
                serde_json::json!("#fff"),
                Some("tint must be #RRGGBB, got '#fff'"),
            ),
            (serde_json::json!("#1A80FF"), None),
        ] {
            std::fs::write(&source, original).unwrap();
            let request = serde_json::json!({"operation":"prepare","source":source,
                "format":"animated","reencode":true,"tint":tint});
            let result = _native::codec_worker::execute_typed_at(None, &request);
            if let Some(message) = error {
                let actual = result.unwrap_err();
                assert_eq!(actual.message, message);
                assert_eq!(
                    actual.kind,
                    if tint.is_null() {
                        "ValueError"
                    } else {
                        "MediaError"
                    }
                );
                assert!(actual.handled);
            } else {
                let result = result.unwrap();
                assert_eq!(result["fmt"], "animated");
                assert_eq!(result["phash"], serde_json::Value::Null);
                assert_eq!(result["extension"], ".tgs");
                assert!(result["content_key"].as_str().unwrap().starts_with("a:"));
            }
            let bytes = std::fs::read(&source).unwrap();
            assert!(bytes.starts_with(&[0x1f, 0x8b]));
            let raw = py
                .import("gzip")
                .unwrap()
                .call_method1("decompress", (PyBytes::new(py, &bytes),))
                .unwrap();
            let data = py
                .import("json")
                .unwrap()
                .call_method1("loads", (raw,))
                .unwrap();
            assert!(
                data.get_item("metadata")
                    .unwrap()
                    .extract::<f64>()
                    .unwrap()
                    .is_nan()
            );
            let colour = data
                .get_item("layers")
                .unwrap()
                .get_item(0)
                .unwrap()
                .get_item("c")
                .unwrap()
                .get_item("k")
                .unwrap();
            assert_eq!(colour.get_item(3).unwrap().extract::<f64>().unwrap(), 0.25);
            if error.is_none() {
                for (index, expected) in [26.0 / 255.0, 128.0 / 255.0, 1.0].into_iter().enumerate()
                {
                    assert_eq!(
                        colour.get_item(index).unwrap().extract::<f64>().unwrap(),
                        expected
                    );
                }
            } else {
                assert!(
                    colour
                        .get_item(0)
                        .unwrap()
                        .extract::<f64>()
                        .unwrap()
                        .is_nan()
                );
                assert_eq!(
                    colour.get_item(1).unwrap().extract::<f64>().unwrap(),
                    f64::INFINITY
                );
            }
            assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        }
    });
    std::fs::remove_file(source).unwrap();
    std::fs::remove_dir(directory).unwrap();
}
