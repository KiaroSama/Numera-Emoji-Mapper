//! Downloaded collection uses native codecs; known IDs and new-item limits keep their meaning.
use _native::{
    catalog::Catalog, collector::Collector, media_adapter::MediaCodec,
    media_native_adapter::NativeMediaAdapter,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[path = "support/bounded_media.rs"]
mod bounded_media;
#[path = "support/read_only_api.rs"]
mod read_only_api;
#[path = "support/repaint_collection_cases.rs"]
mod repaint_collection_cases;
#[path = "support/video_ingest_routes.rs"]
mod video_ingest_routes;
use read_only_api::{Fixture, Reply, TOKEN};

fn sticker(file: &str, fuid: &str, emoji: &str, keyword: &str) -> Value {
    json!({"file_id":file,"file_unique_id":fuid,"emoji":emoji,"keywords":[keyword]})
}
fn pack(stickers: &[Value]) -> Reply {
    Reply::api(
        "getStickerSet",
        "name",
        "fixture",
        json!({"name":"fixture","stickers":stickers}),
    )
}
fn download(file: &str, path: &str, bytes: &[u8]) -> [Reply; 2] {
    [
        Reply::api("getFile", "file_id", file, json!({"file_path":path})),
        Reply::file(path, bytes),
    ]
}
fn assert_empty_temporary(data: &Path) {
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
fn downloaded_collection_reencodes_skips_known_ids_limits_new_items_and_refuses_corrupt_media() {
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
    let directory = parent.join(format!(
        "native downloaded collection {}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).unwrap();
    let directory = _native::tooling_environment::physical_root(&directory).unwrap();
    let data = directory.join("collection");
    std::fs::create_dir(&data).unwrap();
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
    let input_cases = ["alpha-edge", "opaque"].map(|name| {
        identities["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap()
    });
    let inputs = input_cases.map(|case| {
        let path = project.join(case["input"].as_str().unwrap());
        assert_eq!(
            _native::tooling_manifest::digest(&path).unwrap(),
            case["input_sha256"]
        );
        std::fs::read(path).unwrap()
    });
    let known = sticker("must-not-download", "known-fuid", "✅", "known");
    let alpha = sticker("alpha", "alpha-fuid", "😀", "first");
    let alias = sticker("alias", "alias-fuid", "⭐", "second");
    let opaque = sticker("opaque", "opaque-fuid", "😀", "opaque");
    let corrupt = sticker("bad", "must-not-bind", "😀", "bad");
    let mut repaint = sticker("must-not-repaint", "skipped-fuid", "😀", "skip");
    repaint["needs_repainting"] = json!(true);
    let first = [
        known.clone(),
        repaint.clone(),
        alpha.clone(),
        opaque.clone(),
    ];
    let second = [
        known.clone(),
        alpha.clone(),
        alias.clone(),
        corrupt,
        opaque.clone(),
    ];
    let unlimited_stickers = [alpha.clone(), opaque.clone()];
    let third = [known, alpha, alias, opaque, repaint];
    let mut script = vec![pack(&first)];
    script.extend(download("alpha", "alpha.png", &inputs[0]));
    script.push(pack(&second));
    script.extend(download("alias", "alias.png", &inputs[0]));
    script.extend(download("bad", "bad.png", b"not a decodable image"));
    script.extend(download("opaque", "opaque.png", &inputs[1]));
    script.push(pack(&third));
    script.extend(download("alpha", "tint.png", &inputs[0]));
    script.push(pack(&unlimited_stickers));
    script.extend(download("alpha", "alpha.png", &inputs[0]));
    script.extend(download("opaque", "opaque.png", &inputs[1]));
    assert_eq!(script.len(), 18);
    let media = bounded_media::BoundedCodec {
        native: NativeMediaAdapter {
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
    let known_path = data.join("known.bin");
    std::fs::write(&known_path, b"preserved known media").unwrap();
    let mut catalog = Catalog::open(&data.join("catalog.db"), "fixture UTC", &project).unwrap();
    assert!(
        catalog
            .insert(
                &json!({"content_key":"s:known-seed","fmt":"static","file_path":known_path}),
                "fixture UTC"
            )
            .unwrap()
    );
    catalog.record_seen("known-fuid", "s:known-seed").unwrap();
    let mut server = Fixture::new(script);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let tg = _native::telegram::Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
        let mut collector = Collector {
            cat: &mut catalog,
            media: &media,
            data: data.clone(),
            threshold: -1,
        };
        assert_eq!(
            collector
                .fetch_pack(&tg, &runtime, "fixture", 1, "skip")
                .unwrap(),
            json!({"new":1,"dedup":1,"skipped":0,"failed":0,"repaintable":1})
        );
        assert_eq!(collector.cat.all(None).unwrap().len(), 2);
        assert!(
            collector
                .cat
                .dispatch(json!({"operation":"seen","file_unique_id":"opaque-fuid"}))
                .unwrap()
                .is_null()
        );
        assert!(
            collector
                .cat
                .dispatch(json!({"operation":"seen","file_unique_id":"skipped-fuid"}))
                .unwrap()
                .is_null()
        );
        assert_empty_temporary(&data);
        assert_eq!(
            collector
                .fetch_pack(&tg, &runtime, "fixture", 1, "skip")
                .unwrap(),
            json!({"new":1,"dedup":3,"skipped":0,"failed":1,"repaintable":0})
        );
        assert_eq!(collector.cat.all(None).unwrap().len(), 3);
        assert!(
            collector
                .cat
                .dispatch(json!({"operation":"seen","file_unique_id":"must-not-bind"}))
                .unwrap()
                .is_null()
        );
        for (index, name) in ["alpha-edge", "opaque"].into_iter().enumerate() {
            let expected = &input_cases[index]["expected"];
            let key = expected["content_key"].as_str().unwrap();
            let row = collector.cat.get(key).unwrap().unwrap();
            assert_eq!(row["phash"], expected["phash"]);
            let path = Path::new(row["file_path"].as_str().unwrap());
            let bytes = reencoded["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|case| case["name"] == name)
                .unwrap();
            assert_eq!(bytes["input_sha256"], input_cases[index]["input_sha256"]);
            assert_eq!(
                _native::tooling_manifest::digest(path).unwrap(),
                bytes["expected"]["sha256"]
            );
            assert_eq!(path.metadata().unwrap().len(), bytes["expected"]["bytes"]);
            assert_ne!(std::fs::read(path).unwrap(), inputs[index]);
            let fuid = if index == 0 {
                "alpha-fuid"
            } else {
                "opaque-fuid"
            };
            assert_eq!(
                collector
                    .cat
                    .dispatch(json!({"operation":"seen","file_unique_id":fuid}))
                    .unwrap(),
                key
            );
        }
        let alpha_key = input_cases[0]["expected"]["content_key"].as_str().unwrap();
        assert_eq!(
            collector
                .cat
                .dispatch(json!({"operation":"seen","file_unique_id":"alias-fuid"}))
                .unwrap(),
            alpha_key
        );
        let row = collector.cat.get(alpha_key).unwrap().unwrap();
        assert_eq!(row["emojis"], json!(["😀", "⭐"]));
        assert_eq!(row["keywords"], json!(["first", "second"]));
        assert_eq!(row["sources"], json!(["fixture"]));
        let alpha_before = row;
        let before = collector.cat.all(None).unwrap();
        assert_empty_temporary(&data);
        assert_eq!(
            collector
                .fetch_pack(&tg, &runtime, "fixture", 0, "skip")
                .unwrap(),
            json!({"new":0,"dedup":4,"skipped":0,"failed":0,"repaintable":1})
        );
        assert_eq!(collector.cat.all(None).unwrap(), before);
        assert_empty_temporary(&data);
        assert!(
            collector
                .download_ingest(
                    &tg,
                    &runtime,
                    &sticker("alpha", "tint-fuid", "✅", "tinted"),
                    json!({"emojis":["✅"],"keywords":["tinted"],"sources":["fixture-tint"],
                "file_unique_id":"tint-fuid","tint":"#1A80FF"})
                )
                .unwrap()
        );
        let expected = tinted["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == "success-reencode")
            .unwrap();
        assert_eq!(expected["input_sha256"], input_cases[0]["input_sha256"]);
        let tint_key = expected["expected"]["content_key"].as_str().unwrap();
        let row = collector.cat.get(tint_key).unwrap().unwrap();
        assert_eq!(row["phash"], expected["expected"]["phash"]);
        assert_eq!(row["keywords"], json!(["tinted"]));
        assert_eq!(row["sources"], json!(["fixture-tint"]));
        assert!(row.get("tint").is_none());
        let path = Path::new(row["file_path"].as_str().unwrap());
        assert_eq!(
            _native::tooling_manifest::digest(path).unwrap(),
            expected["output"]["sha256"]
        );
        assert_eq!(path.metadata().unwrap().len(), expected["output"]["bytes"]);
        assert_eq!(
            collector
                .cat
                .dispatch(json!({"operation":"seen","file_unique_id":"tint-fuid"}))
                .unwrap(),
            tint_key
        );
        assert_eq!(collector.cat.all(None).unwrap().len(), 4);
        assert_eq!(collector.cat.get(alpha_key).unwrap().unwrap(), alpha_before);
        assert_eq!(
            std::fs::read(&known_path).unwrap(),
            b"preserved known media"
        );
        assert_empty_temporary(&data);
        let unlimited_data = directory.join("unlimited");
        let unlimited_media = media.with_scratch(unlimited_data.join("tmp/codecs"));
        let mut unlimited_catalog =
            Catalog::open(&unlimited_data.join("catalog.db"), "fixture UTC", &project).unwrap();
        let mut unlimited = Collector {
            cat: &mut unlimited_catalog,
            media: &unlimited_media,
            data: unlimited_data.clone(),
            threshold: -1,
        };
        assert_eq!(
            unlimited
                .fetch_pack(&tg, &runtime, "fixture", 0, "skip")
                .unwrap(),
            json!({"new":2,"dedup":0,"skipped":0,"failed":0,"repaintable":0})
        );
        let rows = unlimited.cat.all(None).unwrap();
        assert_eq!(rows.len(), 2);
        for (index, fuid) in ["alpha-fuid", "opaque-fuid"].into_iter().enumerate() {
            let expected = &input_cases[index]["expected"];
            let key = expected["content_key"].as_str().unwrap();
            let row = unlimited.cat.get(key).unwrap().unwrap();
            assert_eq!(row["phash"], expected["phash"]);
            let encoded = reencoded["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|case| case["name"] == input_cases[index]["name"])
                .unwrap();
            assert_eq!(
                _native::tooling_manifest::digest(Path::new(row["file_path"].as_str().unwrap()))
                    .unwrap(),
                encoded["expected"]["sha256"]
            );
            assert_eq!(
                unlimited
                    .cat
                    .dispatch(json!({"operation":"seen","file_unique_id":fuid}))
                    .unwrap(),
                key
            );
        }
        assert_empty_temporary(&unlimited_data);
        assert!(
            Instant::now() < media.deadline,
            "downloaded collector deadline elapsed; evidence retained"
        );
    }));
    let requests = server.finish();
    result.unwrap();
    assert_eq!(requests.unwrap().len(), 18);
    let rows = catalog.all(None).unwrap();
    drop(catalog);
    let mut reopened = Catalog::open(&data.join("catalog.db"), "fixture UTC", &project).unwrap();
    assert_eq!(reopened.all(None).unwrap(), rows);
    assert!(
        reopened
            .dispatch(json!({"operation":"seen","file_unique_id":"must-not-bind"}))
            .unwrap()
            .is_null()
    );
    drop(reopened);
    for (case, bytes) in input_cases.into_iter().zip(&inputs) {
        assert_eq!(
            std::fs::read(project.join(case["input"].as_str().unwrap())).unwrap(),
            *bytes
        );
    }
    assert_empty_temporary(&data);
    repaint_collection_cases::check(&directory, &project, &media, &runtime);
    runtime.shutdown_timeout(Duration::from_secs(1));
    std::fs::remove_dir_all(directory).expect("clean verified downloaded collection fixture");
}

#[test]
fn video_sample_collisions_keep_distinct_media_and_remux_aliases_after_reopen() {
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
    let root = parent.join(format!("native video collisions {}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let root = _native::tooling_environment::physical_root(&root).unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/video-timeline.json"
    ))
    .unwrap();
    let cases = ["blue", "spiked"].map(|name| {
        fixture["cases"]
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
    let key = cases[0]["expected"]["content_key"].as_str().unwrap();
    assert_eq!(cases[1]["expected"]["content_key"], key);
    assert!(fixture["pairs"].as_array().unwrap().iter().any(|pair| {
        pair["left"] == "blue" && pair["right"] == "spiked" && pair["expected"] == false
    }));
    let data = root.join("collection");
    let media = bounded_media::BoundedCodec {
        native: NativeMediaAdapter {
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
    let mut catalog = Catalog::open(&data.join("catalog.db"), "fixture UTC", &project).unwrap();
    let incoming = root.join("incoming.webm");
    for (index, fuid) in ["collision-blue", "collision-spiked"]
        .into_iter()
        .enumerate()
    {
        std::fs::write(&incoming, &inputs[index]).unwrap();
        let mut collector = Collector {
            cat: &mut catalog,
            media: &media,
            data: data.clone(),
            threshold: -1,
        };
        assert!(
            runtime
                .block_on(collector.ingest(
                    &incoming,
                    json!({"emojis":["😀"],"keywords":[fuid],"sources":["video-fixture"],
                "file_unique_id":fuid}),
                    true,
                ))
                .unwrap()
        );
        assert!(!incoming.exists());
    }
    assert_eq!(catalog.all(None).unwrap().len(), 2);
    assert_eq!(
        catalog
            .dispatch(json!({"operation":"seen","file_unique_id":"collision-blue"}))
            .unwrap(),
        key
    );
    let distinct = catalog
        .dispatch(json!({"operation":"seen","file_unique_id":"collision-spiked"}))
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(distinct, key);
    assert_eq!(distinct.matches(':').count(), 2);
    assert!(distinct.ends_with(&format!(":{}", key.strip_prefix("v:").unwrap())));
    let paths = [key, distinct.as_str()].map(|key| {
        PathBuf::from(
            catalog.get(key).unwrap().unwrap()["file_path"]
                .as_str()
                .unwrap(),
        )
    });
    assert_ne!(paths[0], paths[1]);
    let stored = paths
        .iter()
        .map(|path| std::fs::read(path).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        runtime
            .block_on(media.compare(&paths[0], &paths[1], "video"))
            .unwrap(),
        Some(false)
    );
    std::fs::write(&incoming, &inputs[1]).unwrap();
    {
        let mut collector = Collector {
            cat: &mut catalog,
            media: &media,
            data: data.clone(),
            threshold: -1,
        };
        assert!(
            !runtime
                .block_on(collector.ingest(
                    &incoming,
                    json!({"emojis":["⭐"],"keywords":["alias"],"sources":["video-fixture"],
                "file_unique_id":"collision-remux"}),
                    true,
                ))
                .unwrap()
        );
    }
    assert!(!incoming.exists());
    assert_eq!(catalog.all(None).unwrap().len(), 2);
    for (path, bytes) in paths.iter().zip(&stored) {
        assert_eq!(std::fs::read(path).unwrap(), *bytes);
    }
    let rows = catalog.all(None).unwrap();
    drop(catalog);
    let mut reopened = Catalog::open(&data.join("catalog.db"), "fixture UTC", &project).unwrap();
    assert_eq!(reopened.all(None).unwrap(), rows);
    for (fuid, expected) in [
        ("collision-blue", key),
        ("collision-spiked", distinct.as_str()),
        ("collision-remux", distinct.as_str()),
    ] {
        assert_eq!(
            reopened
                .dispatch(json!({"operation":"seen","file_unique_id":fuid}))
                .unwrap(),
            expected
        );
    }
    assert_eq!(
        reopened.get(key).unwrap().unwrap()["keywords"],
        json!(["collision-blue"])
    );
    assert_eq!(
        reopened.get(&distinct).unwrap().unwrap()["keywords"],
        json!(["collision-spiked", "alias"])
    );
    let before_refusal = reopened.all(None).unwrap();
    std::fs::remove_file(&paths[0]).unwrap();
    std::fs::write(&incoming, &inputs[1]).unwrap();
    let error = runtime
        .block_on(_native::ingest::add_verified(
            &mut reopened,
            json!({"content_key":key,"fmt":"video","file_path":incoming,
                "file_unique_id":"collision-unknown"}),
            "fixture UTC",
            -1,
            async |a: &Path, b: &Path, format: &str| {
                Ok(media.compare(a, b, format).await.unwrap_or(None))
            },
            async |_source: &Path, _key: &str| {
                panic!("unknown identity must refuse before creating a collision key")
            },
        ))
        .unwrap_err();
    assert_eq!(
        error,
        "catalog identity is undecidable or ambiguous; incoming media was retained"
    );
    assert_eq!(reopened.all(None).unwrap(), before_refusal);
    assert!(
        reopened
            .dispatch(json!({"operation":"seen","file_unique_id":"collision-unknown"}))
            .unwrap()
            .is_null()
    );
    assert_eq!(std::fs::read(&incoming).unwrap(), inputs[1]);
    assert!(!paths[0].exists());
    assert_eq!(std::fs::read(&paths[1]).unwrap(), stored[1]);
    std::fs::write(&paths[0], &stored[0]).unwrap();
    drop(reopened);

    let occupied = root.join("occupied.webm");
    std::fs::write(&occupied, b"unrelated sentinel").unwrap();
    std::fs::write(&incoming, &inputs[1]).unwrap();
    let alternative = runtime
        .block_on(_native::media_store::store(
            &media,
            &incoming,
            &occupied,
            "video",
            key,
            &json!({"source":"video-fixture"}),
        ))
        .unwrap();
    assert_ne!(alternative, occupied);
    assert_eq!(std::fs::read(&occupied).unwrap(), b"unrelated sentinel");
    assert_eq!(std::fs::read(&alternative).unwrap(), inputs[1]);
    assert!(!incoming.exists());
    assert_empty_temporary(&data);
    video_ingest_routes::check(&root, &project, &media, &runtime, &inputs);
    assert!(
        Instant::now() < media.deadline,
        "video collision deadline elapsed; evidence retained"
    );
    for (case, bytes) in cases.into_iter().zip(&inputs) {
        assert_eq!(
            std::fs::read(project.join(case["input"].as_str().unwrap())).unwrap(),
            *bytes
        );
    }
    runtime.shutdown_timeout(Duration::from_secs(1));
    std::fs::remove_dir_all(root).expect("clean verified video collision fixture");
}
