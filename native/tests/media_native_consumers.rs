//! Actual brand/cache/storage consumers use native IPC without switching the installed adapter.
use _native::{media_adapter::MediaCodec, media_native_adapter::NativeMediaAdapter};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Duration;

struct Scratch(PathBuf);
impl Scratch {
    fn clean(self) {
        let envelopes = self.0.join("envelopes");
        assert_eq!(std::fs::read_dir(&envelopes).unwrap().count(), 0);
        assert!(!_native::process_owner::cleanup_pending(&envelopes));
        std::fs::remove_dir_all(&self.0).expect("clean media consumer fixture");
    }
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[path = "support/bounded_media.rs"]
mod bounded_media;

#[test]
fn brand_panel_and_store_requests_publish_only_answered_native_outputs() {
    let root = _native::tooling_environment::physical_root(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap(),
    )
    .unwrap();
    let path = root
        .join("logs/tmp")
        .join(format!("native media consumers {}", std::process::id()));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::create_dir(&path).unwrap();
    let scratch = Scratch(path);
    let adapter = NativeMediaAdapter {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji")),
        root: root.clone(),
        scratch: scratch.0.join("envelopes"),
    };
    let static_fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    let preview_fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-preview.json"
    ))
    .unwrap();
    let converted_fixture: Value = std::env::var_os("NUMERA_STATIC_CONVERT_ORACLE")
        .map(PathBuf::from)
        .map(|path| serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap())
        .unwrap_or_else(|| {
            serde_json::from_str(include_str!(
                "../../tests/fixtures/native/static-convert.json"
            ))
            .unwrap()
        });
    let source = root.join("tests/fixtures/native/static-codec/alpha-edge.png");
    let before = std::fs::read(&source).unwrap();
    assert_eq!(hash(&before), static_fixture["cases"][3]["input_sha256"]);
    assert_eq!(hash(&before), preview_fixture["cases"][1]["input_sha256"]);
    let (_, provider) = _native::source_static_oracle::fixtures().unwrap();
    _native::source_static_oracle::fixture_identity(&converted_fixture, &provider).unwrap();
    let source_expected = &static_fixture["cases"][3]["expected"];
    let reencoded_fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-reencode.json"
    ))
    .unwrap();
    let reencoded_case = reencoded_fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "alpha-edge")
        .unwrap();
    assert_eq!(hash(&before), reencoded_case["input_sha256"]);
    let conversion = &converted_fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "alpha-edge")
        .unwrap()["expected"];
    let data = scratch.0.join("actual consumers");
    std::fs::create_dir(&data).unwrap();
    let config = _native::config::Config::load_with_environment(
        data.clone(),
        &std::collections::BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("BRAND_LOGO_KEYWORDS".into(), "fixture, logo".into()),
        ]),
    )
    .unwrap();
    let corrupt_logo = data.join("corrupt.png");
    std::fs::write(&corrupt_logo, b"not an image").unwrap();
    let blank_logo = data.join("blank.png");
    {
        use pyo3::prelude::*;
        Python::initialize();
        Python::attach(|py| -> PyResult<()> {
            let image = py
                .import("PIL.Image")?
                .call_method1("new", ("RGBA", (100, 100), (0, 0, 0, 0)))?;
            let saved = image.call_method1("save", (&blank_logo, "PNG"));
            let closed = image.call_method0("close");
            saved?;
            closed?;
            Ok(())
        })
        .unwrap();
    }
    let refused_logos =
        [&corrupt_logo, &blank_logo].map(|path| (path, std::fs::read(path).unwrap()));
    let errors: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/media-continuation/static-errors.json"
    ))
    .unwrap();
    let corrupt_expected = errors["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "convert-corrupt")
        .unwrap();
    assert_eq!(hash(&refused_logos[0].1), corrupt_expected["input_sha256"]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(60), async {
            let admission = bounded_media::BoundedCodec {
                native: adapter.with_scratch(adapter.scratch.clone()),
                deadline: std::time::Instant::now() + Duration::from_secs(60),
            };
            let animated = data.join("admission.tgs");
            let animated_bytes = [vec![0x1f, 0x8b], vec![b'x'; 50]].concat();
            std::fs::write(&animated, &animated_bytes).unwrap();
            for (path, format, expected) in [
                (&blank_logo, "static", false),
                (&corrupt_logo, "static", true),
                (&animated, "animated", true),
            ] {
                assert_eq!(admission.publication_usable(path, format).await.unwrap(), expected);
                assert_eq!(std::fs::read_dir(&adapter.scratch).unwrap().count(), 0);
            }
            assert_eq!(std::fs::read(&animated).unwrap(), animated_bytes);
            let brand = _native::brand_logo::prepare(&adapter, &source, &data, &config)
                .await
                .unwrap();
            let cached = PathBuf::from(brand["file_path"].as_str().unwrap());
            let converted = std::fs::read(&cached).unwrap();
            assert_eq!(hash(&converted), conversion["png_sha256"]);
            assert_eq!(brand["fmt"], "static");
            assert_eq!(brand["emojis"], json!(["✅"]));
            assert_eq!(brand["keywords"], json!(["fixture", "logo"]));
            let decoded = _native::codec_static_identity::metadata(&cached, false).unwrap();
            assert_eq!(decoded["size"], json!([100, 100]));
            assert_eq!(decoded["mode"], "RGBA");
            let again = _native::brand_logo::prepare(&adapter, &source, &data, &config)
                .await
                .unwrap();
            assert_eq!(again, brand);
            assert_eq!(std::fs::read(&cached).unwrap(), converted);
            assert_eq!(std::fs::read_dir(data.join("brand")).unwrap().count(), 1);
            std::fs::write(&cached, &converted[..10]).unwrap();
            let repaired = _native::brand_logo::prepare(&adapter, &source, &data, &config)
                .await
                .unwrap();
            assert_eq!(repaired, brand);
            assert_eq!(std::fs::read(&cached).unwrap(), converted);
            assert!(adapter.call_with_budget(
                json!({"operation":"convert","source":source,"output":cached,"target_format":"static"}),
                Duration::from_secs(20), Duration::from_secs(10),
            ).await.is_err());
            assert_eq!(std::fs::read(&cached).unwrap(), converted);
            assert_eq!(
                _native::brand_logo::prepare(&adapter, &data.join("missing.png"), &data, &config)
                    .await
                    .unwrap_err(),
                "required brand logo is missing"
            );
            for (refused, original) in &refused_logos {
                let error = _native::brand_logo::prepare(&adapter, refused, &data, &config)
                    .await
                    .unwrap_err();
                if *refused == &blank_logo {
                    assert_eq!(error, "blank.png: image is blank (no visible pixels)");
                } else {
                    use pyo3::prelude::*;
                    // PyO3 passes pathlib.Path, which normalizes mixed Windows separators.
                    let source_repr = Python::attach(|py| {
                        (*refused).into_pyobject(py).unwrap().str().unwrap().repr().unwrap().to_string()
                    });
                    let normalized = error.replace(&source_repr, "'$source'");
                    assert_eq!(normalized, corrupt_expected["error"]["message"]);
                }
                assert_eq!(std::fs::read(refused).unwrap(), *original);
                assert_eq!(std::fs::read(&cached).unwrap(), converted);
                assert_eq!(std::fs::read_dir(data.join("brand")).unwrap().count(), 1);
                assert_eq!(std::fs::read_dir(&adapter.scratch).unwrap().count(), 0);
            }

            let logical_root = data.join("configured project");
            std::fs::create_dir(&logical_root).unwrap();
            let external = NativeMediaAdapter {
                executable: adapter.executable.clone(),
                root: logical_root.clone(),
                scratch: logical_root.join("envelopes"),
            };
            let output = data.join("outside configured project/nested/preview.webp");
            assert!(!output.starts_with(&logical_root));
            let request = json!({"operation":"preview","source":source,"format":"static","output":output,"key":"ok","size":52});
            let result = external.call_with_budget(request.clone(), Duration::from_secs(20), Duration::from_secs(10)).await.unwrap();
            let bytes = std::fs::read(&output).unwrap();
            assert_eq!(hash(&bytes), preview_fixture["cases"][1]["expected"]["sha256"]);
            assert_eq!(result["bytes"], preview_fixture["cases"][1]["expected"]["bytes"]);
            assert!(external.call_with_budget(request, Duration::from_secs(20), Duration::from_secs(10)).await.is_err());
            assert_eq!(std::fs::read(&output).unwrap(), bytes);
            assert_eq!(std::fs::read_dir(output.parent().unwrap()).unwrap().count(), 1);
            let blocked = data.join("caller-owned file");
            std::fs::write(&blocked, b"caller bytes must survive output refusal").unwrap();
            assert!(external.call_with_budget(
                json!({"operation":"preview","source":source,"output":blocked.join("preview.webp"),"key":"ok"}),
                Duration::from_secs(20), Duration::from_secs(10),
            ).await.is_err());
            assert_eq!(std::fs::read(&blocked).unwrap(), b"caller bytes must survive output refusal");
            #[cfg(unix)]
            for dangling in [false, true] {
                let physical = data.join(if dangling { "missing output directory" } else { "physical output directory" });
                if !dangling { std::fs::create_dir(&physical).unwrap(); }
                let linked = data.join(if dangling { "dangling output parent" } else { "linked output parent" });
                std::os::unix::fs::symlink(&physical, &linked).unwrap();
                assert!(external.call_with_budget(
                    json!({"operation":"preview","source":source,"output":linked.join("nested/preview.webp"),"key":"ok"}),
                    Duration::from_secs(20), Duration::from_secs(10),
                ).await.is_err());
                assert_eq!(std::fs::read_link(&linked).unwrap(), physical);
                if dangling { assert!(!physical.exists()); }
                else { assert_eq!(std::fs::read_dir(&physical).unwrap().count(), 0); }
                std::fs::remove_file(&linked).unwrap();
            }
            assert_eq!(std::fs::read_dir(&external.scratch).unwrap().count(), 0);
            assert!(!_native::process_owner::cleanup_pending(&external.scratch));
            assert_eq!(std::fs::read(&source).unwrap(), before);

            let preview_adapter = NativeMediaAdapter {
                executable: adapter.executable.clone(),
                root: adapter.root.clone(),
                scratch: adapter.scratch.clone(),
            };
            let preview = _native::panel_preview::Previews::new(preview_adapter, &data, 1);
            let local = data.join("preview-source.png");
            std::fs::copy(&source, &local).unwrap();
            let rendered = preview.bytes("s:fixture", &local, 15, false, 52).await.unwrap();
            let expected = &preview_fixture["cases"][1]["expected"];
            assert_eq!(rendered.len() as u64, expected["bytes"].as_u64().unwrap());
            assert_eq!(hash(&rendered), expected["sha256"]);
            std::fs::remove_file(&local).unwrap();
            assert_eq!(preview.bytes("s:fixture", &local, 15, false, 52).await.unwrap(), rendered);
            assert_eq!(std::fs::read_dir(data.join("preview")).unwrap().count(), 1);
            assert!(preview.bytes("bad/path", &source, 15, false, 52).await.is_err());
            assert!(preview.bytes("s:missing", &local, 15, false, 52).await.is_err());
            assert_eq!(std::fs::read_dir(data.join("preview")).unwrap().count(), 1);

            let fingerprint = adapter.call_with_budget(
                json!({"operation":"prepare","source":source,"reencode":false}),
                Duration::from_secs(20), Duration::from_secs(10),
            ).await.unwrap();
            assert_eq!(fingerprint["content_key"], source_expected["content_key"]);
            assert_eq!(fingerprint["phash"], source_expected["phash"]);
            assert_eq!(adapter.compare(&source, &source, "static").await.unwrap(), Some(true));
            let corrupt = root.join("tests/fixtures/native/static-compare/corrupt.png");
            let other = data.join("corrupt-other.png");
            std::fs::write(&other, b"not identical to the corrupt source").unwrap();
            assert_eq!(adapter.compare(&corrupt, &other, "static").await.unwrap(), None);

            let incoming = data.join("incoming.png");
            let collector_data = data.join("collector");
            let database = collector_data.join("catalog.db");
            let key = source_expected["content_key"].as_str().unwrap();
            let stored_name = format!("{}.webp", key.replace(':', "_"));
            let destination = collector_data.join("media/static").join(&stored_name);
            let retained_png = destination.with_extension("png");
            let mut catalog = _native::catalog::Catalog::open(&database, "fixture UTC", &root).unwrap();
            for (index, label) in ["first", "second"].into_iter().enumerate() {
                std::fs::write(&incoming, &before).unwrap();
                let mut collector = _native::collector::Collector {
                    cat: &mut catalog, media: &adapter, data: collector_data.clone(), threshold: 0,
                };
                let new = collector.ingest(
                    &incoming,
                    json!({"emojis":["😀"],"keywords":[label],"sources":[label],
                        "file_unique_id":format!("collector-fixture-{index}")}),
                    index == 0,
                ).await.unwrap();
                assert_eq!(new, index == 0);
                assert!(!incoming.exists());
                let bytes = std::fs::read(&destination).unwrap();
                assert_eq!(hash(&bytes), reencoded_case["expected"]["sha256"]);
                assert_eq!(bytes.len() as u64, reencoded_case["expected"]["bytes"]);
                assert_ne!(bytes, before);
                assert!(!retained_png.exists());
                assert_eq!(std::fs::read_dir(destination.parent().unwrap()).unwrap().count(), 1);
            }
            let stored_bytes = std::fs::read(&destination).unwrap();
            let canonical = catalog.get(key).unwrap().unwrap();
            assert_eq!(canonical["content_key"], source_expected["content_key"]);
            assert_eq!(canonical["phash"], source_expected["phash"]);
            assert_eq!(canonical["keywords"], json!(["first", "second"]));
            assert_eq!(canonical["sources"], json!(["first", "second"]));
            assert_eq!(canonical["emojis"], json!(["😀"]));
            assert_eq!(catalog.all(None).unwrap().len(), 1);
            for index in 0..2 {
                assert_eq!(catalog.dispatch(json!({"operation":"seen",
                    "file_unique_id":format!("collector-fixture-{index}")})).unwrap(), key);
            }
            std::fs::write(&incoming, &before).unwrap();
            let refused = {
                let mut collector = _native::collector::Collector {
                    cat: &mut catalog, media: &adapter, data: collector_data.clone(), threshold: 17,
                };
                collector.ingest(&incoming, json!({}), false).await.unwrap_err()
            };
            assert_eq!(refused, "invalid phash threshold");
            assert_eq!(std::fs::read(&incoming).unwrap(), before);
            assert_eq!(catalog.get(key).unwrap(), Some(canonical));
            let unknown_rival = collector_data.join("undecodable-rival.png");
            std::fs::write(&unknown_rival, b"not an image").unwrap();
            assert!(catalog.insert(&json!({"content_key":"s:unknown-rival","fmt":"static",
                "file_path":unknown_rival,"phash":source_expected["phash"]}), "fixture UTC").unwrap());
            let snapshot = catalog.all(None).unwrap();
            let uncertain = {
                let mut collector = _native::collector::Collector {
                    cat: &mut catalog, media: &adapter, data: collector_data.clone(), threshold: 0,
                };
                collector.ingest(&incoming, json!({"file_unique_id":"must-not-bind"}), false)
                    .await.unwrap_err()
            };
            assert_eq!(uncertain, "catalog identity is undecidable or ambiguous; incoming media was retained");
            assert_eq!(catalog.all(None).unwrap(), snapshot);
            assert!(catalog.dispatch(json!({"operation":"seen","file_unique_id":"must-not-bind"}))
                .unwrap().is_null());
            assert_eq!(std::fs::read(&destination).unwrap(), stored_bytes);
            assert!(!incoming.exists());
            assert_eq!(std::fs::read(&retained_png).unwrap(), before);
            assert_eq!(std::fs::read(&unknown_rival).unwrap(), b"not an image");
            drop(catalog);
            let mut reopened = _native::catalog::Catalog::open(&database, "fixture UTC", &root).unwrap();
            assert_eq!(reopened.all(None).unwrap(), snapshot);
            for index in 0..2 {
                assert_eq!(reopened.dispatch(json!({"operation":"seen",
                    "file_unique_id":format!("collector-fixture-{index}")})).unwrap(), key);
            }
            drop(reopened);
            let provenance = json!({"source_kind":"native-consumer-fixture"});

            let refused = data.join("refused.png");
            let unknown = b"not an image; never equal to either occupied destination";
            std::fs::write(&refused, unknown).unwrap();
            let digest = hash(unknown);
            let alternative = destination.with_file_name(format!("s_{}_fixture.webp", &digest[..32]));
            let rival = b"another undecodable image";
            std::fs::write(&alternative, rival).unwrap();
            let error = _native::media_store::store(
                &adapter, &refused, &destination, "static", "s:fixture", &provenance,
            ).await.unwrap_err();
            assert!(error.starts_with("media destination collision could not be resolved; incoming media retained at "));
            assert!(!refused.exists());
            assert_eq!(std::fs::read(&destination).unwrap(), stored_bytes);
            assert_eq!(std::fs::read(&retained_png).unwrap(), before);
            assert_eq!(std::fs::read(&alternative).unwrap(), rival);
            let quarantine = destination.parent().unwrap().join("quarantine");
            let retained = quarantine.join(format!("{digest}.webp"));
            assert_eq!(std::fs::read(&retained).unwrap(), unknown);
            let record: Value = serde_json::from_slice(
                &std::fs::read(retained.with_extension("json")).unwrap(),
            ).unwrap();
            assert_eq!(record, json!({
                "reason":"media-destination-collision","format":"static","content_key":"s:fixture",
                "sha256":digest,"size":unknown.len(),"destination":stored_name,"provenance":provenance
            }));
            assert_eq!(std::fs::read_dir(&quarantine).unwrap().count(), 2);
            assert_eq!(std::fs::read_dir(&adapter.scratch).unwrap().count(), 0);

            let gallery_fixture: Value = serde_json::from_str(include_str!(
                "../../tests/fixtures/backend/gallery.json"
            )).unwrap();
            _native::source_gallery_oracle::fixture_identity(&gallery_fixture).unwrap();
            let gallery_case = &gallery_fixture["real_static"];
            let gallery_original = root.join(gallery_case["source"].as_str().unwrap());
            let gallery_input = std::fs::read(&gallery_original).unwrap();
            assert_eq!(hash(&gallery_input), static_fixture["cases"][0]["input_sha256"]);
            let local_art = data.join("gallery-source.png");
            std::fs::write(&local_art, &gallery_input).unwrap();
            let art = std::collections::BTreeMap::from([("111111111".into(), local_art.clone())]);
            let gallery_cache = data.join("gallery");
            let page = _native::pack_gallery::render(
                &gallery_case["document"], &adapter, &art, &gallery_cache,
            ).await.unwrap();
            assert_eq!(page, gallery_case["expected"].as_str().unwrap());
            let thumbnail = gallery_cache.join("111111111.webp");
            let thumbnail_bytes = std::fs::read(&thumbnail).unwrap();
            assert!(!thumbnail_bytes.is_empty());
            assert_eq!(std::fs::read_dir(&gallery_cache).unwrap().count(), 1);
            // render requires an existing art path even when its thumbnail is cached.
            std::fs::write(&local_art, b"cached art must not be decoded again").unwrap();
            let repeated = _native::pack_gallery::render(
                &gallery_case["document"], &adapter, &art, &gallery_cache,
            ).await.unwrap();
            assert_eq!(repeated, page);
            assert_eq!(std::fs::read(thumbnail).unwrap(), thumbnail_bytes);
            assert_eq!(std::fs::read_dir(&gallery_cache).unwrap().count(), 1);
            assert_eq!(std::fs::read(gallery_original).unwrap(), gallery_input);
            assert_eq!(std::fs::read_dir(&adapter.scratch).unwrap().count(), 0);
        }).await.expect("actual native consumers exceeded60s; fixture retained");
    });
    assert_eq!(std::fs::read(source).unwrap(), before);
    scratch.clean();
}
