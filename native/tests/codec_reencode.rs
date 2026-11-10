//! Collected static bytes change without moving the catalog key or any invisible RGB.
use pyo3::prelude::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
struct Image<'py>(Bound<'py, PyAny>);
impl Drop for Image<'_> {
    fn drop(&mut self) {
        self.0.call_method0("close").expect("close fixture image");
    }
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}

#[test]
fn source_static_reencode_preserves_every_pixel_and_is_idempotent() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-reencode.json"
    ))
    .unwrap();
    let identity: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(fixture["source_revision"], identity["source_revision"]);
    assert_eq!(
        fixture["source_sha256"],
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53"
    );
    assert_eq!(fixture["Pillow"], "12.3.0");
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    let directory = tooling_fixture::Root::new("native reencode");
    for case in cases {
        let source = root.join(case["input"].as_str().unwrap());
        let original = std::fs::read(&source).unwrap();
        assert_eq!(digest(&original), case["input_sha256"]);
        let identity_case = identity["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["input"] == case["input"])
            .unwrap();
        assert_eq!(identity_case["input_sha256"], case["input_sha256"]);
        let key = &identity_case["expected"]["content_key"];
        let path = directory
            .0
            .join(format!("{}.png", case["name"].as_str().unwrap()));
        std::fs::write(&path, &original).unwrap();
        assert_eq!(
            _native::codec_static_identity::fingerprint(&path).unwrap()["content_key"],
            *key
        );
        assert_eq!(
            serde_json::json!(_native::codec_image::reencode_static(&path).unwrap()),
            case["expected"]["rewritten"]
        );
        let bytes = std::fs::read(&path).unwrap();
        assert_ne!(bytes, original, "{} stayed a byte clone", case["name"]);
        assert_eq!(
            bytes.len() as u64,
            case["expected"]["bytes"].as_u64().unwrap()
        );
        assert_eq!(digest(&bytes), case["expected"]["sha256"]);
        let rgba: Vec<u8> = Python::attach(|py| {
            let image = Image(
                py.import("PIL.Image")
                    .unwrap()
                    .call_method1("open", (path.clone(),))
                    .unwrap(),
            );
            let rgba = Image(image.0.call_method1("convert", ("RGBA",)).unwrap());
            rgba.0.call_method0("tobytes").unwrap().extract().unwrap()
        });
        assert_eq!(digest(&rgba), case["expected"]["rgba_sha256"]);
        assert_eq!(
            _native::codec_static_identity::fingerprint(&path).unwrap()["content_key"],
            *key,
            "reencode moved the catalog dedup key"
        );
        assert_eq!(
            serde_json::json!(_native::codec_image::reencode_static(&path).unwrap()),
            case["expected"]["repeat"]
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        std::fs::remove_file(path).unwrap();
    }
    let corrupt = directory.0.join("corrupt.png");
    std::fs::write(&corrupt, b"not an image").unwrap();
    assert!(!_native::codec_image::reencode_static(&corrupt).unwrap());
    assert_eq!(std::fs::read(&corrupt).unwrap(), b"not an image");
    std::fs::remove_file(corrupt).unwrap();
    let unknown = directory.0.join("unknown.bin");
    std::fs::write(&unknown, [0, 1, 2]).unwrap();
    let error = _native::codec_worker::execute(&serde_json::json!({
        "operation":"prepare","source":unknown,"format":"sticker-shaped-thing",
        "reencode":true
    }))
    .unwrap_err();
    assert_eq!(error, "unsupported media format");
    assert_eq!(std::fs::read(&unknown).unwrap(), [0, 1, 2]);
    std::fs::remove_file(unknown).unwrap();
    assert!(std::fs::read_dir(&directory.0).unwrap().next().is_none());
    directory.clean().unwrap();
}

#[test]
fn original_static_white_repaint_preserves_the_exact_silhouette() {
    let directory = tooling_fixture::Root::new("original white repaint café");
    let path = directory.0.join("s.png");
    Python::initialize();
    let before = Python::attach(|py| -> PyResult<Vec<u8>> {
        let image = Image(
            py.import("PIL.Image")?
                .call_method1("new", ("RGBA", (100, 100), (0, 0, 0, 0)))?,
        );
        for x in 3..33 {
            for y in 3..33 {
                image
                    .0
                    .call_method1("putpixel", ((x, y), (30, 60, 200, 255)))?;
            }
        }
        image.0.call_method1("save", (&path, "PNG"))?;
        image.0.call_method0("tobytes")?.extract()
    })
    .unwrap();
    assert!(_native::codec_image::repaint_static(&path, [255, 255, 255]).unwrap());
    let after = Python::attach(|py| -> PyResult<Vec<u8>> {
        let image = Image(py.import("PIL.Image")?.call_method1("open", (&path,))?);
        let rgba = Image(image.0.call_method1("convert", ("RGBA",))?);
        assert_eq!(rgba.0.getattr("size")?.extract::<(u32, u32)>()?, (100, 100));
        rgba.0.call_method0("tobytes")?.extract()
    })
    .unwrap();
    assert_eq!(after.len(), 40_000);
    assert_eq!(
        after.iter().skip(3).step_by(4).collect::<Vec<_>>(),
        before.iter().skip(3).step_by(4).collect::<Vec<_>>()
    );
    let visible = after
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 0)
        .collect::<Vec<_>>();
    assert_eq!(visible.len(), 900);
    assert!(visible.iter().all(|pixel| pixel[..3] == [255, 255, 255]));
    directory.clean().unwrap();
}

#[test]
fn original_video_repaint_refusal_preserves_the_file_without_decoding() {
    let directory = tooling_fixture::Root::new("original video repaint refusal");
    let path = directory.0.join("v.webm");
    let original = b"not really a webm";
    std::fs::write(&path, original).unwrap();
    // The codec request reports Err, not the source helper's False. No successful tint is claimed.
    assert_eq!(_native::codec_worker::execute_at(Some(&directory.0), &serde_json::json!({
        "operation":"prepare", "source":path, "format":"video", "reencode":false, "tint":"#FFFFFF"
    })).unwrap_err(), "requested tint could not be applied");
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
    assert!(!_native::process_owner::cleanup_pending(&directory.0));
    directory.clean().unwrap();
}

#[test]
fn original_webp_recipes_change_bytes_without_changing_hidden_pixels_or_keys() {
    let directory = tooling_fixture::Root::new("original WebP recipes café");
    Python::initialize();
    for (name, color, transparent) in [
        ("transparent", (0_u8, 0_u8, 0_u8, 0_u8), true),
        ("opaque", (200, 40, 90, 255), false),
        ("dedup", (10, 180, 60, 255), false),
    ] {
        let path = directory.0.join(format!("{name}.webp"));
        let pixels = Python::attach(|py| -> PyResult<Vec<u8>> {
            let image = Image(
                py.import("PIL.Image")?
                    .call_method1("new", ("RGBA", (80, 60), color))?,
            );
            if transparent {
                for x in 20..60 {
                    for y in 15..45 {
                        image
                            .0
                            .call_method1("putpixel", ((x, y), (200, 40, 90, 255)))?;
                    }
                }
                image.0.call_method1("putpixel", ((2, 2), (7, 9, 11, 0)))?;
            }
            let options = pyo3::types::PyDict::new(py);
            options.set_item("format", "WEBP")?;
            if transparent {
                options.set_item("lossless", true)?;
                options.set_item("exact", true)?;
            }
            image.0.call_method("save", (&path,), Some(&options))?;
            let decoded = Image(py.import("PIL.Image")?.call_method1("open", (&path,))?);
            let rgba = Image(decoded.0.call_method1("convert", ("RGBA",))?);
            rgba.0.call_method0("tobytes")?.extract()
        })
        .unwrap();
        if transparent {
            assert_eq!(
                &pixels[(2 * 80 + 2) * 4..(2 * 80 + 2) * 4 + 4],
                &[7, 9, 11, 0]
            );
        }
        let before = std::fs::read(&path).unwrap();
        let key =
            _native::codec_static_identity::fingerprint(&path).unwrap()["content_key"].clone();
        assert!(
            _native::codec_image::reencode_static(&path).unwrap(),
            "{name}"
        );
        let rewritten = std::fs::read(&path).unwrap();
        assert_ne!(rewritten, before, "{name} remained a byte clone");
        let actual = Python::attach(|py| -> PyResult<Vec<u8>> {
            let image = Image(py.import("PIL.Image")?.call_method1("open", (&path,))?);
            let rgba = Image(image.0.call_method1("convert", ("RGBA",))?);
            rgba.0.call_method0("tobytes")?.extract()
        })
        .unwrap();
        assert_eq!(actual, pixels, "{name} pixels changed");
        assert_eq!(
            _native::codec_static_identity::fingerprint(&path).unwrap()["content_key"],
            key
        );
        assert!(!_native::codec_image::reencode_static(&path).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), rewritten);
    }
    directory.clean().unwrap();
}
