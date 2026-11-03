//! File validation and vector comparison keep refusal distinct from unknown.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

static PATH_API: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

#[pyo3::pyclass]
struct SourceSize {
    value: pyo3::Py<pyo3::PyAny>,
    failure: Option<&'static str>,
}
fn size_failure(kind: &str) -> pyo3::PyErr {
    match kind {
        "OSError" => pyo3::exceptions::PyOSError::new_err("controlled TGS size"),
        "TypeError" => pyo3::exceptions::PyTypeError::new_err("controlled TGS size"),
        "KeyboardInterrupt" => {
            pyo3::exceptions::PyKeyboardInterrupt::new_err("controlled TGS size")
        }
        _ => pyo3::exceptions::PySystemExit::new_err("controlled TGS size"),
    }
}
#[pyo3::pymethods]
impl SourceSize {
    #[getter]
    fn st_size(&self, py: pyo3::Python<'_>) -> pyo3::PyResult<pyo3::Py<pyo3::PyAny>> {
        if let Some(kind) = self.failure {
            Err(size_failure(kind))
        } else {
            Ok(self.value.clone_ref(py))
        }
    }
}
struct RestorePath<'py>(
    pyo3::Bound<'py, pyo3::PyAny>,
    pyo3::Bound<'py, pyo3::PyAny>,
    pyo3::Bound<'py, pyo3::PyAny>,
);
impl Drop for RestorePath<'_> {
    fn drop(&mut self) {
        use pyo3::prelude::*;
        self.0.setattr("stat", &self.1).expect("restore TGS stat");
        self.0
            .setattr("read_bytes", &self.2)
            .expect("restore TGS reads");
    }
}
fn source_size_boundary(source: &std::path::Path) {
    use pyo3::prelude::*;
    use pyo3::types::PyCFunction;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let before = std::fs::read(source).unwrap();
    assert_eq!(
        _native::tooling_manifest::digest(source).unwrap(),
        "7eab5f062ab6ff2b69178b0086356f9fa361ac0f755b4448e05c97c713376135"
    );
    Python::initialize();
    Python::attach(|py| {
        let class = py.import("pathlib").unwrap().getattr("Path").unwrap();
        let original_stat = class.getattr("stat").unwrap();
        let original_read = class.getattr("read_bytes").unwrap();
        let actual_read = class
            .call1((source.to_path_buf(),))
            .unwrap()
            .getattr("read_bytes")
            .unwrap()
            .unbind();
        let read_calls = Arc::new(AtomicUsize::new(0));
        let seen = read_calls.clone();
        let traced =
            PyCFunction::new_closure(py, None, None, move |args, _| -> PyResult<Py<PyAny>> {
                seen.fetch_add(1, Ordering::SeqCst);
                actual_read.bind(args.py()).call0().map(Bound::unbind)
            })
            .unwrap();
        let integer = py.import("builtins").unwrap().getattr("int").unwrap();
        let variants = [
            ("-123456789012345678901234567890", "", "", None),
            ("65536", "", "", None),
            ("65537", "", "", Some("MediaError")),
            ("123456789012345678901234567890", "", "", Some("MediaError")),
            ("bad", "", "", Some("TypeError")),
            ("0", "stat", "OSError", Some("OSError")),
            ("0", "size", "TypeError", Some("TypeError")),
            ("0", "stat", "KeyboardInterrupt", Some("KeyboardInterrupt")),
            ("0", "size", "SystemExit", Some("SystemExit")),
        ];
        for (size, phase, kind, expected) in variants {
            let restore = RestorePath(class.clone(), original_stat.clone(), original_read.clone());
            let value = if size == "bad" {
                pyo3::types::PyString::new(py, size).into_any()
            } else {
                integer.call1((size,)).unwrap()
            }
            .unbind();
            let calls = Arc::new(AtomicUsize::new(0));
            let seen = calls.clone();
            let stat = PyCFunction::new_closure(
                py,
                None,
                None,
                move |args, _| -> PyResult<Py<SourceSize>> {
                    seen.fetch_add(1, Ordering::SeqCst);
                    if phase == "stat" {
                        return Err(size_failure(kind));
                    }
                    Py::new(
                        args.py(),
                        SourceSize {
                            value: value.clone_ref(args.py()),
                            failure: (phase == "size").then_some(kind),
                        },
                    )
                },
            )
            .unwrap();
            read_calls.store(0, Ordering::SeqCst);
            class.setattr("stat", stat).unwrap();
            class.setattr("read_bytes", &traced).unwrap();
            let direct = _native::codec_lottie_validate::validate_file_typed(source);
            let worker = _native::codec_worker::execute_typed_at(
                None,
                &json!({"operation":"usable","source":source,"format":"animated"}),
            )
            .map(|_| ());
            drop(restore);
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            assert_eq!(
                read_calls.load(Ordering::SeqCst),
                if expected.is_none() { 4 } else { 0 }
            );
            for result in [direct, worker] {
                if let Some(expected) = expected {
                    let error = result.unwrap_err();
                    assert_eq!(error.kind, expected);
                    assert_eq!(error.handled, matches!(expected, "MediaError" | "OSError"));
                    if expected == "MediaError" {
                        assert_eq!(
                            error.message,
                            format!("valid.tgs: TGS {size} > 65536 bytes")
                        );
                    } else if !phase.is_empty() {
                        assert_eq!(error.message, "controlled TGS size");
                    } else {
                        assert_eq!(
                            error.message,
                            "'>' not supported between instances of 'str' and 'int'"
                        );
                    }
                } else {
                    result.unwrap();
                }
            }
        }
        let size = class
            .call1((source.to_path_buf(),))
            .unwrap()
            .call_method0("stat")
            .unwrap()
            .getattr("st_size")
            .unwrap()
            .extract::<u64>()
            .unwrap();
        assert_eq!(size, before.len() as u64);
    });
    assert_eq!(std::fs::read(source).unwrap(), before);
}

#[test]
fn original_animation_roundtrip_timeline_masks_and_file_refusals_are_preserved() {
    let _path_api = PATH_API.lock().unwrap();
    use pyo3::prelude::*;
    use pyo3::types::{PyBytes, PyDict};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let source = root.join("tests/fixtures/lottie/red_circle_512.json");
    let original = std::fs::read(&source).unwrap();
    source_size_boundary(&root.join("tests/fixtures/native/tgs-load/valid.tgs"));
    assert_eq!(
        _native::tooling_manifest::digest(&source).unwrap(),
        "f12c38be112dddb2ba6d3217034c8396479e6cfcf86cfeca22fa236381b0f9d1"
    );
    let base: Value = serde_json::from_slice(&original).unwrap();
    let scratch = tooling_fixture::Root::new("original animation contracts café");
    let mut keys = Vec::new();
    for name in ["first.tgs", "second.tgs"] {
        let output = scratch.0.join(name);
        _native::codec_lottie_package::package_typed(&source, &output).unwrap();
        _native::codec_lottie_validate::validate_file_typed(&output).unwrap();
        assert!(std::fs::metadata(&output).unwrap().len() <= 65536);
        assert_eq!(_native::codec_format::detect(&output).unwrap(), "animated");
        let decoded = _native::codec_lottie_load::load(&output).unwrap();
        assert_eq!(
            (decoded["w"].as_u64(), decoded["h"].as_u64()),
            (Some(512), Some(512))
        );
        assert_eq!(decoded, base);
        let identity = _native::codec_lottie_identity::fingerprint_typed(&output).unwrap();
        assert!(identity["content_key"].as_str().unwrap().starts_with("a:"));
        assert_eq!(identity["phash"], Value::Null);
        keys.push(identity["content_key"].clone());
    }
    assert_eq!(keys[0], keys[1]);
    assert_eq!(std::fs::read(source).unwrap(), original);
    let mut cases = Vec::new();
    for (field, value, message) in [
        ("w", json!(100), "canvas"),
        ("op", json!(59940), "> 3.0s"),
        ("op", json!(0), "timeline"),
    ] {
        let mut document = base.clone();
        document[field] = value;
        if field == "w" {
            document["h"] = json!(100);
        }
        if field == "op" && document[field] == 0 {
            document["ip"] = json!(0);
        }
        cases.push((
            format!("invalid-{field}-{}", cases.len()),
            document,
            Some(message),
        ));
    }
    cases.push(("list-root".into(), json!([]), Some("root")));
    for (mode, expected) in [("s", Some("SUBTRACT")), ("a", None)] {
        for nested in [false, true] {
            let mut document = base.clone();
            let layer = json!({"ty":4,"nm":"masked","ip":0,"op":10,
                "masksProperties":[{"mode":mode,"nm":"m"}]});
            if nested {
                document["assets"] = json!([{"id":"comp_0","layers":[layer]}]);
                document["layers"] =
                    json!([{"ty":0,"nm":"precomp","refId":"comp_0","ip":0,"op":10}]);
            } else {
                document["layers"].as_array_mut().unwrap().push(layer);
            }
            cases.push((format!("mask-{mode}-{nested}"), document, expected));
        }
    }
    Python::initialize();
    let compress = |raw: &[u8]| {
        Python::attach(|py| -> PyResult<Vec<u8>> {
            let options = PyDict::new(py);
            options.set_item("mtime", 0)?;
            Ok(py
                .import("gzip")?
                .call_method("compress", (PyBytes::new(py, raw),), Some(&options))?
                .cast_into::<PyBytes>()?
                .as_bytes()
                .to_vec())
        })
        .unwrap()
    };
    for (name, document, message) in cases {
        let path = scratch.0.join(format!("{name}.tgs"));
        let bytes = compress(&serde_json::to_vec(&document).unwrap());
        std::fs::write(&path, &bytes).unwrap();
        let result = _native::codec_lottie_validate::validate_file_typed(&path);
        if let Some(message) = message {
            let error = result.unwrap_err();
            assert_eq!(error.kind, "MediaError", "{name}");
            assert!(error.handled, "{name}");
            assert!(error.message.contains(message), "{name}: {}", error.message);
        } else {
            result.unwrap();
        }
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    for (name, bytes, expected) in [
        (
            "plain.tgs",
            serde_json::to_vec(&base).unwrap(),
            "gzip-compressed",
        ),
        (
            "bomb.tgs",
            compress(&vec![b'0'; 8 * 1024 * 1024 + 1024]),
            "expands beyond",
        ),
    ] {
        let path = scratch.0.join(name);
        std::fs::write(&path, &bytes).unwrap();
        let error = _native::codec_lottie_validate::validate_file_typed(&path).unwrap_err();
        assert_eq!(error.kind, "MediaError");
        assert!(error.handled);
        assert!(error.message.contains(expected), "{}", error.message);
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    let empty = json!({"v":"5.5","w":512,"h":512,"fr":60,"ip":0,"op":60,"layers":[]});
    let path = scratch.0.join("empty.tgs");
    std::fs::write(&path, compress(&serde_json::to_vec(&empty).unwrap())).unwrap();
    let fingerprint = _native::codec_lottie_identity::fingerprint_typed(&path).unwrap();
    let worker = _native::codec_worker::execute_typed_at(
        None,
        &json!({"operation":"fingerprint","format":"animated","source":path}),
    )
    .unwrap();
    assert_eq!(fingerprint, worker);
    assert!(
        fingerprint["content_key"]
            .as_str()
            .unwrap()
            .starts_with("a:")
    );
    assert_eq!(fingerprint["phash"], Value::Null);
    assert!(!_native::process_owner::cleanup_pending(&scratch.0));
    scratch.clean().unwrap();
}

#[test]
fn real_nonfinite_timeline_values_keep_the_original_media_error_boundary() {
    let _path_api = PATH_API.lock().unwrap();
    use pyo3::prelude::*;
    use pyo3::types::{PyBytes, PyDict};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let directory = root
        .join("logs/tmp")
        .join(format!("nonfinite timeline {}", std::process::id()));
    std::fs::create_dir_all(directory.parent().unwrap()).unwrap();
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("x.tgs");
    // Original ATimelineMustBeFiniteNumbers tests require MediaError for each
    // field, not a serialization refusal before the named finite check.
    pyo3::Python::initialize();
    let base = serde_json::json!({"v":"5.5","fr":30,"ip":0,"op":60,"w":512,"h":512,"layers":[]});
    for (field, value) in [
        ("", 0.0),
        ("fr", f64::NAN),
        ("ip", f64::NAN),
        ("op", f64::NAN),
        ("fr", f64::INFINITY),
        ("ip", f64::NEG_INFINITY),
        ("op", f64::INFINITY),
        ("metadata", f64::NAN),
        ("metadata", f64::INFINITY),
    ] {
        let bytes = Python::attach(|py| -> PyResult<Vec<u8>> {
            let json = py.import("json")?;
            let document = json
                .call_method1("loads", (base.to_string(),))?
                .cast_into::<PyDict>()?;
            if !field.is_empty() {
                document.set_item(field, value)?;
            }
            let raw = json
                .call_method1("dumps", (document,))?
                .call_method1("encode", ("utf-8",))?;
            let options = PyDict::new(py);
            options.set_item("mtime", 0)?;
            Ok(py
                .import("gzip")?
                .call_method("compress", (raw,), Some(&options))?
                .cast_into::<PyBytes>()?
                .as_bytes()
                .to_vec())
        })
        .unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let result = _native::codec_lottie_validate::validate_file_typed(&path);
        if field.is_empty() || field == "metadata" {
            result.unwrap();
        } else {
            let error = result.unwrap_err();
            assert_eq!(error.kind, "MediaError");
            assert!(error.handled);
            assert!(
                error.message.contains(&format!("{field} is ")),
                "{}",
                error.message
            );
            assert!(error.message.ends_with(", not a finite number"));
        }
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    let nonnumeric = br#"{"v":"5.5","fr":"thirty","ip":0,"op":60,"w":512,"h":512,"layers":[]}"#;
    let bytes = Python::attach(|py| -> PyResult<Vec<u8>> {
        let options = PyDict::new(py);
        options.set_item("mtime", 0)?;
        Ok(py
            .import("gzip")?
            .call_method("compress", (PyBytes::new(py, nonnumeric),), Some(&options))?
            .cast_into::<PyBytes>()?
            .as_bytes()
            .to_vec())
    })
    .unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let error = _native::codec_lottie_validate::validate_file_typed(&path).unwrap_err();
    assert_eq!(error.kind, "MediaError");
    assert!(error.handled);
    assert!(error.message.contains("fr"));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn source_tgs_file_refusals_and_vector_unknowns_are_preserved() {
    let _path_api = PATH_API.lock().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/lottie-file.json")).unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let hash = |bytes: &[u8]| {
        Sha256::digest(bytes)
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>()
    };
    let validations = fixture["validations"].as_array().unwrap();
    assert_eq!(validations.len(), 7);
    let directory = tooling_fixture::Root::new("lottie file");
    for case in validations {
        let path = if let Some(encoded) = case["input_base64"].as_str() {
            use base64::Engine;
            let text = std::fs::read_to_string(root.join(encoded)).unwrap();
            let path = directory.0.join(case["name"].as_str().unwrap());
            std::fs::write(
                &path,
                base64::engine::general_purpose::STANDARD
                    .decode(text.trim())
                    .unwrap(),
            )
            .unwrap();
            path
        } else {
            root.join(case["input"].as_str().unwrap())
        };
        assert_eq!(hash(&std::fs::read(&path).unwrap()), case["input_sha256"]);
        let result = _native::codec_lottie_validate::validate_file(&path);
        let worker = _native::codec_worker::execute(
            &json!({"operation":"usable","source":path,"format":"animated"}),
        );
        let typed = _native::codec_worker::execute_typed_at(
            None,
            &json!({"operation":"usable","source":path,"format":"animated"}),
        );
        if case.get("expected").is_some() {
            result.unwrap();
            assert_eq!(worker.unwrap(), true);
            assert_eq!(typed.unwrap(), true);
        } else {
            assert_eq!(result.unwrap_err(), case["error"].as_str().unwrap());
            assert_eq!(worker.unwrap_err(), case["error"].as_str().unwrap());
            let error = typed.unwrap_err();
            assert_eq!(error.kind, case["error_kind"].as_str().unwrap());
            assert_eq!(error.message, case["error"].as_str().unwrap());
            if case["error_kind"] == "MediaError" {
                let reply = _native::codec_error::CodecError::worker_reply(Err(error)).unwrap();
                assert_eq!(reply.exit, 1);
                assert_eq!(reply.envelope, json!({"ok":false,"error":case["error"]}));
            } else {
                let escaped = _native::codec_error::CodecError::worker_reply(Err(error))
                    .err()
                    .unwrap();
                assert_eq!(escaped.kind, case["error_kind"].as_str().unwrap());
            }
        }
    }
    let comparisons = fixture["comparisons"].as_array().unwrap();
    assert_eq!(comparisons.len(), 6);
    for case in comparisons {
        let a = root.join(case["a"].as_str().unwrap());
        let b = root.join(case["b"].as_str().unwrap());
        assert_eq!(hash(&std::fs::read(&a).unwrap()), case["a_sha256"]);
        assert_eq!(hash(&std::fs::read(&b).unwrap()), case["b_sha256"]);
        assert_eq!(
            json!(_native::codec_lottie_identity::same_image(&a, &b)),
            case["same_image"]
        );
        for operation in ["same_image", "compare"] {
            assert_eq!(
                _native::codec_worker::execute(&json!({"operation":operation,
                "source":a,"other":b,"format":"animated"}))
                .unwrap(),
                case[operation]
            );
        }
    }
    // The original validator calls Path.stat then Path.read_bytes before its
    // decoder catch. Compare actual stdlib exception identities, errno and paths.
    pyo3::Python::initialize();
    for path in [
        directory.0.join("missing.tgs"),
        root.join("tests/fixtures/native/tgs-load/valid.tgs/child"),
        directory.0.clone(),
    ] {
        use pyo3::prelude::*;
        let expected = Python::attach(|py| -> PyResult<(String, String)> {
            let source = py
                .import("pathlib")?
                .getattr("Path")?
                .call1((path.clone(),))?;
            let error = match source.call_method0("stat") {
                Err(error) => error,
                Ok(_) => source.call_method0("read_bytes").unwrap_err(),
            };
            assert!(error.is_instance_of::<pyo3::exceptions::PyOSError>(py));
            Ok((
                error.get_type(py).name()?.to_string(),
                error.value(py).str()?.to_string(),
            ))
        })
        .unwrap();
        let actual = _native::codec_lottie_validate::validate_file_typed(&path).unwrap_err();
        assert_eq!(actual.kind, expected.0);
        assert_eq!(actual.message, expected.1);
        assert!(actual.handled);
        assert_eq!(
            _native::codec_lottie_validate::validate_file(&path).unwrap_err(),
            expected.1
        );
    }
    for (name, expected) in [("opaque.png", true), ("transparent-rgb.png", false)] {
        assert_eq!(
            _native::codec_worker::execute(&json!({"operation":"usable","format":"static",
            "source":root.join("tests/fixtures/native/static-codec").join(name)}))
            .unwrap(),
            expected
        );
    }
}
