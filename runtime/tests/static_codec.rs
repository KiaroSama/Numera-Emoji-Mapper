//! Frozen source fixtures exercise the interpreter-free worker's static contracts.
use numera_runtime::{codec_format, codec_image, codec_worker, native_raster as raster};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}
fn fixture(name: &str) -> Value {
    serde_json::from_slice(
        &std::fs::read(project().join(format!("tests/fixtures/native/{name}.json"))).unwrap(),
    )
    .unwrap()
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "numera-static-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn worker(operation: &str, source: &Path) -> Value {
    codec_worker::execute_typed_at(
        None,
        &json!({"operation":operation,"source":source,"format":"static"}),
    )
    .unwrap()
}

#[test]
fn original_static_fingerprints_and_metadata_operation_schemas_match() {
    let evidence = fixture("static-codec");
    for case in evidence["cases"].as_array().unwrap() {
        let path = project().join(case["input"].as_str().unwrap());
        assert_eq!(
            raster::digest_hex(&std::fs::read(&path).unwrap()),
            case["input_sha256"]
        );
        let expected = &case["expected"];
        assert_eq!(
            worker("image_info", &path),
            json!({"size":expected["size"],"mode":expected["mode"],"blank":expected["blank"]})
        );
        assert_eq!(worker("dimensions", &path), expected["size"]);
        assert_eq!(
            worker("usable", &path),
            !expected["blank"].as_bool().unwrap()
        );
        assert_eq!(
            worker("fingerprint", &path),
            json!({"content_key":expected["content_key"],"phash":expected["phash"],"fmt":"static","extension":".png"})
        );
        assert_eq!(
            codec_image::metadata(&path, false).unwrap(),
            json!({"size":expected["size"],"mode":expected["mode"]})
        );
    }
}

#[test]
fn original_static_and_svg_conversion_preserves_fitted_pixel_hashes() {
    let scratch = Scratch::new();
    let evidence = fixture("static-convert");
    for (index, case) in evidence["cases"].as_array().unwrap().iter().enumerate() {
        let source = project().join(case["input"].as_str().unwrap());
        assert_eq!(
            raster::digest_hex(&std::fs::read(&source).unwrap()),
            case["input_sha256"]
        );
        let output = scratch.0.join(format!("{index}.png"));
        let actual = codec_image::to_static_png_typed(&source, &output);
        if let Some(error) = case["error"].as_str() {
            assert_eq!(actual.unwrap_err().message, error);
            assert!(!output.exists());
            continue;
        }
        let actual = actual.unwrap();
        for field in ["size", "mode", "rgba_sha256"] {
            assert_eq!(
                actual[field], case["expected"][field],
                "{}: {field}",
                case["name"]
            );
        }
        let bytes = std::fs::read(&output).unwrap();
        let decoded = raster::decode(&bytes).unwrap();
        // Encoder bytes may differ; decoded pixels and the existing reference hash cannot.
        assert_eq!(
            raster::digest_hex(decoded.rgba.as_raw()),
            case["expected"]["rgba_sha256"]
        );
        assert_eq!(actual["png_sha256"], raster::digest_hex(&bytes));
        assert_eq!(actual["bytes"], bytes.len());
    }
    let svg = project().join("tests/fixtures/native/static-codec/gradient.svg");
    let rendered = numera_runtime::native_svg::load_rgba(&svg).unwrap();
    assert_eq!(rendered.dimensions(), (256, 192));
}

#[test]
fn frozen_provider_admission_is_checked_before_enlarging_sparse_images() {
    let scratch = Scratch::new();
    let evidence = fixture("provider-image");
    for (index, case) in evidence["cases"].as_array().unwrap().iter().enumerate() {
        let bytes = if let Some(path) = case["input"].as_str() {
            let bytes = std::fs::read(project().join(path)).unwrap();
            assert_eq!(raster::digest_hex(&bytes), case["input_sha256"]);
            bytes
        } else {
            case["data"].as_str().unwrap().as_bytes().to_vec()
        };
        let output = scratch.0.join(format!("{index}.png"));
        let accepted = codec_image::provider_png(&bytes, &output).unwrap();
        assert_eq!(
            accepted,
            case["expected"]["accepted"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(output.exists(), accepted);
        if accepted {
            let decoded = raster::open(&output).unwrap();
            assert_eq!(decoded.rgba.dimensions(), (100, 100));
            assert_eq!(
                raster::digest_hex(decoded.rgba.as_raw()),
                case["expected"]["rgba_sha256"]
            );
        }
    }
    assert!(codec_image::provider_png(b"invalid", Path::new("relative.png")).is_err());
}

#[test]
fn historical_format_sniff_extension_and_collision_algorithms_are_exact() {
    let scratch = Scratch::new();
    let evidence = fixture("media-format");
    for case in evidence["cases"].as_array().unwrap() {
        let bytes = hex(case["header_hex"].as_str().unwrap());
        let path = scratch.0.join(case["name"].as_str().unwrap());
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(
            codec_format::detect_bytes(&bytes),
            case["expected"]["header_format"]
        );
        assert_eq!(
            codec_format::detect_typed(&path).unwrap(),
            case["expected"]["format"]
        );
        assert_eq!(
            codec_format::extension_typed(&path, "static").unwrap(),
            case["expected"]["extension"]
        );
    }
    let jpeg = scratch.0.join("header.jpeg");
    std::fs::write(&jpeg, [0xff, 0xd8, 0xff]).unwrap();
    assert_eq!(
        codec_format::extension_typed(&jpeg, "static").unwrap(),
        ".png"
    );
    let path = scratch.0.join("collision.bin");
    std::fs::write(
        &path,
        hex(evidence["collision_input_hex"].as_str().unwrap()),
    )
    .unwrap();
    for case in evidence["collisions"].as_array().unwrap() {
        assert_eq!(
            codec_format::collision_key_typed(&path, case["key"].as_str().unwrap()).unwrap(),
            case["expected"]
        );
    }
}

#[test]
fn frozen_static_comparison_preserves_unknown_and_raw_byte_shortcut() {
    let evidence = fixture("static-compare");
    for case in evidence["cases"].as_array().unwrap() {
        let source = project().join(case["a"].as_str().unwrap());
        let other = project().join(case["b"].as_str().unwrap());
        let actual = codec_worker::execute_typed_at(
            None,
            &json!({"operation":"same_image","source":source,"other":other,"format":"static"}),
        )
        .unwrap();
        assert_eq!(actual, case["expected"], "{}", case["name"]);
    }
    let corrupt = project().join("tests/fixtures/native/static-compare/corrupt.png");
    assert_eq!(
        codec_worker::execute_typed_at(
            None,
            &json!({"operation":"compare","source":corrupt,"other":corrupt,"format":"static"})
        )
        .unwrap(),
        true
    );
}

#[test]
fn frozen_static_preview_keeps_original_pixels_hidden_rgb_and_dimensions() {
    let scratch = Scratch::new();
    let evidence = fixture("static-preview");
    for (index, case) in evidence["cases"].as_array().unwrap().iter().enumerate() {
        let source = project().join(case["input"].as_str().unwrap());
        let output = scratch.0.join(format!("{index}.webp"));
        let actual = codec_worker::execute_typed_at(
            None,
            &json!({"operation":"preview","source":source,"output":output,"format":"static","key":case["name"],"size":case["size"],"fps":case["fps"],"still":case["still"]}),
        );
        if let Some(kind) = case["error_kind"].as_str() {
            assert_eq!(actual.unwrap_err().kind, kind);
            assert!(!output.exists());
            continue;
        }
        let actual = actual.unwrap();
        let bytes = std::fs::read(&output).unwrap();
        let decoded = raster::decode(&bytes).unwrap();
        assert_eq!(actual["bytes"], bytes.len());
        assert_eq!(
            json!([decoded.rgba.width(), decoded.rgba.height()]),
            case["expected"]["size"]
        );
        assert_eq!(
            raster::digest_hex(decoded.rgba.as_raw()),
            case["expected"]["rgba_sha256"],
            "{}",
            case["name"]
        );
    }
}

#[test]
fn native_gallery_matches_independent_historical_pillow_decoded_pixels() {
    // Source Pillow 12.3.0 oracle: RGBA, thumbnail Lanczos 88x88, WebP
    // quality65/exact=true. Encoded-byte differences do not relax pixel identity.
    let scratch = Scratch::new();
    for (name, dimensions, expected) in [
        (
            "alpha-edge",
            (13, 9),
            "eb4e10cfca964fa70cee8a179444061824846765afbc4c1f9f44c6db36a327ae",
        ),
        (
            "transparent-rgb",
            (13, 9),
            "3915b3215329bef1f8cdee479d2fad69cf9af3a654a60e2a0056b7399cc21287",
        ),
        (
            "public-logo",
            (88, 88),
            "aa41e7d3fbf9fcea9455562001015b5d1acb089f3e3896e75a6cc3e7769b60f8",
        ),
    ] {
        let source = project().join(format!("tests/fixtures/native/static-codec/{name}.png"));
        let output = scratch.0.join(format!("{name}.webp"));
        let result = codec_worker::execute_typed_at(
            None,
            &json!({"operation":"gallery_thumb","source":source,"output":output,"format":"static"}),
        )
        .unwrap();
        let encoded = std::fs::read(&output).unwrap();
        assert_eq!(result["bytes"], encoded.len());
        let decoded = raster::decode(&encoded).unwrap();
        assert_eq!(decoded.rgba.dimensions(), dimensions);
        assert_eq!(
            raster::digest_hex(decoded.rgba.as_raw()),
            expected,
            "{name}"
        );
    }
}

#[test]
fn static_reencode_and_repaint_keep_alpha_skip_corruption_and_retain_idempotence() {
    let scratch = Scratch::new();
    let source = scratch.0.join("source.png");
    let original =
        std::fs::read(project().join("tests/fixtures/native/static-codec/alpha-edge.png")).unwrap();
    std::fs::write(&source, &original).unwrap();
    let rgba = raster::decode(&original).unwrap().rgba;
    let result = codec_image::reencode_static_typed(&source).unwrap();
    assert!(result.rewritten);
    assert!(result.warning.is_none());
    assert_eq!(raster::open(&source).unwrap().rgba, rgba);
    let encoded = std::fs::read(&source).unwrap();
    assert!(
        !codec_image::reencode_static_typed(&source)
            .unwrap()
            .rewritten
    );
    assert_eq!(std::fs::read(&source).unwrap(), encoded);
    for _ in 0..2 {
        assert!(
            codec_image::repaint_static_typed(&source, [17, 34, 51])
                .unwrap()
                .rewritten
        );
        let tinted = raster::open(&source).unwrap().rgba;
        for (original, tinted) in rgba.pixels().zip(tinted.pixels()) {
            assert_eq!(tinted.0, [17, 34, 51, original[3]]);
        }
    }
    let corrupt = scratch.0.join("corrupt.png");
    let bytes = b"not an image";
    std::fs::write(&corrupt, bytes).unwrap();
    let result = codec_image::reencode_static_typed(&corrupt).unwrap();
    assert!(!result.rewritten);
    assert!(
        result
            .warning
            .unwrap()
            .starts_with("re-encode skipped for corrupt.png (static):")
    );
    assert_eq!(std::fs::read(&corrupt).unwrap(), bytes);
}

#[test]
fn svgz_decoding_is_bounded_before_the_svg_parser_and_keeps_normal_pixels() {
    use std::io::Write;
    let scratch = Scratch::new();
    let svg =
        std::fs::read(project().join("tests/fixtures/native/static-codec/gradient.svg")).unwrap();
    let mut compressed = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    compressed.write_all(&svg).unwrap();
    let path = scratch.0.join("gradient.svgz");
    std::fs::write(&path, compressed.finish().unwrap()).unwrap();
    assert_eq!(
        numera_runtime::native_svg::load_rgba(&path).unwrap(),
        numera_runtime::native_svg::load_rgba(
            &project().join("tests/fixtures/native/static-codec/gradient.svg")
        )
        .unwrap()
    );
    let mut compressed = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    compressed
        .write_all(&vec![b' '; 8 * 1024 * 1024 + 1])
        .unwrap();
    let compressed = compressed.finish().unwrap();
    assert!(compressed.len() < 100_000);
    let path = scratch.0.join("oversized.svgz");
    std::fs::write(&path, compressed).unwrap();
    assert!(
        numera_runtime::native_svg::load_rgba(&path)
            .unwrap_err()
            .message
            .contains("8 MiB")
    );
}

fn image_svg(href: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="16"><image href="{href}" width="32" height="16" preserveAspectRatio="none"/></svg>"#
    )
}

#[test]
fn svg_referenced_rasters_share_native_dimension_and_input_byte_bounds() {
    use base64::Engine;
    use image::ImageEncoder;
    let scratch = Scratch::new();
    // A tiny compressed body with an out-of-policy width must refuse before
    // either the native validator or resvg allocates an oversized canvas.
    let pixels = vec![255; 20_000 * 4];
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&pixels, 20_000, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    std::fs::write(scratch.0.join("wide.png"), &png).unwrap();
    let data = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&png)
    );
    for (index, href) in ["wide.png", data.as_str()].into_iter().enumerate() {
        let path = scratch.0.join(format!("wide-{index}.svg"));
        std::fs::write(&path, image_svg(href)).unwrap();
        let error = numera_runtime::native_svg::load_rgba(&path).unwrap_err();
        assert!(error.message.contains("decode bounds"), "{}", error.message);
    }
    let file = std::fs::File::create(scratch.0.join("oversized.png")).unwrap();
    file.set_len(128 * 1024 * 1024 + 1).unwrap();
    drop(file);
    let path = scratch.0.join("oversized-reference.svg");
    std::fs::write(&path, image_svg("oversized.png")).unwrap();
    assert!(
        numera_runtime::native_svg::load_rgba(&path)
            .unwrap_err()
            .message
            .contains("input bounds")
    );
}

#[test]
fn svg_relative_data_and_nested_images_preserve_supported_leaf_rendering() {
    use base64::Engine;
    let scratch = Scratch::new();
    let png = raster::encode_png(&image::RgbaImage::from_pixel(
        2,
        1,
        image::Rgba([19, 107, 203, 255]),
    ))
    .unwrap();
    std::fs::write(scratch.0.join("asset.png"), &png).unwrap();
    let data = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&png)
    );
    let leaf = r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="16"><rect width="32" height="16" fill="#136bcb"/></svg>"##;
    std::fs::write(scratch.0.join("leaf.svg"), leaf).unwrap();
    let nested = format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(leaf)
    );
    for (index, href) in ["asset.png", data.as_str(), "leaf.svg", nested.as_str()]
        .into_iter()
        .enumerate()
    {
        let path = scratch.0.join(format!("valid-{index}.svg"));
        std::fs::write(&path, image_svg(href)).unwrap();
        let rendered = numera_runtime::native_svg::load_rgba(&path).unwrap();
        assert_eq!(rendered.dimensions(), (256, 128));
        assert_eq!(rendered[(128, 64)].0, [19, 107, 203, 255]);
    }
    // External references in an SVG loaded as an image are ignored by the SVG
    // image contract, so this file reference cannot recurse back into itself.
    let path = scratch.0.join("self.svg");
    std::fs::write(&path, image_svg("self.svg")).unwrap();
    assert!(raster::is_blank(
        &numera_runtime::native_svg::load_rgba(&path).unwrap()
    ));
    for (index, href) in [
        "missing.png",
        "https://invalid.example/ignored.png",
        "data:image/unsupported;base64,bm90IGFuIGltYWdl",
    ]
    .into_iter()
    .enumerate()
    {
        let path = scratch.0.join(format!("ignored-{index}.svg"));
        let svg = image_svg(href).replace(
            "<image",
            r##"<rect width="32" height="16" fill="#136bcb"/><image"##,
        );
        std::fs::write(&path, svg).unwrap();
        assert_eq!(
            numera_runtime::native_svg::load_rgba(&path).unwrap()[(128, 64)].0,
            [19, 107, 203, 255]
        );
    }
}

#[test]
fn svg_nested_image_depth_and_count_budgets_refuse_excess_work() {
    use base64::Engine;
    let scratch = Scratch::new();
    let mut nested =
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="16"/>"#.to_owned();
    for _ in 0..10 {
        nested = image_svg(&format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(nested)
        ));
    }
    let path = scratch.0.join("deep.svg");
    std::fs::write(&path, nested).unwrap();
    assert!(
        numera_runtime::native_svg::load_rgba(&path)
            .unwrap_err()
            .message
            .contains("depth limit")
    );
    let data = format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD
            .encode(r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>"#)
    );
    let image = format!(r#"<image href="{data}" width="1" height="1"/>"#);
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="16">{}</svg>"#,
        image.repeat(65)
    );
    let path = scratch.0.join("count.svg");
    std::fs::write(&path, svg).unwrap();
    assert!(
        numera_runtime::native_svg::load_rgba(&path)
            .unwrap_err()
            .message
            .contains("count limit")
    );
}
