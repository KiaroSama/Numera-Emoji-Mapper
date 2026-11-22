//! Python-free Lottie/TGS loading, catalogue identity, packaging, validation and rendering.
//!
//! JSON numbers and strings are retained independently of serde_json: catalogue keys historically
//! use Python's integer/float distinction, ASCII escaping and nonfinite JSON extension. Rendering
//! calls the exact native rlottie revision used by the historical 1.3.8 wrapper (see vendor pin).
#[cfg(test)]
use std::path::Path;

#[path = "native_lottie/document.rs"]
mod document;
#[path = "native_lottie/json_parser.rs"]
mod json_parser;
#[path = "native_lottie/json_value.rs"]
mod json_value;
#[path = "native_lottie/rendering.rs"]
mod rendering;

pub use document::{Document, canonical_bytes, fingerprint, load, source_bytes, validate_file};
pub use json_value::float_repr;
pub use rendering::Animation;

pub const MAX_UNPACKED: usize = 8 * 1024 * 1024;
pub const MAX_TGS_BYTES: usize = 65536;
pub const MAX_SOURCE_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct LottieError {
    pub kind: String,
    pub message: String,
    pub raw_fallback: bool,
    pub handled: bool,
    pub caught_by_exception: bool,
}
impl LottieError {
    fn new(kind: &str, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            raw_fallback: false,
            handled: matches!(
                kind,
                "MediaError" | "ValueError" | "OSError" | "RuntimeError"
            ),
            caught_by_exception: true,
        }
    }
    fn media(message: impl Into<String>) -> Self {
        Self::new("MediaError", message)
    }
    fn io(error: std::io::Error) -> Self {
        let mut result = Self::new("OSError", error.to_string());
        result.raw_fallback = true;
        result
    }
}
impl std::fmt::Display for LottieError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for LottieError {}
pub type Result<T> = std::result::Result<T, LottieError>;

#[cfg(test)]
pub(crate) fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .filter(|root| root.join("tests/fixtures").is_dir())
        .or_else(|| {
            Path::new(file!())
                .ancestors()
                .nth(3)
                .filter(|root| root.join("tests/fixtures").is_dir())
        })
        .expect("repository fixture directory")
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::document::{MAX_IMAGE_ASSETS, hash_source};
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use image::RgbaImage;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    fn root() -> std::path::PathBuf {
        fixture_root()
    }
    fn digest(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[test]
    fn historical_catalogue_identities_are_unchanged() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/animated-key.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let source = root().join(case["input"].as_str().unwrap());
            let actual = fingerprint(&source);
            if let Some(expected) = case.get("expected") {
                assert_eq!(&actual.unwrap(), expected, "{}", source.display());
            } else {
                let error = actual.unwrap_err();
                assert_eq!(error.kind, case["error_kind"]);
                assert_eq!(error.message, case["error"]);
            }
        }
    }

    #[test]
    fn every_historical_validation_case_keeps_its_policy_and_error_class() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/lottie-validation.json"
        ))
        .unwrap();
        for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            let document = Document::from_value(&case["document"]).unwrap();
            let actual = document.validate(
                case["filename"].as_str().unwrap(),
                case["compressed_size"].as_u64().unwrap(),
            );
            if case["expected"].is_null() {
                assert!(actual.is_ok(), "case {index}: {actual:?}");
            } else {
                let error = actual.unwrap_err();
                assert_eq!(error.kind, case["source_error_kind"], "case {index}");
                assert_eq!(error.message, case["expected"], "case {index}");
            }
        }
    }

    #[test]
    fn gzip_packaging_matches_the_historical_fixture() {
        let source = load(&root().join("tests/fixtures/native/tgs-package/valid.json")).unwrap();
        let bytes = source.package_bytes("valid.json", "valid.tgs").unwrap();
        assert_eq!(bytes.len(), 100);
        assert_eq!(
            digest(&bytes),
            "8b83d1e5effe7f27100badeeba5fc39883fd2835986305ef6f7e8c736b0040eb"
        );
        assert_eq!(
            Document::from_bytes(&bytes, "valid.tgs")
                .unwrap()
                .canonical_bytes(),
            source.canonical_bytes()
        );
    }

    #[test]
    fn malformed_and_nonfinite_documents_are_not_silently_normalized() {
        let base = r#"{"v":"5.5","fr":30,"ip":0,"op":60,"w":512,"h":512,"layers":[]}"#;
        for key in ["fr", "ip", "op"] {
            let mut value: Value = serde_json::from_str(base).unwrap();
            value[key] = Value::String("nan".into());
            assert!(
                Document::from_value(&value)
                    .unwrap()
                    .validate("bad.tgs", 100)
                    .unwrap_err()
                    .message
                    .contains(key)
            );
        }
        let doc = Document::from_bytes(
            br#"{"n":NaN,"i":Infinity,"z":-0.0,"s":"\ud800","p":"\ud83d\ude00"}"#,
            "special",
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(doc.canonical_bytes()).unwrap(),
            r#"{"i":Infinity,"n":NaN,"p":"\ud83d\ude00","s":"\ud800","z":-0.0}"#
        );
        assert!(doc.to_value().is_err());
        let error = Document::from_bytes(b"{invalid", "bad.json").unwrap_err();
        assert_eq!(
            error.message,
            "Expecting property name enclosed in double quotes: line 1 column 2 (char 1)"
        );
        assert!(error.raw_fallback);
    }

    #[test]
    fn plain_json_obeys_the_same_decoded_byte_boundary_as_gzip() {
        let mut plain = vec![b' '; MAX_UNPACKED + 1];
        plain[..2].copy_from_slice(b"{}");
        assert!(Document::from_bytes(&plain[..MAX_UNPACKED], "plain.json").is_ok());
        let error = Document::from_bytes(&plain, "plain.json").unwrap_err();
        assert_eq!(error.kind, "MediaError");
        assert_eq!(
            error.message,
            format!("plain.json: Lottie expands beyond {MAX_UNPACKED} bytes")
        );
        assert!(!error.raw_fallback);
    }

    #[test]
    fn oversized_sparse_sources_are_refused_before_reading_or_fallback_hashing() {
        struct Scratch(std::path::PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let path = Scratch(std::env::temp_dir().join(format!(
            "numera-lottie-read-bound-{}-{}.json", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        )));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path.0)
            .unwrap();
        let size = MAX_SOURCE_BYTES as u64 + 1;
        file.set_len(size).unwrap();
        drop(file);
        for error in [
            source_bytes(&path.0).unwrap_err(),
            load(&path.0).unwrap_err(),
            hash_source(&path.0).unwrap_err(),
            fingerprint(&path.0).unwrap_err(),
        ] {
            assert_eq!(error.kind, "MediaError");
            assert!(
                error
                    .message
                    .ends_with(&format!("Lottie source exceeds {MAX_SOURCE_BYTES} bytes"))
            );
            assert!(!error.raw_fallback);
        }
        // Telegram's smaller encoded-size rejection still precedes magic/JSON inspection.
        let error = validate_file(&path.0).unwrap_err();
        assert_eq!(error.kind, "MediaError");
        assert_eq!(
            error.message,
            format!(
                "{}: TGS {size} > {MAX_TGS_BYTES} bytes",
                path.0.file_name().unwrap().to_string_lossy()
            )
        );
    }

    fn image_document(png: &[u8], count: usize) -> Document {
        let path = format!("data:image/png;base64,{}", STANDARD.encode(png));
        let assets = (0..count)
            .map(|index| {
                json!({
                    "id":format!("image{index}"),"w":2,"h":2,"p":path,"e":1
                })
            })
            .collect::<Vec<_>>();
        Document::from_value(&json!({
            "v":"5.7.4","w":2,"h":2,"fr":60,"ip":0,"op":1,"assets":assets,
            "layers":[{"ty":2,"ind":1,"refId":"image0","sr":1,"ip":0,"op":1,"st":0,"bm":0,
                "ks":{"o":{"a":0,"k":100},"r":{"a":0,"k":0},"p":{"a":0,"k":[0,0,0]},
                    "a":{"a":0,"k":[0,0,0]},"s":{"a":0,"k":[100,100,100]}}}]
        }))
        .unwrap()
    }

    #[test]
    fn embedded_image_headers_and_asset_counts_are_bounded_before_native_entry() {
        let original = crate::native_raster::encode_png(&RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([220, 30, 50, 255]),
        ))
        .unwrap();
        for (width, height) in [(16_385_u32, 1_u32), (8193, 8193)] {
            let mut png = original.clone();
            png[16..20].copy_from_slice(&width.to_be_bytes());
            png[20..24].copy_from_slice(&height.to_be_bytes());
            // Keep a valid IHDR checksum so refusal proves dimension/pixel policy,
            // rather than relying on corruption stopping a later image decoder.
            let mut crc = 0xffff_ffff_u32;
            for &byte in &png[12..29] {
                crc ^= u32::from(byte);
                for _ in 0..8 {
                    crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
                }
            }
            png[29..33].copy_from_slice(&(!crc).to_be_bytes());
            let error = Animation::from_document(&image_document(&png, 1))
                .err()
                .unwrap();
            assert_eq!(error.kind, "MediaError");
            assert!(error.message.contains("decode bounds"), "{error}");
        }
        let error = Animation::from_document(&image_document(&original, MAX_IMAGE_ASSETS + 1))
            .err()
            .unwrap();
        assert!(error.message.contains("image asset count limit"));
    }

    #[test]
    fn checked_embedded_raster_is_rendered_by_the_actual_native_engine() {
        let png = crate::native_raster::encode_png(&RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([220, 30, 50, 255]),
        ))
        .unwrap();
        let document = image_document(&png, 1);
        let mut animation = Animation::from_document(&document).unwrap();
        let rgba = animation.render_rgba(0., 2, 2).unwrap();
        // Independent rlottie-python1.3.8 observation, including the native
        // image compositor's opacity/premultiplication rounding.
        assert!(rgba.pixels().all(|pixel| pixel.0 == [218, 29, 49, 253]));

        let mut unpadded = document.to_value().unwrap();
        let path = unpadded["assets"][0]["p"]
            .as_str()
            .unwrap()
            .trim_end_matches('=')
            .replace('+', "-")
            .replace('/', "_");
        unpadded["assets"][0]["p"] = json!(path);
        let mut unpadded =
            Animation::from_document(&Document::from_value(&unpadded).unwrap()).unwrap();
        assert_eq!(unpadded.render_rgba(0., 2, 2).unwrap(), rgba);

        let mut optional = document.to_value().unwrap();
        optional["assets"][0]["e"] = json!(0);
        optional["assets"][0]["p"] = json!(format!(
            "numera-missing-external-{}.png",
            std::process::id()
        ));
        let optional = Document::from_value(&optional).unwrap();
        assert!(Animation::from_document(&optional).is_ok());
    }

    #[test]
    fn same_native_engine_renders_the_historical_frames() {
        // Independent rlottie-python1.3.8 raw RGBA observations, before lossy WebP encoding.
        let mut animation =
            Animation::from_path(&root().join("tests/fixtures/native/lottie-preview/moving.json"))
                .unwrap();
        assert_eq!(animation.size(), (512, 512));
        assert_eq!(animation.total_frames(), 61);
        assert_eq!(animation.frame_rate(), 60.);
        assert_eq!(animation.duration(), 1.);
        for (position, expected) in [
            (
                0.,
                "6bc5c1dfb887be7987713544cef01274e388f37fcb6accb44913772f4c7eb140",
            ),
            (
                0.25,
                "5c0156aa37de481620f02e4a2c2aaa82fecbc527188716cd1c580f2d1f949f3d",
            ),
            (
                0.5,
                "820e978ddc10d87713fc9036ba7f6058e82552ab1f60ba3eda793f2b8f9616d8",
            ),
            (
                0.75,
                "05aaa7422503984bc7bee40594c9051291ed489e7729962412ea42b749c82c36",
            ),
        ] {
            let frame = animation.frame_at_pos(position).unwrap();
            assert_eq!(
                digest(
                    animation
                        .render_rgba(frame as f64, 52, 52)
                        .unwrap()
                        .as_raw()
                ),
                expected,
                "position {position}"
            );
        }
        assert!(animation.render_rgba(f64::NAN, 52, 52).is_err());
        assert!(animation.render_rgba(0., 0, 52).is_err());
        assert!(animation.render_rgba(1000., 52, 52).is_err());
        assert!(animation.frame_at_pos(f64::NAN).is_err());
        assert!(animation.frame_at_pos(1.1).is_err());
        let invalid = Document::from_bytes(b"{}", "invalid.json").unwrap();
        assert!(Animation::from_document(&invalid).is_err());
    }

    #[test]
    fn tint_preserves_keyframe_alpha_and_gradient_opacity() {
        let value = json!({"c":{"k":[{"s":[1.,0.,0.,0.25],"e":[1.,0.,0.,0.5]}]},
            "g":{"p":2,"k":{"k":[0.,1.,0.,0.,1.,0.,1.,0.,0.,0.2,1.,0.8]}}});
        let mut document = Document::from_value(&value).unwrap();
        document.tint([0, 0, 255]);
        let result = document.to_value().unwrap();
        assert_eq!(result["c"]["k"][0]["s"], json!([0., 0., 1., 0.25]));
        assert_eq!(result["c"]["k"][0]["e"], json!([0., 0., 1., 0.5]));
        assert_eq!(
            result["g"]["k"]["k"],
            json!([0., 0., 0., 1., 1., 0., 0., 1., 0., 0.2, 1., 0.8])
        );
    }

    #[test]
    fn every_historical_tint_tree_and_gradient_is_preserved() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/native/lottie-tint.json"))
                .unwrap();
        for category in ["trees", "gradients"] {
            for (index, case) in fixture[category].as_array().unwrap().iter().enumerate() {
                let wrapper = if category == "gradients" {
                    json!({"g":case["input"]})
                } else {
                    json!({"tree":case["input"]})
                };
                let mut document = Document::from_value(&wrapper).unwrap();
                document.tint([18, 52, 86]);
                let output = document.to_value().unwrap();
                let field = if category == "gradients" { "g" } else { "tree" };
                assert_eq!(output[field], case["expected"], "{category} case {index}");
            }
        }
    }
}
