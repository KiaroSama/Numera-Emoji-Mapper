//! Original static identity/conversion failures preserve Python class, text and no partial output.
use pyo3::prelude::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

static IMAGE_API: Mutex<()> = Mutex::new(());

#[path = "support/static_identity_faults.rs"]
mod static_identity_faults;
#[path = "support/static_premultiply_faults.rs"]
mod static_premultiply_faults;

#[test]
fn static_hash_suppresses_only_ordinary_source_exceptions() {
    let _image_api = IMAGE_API.lock().unwrap();
    static_identity_faults::hash_exceptions();
    static_premultiply_faults::check(false);
}

#[test]
fn source_static_fingerprint_retries_only_the_key_after_an_ordinary_initial_failure() {
    let _image_api = IMAGE_API.lock().unwrap();
    static_identity_faults::initial_key_retry();
}

#[test]
fn source_alpha_calls_keep_order_short_circuit_and_actual_errors() {
    use pyo3::types::PyCFunction;
    let _image_api = IMAGE_API.lock().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let source = root.join("tests/fixtures/native/static-codec/alpha-edge.png");
    let before = std::fs::read(&source).unwrap();
    let scratch = tooling_fixture::Root::new("alpha source boundary");
    Python::initialize();
    Python::attach(|py| {
        let module = py.import("PIL.Image").unwrap();
        let class = module.getattr("Image").unwrap();
        let open = module.getattr("open").unwrap();
        for kind in ["OSError", "TypeError", "KeyboardInterrupt", "SystemExit"] {
            let fault = PyCFunction::new_closure(py, None, None, move |_, _| -> PyResult<()> {
                Err(match kind {
                    "OSError" => pyo3::exceptions::PyOSError::new_err("controlled alpha open"),
                    "TypeError" => pyo3::exceptions::PyTypeError::new_err("controlled alpha open"),
                    "KeyboardInterrupt" => {
                        pyo3::exceptions::PyKeyboardInterrupt::new_err("controlled alpha open")
                    }
                    _ => pyo3::exceptions::PySystemExit::new_err("controlled alpha open"),
                })
            })
            .unwrap();
            module.setattr("open", fault).unwrap();
            let publication =
                _native::publication_media::publication_static_usable(py, source.clone());
            let provider =
                _native::codec_image::provider_png(&before, &scratch.0.join("not-created.png"));
            module.setattr("open", &open).unwrap();
            if matches!(kind, "OSError" | "TypeError") {
                assert!(publication.unwrap());
                assert!(!provider.unwrap());
            } else {
                assert_eq!(publication.unwrap_err().get_type(py).name().unwrap(), kind);
                assert!(
                    provider.is_err(),
                    "BaseException is not an optional decode refusal"
                );
            }
        }
        for method in ["convert", "split", "getbbox", "histogram"] {
            let original = class.getattr(method).unwrap();
            for kind in [
                "OSError",
                "ValueError",
                "TypeError",
                "KeyboardInterrupt",
                "SystemExit",
            ] {
                let calls = Arc::new(AtomicUsize::new(0));
                let hits = calls.clone();
                let fault = PyCFunction::new_closure(py, None, None, move |_, _| -> PyResult<()> {
                    hits.fetch_add(1, Ordering::SeqCst);
                    Err(match kind {
                        "OSError" => pyo3::exceptions::PyOSError::new_err("controlled alpha call"),
                        "ValueError" => {
                            pyo3::exceptions::PyValueError::new_err("controlled alpha call")
                        }
                        "TypeError" => {
                            pyo3::exceptions::PyTypeError::new_err("controlled alpha call")
                        }
                        "KeyboardInterrupt" => {
                            pyo3::exceptions::PyKeyboardInterrupt::new_err("controlled alpha call")
                        }
                        _ => pyo3::exceptions::PySystemExit::new_err("controlled alpha call"),
                    })
                })
                .unwrap();
                class.setattr(method, fault).unwrap();
                let metadata = _native::codec_static_identity::metadata(&source, true);
                let worker = _native::codec_worker::execute_typed_at(
                    None,
                    &json!({"operation":"usable","source":source,"format":"static"}),
                );
                let publication =
                    _native::publication_media::publication_static_usable(py, source.clone());
                class.setattr(method, &original).unwrap();
                for error in [metadata.unwrap_err(), worker.unwrap_err()] {
                    assert_eq!(error.kind, kind);
                    assert_eq!(error.message, "controlled alpha call");
                    assert_eq!(error.handled, matches!(kind, "OSError" | "ValueError"));
                }
                let error = publication.unwrap_err();
                assert_eq!(error.get_type(py).name().unwrap(), kind);
                assert_eq!(error.value(py).to_string(), "controlled alpha call");
                assert_eq!(calls.load(Ordering::SeqCst), 3);
            }
        }
        let bbox = class.getattr("getbbox").unwrap();
        let histogram = class.getattr("histogram").unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let hits = calls.clone();
        class
            .setattr(
                "getbbox",
                PyCFunction::new_closure(py, None, None, |_, _| -> PyResult<()> { Ok(()) })
                    .unwrap(),
            )
            .unwrap();
        class
            .setattr(
                "histogram",
                PyCFunction::new_closure(py, None, None, move |_, _| -> PyResult<()> {
                    hits.fetch_add(1, Ordering::SeqCst);
                    Err(pyo3::exceptions::PyTypeError::new_err(
                        "histogram must be skipped",
                    ))
                })
                .unwrap(),
            )
            .unwrap();
        let result = _native::codec_static_identity::metadata(&source, true);
        class.setattr("getbbox", bbox).unwrap();
        class.setattr("histogram", &histogram).unwrap();
        assert_eq!(result.unwrap()["blank"], true);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let bbox = class.getattr("getbbox").unwrap();
        class
            .setattr(
                "getbbox",
                PyCFunction::new_closure(py, None, None, |_, _| -> PyResult<bool> { Ok(false) })
                    .unwrap(),
            )
            .unwrap();
        let bins = PyCFunction::new_closure(py, None, None, |_, _| -> PyResult<Vec<u64>> {
            let mut counts = vec![0; 256];
            counts[255] = 9;
            Ok(counts)
        })
        .unwrap();
        class.setattr("histogram", bins).unwrap();
        let result = _native::codec_static_identity::metadata(&source, true);
        class.setattr("getbbox", bbox).unwrap();
        class.setattr("histogram", &histogram).unwrap();
        assert_eq!(
            result.unwrap()["blank"],
            false,
            "only None, not other falsey bbox values, short-circuits"
        );
        for counts in [vec![0i64; 255], vec![-1; 256], vec![0; 257]] {
            let bins = counts.clone();
            let replacement =
                PyCFunction::new_closure(py, None, None, move |_, _| -> PyResult<Vec<i64>> {
                    Ok(bins.clone())
                })
                .unwrap();
            class.setattr("histogram", replacement).unwrap();
            let native = _native::codec_static_identity::metadata(&source, true);
            let public = _native::publication_media::publication_static_usable(py, source.clone());
            let worker = _native::codec_worker::execute_typed_at(
                Some(&root),
                &json!({"operation":"publication_usable","source":source,"format":"static"}),
            );
            class.setattr("histogram", &histogram).unwrap();
            let error = worker.unwrap_err();
            assert_eq!(error.kind, "UnverifiedCodecError");
            assert!(!error.handled);
            let error = native.unwrap_err();
            assert_eq!(error.kind, "UnverifiedCodecError");
            assert!(!error.handled);
            let error = public.unwrap_err();
            assert_eq!(error.get_type(py).name().unwrap(), "RuntimeError");
            assert_eq!(
                error.value(py).to_string(),
                "native alpha histogram is unverified"
            );
        }
        let huge = py
            .import("builtins")
            .unwrap()
            .getattr("int")
            .unwrap()
            .call1(("184467440737095516160000",))
            .unwrap()
            .unbind();
        let large = PyCFunction::new_closure(
            py,
            None,
            None,
            move |args, _| -> PyResult<Py<pyo3::types::PyList>> {
                let py = args.py();
                let bins = pyo3::types::PyList::new(py, vec![0; 256])?;
                bins.set_item(255, huge.bind(py))?;
                Ok(bins.unbind())
            },
        )
        .unwrap();
        class.setattr("histogram", large).unwrap();
        let result = _native::codec_static_identity::metadata(&source, true);
        class.setattr("histogram", &histogram).unwrap();
        assert_eq!(
            result.unwrap()["blank"],
            false,
            "large positive counts cannot overflow into blank"
        );
        let exits = Arc::new(AtomicUsize::new(0));
        let observed = exits.clone();
        let file_class = py
            .import("PIL.PngImagePlugin")
            .unwrap()
            .getattr("PngImageFile")
            .unwrap();
        let original_exit = file_class.getattr("__exit__").unwrap();
        let exit = PyCFunction::new_closure(py, None, None, move |args, _| -> PyResult<()> {
            assert!(args.get_item(1)?.is_none());
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
        let exit = py
            .import("functools")
            .unwrap()
            .getattr("partialmethod")
            .unwrap()
            .call1((exit,))
            .unwrap();
        let bad = PyCFunction::new_closure(py, None, None, |_, _| -> PyResult<Vec<i64>> {
            Ok(vec![0; 1])
        })
        .unwrap();
        file_class.setattr("__exit__", exit).unwrap();
        class.setattr("histogram", bad).unwrap();
        let result = _native::codec_static_identity::metadata(&source, true);
        file_class.setattr("__exit__", original_exit).unwrap();
        class.setattr("histogram", &histogram).unwrap();
        assert_eq!(result.unwrap_err().kind, "UnverifiedCodecError");
        assert_eq!(
            exits.load(Ordering::SeqCst),
            1,
            "native histogram uncertainty is reported after successful context exit"
        );
        let events = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut originals = Vec::new();
        for method in ["convert", "split", "getbbox", "histogram"] {
            let original = class.getattr(method).unwrap();
            let call = original.clone().unbind();
            let seen = events.clone();
            let traced = PyCFunction::new_closure(
                py,
                None,
                None,
                move |args, kwargs| -> PyResult<Py<PyAny>> {
                    seen.lock().unwrap().push(method.into());
                    call.bind(args.py()).call(args, kwargs).map(Bound::unbind)
                },
            )
            .unwrap();
            // A Python function descriptor binds self; the builtin callback is its callable body.
            let descriptor = py
                .import("functools")
                .unwrap()
                .getattr("partialmethod")
                .unwrap()
                .call1((traced,))
                .unwrap();
            originals.push((method, original));
            class.setattr(method, descriptor).unwrap();
        }
        let result = _native::codec_worker::execute_typed_at(
            None,
            &json!({"operation":"usable","source":source,"format":"static"}),
        );
        for (method, original) in originals {
            class.setattr(method, original).unwrap();
        }
        assert_eq!(result.unwrap(), true);
        assert_eq!(
            *events.lock().unwrap(),
            ["convert", "split", "getbbox", "histogram"]
        );
        let original = class.getattr("histogram").unwrap();
        let audit_events = Arc::new(Mutex::new(Vec::new()));
        let seen = audit_events.clone();
        let bytes_method = class.getattr("tobytes").unwrap();
        let bytes_call = bytes_method.clone().unbind();
        let bytes_trace =
            PyCFunction::new_closure(py, None, None, move |args, kwargs| -> PyResult<Py<PyAny>> {
                seen.lock().unwrap().push("bytes");
                bytes_call
                    .bind(args.py())
                    .call(args, kwargs)
                    .map(Bound::unbind)
            })
            .unwrap();
        let bytes_trace = py
            .import("functools")
            .unwrap()
            .getattr("partialmethod")
            .unwrap()
            .call1((bytes_trace,))
            .unwrap();
        class.setattr("tobytes", bytes_trace).unwrap();
        let seen = audit_events.clone();
        let refused = PyCFunction::new_closure(py, None, None, move |_, _| -> PyResult<()> {
            seen.lock().unwrap().push("histogram");
            Err(pyo3::exceptions::PyTypeError::new_err(
                "no encode after alpha failure",
            ))
        })
        .unwrap();
        class.setattr("histogram", refused).unwrap();
        let output = scratch.0.join("rejected.png");
        let converted = _native::codec_image::to_static_png_typed(&source, &output);
        let provider = _native::codec_image::provider_png(&before, &output);
        audit_events.lock().unwrap().clear();
        let audited = _native::codec_image::audit_image(&before);
        class.setattr("histogram", original).unwrap();
        class.setattr("tobytes", bytes_method).unwrap();
        assert_eq!(
            *audit_events.lock().unwrap(),
            ["bytes", "histogram"],
            "audit identity bytes precede blankness"
        );
        let error = converted.unwrap_err();
        assert_eq!(error.kind, "TypeError");
        assert_eq!(error.message, "no encode after alpha failure");
        assert!(provider.is_err());
        assert!(audited.is_err());
        assert!(!output.exists());
        assert_eq!(std::fs::read_dir(&scratch.0).unwrap().count(), 0);
    });
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let path = root.join(case["input"].as_str().unwrap());
        let raw = std::fs::read(&path).unwrap();
        assert_eq!(hash(&raw), case["input_sha256"]);
        assert_eq!(
            _native::codec_static_identity::metadata(&path, true).unwrap()["blank"],
            case["expected"]["blank"]
        );
        let expected = !case["expected"]["blank"].as_bool().unwrap();
        assert_eq!(
            _native::codec_worker::execute_typed_at(
                None,
                &json!({"operation":"usable","source":path,"format":"static"})
            )
            .unwrap(),
            expected
        );
        Python::attach(|py| {
            assert_eq!(
                _native::publication_media::publication_static_usable(py, path.clone()).unwrap(),
                expected
            )
        });
        assert_eq!(std::fs::read(path).unwrap(), raw);
    }
    assert_eq!(std::fs::read(source).unwrap(), before);
    scratch.clean().unwrap();
}

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
#[test]
fn original_static_fingerprint_and_conversion_errors_keep_class_and_outputs() {
    let _image_api = IMAGE_API.lock().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/media-continuation/static-errors.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(fixture["source_runtime"], "CPython3.11.9");
    assert_eq!(fixture["Pillow"], "12.3.0");
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    let scratch = tooling_fixture::Root::new("typed static errors");
    for (index, case) in cases.iter().enumerate() {
        let source = root.join(case["input"].as_str().unwrap());
        let before = std::fs::read(&source).unwrap();
        assert_eq!(hash(&before), case["input_sha256"]);
        let output = scratch.0.join(format!("{index}.png"));
        let operation = case["operation"].as_str().unwrap();
        let request = json!({"operation":operation,"source":source,"format":"static","target_format":"static","output":output});
        let result = _native::codec_worker::execute_typed_at(None, &request);
        if let Some(expected) = case.get("expected") {
            let result = result.unwrap();
            if operation == "fingerprint" {
                assert_eq!(result, *expected);
            } else {
                let bytes = std::fs::read(&output).unwrap();
                assert_eq!(hash(&bytes), expected["sha256"]);
                assert_eq!(bytes.len() as u64, expected["bytes"].as_u64().unwrap());
                assert_eq!(
                    _native::codec_static_identity::fingerprint(&output).unwrap()["content_key"],
                    result["content_key"]
                );
            }
        } else {
            let error = result.unwrap_err();
            assert_eq!(error.kind, case["error"]["kind"].as_str().unwrap());
            assert!(error.handled);
            let message = error.message.replace(
                &_native::python_repr::quoted(&source.to_string_lossy()),
                "'$source'",
            );
            assert_eq!(message, case["error"]["message"].as_str().unwrap());
        }
        assert_eq!(output.exists(), case["output_exists"].as_bool().unwrap());
        assert_eq!(std::fs::read(source).unwrap(), before);
    }
    scratch.clean().unwrap();
}
