//! Exact public renderer parity; codec tuples are explicit boundary evidence, not real-media claims.
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
        python: PathBuf::from("must-not-run"),
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
    let python = std::env::var_os("PYO3_PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            root.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        });
    let media = MediaAdapter {
        python,
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
    if std::env::var("NUMERA_CAPTURE_GALLERY_PREVIEW").as_deref() == Ok("1") {
        let preview = root.join("logs/gallery-preview");
        std::fs::create_dir_all(&preview).unwrap();
        if let Ok(page) = &result {
            std::fs::write(preview.join("index.html"), page.as_bytes()).unwrap();
        }
    }
    std::fs::remove_dir_all(&workspace).unwrap();
    let page = result.unwrap();
    assert_same(&page, case["expected"].as_str().unwrap());
    assert_eq!(repeated.unwrap(), page);
}

#[test]
fn cancelled_gallery_reaps_codec_before_removing_outer_staging() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let workspace = root
        .join("logs")
        .join(format!("gallery-cancel-{}", std::process::id()));
    std::fs::create_dir_all(workspace.join("emojikit")).unwrap();
    std::fs::write(workspace.join("emojikit/__init__.py"), b"").unwrap();
    // Linux must see the inherited-process-group protocol even for this
    // blocking fake codec; otherwise it refuses before the cancellation seam.
    std::fs::write(
        workspace.join("emojikit/media.py"),
        b"NATIVE_CODEC_GROUP_VERSION = 1\n",
    )
    .unwrap();
    std::fs::write(workspace.join("emojikit/media_bridge.py"), b"import pathlib,threading\npathlib.Path('ready').write_text('ready',encoding='utf-8')\nthreading.Event().wait()\n").unwrap();
    let source = workspace.join("image.png");
    std::fs::write(&source, b"codec cancellation fixture, not an image decode").unwrap();
    let python = std::env::var_os("PYO3_PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            root.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        });
    let media = MediaAdapter {
        python,
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
    std::fs::remove_dir_all(&workspace).unwrap();
    assert!(reached, "blocking codec never reached cancellation seam");
    assert_eq!(
        staging, 0,
        "cancelled renderer stranded its outer staging directory"
    );
}

fn assert_same(actual: &str, expected: &str) {
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
