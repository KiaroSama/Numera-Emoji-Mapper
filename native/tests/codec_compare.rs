//! Static identity remains tri-state and requires both structure and colour agreement.
use pyo3::prelude::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
#[path = "support/static_premultiply_faults.rs"]
mod static_premultiply_faults;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

static PATH_API: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[pyclass]
struct PathFault {
    kind: &'static str,
    hits: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
#[pymethods]
impl PathFault {
    #[pyo3(signature=(*_args, **_kwargs))]
    fn __call__(
        &self,
        _args: &Bound<'_, pyo3::types::PyTuple>,
        _kwargs: Option<&Bound<'_, pyo3::types::PyDict>>,
    ) -> PyResult<()> {
        self.hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(match self.kind {
            "OSError" => pyo3::exceptions::PyOSError::new_err("controlled comparison read"),
            "TypeError" => pyo3::exceptions::PyTypeError::new_err("controlled comparison read"),
            "KeyboardInterrupt" => {
                pyo3::exceptions::PyKeyboardInterrupt::new_err("controlled comparison read")
            }
            _ => pyo3::exceptions::PySystemExit::new_err("controlled comparison read"),
        })
    }
}
struct RestorePath<'py>(Bound<'py, PyAny>, &'static str, Bound<'py, PyAny>);
impl Drop for RestorePath<'_> {
    fn drop(&mut self) {
        self.0
            .setattr(self.1, &self.2)
            .expect("restore external path method");
    }
}

#[test]
fn comparison_shortcut_precedes_tools_and_preserves_actual_path_errors() {
    let _path_api = PATH_API.lock().unwrap();
    use serde_json::json;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let root = tooling_fixture::Root::new("comparison byte boundary café");
    let left = root.0.join("left.webm");
    let right = root.0.join("right.webm");
    std::fs::write(&left, b"equal undecodable bytes").unwrap();
    std::fs::write(&right, b"equal undecodable bytes").unwrap();
    let request = json!({"operation":"compare","source":left,"other":right,"format":"video"});
    assert_eq!(
        _native::codec_worker::execute_typed_at(None, &request).unwrap(),
        true
    );
    let mut image = request.clone();
    image["operation"] = json!("same_image");
    assert!(
        _native::codec_worker::execute_typed_at(None, &image).is_err(),
        "same_image must not take the byte shortcut"
    );
    Python::initialize();
    Python::attach(|py| {
        let class = py.import("pathlib").unwrap().getattr("Path").unwrap();
        for method in ["stat", "read_bytes"] {
            let restore = RestorePath(class.clone(), method, class.getattr(method).unwrap());
            for kind in ["OSError", "TypeError", "KeyboardInterrupt", "SystemExit"] {
                let hits = Arc::new(AtomicUsize::new(0));
                class
                    .setattr(
                        method,
                        Py::new(
                            py,
                            PathFault {
                                kind,
                                hits: hits.clone(),
                            },
                        )
                        .unwrap(),
                    )
                    .unwrap();
                let error = _native::codec_worker::execute_typed_at(None, &request).unwrap_err();
                assert_eq!(error.kind, kind);
                assert_eq!(error.message, "controlled comparison read");
                assert_eq!(error.handled, kind == "OSError");
                assert_eq!(
                    hits.load(Ordering::SeqCst),
                    1,
                    "first failure must stop the shortcut"
                );
                if method == "read_bytes" {
                    std::fs::write(&right, b"different size").unwrap();
                    let error =
                        _native::codec_worker::execute_typed_at(None, &request).unwrap_err();
                    assert_eq!(error.kind, "ValueError");
                    assert_eq!(
                        error.message,
                        "video worker requires an explicit project root"
                    );
                    assert_eq!(
                        hits.load(Ordering::SeqCst),
                        1,
                        "different sizes must not read either file"
                    );
                    std::fs::write(&right, b"equal undecodable bytes").unwrap();
                }
            }
            drop(restore);
        }
    });
    let animated = serde_json::json!({"operation":"same_image","source":left,"other":right,"format":"animated"});
    Python::attach(|py| {
        let class = py.import("pathlib").unwrap().getattr("Path").unwrap();
        let restore = RestorePath(
            class.clone(),
            "read_bytes",
            class.getattr("read_bytes").unwrap(),
        );
        for kind in ["OSError", "TypeError", "KeyboardInterrupt", "SystemExit"] {
            let hits = Arc::new(AtomicUsize::new(0));
            class
                .setattr(
                    "read_bytes",
                    Py::new(
                        py,
                        PathFault {
                            kind,
                            hits: hits.clone(),
                        },
                    )
                    .unwrap(),
                )
                .unwrap();
            let result = _native::codec_worker::execute_typed_at(None, &animated);
            if matches!(kind, "OSError" | "TypeError") {
                assert_eq!(result.unwrap(), Value::Null);
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.kind, kind);
                assert_eq!(error.message, "controlled comparison read");
                assert!(!error.handled);
            }
            assert_eq!(
                hits.load(Ordering::SeqCst),
                if kind == "OSError" { 2 } else { 1 }
            );
        }
        drop(restore);
    });
    let left_json = b"{\"v\":\"5.5\",\"fr\":30}";
    let right_json = b"{\"fr\":30, \"v\":\"5.5\"}";
    std::fs::write(&left, left_json).unwrap();
    std::fs::write(&right, right_json).unwrap();
    assert_eq!(
        _native::codec_worker::execute_typed_at(None, &animated).unwrap(),
        true
    );
    std::fs::write(&right, b"{\"fr\":31,\"v\":\"5.5\"}").unwrap();
    assert_eq!(
        _native::codec_worker::execute_typed_at(None, &animated).unwrap(),
        Value::Null
    );
    std::fs::write(&left, b"equal undecodable bytes").unwrap();
    std::fs::write(&right, b"equal undecodable bytes").unwrap();
    let native = _native::codec_lottie_identity::same_image_typed(
        std::path::Path::new("relative.tgs"),
        &right,
    )
    .unwrap_err();
    assert_eq!(native.kind, "UnverifiedCodecError");
    assert_eq!(
        native.message,
        "Lottie source must be an absolute regular non-symlink file"
    );
    assert!(!native.handled);
    assert_eq!(std::fs::read(left).unwrap(), b"equal undecodable bytes");
    assert_eq!(std::fs::read(right).unwrap(), b"equal undecodable bytes");
    root.clean().unwrap();
}

#[test]
fn source_static_comparison_preserves_colour_alpha_and_unknown() {
    let _path_api = PATH_API.lock().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-compare.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    for case in cases {
        let a = root.join(case["a"].as_str().unwrap());
        let b = root.join(case["b"].as_str().unwrap());
        for (path, key) in [(&a, "a_sha256"), (&b, "b_sha256")] {
            assert_eq!(
                Sha256::digest(std::fs::read(path).unwrap())
                    .iter()
                    .map(|v| format!("{v:02x}"))
                    .collect::<String>(),
                case[key]
            );
        }
        let request =
            serde_json::json!({"operation":"same_image","source":a,"other":b,"format":"static"});
        assert_eq!(
            _native::codec_worker::execute(&request).unwrap(),
            case["expected"]
        );
        assert_eq!(
            _native::codec_worker::execute_typed_at(None, &request).unwrap(),
            case["expected"],
        );
        assert_eq!(
            serde_json::json!(_native::codec_image::same_image(&a, &b)),
            case["expected"],
            "{}",
            case["name"]
        );
    }
}

#[test]
fn typed_static_comparison_finishes_both_keys_before_hash_and_catches_only_exception() {
    let _path_api = PATH_API.lock().unwrap();
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-compare.json"
    ))
    .unwrap();
    let case = &fixture["cases"][1];
    assert_eq!(case["name"], "red-near-red");
    let left = root.join(case["a"].as_str().unwrap());
    let right = root.join(case["b"].as_str().unwrap());
    let before = [&left, &right].map(|path| std::fs::read(path).unwrap());
    for (bytes, key) in before.iter().zip(["a_sha256", "b_sha256"]) {
        assert_eq!(
            Sha256::digest(bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            case[key]
        );
    }
    Python::initialize();
    Python::attach(|py| {
        let module = py.import("PIL.Image").unwrap();
        let class = module.getattr("Image").unwrap();
        let original_open = module.getattr("open").unwrap();
        let restore_split = RestorePath(class.clone(), "split", class.getattr("split").unwrap());
        let split_hits = Arc::new(AtomicUsize::new(0));
        class
            .setattr(
                "split",
                Py::new(
                    py,
                    PathFault {
                        kind: "TypeError",
                        hits: split_hits.clone(),
                    },
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            _native::codec_image::same_image_typed(&left, &left).unwrap(),
            Some(true)
        );
        assert_eq!(
            split_hits.load(Ordering::SeqCst),
            0,
            "equal keys never compute phash"
        );
        drop(restore_split);
        let class_restore = RestorePath(class.clone(), "split", class.getattr("split").unwrap());
        let trace_open_restore = RestorePath(
            module.as_any().clone(),
            "open",
            module.getattr("open").unwrap(),
        );
        let paths = Arc::new(std::sync::Mutex::new(Vec::new()));
        let observed = paths.clone();
        let original = original_open.clone().unbind();
        let traced = pyo3::types::PyCFunction::new_closure(
            py,
            None,
            None,
            move |args, kwargs| -> PyResult<Py<PyAny>> {
                observed.lock().unwrap().push(PathBuf::from(
                    args.get_item(0)?
                        .call_method0("__fspath__")?
                        .extract::<String>()?,
                ));
                original
                    .bind(args.py())
                    .call(args, kwargs)
                    .map(Bound::unbind)
            },
        )
        .unwrap();
        module.setattr("open", traced).unwrap();
        class
            .setattr(
                "split",
                Py::new(
                    py,
                    PathFault {
                        kind: "TypeError",
                        hits: Arc::new(AtomicUsize::new(0)),
                    },
                )
                .unwrap(),
            )
            .unwrap();
        let key_order = _native::codec_image::same_image_typed(&left, &right);
        drop(class_restore);
        drop(trace_open_restore);
        assert_eq!(key_order.unwrap(), None);
        assert_eq!(
            paths.lock().unwrap().as_slice(),
            [&left, &right, &left].map(|p| p.to_path_buf()).as_slice(),
            "right key precedes first phash failure"
        );
        for method in ["open", "split"] {
            let target = if method == "open" {
                module.as_any().clone()
            } else {
                class.clone()
            };
            let restore = RestorePath(target.clone(), method, target.getattr(method).unwrap());
            for kind in ["OSError", "TypeError", "KeyboardInterrupt", "SystemExit"] {
                let calls = Arc::new(AtomicUsize::new(0));
                let fault = Py::new(
                    py,
                    PathFault {
                        kind,
                        hits: calls.clone(),
                    },
                )
                .unwrap();
                target.setattr(method, fault).unwrap();
                let request = serde_json::json!({"operation":"same_image","source":left,"other":right,"format":"static"});
                let result = _native::codec_worker::execute_typed_at(None, &request);
                if matches!(kind, "OSError" | "TypeError") {
                    assert_eq!(result.unwrap(), Value::Null);
                } else {
                    let error = result.unwrap_err();
                    assert_eq!(error.kind, kind);
                    assert!(!error.handled);
                }
                assert_eq!(calls.load(Ordering::SeqCst), 1);
                if method == "split" {
                    let raw = [20, 40, 60, 255].repeat(64 * 64);
                    let error = _native::codec_image::same_frame_typed(&raw, &raw).unwrap_err();
                    assert_eq!(
                        error.kind, kind,
                        "_dhash itself is not ordinary-caught in frame comparison"
                    );
                    assert_eq!(_native::codec_image::same_frame(&raw, &raw), None);
                }
                assert_eq!(_native::codec_image::same_image(&left, &right), None);
            }
            drop(restore);
        }
        let tobytes_restore =
            RestorePath(class.clone(), "tobytes", class.getattr("tobytes").unwrap());
        let count = Arc::new(AtomicUsize::new(0));
        let hits = count.clone();
        let malformed = pyo3::types::PyCFunction::new_closure(
            py,
            None,
            None,
            move |args, _| -> PyResult<Py<pyo3::types::PyBytes>> {
                let index = hits.fetch_add(1, Ordering::SeqCst);
                let bytes = match index {
                    0 => vec![1; 64 * 64 * 4],
                    1 => vec![2; 64 * 64 * 4],
                    _ => Vec::new(),
                };
                Ok(pyo3::types::PyBytes::new(args.py(), &bytes).unbind())
            },
        )
        .unwrap();
        class.setattr("tobytes", malformed).unwrap();
        let result = _native::codec_image::same_image_typed(&left, &right);
        drop(tobytes_restore);
        assert_eq!(
            result.unwrap_err().kind,
            "UnverifiedCodecError",
            "a malformed dHash buffer is native unknown, not an ordinary source failure"
        );
        let trace = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = trace.clone();
        let open = original_open.unbind();
        let replacement = pyo3::types::PyCFunction::new_closure(
            py,
            None,
            None,
            move |args, kwargs| -> PyResult<Py<PyAny>> {
                let text = args
                    .get_item(0)?
                    .call_method0("__fspath__")?
                    .extract::<String>()?;
                seen.lock().unwrap().push(PathBuf::from(text));
                open.bind(args.py()).call(args, kwargs).map(Bound::unbind)
            },
        )
        .unwrap();
        let restore_open = RestorePath(
            module.as_any().clone(),
            "open",
            module.getattr("open").unwrap(),
        );
        module.setattr("open", replacement).unwrap();
        let result = _native::codec_image::same_image_typed(&left, &right);
        drop(restore_open);
        assert_eq!(result.unwrap(), Some(true));
        let order = trace.lock().unwrap();
        let expected = [&left, &right, &left, &right, &left, &right].map(|p| p.to_path_buf());
        assert_eq!(
            order.as_slice(),
            expected.as_slice(),
            "both keys, then both phashes, then both colours"
        );
    });
    static_premultiply_faults::check(true);
    let black = [0, 0, 0, 255].repeat(64 * 64);
    let boundary = [10, 11, 11, 255].repeat(64 * 64);
    let mut above_boundary = boundary.clone();
    above_boundary[2] = 12;
    assert_eq!(
        _native::codec_image::frame_phash_typed(&black).unwrap(),
        Some(0)
    );
    for (label, right, expected) in [
        ("mean 7.5", [10, 10, 10, 255].repeat(64 * 64), true),
        ("mean 9", [12, 12, 12, 255].repeat(64 * 64), false),
        ("mean exactly 8", boundary, true),
        ("mean 8 plus 1/16384", above_boundary, false),
    ] {
        // Pillow's integer RGB-to-L conversion maps both (10,11,11) and (10,11,12)
        // to 11. Their dHashes stay zero, so the last refusal must be the colour test.
        assert_eq!(
            _native::codec_image::frame_phash_typed(&right).unwrap(),
            Some(0),
            "{label}"
        );
        let unchanged = right.clone();
        assert_eq!(
            _native::codec_image::same_frame_typed(&black, &right).unwrap(),
            Some(expected),
            "{label}"
        );
        assert_eq!(
            _native::codec_image::same_frame_typed(&right, &black).unwrap(),
            Some(expected),
            "reverse {label}"
        );
        assert_eq!(right, unchanged, "{label} input changed");
    }
    assert_eq!(black, [0, 0, 0, 255].repeat(64 * 64));
    let error = _native::codec_image::same_frame_typed(b"", b"").unwrap_err();
    assert_eq!(error.kind, "UnverifiedCodecError");
    assert_eq!(_native::codec_image::same_frame(b"", b""), None);
    let error = _native::codec_image::same_image_typed(std::path::Path::new("relative.png"), &left)
        .unwrap_err();
    assert_eq!(error.kind, "UnverifiedCodecError");
    for (path, bytes) in [&left, &right].into_iter().zip(before) {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn original_upload_artwork_accepts_lossy_reencoding_but_not_foreign_colour_or_unknown() {
    let _path_api = PATH_API.lock().unwrap();
    let root = tooling_fixture::Root::new("original comparison artwork café");
    Python::initialize();
    let (ours, other, encoded, squares) = Python::attach(|py| -> PyResult<_> {
        let module = py.import("PIL.Image")?;
        let draw = py.import("PIL.ImageDraw")?;
        let mut art = Vec::new();
        for (name, seed) in [("ours", 0), ("other", 9)] {
            let image = module.call_method1("new", ("RGBA", (100, 100), (0, 0, 0, 0)))?;
            let result = (|| -> PyResult<_> {
                let pen = draw.call_method1("Draw", (&image,))?;
                let options = pyo3::types::PyDict::new(py);
                options.set_item("fill", (20 + seed * 90, 120, 220 - seed * 60, 255))?;
                pen.call_method("ellipse", ([8, 8, 92, 92],), Some(&options))?;
                options.set_item("fill", (255, 255, 255, 255))?;
                options.set_item("radius", 8)?;
                pen.call_method(
                    "rounded_rectangle",
                    ([30 - seed * 12, 34, 70, 66 + seed * 14],),
                    Some(&options),
                )?;
                let path = root.0.join(format!("{name}.png"));
                image.call_method1("save", (&path, "PNG"))?;
                Ok(path)
            })();
            let closed = image.call_method0("close");
            closed?;
            art.push(result?);
        }
        let image = module.call_method1("open", (&art[0],))?;
        let rgba = image.call_method1("convert", ("RGBA",))?;
        let encoded = root.0.join("ours.webp");
        let options = pyo3::types::PyDict::new(py);
        options.set_item("format", "WEBP")?;
        options.set_item("quality", 90)?;
        options.set_item("exact", true)?;
        let saved = rgba.call_method("save", (&encoded,), Some(&options));
        let rgba_closed = rgba.call_method0("close");
        let image_closed = image.call_method0("close");
        saved?;
        rgba_closed?;
        image_closed?;
        let mut squares = Vec::new();
        for (name, color) in [("red", (200, 30, 30, 255)), ("green", (10, 200, 40, 255))] {
            let image = module.call_method1("new", ("RGBA", (100, 100), (0, 0, 0, 0)))?;
            let result = (|| -> PyResult<_> {
                let options = pyo3::types::PyDict::new(py);
                options.set_item("fill", color)?;
                draw.call_method1("Draw", (&image,))?.call_method(
                    "rectangle",
                    ([20, 20, 79, 79],),
                    Some(&options),
                )?;
                let path = root.0.join(format!("{name}.png"));
                image.call_method1("save", (&path, "PNG"))?;
                Ok(path)
            })();
            image.call_method0("close")?;
            squares.push(result?);
        }
        Ok((art.remove(0), art.remove(0), encoded, squares))
    })
    .unwrap();
    let before = [&ours, &other, &encoded, &squares[0], &squares[1]]
        .map(|path| std::fs::read(path).unwrap());
    let info = |path: &std::path::Path| _native::codec_static_identity::fingerprint(path).unwrap();
    assert_ne!(
        info(&ours)["content_key"],
        info(&encoded)["content_key"],
        "fixture must reproduce exact-key drift"
    );
    assert_eq!(
        _native::codec_image::same_image(&encoded, &ours),
        Some(true)
    );
    assert_eq!(_native::codec_image::same_image(&ours, &ours), Some(true));
    assert_eq!(
        _native::codec_image::same_image(&encoded, &other),
        Some(false)
    );
    let hashes = squares
        .iter()
        .map(|path| info(path)["phash"].as_u64().unwrap())
        .collect::<Vec<_>>();
    assert!(
        (hashes[0] ^ hashes[1]).count_ones() <= 6,
        "fixture must exercise the structural collision"
    );
    assert_eq!(
        _native::codec_image::same_image(&squares[0], &squares[1]),
        Some(false)
    );
    assert_eq!(
        _native::codec_worker::execute(&serde_json::json!({
            "operation":"same_image","source":ours,"other":other,"format":"animated"
        }))
        .unwrap(),
        Value::Null
    );
    let bad = root.0.join("torn.png");
    std::fs::write(&bad, b"not an image").unwrap();
    assert_eq!(_native::codec_image::same_image(&bad, &ours), None);
    assert_eq!(_native::codec_image::same_image(&ours, &bad), None);
    assert_eq!(std::fs::read(&bad).unwrap(), b"not an image");
    for (path, bytes) in [&ours, &other, &encoded, &squares[0], &squares[1]]
        .into_iter()
        .zip(before)
    {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    root.clean().unwrap();
}
