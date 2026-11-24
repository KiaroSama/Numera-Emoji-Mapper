// Original-main Rust test; never counted toward1516 Python IDs.
// Source native/tests/gallery_parity.rs @9c3cbb43305b; SHA256 9381e85ae9fa4c11f196abf7d7f739cedce1f4e3f1ff5fd7f110f1054d650bfa
extern crate numera_runtime as _native;
#[path = "support/gallery_oracle.rs"]
mod gallery_oracle;
#[path = "original_main_rust_support/mod.rs"]
mod support;
// Exact public renderer parity; codec tuples are explicit boundary evidence, not real-media claims.
use _native::{media_adapter::MediaAdapter, pack_gallery};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[test]
fn incumbent_markup_controls_and_safe_roster_json_match_source() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(root.join("tests/fixtures/backend/gallery.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        fixture["source"],
        "d173116:pack_gallery.render+script_json.json_for_script"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let media = MediaAdapter {
        executable: PathBuf::from("must-not-run"),
        root: root.clone(),
        scratch: root.join("logs/unused-gallery-codec"),
    };
    // With no art, render cannot start a codec. The empty-family tracer already
    // catches markup/script drift without any child process or image fixture.
    let case = cases.iter().find(|c| c["name"] == "coins").unwrap();
    let html = runtime
        .block_on(pack_gallery::render(
            &case["document"],
            &media,
            &BTreeMap::new(),
            &root.join("logs/unused-gallery-cache"),
        ))
        .unwrap();
    assert_same(&html, case["expected"].as_str().unwrap());
    for case in cases {
        let thumbnails: BTreeMap<String, pack_gallery::Thumbnail> =
            serde_json::from_value(case["thumbnails"].clone()).unwrap();
        let html = pack_gallery::render_page(&case["document"], &thumbnails).unwrap();
        assert_same(&html, case["expected"].as_str().unwrap());
    }
}

#[test]
fn real_public_static_codec_and_native_cache_match_original_page() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(root.join("tests/fixtures/backend/gallery.json")).unwrap(),
    )
    .unwrap();
    let case = &fixture["real_static"];
    let workspace = root
        .join("logs")
        .join(format!("gallery-codec-test-{}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    let media = MediaAdapter {
        executable: env!("CARGO_BIN_EXE_numera-emoji").into(),
        root: root.clone(),
        scratch: workspace.join("codec"),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let art = BTreeMap::from([(
        "111111111".into(),
        root.join(case["source"].as_str().unwrap()),
    )]);
    let cache = workspace.join("thumbs");
    let result = runtime.block_on(pack_gallery::render(
        &case["document"],
        &media,
        &art,
        &cache,
    ));
    let repeated = runtime.block_on(pack_gallery::render(
        &case["document"],
        &media,
        &art,
        &cache,
    ));
    std::fs::remove_dir_all(&workspace).unwrap();
    let page = result.unwrap();
    assert_same(&page, case["expected"].as_str().unwrap());
    assert_eq!(repeated.unwrap(), page);
}

#[test]
fn cancelled_gallery_reaps_codec_before_removing_outer_staging() {
    let scratch = support::Scratch::new("gallery-cancel");
    let workspace = scratch.0.clone();
    let executable = scratch.worker("blocking-codec");
    let source = workspace.join("image.png");
    std::fs::write(&source, b"codec cancellation fixture, not an image decode").unwrap();
    let media = MediaAdapter {
        executable,
        root: workspace.clone(),
        scratch: workspace.join("codec"),
    };
    let cache = workspace.join("cache");
    let doc = serde_json::json!({"family":"general","set_name":"fixture","emoji":[{"custom_emoji_id":"111111111","format":"static"}]});
    let art = BTreeMap::from([("111111111".into(), source)]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let reached = runtime.block_on(async {
        let rendering = pack_gallery::render(&doc, &media, &art, &cache);
        tokio::pin!(rendering);
        let ready = async {
            while !workspace.join("ready").is_file() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        };
        tokio::select! {
            _ = tokio::time::timeout(std::time::Duration::from_secs(5), ready) => workspace.join("ready").is_file(),
            _ = &mut rendering => false,
        }
    });
    let staging = std::fs::read_dir(&cache)
        .map(|entries| entries.count())
        .unwrap_or(0);
    if reached {
        support::terminated(
            std::fs::read_to_string(workspace.join("ready"))
                .unwrap()
                .parse()
                .unwrap(),
        );
    }
    assert!(reached, "blocking codec never reached cancellation seam");
    assert_eq!(
        staging, 0,
        "cancelled renderer stranded its outer staging directory"
    );
}

fn assert_same(actual: &str, expected: &str) {
    let expected = gallery_oracle::with_reviewed_motion_gate(expected);
    if actual != expected {
        let offset = actual
            .bytes()
            .zip(expected.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(actual.len().min(expected.len()));
        panic!(
            "source HTML differs at byte{offset}: actual{}bytes expected{}bytes",
            actual.len(),
            expected.len()
        );
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
