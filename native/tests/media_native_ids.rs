//! Requested-ID collection reaches native codecs without selecting them in the installed command.
use _native::{
    catalog::Catalog, collector::Collector, fetch_ids_cli, logging::RunLog,
    media_adapter::MediaCodec,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[path = "support/bounded_media.rs"]
mod bounded_media;
#[path = "support/read_only_api.rs"]
mod read_only_api;
use read_only_api::{Fixture, Reply};

fn sticker(id: &str, file: &str, fuid: &str, repaint: bool) -> Value {
    json!({"custom_emoji_id":id,"file_id":file,"file_unique_id":fuid,
        "emoji":"😀","needs_repainting":repaint})
}
fn resolve(ids: &[String], stickers: &[Value]) -> Reply {
    Reply::api(
        "getCustomEmojiStickers",
        "custom_emoji_ids",
        &json!(ids).to_string(),
        json!(stickers),
    )
}
fn download(file: &str, path: &str, bytes: &[u8]) -> [Reply; 2] {
    [
        Reply::api("getFile", "file_id", file, json!({"file_path":path})),
        Reply::file(path, bytes),
    ]
}
fn empty_scratch(data: &Path) {
    let temporary = data.join("tmp");
    let paths = std::fs::read_dir(&temporary)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(paths, [temporary.join("codecs")]);
    assert_eq!(std::fs::read_dir(&paths[0]).unwrap().count(), 0);
    assert!(!_native::process_owner::cleanup_pending(&temporary));
}

#[test]
fn requested_ids_preserve_gates_tint_identity_dedup_missing_and_batch_order() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let project = _native::tooling_environment::physical_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
    )
    .unwrap();
    let parent =
        _native::tooling_environment::confined_directory(&project, "logs/tmp", true).unwrap();
    let root = parent.join(format!("native requested ids {}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let root = _native::tooling_environment::physical_root(&root).unwrap();
    let identities: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    let reencoded: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-reencode.json"
    ))
    .unwrap();
    let tinted: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/media-continuation/prepare-tint.json"
    ))
    .unwrap();
    let cases = ["opaque", "alpha-edge", "public-logo"].map(|name| {
        identities["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap()
    });
    let inputs = cases.map(|case| {
        let path = project.join(case["input"].as_str().unwrap());
        assert_eq!(
            _native::tooling_manifest::digest(&path).unwrap(),
            case["input_sha256"]
        );
        std::fs::read(path).unwrap()
    });
    let ids = ["101", "102", "103"].map(str::to_owned);
    let stickers = [
        sticker("101", "opaque", "opaque-id-fuid", false),
        sticker("102", "alpha", "alpha-id-fuid", true),
        sticker("103", "logo", "logo-id-fuid", false),
    ];
    let failed_ids = ["104", "105"].map(str::to_owned);
    let corrupt = sticker("104", "bad", "must-not-bind", false);
    let batches = (10000..10201)
        .rev()
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    let mut script = vec![resolve(&ids, &stickers)];
    script.extend(download("opaque", "opaque.png", &inputs[0]));
    script.extend(download("logo", "logo.png", &inputs[2]));
    script.push(resolve(&ids, &stickers));
    script.extend(download("alpha", "alpha.png", &inputs[1]));
    script.push(resolve(&ids, &stickers));
    script.extend(download("alpha", "tint.png", &inputs[1]));
    script.push(resolve(&ids, &stickers));
    script.extend(download("alpha", "tint.png", &inputs[1]));
    script.push(resolve(&failed_ids, &[corrupt]));
    script.extend(download("bad", "bad.png", b"not a decodable image"));
    script.push(resolve(&batches[..200], &[]));
    script.push(resolve(&batches[200..], &[]));
    for _ in 0..2 {
        script.push(resolve(&ids, &stickers));
        script.extend(download("opaque", "opaque.png", &inputs[0]));
        script.extend(download("alpha", "alpha.png", &inputs[1]));
        script.extend(download("logo", "logo.png", &inputs[2]));
    }
    assert_eq!(script.len(), 33);
    let data = root.join("collection");
    let media = bounded_media::BoundedCodec {
        native: _native::media_native_adapter::NativeMediaAdapter {
            executable: PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji")),
            root: project.clone(),
            scratch: data.join("tmp/codecs"),
        },
        deadline: Instant::now() + Duration::from_secs(60),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let log = RunLog::open(
        &root,
        "requested-ids-test",
        vec![read_only_api::TOKEN.into()],
    );
    let log_path = log.path.clone().expect("fixture file log required");
    let mut catalog = Catalog::open(&data.join("catalog.db"), "fixture UTC", &project).unwrap();
    let mut server = Fixture::new(script);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let tg = _native::telegram::Telegram::new(read_only_api::TOKEN.into(), server.base.clone())
            .unwrap();
        let mut collector = Collector {
            cat: &mut catalog,
            media: &media,
            data: data.clone(),
            threshold: -1,
        };
        for (mode, tint, expected) in [
            ("invalid", None, "invalid repaintable choice"),
            ("skip", Some("#fff"), "tint must be #RRGGBB, got '#fff'"),
            (
                "keep",
                Some("#GG0000"),
                "tint must be #RRGGBB, got '#GG0000'",
            ),
        ] {
            assert_eq!(
                fetch_ids_cli::fetch_ids(&mut collector, &tg, &runtime, &ids, mode, tint, &log)
                    .unwrap_err(),
                expected
            );
            assert!(collector.cat.all(None).unwrap().is_empty());
            assert!(!data.join("tmp").exists());
        }
        assert_eq!(
            fetch_ids_cli::fetch_ids(&mut collector, &tg, &runtime, &ids, "skip", None, &log)
                .unwrap(),
            json!({"new":2,"dedup":0,"failed":0,"missing":0,"repaintable":1,"repainted":0})
        );
        let before = collector.cat.all(None).unwrap();
        assert_eq!(before.len(), 2);
        assert!(
            collector
                .cat
                .dispatch(json!({"operation":"seen","file_unique_id":"alpha-id-fuid"}))
                .unwrap()
                .is_null()
        );
        empty_scratch(&data);
        assert_eq!(
            fetch_ids_cli::fetch_ids(&mut collector, &tg, &runtime, &ids, "keep", None, &log)
                .unwrap(),
            json!({"new":1,"dedup":2,"failed":0,"missing":0,"repaintable":0,"repainted":0})
        );
        assert_eq!(collector.cat.all(None).unwrap().len(), 3);
        for (index, id) in ids.iter().enumerate() {
            let expected = &cases[index]["expected"];
            let row = collector
                .cat
                .get(expected["content_key"].as_str().unwrap())
                .unwrap()
                .unwrap();
            assert_eq!(row["phash"], expected["phash"]);
            assert_eq!(row["keywords"], json!([format!("premium-id:{id}")]));
            assert_eq!(row["sources"], json!(["bot-inventory"]));
            let bytes = reencoded["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|case| case["name"] == cases[index]["name"])
                .unwrap();
            let path = Path::new(row["file_path"].as_str().unwrap());
            assert_eq!(
                _native::tooling_manifest::digest(path).unwrap(),
                bytes["expected"]["sha256"]
            );
            assert_eq!(path.metadata().unwrap().len(), bytes["expected"]["bytes"]);
        }
        let original_alpha = collector
            .cat
            .get(cases[1]["expected"]["content_key"].as_str().unwrap())
            .unwrap()
            .unwrap();
        empty_scratch(&data);
        let tint_case = tinted["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == "success-reencode")
            .unwrap();
        let tint_key = tint_case["expected"]["content_key"].as_str().unwrap();
        for (new, dedup) in [(1, 2), (0, 3)] {
            assert_eq!(
                fetch_ids_cli::fetch_ids(
                    &mut collector,
                    &tg,
                    &runtime,
                    &ids,
                    "skip",
                    Some("#1a80ff"),
                    &log
                )
                .unwrap(),
                json!({"new":new,"dedup":dedup,"failed":0,"missing":0,"repaintable":0,"repainted":1})
            );
            let row = collector.cat.get(tint_key).unwrap().unwrap();
            assert_eq!(row["phash"], tint_case["expected"]["phash"]);
            assert_eq!(row["keywords"], json!(["premium-id:102", "tint:#1A80FF"]));
            assert_eq!(row["sources"], json!(["bot-inventory"]));
            assert!(row.get("tint").is_none());
            let path = Path::new(row["file_path"].as_str().unwrap());
            assert_eq!(
                _native::tooling_manifest::digest(path).unwrap(),
                tint_case["output"]["sha256"]
            );
            assert_eq!(path.metadata().unwrap().len(), tint_case["output"]["bytes"]);
            assert_eq!(
                collector
                    .cat
                    .dispatch(json!({"operation":"seen","file_unique_id":"alpha-id-fuid"}))
                    .unwrap(),
                cases[1]["expected"]["content_key"]
            );
            assert_eq!(
                collector
                    .cat
                    .get(cases[1]["expected"]["content_key"].as_str().unwrap())
                    .unwrap()
                    .unwrap(),
                original_alpha
            );
            assert_eq!(collector.cat.all(None).unwrap().len(), 4);
            empty_scratch(&data);
        }
        let before = collector.cat.all(None).unwrap();
        assert_eq!(
            fetch_ids_cli::fetch_ids(
                &mut collector,
                &tg,
                &runtime,
                &failed_ids,
                "skip",
                None,
                &log
            )
            .unwrap(),
            json!({"new":0,"dedup":0,"failed":1,"missing":1,"repaintable":0,"repainted":0})
        );
        assert_eq!(collector.cat.all(None).unwrap(), before);
        assert!(
            collector
                .cat
                .dispatch(json!({"operation":"seen","file_unique_id":"must-not-bind"}))
                .unwrap()
                .is_null()
        );
        empty_scratch(&data);
        assert_eq!(
            fetch_ids_cli::fetch_ids(&mut collector, &tg, &runtime, &batches, "skip", None, &log)
                .unwrap(),
            json!({"new":0,"dedup":0,"failed":0,"missing":201,"repaintable":0,"repainted":0})
        );
        assert_eq!(collector.cat.all(None).unwrap(), before);
        for (name, mode, tint, repainted) in [
            ("fresh keep", "keep", None, 0),
            ("fresh tint", "skip", Some("\u{1c}1a80ff\u{1f}"), 1),
        ] {
            let fresh_data = root.join(name);
            let fresh_media = media.with_scratch(fresh_data.join("tmp/codecs"));
            let mut fresh_catalog =
                Catalog::open(&fresh_data.join("catalog.db"), "fixture UTC", &project).unwrap();
            let mut fresh = Collector {
                cat: &mut fresh_catalog,
                media: &fresh_media,
                data: fresh_data.clone(),
                threshold: -1,
            };
            assert_eq!(
                fetch_ids_cli::fetch_ids(&mut fresh, &tg, &runtime, &ids, mode, tint, &log)
                    .unwrap(),
                json!({"new":3,"dedup":0,"failed":0,"missing":0,"repaintable":0,"repainted":repainted})
            );
            let rows = fresh.cat.all(None).unwrap();
            assert_eq!(rows.len(), 3);
            for (index, id) in ids.iter().enumerate() {
                let tinted = tint.is_some() && index == 1;
                let expected = if tinted {
                    &tint_case["expected"]
                } else {
                    &cases[index]["expected"]
                };
                let key = expected["content_key"].as_str().unwrap();
                let row = fresh.cat.get(key).unwrap().unwrap();
                assert_eq!(row["phash"], expected["phash"]);
                assert_eq!(row["emojis"], json!(["😀"]));
                assert_eq!(row["sources"], json!(["bot-inventory"]));
                let mut keywords = vec![format!("premium-id:{id}")];
                if tinted {
                    keywords.push("tint:#1A80FF".into());
                }
                assert_eq!(row["keywords"], json!(keywords));
                assert_eq!(
                    fresh
                        .cat
                        .dispatch(json!({"operation":"seen","file_unique_id":stickers[index]["file_unique_id"]}))
                        .unwrap(),
                    key
                );
                let output = if tinted {
                    &tint_case["output"]
                } else {
                    &reencoded["cases"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|case| case["name"] == cases[index]["name"])
                        .unwrap()["expected"]
                };
                let path = Path::new(row["file_path"].as_str().unwrap());
                assert_eq!(
                    _native::tooling_manifest::digest(path).unwrap(),
                    output["sha256"]
                );
                assert_eq!(path.metadata().unwrap().len(), output["bytes"]);
            }
            let marks = rows
                .iter()
                .flat_map(|row| row["keywords"].as_array().unwrap())
                .filter_map(Value::as_str)
                .filter(|keyword| keyword.starts_with("tint:"))
                .collect::<Vec<_>>();
            assert_eq!(
                marks,
                if tint.is_some() {
                    vec!["tint:#1A80FF"]
                } else {
                    vec![]
                }
            );
            empty_scratch(&fresh_data);
            drop(fresh_catalog);
            let reopened =
                Catalog::open(&fresh_data.join("catalog.db"), "fixture UTC", &project).unwrap();
            assert_eq!(reopened.all(None).unwrap(), rows);
        }
        assert!(
            Instant::now() < media.deadline,
            "requested-ID deadline elapsed; evidence retained"
        );
    }));
    let requests = server.finish();
    result.unwrap();
    assert_eq!(requests.unwrap().len(), 33);
    let rows = catalog.all(None).unwrap();
    drop(catalog);
    let reopened = Catalog::open(&data.join("catalog.db"), "fixture UTC", &project).unwrap();
    assert_eq!(reopened.all(None).unwrap(), rows);
    drop(reopened);
    log.finish(0);
    drop(log);
    let text = std::fs::read_to_string(log_path).unwrap();
    assert!(text.contains("[WARNING] [collector]"));
    assert!(!text.contains(read_only_api::TOKEN));
    assert!(text.contains("exit=0"));
    for (case, bytes) in cases.into_iter().zip(&inputs) {
        assert_eq!(
            std::fs::read(project.join(case["input"].as_str().unwrap())).unwrap(),
            *bytes
        );
    }
    empty_scratch(&data);
    std::fs::remove_dir_all(root).expect("clean verified requested-ID fixture");
}

#[test]
fn mixed_requested_ids_preserve_good_formats_when_download_or_resolution_fails() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let project = _native::tooling_environment::physical_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
    )
    .unwrap();
    let parent =
        _native::tooling_environment::confined_directory(&project, "logs/tmp", true).unwrap();
    let root = parent.join(format!("native mixed requested ids {}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let root = _native::tooling_environment::physical_root(&root).unwrap();
    let fixtures = [
        include_str!("../../tests/fixtures/native/static-codec.json"),
        include_str!("../../tests/fixtures/native/animated-key.json"),
        include_str!("../../tests/fixtures/native/video-codec.json"),
    ]
    .map(|text| serde_json::from_str::<Value>(text).unwrap());
    let paths = [
        "tests/fixtures/native/static-codec/alpha-edge.png",
        "tests/fixtures/native/tgs-package/valid.json",
        "tests/fixtures/native/video-codec/alpha.webm",
    ];
    let cases = std::array::from_fn::<_, 3, _>(|index| {
        fixtures[index]["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["input"] == paths[index])
            .unwrap()
    });
    let inputs = cases.map(|case| {
        let path = project.join(case["input"].as_str().unwrap());
        assert_eq!(
            _native::tooling_manifest::digest(&path).unwrap(),
            case["input_sha256"]
        );
        std::fs::read(path).unwrap()
    });
    let ids = [
        "111111111",
        "222222222",
        "333333333",
        "444444444",
        "555555555",
    ]
    .map(str::to_owned);
    let formats = ["static", "animated", "video"];
    let mut stickers = formats
        .iter()
        .enumerate()
        .map(|(index, format)| {
            let mut value = sticker(&ids[index], format, &format!("mixed-{format}"), false);
            value["is_animated"] = json!(*format == "animated");
            value["is_video"] = json!(*format == "video");
            value
        })
        .collect::<Vec<_>>();
    stickers.push(sticker(&ids[3], "bad", "mixed-bad", false));
    let mut script = vec![resolve(&ids, &stickers)];
    for (index, format) in formats.iter().enumerate() {
        script.extend(download(format, format, &inputs[index]));
    }
    script.extend(download("bad", "bad", b"not a decodable image"));
    assert_eq!(script.len(), 9);
    let data = root.join("collection");
    let media = bounded_media::BoundedCodec {
        native: _native::media_native_adapter::NativeMediaAdapter {
            executable: PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji")),
            root: project.clone(),
            scratch: data.join("tmp/codecs"),
        },
        deadline: Instant::now() + Duration::from_secs(60),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let log = RunLog::open(&root, "mixed-ids-test", vec![read_only_api::TOKEN.into()]);
    let mut catalog = Catalog::open(&data.join("catalog.db"), "fixture UTC", &project).unwrap();
    let mut server = Fixture::new(script);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let tg = _native::telegram::Telegram::new(read_only_api::TOKEN.into(), server.base.clone())
            .unwrap();
        let mut collector = Collector {
            cat: &mut catalog,
            media: &media,
            data: data.clone(),
            threshold: -1,
        };
        let counts =
            fetch_ids_cli::fetch_ids(&mut collector, &tg, &runtime, &ids, "keep", None, &log)
                .unwrap();
        assert_eq!(
            counts,
            json!({"new":3,"dedup":0,"failed":1,"missing":1,"repaintable":0,"repainted":0})
        );
        assert_eq!(
            _native::collector::ingest_exit_code(
                counts["new"].as_u64().unwrap() + counts["dedup"].as_u64().unwrap(),
                counts["failed"].as_u64().unwrap() + counts["missing"].as_u64().unwrap(),
            ),
            3
        );
        assert_eq!(collector.cat.all(None).unwrap().len(), 3);
        let reencoded: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/static-reencode.json"
        ))
        .unwrap();
        let animated: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/native/tgs-prepare.json"))
                .unwrap();
        for (index, format) in formats.iter().enumerate() {
            let expected = &cases[index]["expected"];
            let key = expected["content_key"].as_str().unwrap();
            let row = collector.cat.get(key).unwrap().unwrap();
            assert_eq!(row["fmt"], *format);
            assert_eq!(row["phash"], expected["phash"]);
            assert_eq!(
                row["keywords"],
                json!([format!("premium-id:{}", ids[index])])
            );
            assert_eq!(row["sources"], json!(["bot-inventory"]));
            assert_eq!(
                collector
                    .cat
                    .dispatch(json!({"operation":"seen",
                    "file_unique_id":format!("mixed-{format}")}))
                    .unwrap(),
                key
            );
            let path = Path::new(row["file_path"].as_str().unwrap());
            let bytes = std::fs::read(path).unwrap();
            assert!(!bytes.is_empty());
            assert_ne!(bytes, inputs[index], "{format} must use its own encoding");
            if index < 2 {
                let fixture = if index == 0 { &reencoded } else { &animated };
                let name = if index == 0 { "alpha-edge" } else { "unicode" };
                let encoded = fixture["cases"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|case| case["name"] == name)
                    .unwrap();
                assert_eq!(encoded["input_sha256"], cases[index]["input_sha256"]);
                assert_eq!(
                    _native::tooling_manifest::digest(path).unwrap(),
                    encoded["expected"]["sha256"]
                );
                assert_eq!(path.metadata().unwrap().len(), encoded["expected"]["bytes"]);
            }
        }
        assert!(
            collector
                .cat
                .dispatch(json!({"operation":"seen","file_unique_id":"mixed-bad"}))
                .unwrap()
                .is_null()
        );
        empty_scratch(&data);
        assert!(
            Instant::now() < media.deadline,
            "mixed-ID deadline elapsed; evidence retained"
        );
    }));
    let requests = server.finish();
    result.unwrap();
    assert_eq!(requests.unwrap().len(), 9);
    let rows = catalog.all(None).unwrap();
    drop(catalog);
    let reopened = Catalog::open(&data.join("catalog.db"), "fixture UTC", &project).unwrap();
    assert_eq!(reopened.all(None).unwrap(), rows);
    drop(reopened);
    log.finish(0);
    drop(log);
    for (index, path) in paths.iter().enumerate() {
        assert_eq!(std::fs::read(project.join(path)).unwrap(), inputs[index]);
    }
    empty_scratch(&data);
    std::fs::remove_dir_all(root).expect("clean verified mixed-ID fixture");
}
