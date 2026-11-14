//! Provider logos reject sparse source pixels before fitting, unlike general conversion.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[path = "support/static_fitting_fixture.rs"]
mod static_fitting_fixture;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn provider_source_blank_boundary_alpha_and_decode_refusals_match_source() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(
            std::env::var_os("NUMERA_PROVIDER_IMAGE_ORACLE")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("tests/fixtures/native/provider-image.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        fixture["source_hashes"]["coins/_paprika_api.py"],
        "34742583025adaad3c7513ce0031c13d32759e29cfa8076609150a0badeb1acc"
    );
    let (original, _) = _native::source_static_oracle::fixtures().unwrap();
    _native::source_static_oracle::fixture_identity(&original, &fixture).unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 7);
    let scratch = root
        .join("logs/tmp")
        .join(format!("provider image {}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let bytes = if let Some(input) = case["input"].as_str() {
            let bytes = std::fs::read(root.join(input)).unwrap();
            assert_eq!(hash(&bytes), case["input_sha256"]);
            bytes
        } else {
            case["data"].as_str().unwrap().as_bytes().to_vec()
        };
        let output = scratch.join(format!("{}.png", case["name"].as_str().unwrap()));
        let accepted = _native::codec_image::provider_png(&bytes, &output).unwrap();
        assert_eq!(
            accepted,
            case["expected"]["accepted"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        if accepted {
            let encoded = std::fs::read(&output).unwrap();
            assert_eq!(hash(&encoded), case["expected"]["png_sha256"]);
            let info = _native::codec_image::inspect_and_fingerprint(&output).unwrap();
            assert_eq!(info["size"], case["expected"]["size"]);
            assert_eq!(info["mode"], case["expected"]["mode"]);
            assert!(_native::codec_image::provider_png(&bytes, &output).is_err());
            assert_eq!(std::fs::read(&output).unwrap(), encoded);
            std::fs::remove_file(output).unwrap();
        } else {
            assert!(!output.exists());
        }
    }
    original_provider_image_assertions(&scratch);
    static_fitting_fixture::check(
        &root.join("tests/fixtures/native/static-codec/alpha-edge.png"),
        true,
    );
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 0);
    std::fs::remove_dir(&scratch).unwrap();
}

fn original_provider_image_assertions(scratch: &std::path::Path) {
    use pyo3::prelude::*;
    use pyo3::types::PyList;
    Python::initialize();
    let inputs = Python::attach(|py| -> PyResult<Vec<Vec<u8>>> {
        let module = py.import("PIL.Image")?;
        assert_eq!(
            module.getattr("__version__")?.extract::<String>()?,
            "12.3.0"
        );
        let mut inputs = Vec::new();
        for variant in ["transparent", "sparse", "gradient"] {
            let buffer = py.import("io")?.getattr("BytesIO")?.call0()?;
            let result: PyResult<Vec<u8>> = (|| {
                let side = if variant == "gradient" { 100 } else { 64 };
                let image = module.call_method1("new", ("RGBA", (side, side), (0, 0, 0, 0)))?;
                let result = (|| {
                    if variant == "sparse" {
                        for x in 0..4 {
                            image.call_method1("putpixel", ((x, 0), (255, 0, 0, 255)))?;
                        }
                    } else if variant == "gradient" {
                        let pixels = (0..100)
                            .flat_map(|_| (0..100).map(|x| (x * 2, x * 2, x * 2, 255)))
                            .collect::<Vec<_>>();
                        image.call_method1("putdata", (PyList::new(py, pixels)?,))?;
                    }
                    image.call_method1("save", (&buffer, "PNG"))?;
                    buffer.call_method0("getvalue")?.extract::<Vec<u8>>()
                })();
                let closed = image.call_method0("close");
                let bytes = result?;
                closed?;
                Ok(bytes)
            })();
            let closed = buffer.call_method0("close");
            let bytes = result?;
            closed?;
            inputs.push(bytes);
        }
        Ok(inputs)
    })
    .unwrap();
    for (index, bytes) in inputs.iter().enumerate() {
        let output = scratch.join(format!("original-{index}.png"));
        let before = hash(bytes);
        assert_eq!(
            _native::codec_image::provider_png(bytes, &output).unwrap(),
            index == 2
        );
        assert_eq!(hash(bytes), before);
        if index == 2 {
            let info = _native::codec_image::inspect_and_fingerprint(&output).unwrap();
            assert_eq!(info["size"], serde_json::json!([100, 100]));
            assert_eq!(info["mode"], "RGBA");
            std::fs::remove_file(output).unwrap();
        } else {
            assert!(!output.exists());
        }
        assert_eq!(std::fs::read_dir(scratch).unwrap().count(), 0);
    }
}
