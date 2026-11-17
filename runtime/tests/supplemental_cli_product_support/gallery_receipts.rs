//! Supplemental frozen all-format gallery bytes, cache and input immutability.
use crate::root::Root;
use _native::roster;
use numera_runtime as _native;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
#[path = "bounded_media.rs"]
mod bounded_media;
fn gallery_fixture() -> Value {
    let preview =
        include_str!("../../../tests/fixtures/native/lottie-preview.json").replace("\r\n", "\n");
    let gallery =
        include_str!("../../../tests/fixtures/native/gallery-codec.json").replace("\r\n", "\n");
    use sha2::{Digest, Sha256};
    assert_eq!(
        Sha256::digest(preview.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        "8f301692a45669d72fc3962ef4bb51c5af9765c6841acdbae919796a6b0364d1"
    );
    assert_eq!(
        Sha256::digest(gallery.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        "b52cf02336bfd4faa5eb1e582b67ba5b8728afd4314894ccb21f0e3a4664b4e6"
    );
    let committed: Value = serde_json::from_str(&gallery).unwrap();
    fn recipe(mut v: Value) -> Value {
        let cases = v["cases"].as_array_mut().filter(|c| c.len() == 5).unwrap();
        for c in cases {
            if c["format"] == "animated" {
                c.as_object_mut().unwrap().remove("sha256");
                c.as_object_mut().unwrap().remove("bytes");
            }
        }
        v
    }
    let original_recipe = recipe(committed.clone());
    let mut changed_input = committed.clone();
    changed_input["cases"][2]["input"] = json!("wrong-input.json");
    assert_ne!(recipe(changed_input), original_recipe);
    committed
}
// Native supplemental: native/tests/roster_state.rs::original_all_format_gallery_embeds_real_bytes_and_reuses_cache_without_touching_inputs
#[test]
fn original_all_format_gallery_embeds_real_bytes_and_reuses_cache_without_touching_inputs() {
    use _native::media_native_adapter::NativeMediaAdapter;
    use std::time::{Duration, Instant};
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let root = Root::new("roster thumbnails café");
    let native = NativeMediaAdapter {
        executable: std::path::PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji")),
        root: project.into(),
        scratch: root.0.join("envelopes"),
    };
    let media = bounded_media::BoundedCodec {
        native,
        deadline: Instant::now() + Duration::from_secs(60),
    };
    let fixture = gallery_fixture();
    let cases = fixture["cases"].as_array().unwrap();
    let mut art = BTreeMap::new();
    let mut originals = Vec::new();
    for (index, id, extension) in [(0, "111", "png"), (2, "222", "json"), (4, "333", "webm")] {
        let case = &cases[index];
        let source = project.join(case["input"].as_str().unwrap());
        _native::tooling_environment::reject_links(&source, "gallery fixture cannot follow links")
            .unwrap();
        let bytes = std::fs::read(&source).unwrap();
        assert_eq!(
            _native::codec_format::file_sha256(&source).unwrap(),
            case["input_sha256"]
        );
        let local = root.0.join(format!("{id}.{extension}"));
        std::fs::write(&local, &bytes).unwrap();
        art.insert(id.into(), local.clone());
        originals.push((source, local, bytes));
    }
    let doc = roster::pack(&json!({"name":"s","index":1,"logo":false}),
        &json!({"stickers":[{"custom_emoji_id":"111"},{"custom_emoji_id":"222","is_animated":true},{"custom_emoji_id":"333","is_video":true}]}),
        "general", &json!({}), &json!({}), "2026-10-07 00:00:00 UTC").unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let cache = root.0.join("thumbs");
    let page = runtime
        .block_on(_native::pack_gallery::render(&doc, &media, &art, &cache))
        .unwrap();
    assert!(page.contains("data:image/webp;base64,") && page.contains("data:video/webm;base64,"));
    assert!(!page.contains("no preview</div>"));
    let expected = [
        ("111.webp", 0),
        ("222.webp", 2),
        ("222_still.webp", 3),
        ("333.webm", 4),
    ];
    let mut before = BTreeMap::new();
    for (name, index) in expected {
        let path = cache.join(name);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len() as u64, cases[index]["bytes"].as_u64().unwrap());
        if index == 2 {
            // Existing reference policy: codec_lottie_preview's retained animated
            // gallery test accepts lossy WebP representation changes. The frozen
            // d006.../6262 receipt stays untouched; only this derivative's SHA
            // becomes semantic geometry/timing proof, never raw/RGBA equivalence.
            let source: Value =
                serde_json::from_slice(&std::fs::read(&art["222"]).unwrap()).unwrap();
            verify_gallery_animation(&bytes, &source);
        } else {
            assert_eq!(
                _native::codec_format::file_sha256(&path).unwrap(),
                cases[index]["sha256"]
            );
        }
        before.insert(name, (bytes, path.metadata().unwrap().modified().unwrap()));
    }
    assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 4);
    assert!(
        page.contains("src=\"data:image/webp;base64,")
            && page.contains("data-still=\"data:image/webp;base64,")
    );
    // A fresh cache forces the actual codec to run again in fresh owned workers;
    // cached reuse below independently preserves bytes and file mtimes.
    let rebuilt_cache = root.0.join("rebuilt-thumbs");
    let rebuilt_page = runtime
        .block_on(_native::pack_gallery::render(
            &doc,
            &media,
            &art,
            &rebuilt_cache,
        ))
        .unwrap();
    assert_eq!(rebuilt_page, page);
    assert_eq!(std::fs::read_dir(&rebuilt_cache).unwrap().count(), 4);
    for (name, _) in expected {
        assert_eq!(
            std::fs::read(rebuilt_cache.join(name)).unwrap(),
            std::fs::read(cache.join(name)).unwrap(),
            "fresh actual codec output must repeat exactly for {name}"
        );
    }
    let again = runtime
        .block_on(_native::pack_gallery::render(&doc, &media, &art, &cache))
        .unwrap();
    assert_eq!(page, again);
    for (name, (bytes, modified)) in before {
        let path = cache.join(name);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(path.metadata().unwrap().modified().unwrap(), modified);
    }
    for (source, local, bytes) in originals {
        assert_eq!(std::fs::read(source).unwrap(), bytes);
        assert_eq!(std::fs::read(local).unwrap(), bytes);
    }
    let missing_cache = root.0.join("missing-cache");
    let missing = BTreeMap::from([("111".into(), root.0.join("absent.png"))]);
    let page = runtime
        .block_on(_native::pack_gallery::render(
            &doc,
            &media,
            &missing,
            &missing_cache,
        ))
        .unwrap();
    assert!(page.contains("no preview</div>"));
    assert!(!missing_cache.exists());
    assert!(!media.native.scratch.exists());
    assert_eq!(std::fs::read_dir(cache).unwrap().count(), 4);
    runtime.shutdown_timeout(Duration::from_secs(1));
    root.clean().unwrap();
}
// Approved lossy-gallery semantics. No encoder/reference output generates expectations.
fn verify_gallery_animation(bytes: &[u8], source: &serde_json::Value) {
    use numera_runtime::native_raster as raster;
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8,
        bytes.len()
    );
    let decoded = raster::decode_animation(bytes, raster::DecodeLimits::default()).unwrap();
    assert_eq!(decoded.loop_count, 0);
    assert_eq!(decoded.frames.len(), 9);
    assert_eq!(source["w"], 512);
    assert_eq!(source["h"], 512);
    assert_eq!(source["fr"], 60);
    assert_eq!(source["ip"], 0);
    assert_eq!(source["op"], 60);
    let motion = &source["layers"][0]["ks"]["p"]["k"][0];
    assert_eq!(motion["s"], serde_json::json!([128, 256, 0]));
    assert_eq!(motion["e"], serde_json::json!([384, 256, 0]));
    assert_eq!(motion["o"]["x"], motion["o"]["y"]);
    assert_eq!(motion["i"]["x"], motion["i"]["y"]);
    let shapes = &source["layers"][0]["shapes"];
    assert_eq!(shapes[0]["s"]["k"], serde_json::json!([205, 205]));
    assert_eq!(shapes[1]["c"]["k"], serde_json::json!([1, 0, 0, 1]));
    let radius = 205. / 2. * 88. / 512.;
    let expected_area = std::f64::consts::PI * radius * radius;
    let original_frame_indices = [0., 7., 13., 20., 27., 33., 40., 47., 53.];
    for (index, frame) in decoded.frames.iter().enumerate() {
        assert_eq!(frame.rgba.dimensions(), (88, 88));
        assert_eq!(frame.duration_ms, 111);
        let source_frame = (60. * index as f64 / 9.).round();
        assert_eq!(source_frame, original_frame_indices[index]);
        let center_x = (128. + 256. * source_frame / 60.) * 88. / 512.;
        let center_y = 256. * 88. / 512.;
        let mut alpha_area = 0.;
        for (x, y, pixel) in frame.rgba.enumerate_pixels() {
            let distance = (f64::from(x) + 0.5 - center_x).hypot(f64::from(y) + 0.5 - center_y);
            alpha_area += f64::from(pixel[3]) / 255.;
            // The 1.5px border admits antialiasing at the source ellipse edge.
            // Interior/exterior opacity remains exact. Quality65 lossy gallery
            // color allows red>=220 and green/blue<=40, never a raw-pixel claim.
            if distance < radius - 1.5 {
                assert_eq!(pixel[3], 255, "frame{index} interior({x},{y})");
                assert!(
                    pixel[0] >= 220 && pixel[1] <= 40 && pixel[2] <= 40,
                    "frame{index} interior color({x},{y}):{pixel:?}"
                );
            } else if distance > radius + 1.5 {
                assert_eq!(pixel[3], 0, "frame{index} exterior({x},{y})");
            }
        }
        // Integrated opacity must still cover the analytic circle within2%.
        assert!(
            (alpha_area - expected_area).abs() <= expected_area * 0.02,
            "frame{index} area{alpha_area} expected{expected_area}"
        );
    }
}
