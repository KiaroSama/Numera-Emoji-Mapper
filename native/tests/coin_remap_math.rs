//! Independent source signature/BLAS results at image and bounded numerical seams.
use serde_json::Value;
use sha2::{Digest, Sha256};
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
#[test]
fn source_signatures_and_nearest_preserve_alpha_first_ties_and_chunk_boundary() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-remap-math.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "b66136d42adc3448d44e67c36c9f8c827a4e63d6ce259af5d6b437299b043ed7"
    );
    for case in fixture["images"].as_array().unwrap() {
        let data = std::fs::read(root.join(case["input"].as_str().unwrap())).unwrap();
        assert_eq!(
            Sha256::digest(&data)
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>(),
            case["input_sha256"].as_str().unwrap()
        );
        assert_eq!(
            serde_json::json!(_native::codec_image::remap_signature(&data).unwrap()),
            case["expected"]
        );
    }
    for case in fixture["nearest"].as_array().unwrap() {
        let rows = |key: &str| {
            case[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    row.as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_f64().unwrap() as f32)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        let result = _native::coin_remap_math::nearest(&rows("local"), &rows("live")).unwrap();
        assert_eq!(serde_json::json!(result.indices), case["expected"]["index"]);
        assert_eq!(serde_json::json!(result.squared), case["expected"]["d2"]);
        assert_eq!(
            result.second.len(),
            case["expected"]["second"].as_array().unwrap().len()
        );
        for (actual, expected) in result
            .second
            .iter()
            .zip(case["expected"]["second"].as_array().unwrap())
        {
            if expected.is_null() {
                assert_eq!(*actual, f64::INFINITY);
            } else {
                assert_eq!(*actual, expected.as_f64().unwrap());
            }
        }
    }
    original_signature_assertions();
    assert!(_native::codec_image::remap_signature(b"not image").is_err());
    assert!(_native::coin_remap_math::nearest(&[vec![1.0]], &[]).is_err());
    assert!(_native::coin_remap_math::nearest(&[vec![f32::NAN]], &[vec![1.0]]).is_err());
    assert!(_native::coin_remap_math::nearest(&[vec![1.0]], &[vec![1.0, 2.0]]).is_err());
}

#[test]
fn malformed_nearest_arrays_refuse_before_any_map_or_candidate_write() {
    use _native::coin_remap_math::Nearest;
    use _native::coin_remap_policy::{Options, matches};
    let fixture = tooling_fixture::Root::new("remap nearest result admission");
    let root = &fixture.0;
    let output = root.join("map.json");
    let candidates = root.join("candidate.json");
    let before = b"{\"old\":\"preserved\"}\n";
    std::fs::write(&output, before).unwrap();
    let tickers = vec!["btc".into(), "eth".into()];
    let cids = vec!["111111111".into(), "222222222".into()];
    for (indices, squared, second) in [
        (vec![0], vec![0.0], vec![20000.0]),
        (vec![0], vec![0.0, 0.0], vec![20000.0, 20000.0]),
        (vec![0, 1], vec![0.0], vec![20000.0, 20000.0]),
        (vec![0, 1], vec![0.0, 0.0], vec![20000.0]),
        (vec![0, 1, 0], vec![0.0, 0.0], vec![20000.0, 20000.0]),
        (vec![0, 1], vec![0.0, 0.0, 0.0], vec![20000.0, 20000.0]),
        (vec![0, 1], vec![0.0, 0.0], vec![20000.0, 20000.0, 20000.0]),
        (vec![0, 2], vec![0.0, 0.0], vec![20000.0, 20000.0]),
    ] {
        let result = matches(
            &tickers,
            &cids,
            Nearest {
                indices,
                squared,
                second,
            },
        )
        .and_then(|matches| {
            _native::coin_remap_policy::decide(
                &matches,
                Options {
                    apply: true,
                    max_distance: Some(100.0),
                    min_margin: None,
                    incomplete_sets: 0,
                    failed_images: 0,
                },
            )
        })
        .and_then(|decision| _native::coin_remap::persist(root, &output, &candidates, &decision));
        assert!(
            result.is_err(),
            "malformed parallel arrays must never reach persistence"
        );
        assert_eq!(std::fs::read(&output).unwrap(), before);
        assert!(!candidates.exists());
        assert!(!output.with_extension("prebroken.json").exists());
        assert!(!root.join(".locks").exists());
    }
    let complete = matches(
        &tickers,
        &cids,
        Nearest {
            indices: vec![1, 0],
            squared: vec![9.0, 16.0],
            second: vec![100.0, f64::INFINITY],
        },
    )
    .unwrap();
    assert_eq!(complete.len(), 2);
    assert_eq!(
        (&complete[0].ticker, &complete[0].cid),
        (&tickers[0], &cids[1])
    );
    assert_eq!((complete[0].distance, complete[0].margin), (3.0, 7.0));
    assert_eq!(
        (&complete[1].ticker, &complete[1].cid),
        (&tickers[1], &cids[0])
    );
    assert_eq!(
        (complete[1].distance, complete[1].margin),
        (4.0, f64::INFINITY)
    );
    fixture.clean().unwrap();
}

fn original_signature_assertions() {
    use pyo3::prelude::*;
    use pyo3::types::PyList;
    Python::initialize();
    let images = Python::attach(|py| -> PyResult<Vec<Vec<u8>>> {
        let module = py.import("PIL.Image")?;
        let mut images = Vec::new();
        for shape in ["left", "ring", "red", "blue"] {
            let image = module.call_method1("new", ("RGBA", (100, 100), (0, 0, 0, 0)))?;
            let buffer = py.import("io")?.getattr("BytesIO")?.call0()?;
            let result = (|| -> PyResult<Vec<u8>> {
                let mut pixels = vec![(0, 0, 0, 0); 10000];
                for y in 0..100_usize {
                    for x in 0..100_usize {
                        let radius = ((x as f64 - 50.0).powi(2) + (y as f64 - 50.0).powi(2)).sqrt();
                        let selected = match shape {
                            "left" => (10..45).contains(&x) && (10..90).contains(&y),
                            "ring" => {
                                (20..80).contains(&x)
                                    && (20..80).contains(&y)
                                    && (22.0..=29.0).contains(&radius)
                            }
                            _ => (20..80).contains(&x) && (20..80).contains(&y),
                        };
                        if selected {
                            pixels[y * 100 + x] = match shape {
                                "red" => (220, 30, 30, 255),
                                "blue" => (30, 30, 220, 255),
                                _ => (0, 0, 0, 255),
                            };
                        }
                    }
                }
                image.call_method1("putdata", (PyList::new(py, pixels)?,))?;
                image.call_method1("save", (&buffer, "PNG"))?;
                buffer.call_method0("getvalue")?.extract()
            })();
            let image_closed = image.call_method0("close");
            let buffer_closed = buffer.call_method0("close");
            let bytes = result?;
            image_closed?;
            buffer_closed?;
            images.push(bytes);
        }
        Ok(images)
    })
    .unwrap();
    let signatures = images
        .iter()
        .map(|image| _native::codec_image::remap_signature(image).unwrap())
        .collect::<Vec<_>>();
    assert!(signatures.iter().all(|signature| signature.len() == 1024));
    assert!(signatures[0].iter().any(|value| *value > 0));
    assert_ne!(
        signatures[0], signatures[1],
        "different black marks must remain distinguishable"
    );
    assert_ne!(
        signatures[2], signatures[3],
        "same silhouette must preserve different colours"
    );
    let identical = _native::coin_remap_math::nearest(
        &[vec![255.0; 768]],
        &std::array::from_fn::<_, 3, _>(|_| vec![255.0; 768]),
    )
    .unwrap();
    assert_eq!(identical.indices, [0]);
    assert_eq!(identical.squared.len(), 1);
    assert!(identical.squared.iter().all(|value| *value >= 0.0));
    assert!(
        identical
            .squared
            .iter()
            .all(|value| value.sqrt().is_finite())
    );
    assert!(identical.second.iter().all(|value| *value >= 0.0));
    let single = _native::coin_remap_math::nearest(&[vec![0.0; 768]], &[vec![1.0; 768]]).unwrap();
    assert_eq!(single.indices, [0]);
    assert_eq!(single.squared, [768.0]);
    assert_eq!(single.second, [f64::INFINITY]);
}
