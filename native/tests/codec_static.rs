//! Static identity is source-generated, including hidden RGB and original image mode.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[path = "support/static_fitting_fixture.rs"]
mod static_fitting_fixture;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
static SVG_API: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn static_conversion_preserves_source_png_pixels_and_blank_refusal() {
    let _svg = SVG_API.lock().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(
            std::env::var_os("NUMERA_STATIC_CONVERT_ORACLE")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("tests/fixtures/native/static-convert.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let (_, provider) = _native::source_static_oracle::fixtures().unwrap();
    _native::source_static_oracle::fixture_identity(&fixture, &provider).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    let directory = root
        .join("logs")
        .join(format!("static conversion {}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for case in cases {
        let source = root.join(case["input"].as_str().unwrap());
        let output = directory.join(format!("{}.png", case["name"].as_str().unwrap()));
        let result = _native::codec_image::to_static_png_typed(&source, &output);
        if let Some(error) = case["error"].as_str() {
            let actual = result.unwrap_err();
            assert_eq!(actual.kind, "MediaError");
            assert!(actual.handled);
            assert_eq!(actual.message, error);
            assert!(!output.exists());
        } else {
            assert_eq!(result.unwrap(), case["expected"], "{}", case["name"]);
            let bytes = std::fs::read(&output).unwrap();
            assert_eq!(
                Sha256::digest(&bytes)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>(),
                case["expected"]["png_sha256"]
            );
            assert!(_native::codec_image::to_static_png(&source, &output).is_err());
            assert_eq!(std::fs::read(&output).unwrap(), bytes);
            std::fs::remove_file(output).unwrap();
        }
    }
    original_blank_faint_and_visible_conversions(&directory);
    original_partial_alpha_gradient(&directory);
    static_fitting_fixture::check(
        &root.join("tests/fixtures/native/static-codec/alpha-edge.png"),
        false,
    );
    std::fs::remove_dir(directory).unwrap();
}

fn original_blank_faint_and_visible_conversions(directory: &std::path::Path) {
    use pyo3::prelude::*;

    Python::initialize();
    for (name, color, accepted) in [
        ("blank", (0, 0, 0, 0), false),
        ("faint", (255, 0, 0, 10), false),
        ("solid", (255, 0, 0, 255), true),
    ] {
        let source = directory.join(format!("{name}.png"));
        let output = directory.join(format!("{name}-out.png"));
        Python::attach(|py| -> PyResult<()> {
            let module = py.import("PIL.Image")?;
            assert_eq!(
                module.getattr("__version__")?.extract::<String>()?,
                "12.3.0"
            );
            let image = module.call_method1("new", ("RGBA", (120, 90), color))?;
            let saved = image.call_method1("save", (&source, "PNG"));
            let closed = image.call_method0("close");
            saved?;
            closed?;
            Ok(())
        })
        .unwrap();
        let before = std::fs::read(&source).unwrap();
        let result = _native::codec_image::to_static_png_typed(&source, &output);
        if accepted {
            assert_eq!(result.unwrap()["size"], serde_json::json!([100, 100]));
            let actual = _native::codec_static_identity::metadata(&output, true).unwrap();
            assert_eq!(actual["size"], serde_json::json!([100, 100]));
            assert_eq!(actual["mode"], "RGBA");
            assert_eq!(actual["blank"], false);
            std::fs::remove_file(&output).unwrap();
        } else {
            let error = result.unwrap_err();
            assert_eq!(error.kind, "MediaError", "{name}");
            assert!(error.handled, "{name}");
            assert_eq!(
                error.message,
                format!("{name}.png: image is blank (no visible pixels)")
            );
            assert!(!output.exists(), "refused {name} left output");
        }
        assert_eq!(
            std::fs::read(&source).unwrap(),
            before,
            "{name} input changed"
        );
        std::fs::remove_file(source).unwrap();
        assert!(
            std::fs::read_dir(directory).unwrap().next().is_none(),
            "{name} left staging"
        );
    }
}

fn original_partial_alpha_gradient(directory: &std::path::Path) {
    use pyo3::prelude::*;
    use pyo3::types::PyBytes;

    let source = directory.join("alpha-gradient.png");
    let output = directory.join("alpha-gradient-out.png");
    let mut raw = Vec::with_capacity(100 * 100 * 4);
    for _y in 0..100 {
        for x in 0..100_u32 {
            raw.extend_from_slice(&[0, 0, 255, (x * 255 / 99).max(1) as u8]);
        }
    }
    Python::attach(|py| -> PyResult<()> {
        let image = py
            .import("PIL.Image")?
            .call_method1("frombytes", ("RGBA", (100, 100), PyBytes::new(py, &raw)))?;
        let saved = image.call_method1("save", (&source, "PNG"));
        let closed = image.call_method0("close");
        saved?;
        closed?;
        Ok(())
    })
    .unwrap();
    let before = std::fs::read(&source).unwrap();
    let converted = _native::codec_image::to_static_png_typed(&source, &output).unwrap();
    assert_eq!(converted["size"], serde_json::json!([100, 100]));
    Python::attach(|py| -> PyResult<()> {
        let image = py.import("PIL.Image")?.call_method1("open", (&output,))?;
        let samples = (|| -> PyResult<Vec<[u8; 4]>> {
            assert_eq!(image.getattr("mode")?.extract::<String>()?, "RGBA");
            [25, 50, 75]
                .into_iter()
                .map(|x| image.call_method1("getpixel", ((x, 50),))?.extract())
                .collect()
        })();
        let closed = image.call_method0("close");
        let samples = samples?;
        closed?;
        for (x, pixel) in [25, 50, 75].into_iter().zip(samples) {
            assert_eq!(pixel, [0, 0, 255, (x * 255 / 99) as u8]);
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(std::fs::read(&source).unwrap(), before);
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(output).unwrap();
    assert!(std::fs::read_dir(directory).unwrap().next().is_none());
}

#[test]
fn original_static_hashing_detection_and_visibility_recipes_keep_their_contracts() {
    use pyo3::prelude::*;
    let _svg = SVG_API.lock().unwrap();
    use pyo3::types::PyDict;
    use serde_json::json;

    fn save(image: &Bound<'_, PyAny>, path: &std::path::Path, format: &str) -> PyResult<()> {
        let options = PyDict::new(image.py());
        options.set_item("format", format)?;
        if format == "WEBP" {
            options.set_item("lossless", true)?;
        }
        let saved = image.call_method("save", (path,), Some(&options));
        let closed = image.call_method0("close");
        saved?;
        closed?;
        Ok(())
    }

    let scratch = tooling_fixture::Root::new("original static identities café");
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let module = py.import("PIL.Image")?;
        assert_eq!(
            module.getattr("__version__")?.extract::<String>()?,
            "12.3.0"
        );
        for (name, hidden) in [("shape", false), ("hidden", true)] {
            let image = module.call_method1("new", ("RGBA", (64, 64), (0, 0, 0, 0)))?;
            let prepared = (|| -> PyResult<()> {
                for x in 0..64 {
                    for y in 0..64 {
                        if (10..40).contains(&x) && (8..52).contains(&y) {
                            image.call_method1("putpixel", ((x, y), (30, 160, 220, 255)))?;
                        } else if hidden {
                            image.call_method1(
                                "putpixel",
                                ((x, y), ((x * 7) % 256, (y * 13) % 256, 90, 0)),
                            )?;
                        }
                    }
                }
                Ok(())
            })();
            if let Err(error) = prepared {
                image.call_method0("close")?;
                return Err(error);
            }
            save(&image, &scratch.0.join(format!("{name}.png")), "PNG")?;
        }
        for (name, start, end) in [("left", 4, 30), ("right", 34, 60)] {
            let image = module.call_method1("new", ("RGBA", (64, 64), (0, 0, 0, 0)))?;
            let prepared = (|| -> PyResult<()> {
                for x in start..end {
                    for y in 4..60 {
                        image.call_method1("putpixel", ((x, y), (200, 40, 40, 255)))?;
                    }
                }
                Ok(())
            })();
            if let Err(error) = prepared {
                image.call_method0("close")?;
                return Err(error);
            }
            save(&image, &scratch.0.join(format!("{name}.png")), "PNG")?;
        }
        let stripes = module.call_method1("new", ("RGB", (40, 40), (10, 90, 200)))?;
        let prepared = (|| -> PyResult<Bound<'_, PyAny>> {
            for x in (0..40).step_by(3) {
                for y in 0..40 {
                    stripes.call_method1("putpixel", ((x, y), (240, 240, 10)))?;
                }
            }
            stripes.call_method1("convert", ("RGBA",))
        })();
        let rgba = match prepared {
            Ok(rgba) => rgba,
            Err(error) => {
                stripes.call_method0("close")?;
                return Err(error);
            }
        };
        let saved = save(&stripes, &scratch.0.join("stripes-rgb.png"), "PNG");
        let rgba_saved = save(&rgba, &scratch.0.join("stripes-rgba.png"), "PNG");
        saved?;
        rgba_saved?;
        for (name, size, color) in [
            ("red", (80, 60), (200, 30, 30, 255)),
            ("same", (80, 60), (200, 30, 30, 255)),
            ("green", (80, 60), (30, 200, 30, 255)),
            ("fingerprint", (80, 60), (200, 40, 60, 255)),
            ("detected", (80, 60), (0, 128, 255, 255)),
            ("tall", (40, 90), (10, 20, 30, 255)),
            ("gray", (64, 64), (120, 120, 120, 255)),
        ] {
            let image = module.call_method1("new", ("RGBA", size, color))?;
            if matches!(name, "gray" | "detected") {
                let clone = image.call_method0("copy");
                let png = save(&image, &scratch.0.join(format!("{name}.png")), "PNG");
                let clone = clone?;
                let webp = if name == "gray" {
                    save(&clone, &scratch.0.join("gray.webp"), "WEBP")
                } else {
                    let saved =
                        clone.call_method1("save", (scratch.0.join("detected.webp"), "WEBP"));
                    let closed = clone.call_method0("close");
                    saved.and(closed).map(|_| ())
                };
                png?;
                webp?;
            } else {
                save(&image, &scratch.0.join(format!("{name}.png")), "PNG")?;
            }
        }
        for (name, alpha, visible) in [
            ("blank", 0, 0),
            ("visible", 255, 10000),
            ("floor", 10, 10000),
            ("above", 11, 10000),
            ("eight", 255, 8),
            ("nine", 255, 9),
        ] {
            let image = module.call_method1(
                "new",
                (
                    "RGBA",
                    (100, 100),
                    (5, 5, 5, if visible == 10000 { alpha } else { 0 }),
                ),
            )?;
            let prepared = (|| -> PyResult<()> {
                if visible < 10000 {
                    for x in 0..visible {
                        image.call_method1("putpixel", ((x, 0), (255, 255, 255, alpha)))?;
                    }
                }
                Ok(())
            })();
            if let Err(error) = prepared {
                image.call_method0("close")?;
                return Err(error);
            }
            save(&image, &scratch.0.join(format!("{name}.png")), "PNG")?;
        }
        Ok(())
    })
    .unwrap();
    let inspect =
        |name: &str| _native::codec_static_identity::fingerprint(&scratch.0.join(name)).unwrap();
    let phash = |name: &str| {
        inspect(name)["phash"]
            .as_u64()
            .expect("real image must have a dHash")
    };
    assert_eq!(
        phash("shape.png"),
        phash("hidden.png"),
        "invisible RGB reached dHash"
    );
    assert!((phash("left.png") ^ phash("right.png")).count_ones() > 6);
    assert_eq!(phash("stripes-rgb.png"), phash("stripes-rgba.png"));
    assert_eq!(
        inspect("red.png")["content_key"],
        inspect("same.png")["content_key"]
    );
    assert_ne!(
        inspect("red.png")["content_key"],
        inspect("green.png")["content_key"]
    );
    assert!((phash("gray.png") ^ phash("gray.webp")).count_ones() <= 5);
    let fingerprint = inspect("fingerprint.png");
    assert_eq!(
        fingerprint["content_key"],
        "s:26778a8fe55e1ff5eca423cee089e3cb"
    );
    assert_eq!(fingerprint["phash"], 0);
    assert_eq!(
        _native::codec_image::source_dhash(&scratch.0.join("fingerprint.png")).unwrap(),
        0
    );
    let worker = _native::codec_worker::execute_typed_at(
        None,
        &json!({
            "operation":"fingerprint", "source":scratch.0.join("fingerprint.png"), "format":"static"
        }),
    )
    .unwrap();
    assert_eq!(worker["content_key"], fingerprint["content_key"]);
    assert_eq!(worker["phash"], fingerprint["phash"]);
    for name in ["detected.png", "detected.webp"] {
        assert_eq!(
            _native::codec_format::detect(&scratch.0.join(name)).unwrap(),
            "static"
        );
    }
    for (name, blank) in [
        ("blank", true),
        ("visible", false),
        ("floor", true),
        ("above", false),
        ("eight", true),
        ("nine", false),
    ] {
        assert_eq!(inspect(&format!("{name}.png"))["blank"], blank, "{name}");
    }
    let source = scratch.0.join("tall.png");
    let before = std::fs::read(&source).unwrap();
    let output = scratch.0.join("tall-out.png");
    _native::codec_image::to_static_png_typed(&source, &output).unwrap();
    assert_eq!(
        _native::codec_static_identity::metadata(&output, false).unwrap()["size"],
        json!([100, 100])
    );
    assert_eq!(std::fs::read(source).unwrap(), before);
    let bad = scratch.0.join("bad.png");
    std::fs::write(&bad, b"not an image at all").unwrap();
    let direct = _native::codec_static_identity::fingerprint(&bad).unwrap_err();
    let worker = _native::codec_worker::execute_typed_at(
        None,
        &json!({"operation":"fingerprint","format":"static","source":bad}),
    )
    .unwrap_err();
    assert_eq!(direct.kind, "UnidentifiedImageError");
    assert_eq!(worker.kind, direct.kind);
    assert_eq!(worker.message, direct.message);
    assert_eq!(std::fs::read(bad).unwrap(), b"not an image at all");
    assert!(!_native::process_owner::cleanup_pending(&scratch.0));
    scratch.clean().unwrap();
}

#[test]
fn original_svg_and_keyword_consumers_do_not_import_retired_applications() {
    let _svg = SVG_API.lock().unwrap();
    use pyo3::prelude::*;
    use pyo3::types::PyDict;

    let scratch = tooling_fixture::Root::new("original SVG boundary café");
    let source = scratch.0.join("art.svg");
    let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"64\"><rect width=\"64\" height=\"64\" fill=\"#ff0000\"/></svg>";
    std::fs::write(&source, svg).unwrap();
    let keywords = scratch.0.join("keywords.csv");
    let csv = b"ticker,keywords\nABC,\"abc, Alpha\"\n";
    std::fs::write(&keywords, csv).unwrap();
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let modules = py
            .import("sys")?
            .getattr("modules")?
            .cast_into::<PyDict>()?;
        let names = [
            "emojikit.build_pack",
            "emojikit.make_emoji_pngs",
            "coins._paprika_api",
            "coins._provider_publish",
        ];
        let previous = names
            .iter()
            .map(|name| modules.get_item(*name))
            .collect::<PyResult<Vec<_>>>()?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> PyResult<()> {
            for name in names {
                modules.set_item(name, py.None())?;
            }
            assert!(
                _native::codec_process::codec_budget(None)
                    .unwrap()
                    .wall
                    .as_secs()
                    > 0
            );
            let image = _native::codec_svg::load_rgba(py, &source)?;
            let inspected = (|| -> PyResult<(String, [u8; 4])> {
                Ok((
                    image.getattr("mode")?.extract()?,
                    image.call_method1("getpixel", ((32, 32),))?.extract()?,
                ))
            })();
            let closed = image.call_method0("close");
            let (mode, pixel) = inspected?;
            closed?;
            assert_eq!(mode, "RGBA");
            assert_eq!(pixel, [255, 0, 0, 255]);
            let output = scratch.0.join("fitted.png");
            assert_eq!(
                _native::codec_image::to_static_png_typed(&source, &output).unwrap()["size"],
                serde_json::json!([100, 100])
            );
            assert_eq!(
                _native::ticker_keywords::load(&keywords).unwrap(),
                [("abc".into(), "abc, Alpha".into())].into_iter().collect()
            );
            for name in names {
                assert!(
                    modules.get_item(name)?.unwrap().is_none(),
                    "{name} was imported"
                );
            }
            assert!(!scratch.0.join("catalog.db").exists());
            assert_eq!(std::fs::read(&source).unwrap(), svg);
            assert_eq!(std::fs::read(&keywords).unwrap(), csv);
            assert_eq!(std::fs::read_dir(&scratch.0).unwrap().count(), 3);
            Ok(())
        }));
        // Restore only the entries owned here, including on an assertion unwind.
        for (name, before) in names.iter().zip(previous) {
            match &before {
                Some(value) => modules.set_item(*name, value)?,
                None if modules.contains(*name)? => modules.del_item(*name)?,
                None => {}
            }
            let after = modules.get_item(*name)?;
            assert!(match (&before, after) {
                (Some(before), Some(after)) => before.is(&after),
                (None, None) => true,
                _ => false,
            });
        }
        match result {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    })
    .unwrap();
    assert!(!_native::process_owner::cleanup_pending(&scratch.0));
    scratch.clean().unwrap();
}

#[test]
fn svg_conversion_refusal_keeps_source_error_class_and_no_output() {
    use pyo3::prelude::*;
    let _svg = SVG_API.lock().unwrap();
    let scratch = tooling_fixture::Root::new("SVG renderer refusal café");
    let source = scratch.0.join("broken.svg");
    let original = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><broken";
    std::fs::write(&source, original).unwrap();
    Python::initialize();
    Python::attach(|py| {
        let renderer = py.import("resvg_py").unwrap();
        let saved = renderer.getattr("svg_to_bytes").unwrap();
        for (index, kind) in [
            "OSError",
            "ValueError",
            "TypeError",
            "KeyboardInterrupt",
            "SystemExit",
        ]
        .into_iter()
        .enumerate()
        {
            let output = scratch.0.join(format!("{index}.png"));
            let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let hits = calls.clone();
            let refused = pyo3::types::PyCFunction::new_closure(
                py,
                None,
                None,
                move |args, kwargs| -> PyResult<()> {
                    hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    assert!(args.is_empty());
                    assert_eq!(
                        kwargs
                            .unwrap()
                            .get_item("width")?
                            .unwrap()
                            .extract::<u32>()?,
                        256
                    );
                    Err(match kind {
                        "OSError" => pyo3::exceptions::PyOSError::new_err("controlled SVG call"),
                        "ValueError" => {
                            pyo3::exceptions::PyValueError::new_err("controlled SVG call")
                        }
                        "TypeError" => {
                            pyo3::exceptions::PyTypeError::new_err("controlled SVG call")
                        }
                        "KeyboardInterrupt" => {
                            pyo3::exceptions::PyKeyboardInterrupt::new_err("controlled SVG call")
                        }
                        _ => pyo3::exceptions::PySystemExit::new_err("controlled SVG call"),
                    })
                },
            )
            .unwrap();
            renderer.setattr("svg_to_bytes", refused).unwrap();
            let typed = _native::codec_image::to_static_png_typed(&source, &output);
            let legacy = _native::codec_image::to_static_png(&source, &output);
            renderer.setattr("svg_to_bytes", &saved).unwrap();
            let error = typed.unwrap_err();
            if matches!(kind, "KeyboardInterrupt" | "SystemExit") {
                assert_eq!(error.kind, kind);
                assert_eq!(error.message, "controlled SVG call");
                assert!(!error.handled);
                assert!(_native::codec_error::CodecError::worker_reply(Err(error)).is_err());
            } else {
                assert_eq!(error.kind, "MediaError");
                assert_eq!(error.message, "failed to render SVG: broken.svg");
                assert!(error.handled);
                let reply = _native::codec_error::CodecError::worker_reply(Err(error)).unwrap();
                assert_eq!(reply.exit, 1);
                assert_eq!(
                    reply.envelope,
                    serde_json::json!({"ok":false,"error":"failed to render SVG: broken.svg"})
                );
            }
            assert_eq!(
                legacy.unwrap_err(),
                if kind == "ValueError" {
                    "controlled SVG call"
                } else {
                    "external static conversion failed"
                }
            );
            assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
            assert!(!output.exists());
            assert_eq!(std::fs::read(&source).unwrap(), original);
            assert_eq!(std::fs::read_dir(&scratch.0).unwrap().count(), 1);
        }
    });
    scratch.clean().unwrap();
}

#[test]
fn static_image_info_and_fingerprint_match_pinned_source() {
    let _svg = SVG_API.lock().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(root.join("tests/fixtures/native/static-codec.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let source = root.join(case["input"].as_str().unwrap());
        let bytes = std::fs::read(&source).unwrap();
        assert_eq!(
            Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            case["input_sha256"].as_str().unwrap(),
            "{} fixture bytes changed",
            case["name"]
        );
        let actual = _native::codec_image::inspect_and_fingerprint(&source).unwrap();
        assert_eq!(actual, case["expected"], "{}", case["name"]);
    }
}
