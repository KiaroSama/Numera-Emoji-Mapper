//! All three public ingest routes must preserve a real sampled-key collision and foreign scratch.
use super::{download, pack, sticker};
use _native::{
    catalog::Catalog, collector::Collector, config::Config, fetch_ids_cli, local_cli,
    logging::RunLog, media_adapter::MediaCodec, telegram::Telegram,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

pub fn check(
    root: &Path,
    project: &Path,
    media: &super::bounded_media::BoundedCodec,
    runtime: &tokio::runtime::Runtime,
    inputs: &[Vec<u8>; 2],
) {
    let sources = [root.join("route-blue.webm"), root.join("route-spiked.webm")];
    for (path, bytes) in sources.iter().zip(inputs) {
        std::fs::write(path, bytes).unwrap();
    }
    for route in ["pack", "ids", "local"] {
        let data = root.join(format!("route-{route}"));
        let temporary = data.join("tmp");
        std::fs::create_dir_all(&temporary).unwrap();
        let sentinel = temporary.join("another-writer.download");
        std::fs::write(&sentinel, b"not this run's scratch file").unwrap();
        let codec = media.with_scratch(temporary.join("codecs"));
        let log = RunLog::open(&data, "collision-route", vec![]);
        if route == "local" {
            let config = Config::load_with_environment(
                root.into(),
                &BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]),
            )
            .unwrap();
            let arguments = vec![
                sources[0].to_str().unwrap().into(),
                sources[1].to_str().unwrap().into(),
                "--data-dir".into(),
                data.to_str().unwrap().into(),
            ];
            assert_eq!(
                local_cli::run_with_codec(&config, &arguments, &log, |scratch| Ok(
                    media.with_scratch(scratch)
                ))
                .unwrap(),
                0
            );
        } else {
            let ids = ["5000000000000000001", "5000000000000000002"];
            let stickers = ids
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    let mut value = sticker(
                        &format!("route-{index}"),
                        &format!("route-fuid-{index}"),
                        "x",
                        "collision",
                    );
                    value["custom_emoji_id"] = json!(id);
                    value["is_video"] = json!(true);
                    value
                })
                .collect::<Vec<_>>();
            let first = if route == "pack" {
                pack(&stickers)
            } else {
                super::read_only_api::Reply::api(
                    "getCustomEmojiStickers",
                    "custom_emoji_ids",
                    &json!(ids).to_string(),
                    json!(stickers),
                )
            };
            let mut script = vec![first];
            for (index, bytes) in inputs.iter().enumerate() {
                script.extend(download(
                    &format!("route-{index}"),
                    &format!("route-{index}.webm"),
                    bytes,
                ));
            }
            let mut server = super::read_only_api::Fixture::new(script);
            let tg =
                Telegram::new(super::read_only_api::TOKEN.into(), server.base.clone()).unwrap();
            let mut catalog = Catalog::open(&data.join("catalog.db"), "fixture", project).unwrap();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut collector = Collector {
                    cat: &mut catalog,
                    media: &codec,
                    data: data.clone(),
                    threshold: -1,
                };
                let counts = if route == "pack" {
                    collector
                        .fetch_pack(&tg, runtime, "fixture", 0, "keep")
                        .unwrap()
                } else {
                    fetch_ids_cli::fetch_ids(
                        &mut collector,
                        &tg,
                        runtime,
                        &ids.map(str::to_owned),
                        "keep",
                        None,
                        &log,
                    )
                    .unwrap()
                };
                assert_eq!(counts["new"], 2, "{route}: {counts}");
                assert_eq!(counts["failed"], 0, "{route}: {counts}");
                assert_eq!(counts["dedup"], 0, "{route}: {counts}");
                let rows = collector.cat.all(Some("video")).unwrap();
                assert_eq!(rows.len(), 2);
                for index in 0..2 {
                    let seen = collector.cat.dispatch(json!({"operation":"seen","file_unique_id":format!("route-fuid-{index}")})).unwrap();
                    assert!(rows.iter().any(|row| row["content_key"] == seen));
                }
            }));
            let requests = server.finish();
            result.unwrap();
            assert_eq!(requests.unwrap().len(), 5, "{route}");
            drop(catalog);
        }
        let catalog = Catalog::open(&data.join("catalog.db"), "reopen", project).unwrap();
        let rows = catalog.all(Some("video")).unwrap();
        assert_eq!(
            rows.len(),
            2,
            "{route}: storage-before-catalog lost collision"
        );
        let keys = rows
            .iter()
            .map(|row| row["content_key"].as_str().unwrap())
            .collect::<BTreeSet<_>>();
        assert_eq!(keys.len(), 2);
        let paths = rows
            .iter()
            .map(|row| PathBuf::from(row["file_path"].as_str().unwrap()))
            .collect::<Vec<_>>();
        assert_ne!(paths[0], paths[1]);
        assert!(paths.iter().all(|path| path.is_file()));
        assert_eq!(
            runtime
                .block_on(codec.compare(&paths[0], &paths[1], "video"))
                .unwrap(),
            Some(false)
        );
        drop(catalog);
        assert_eq!(
            std::fs::read(&sentinel).unwrap(),
            b"not this run's scratch file"
        );
        assert_eq!(
            std::fs::read_dir(temporary.join("codecs")).unwrap().count(),
            0
        );
        let expected = BTreeSet::from(["another-writer.download".into(), "codecs".into()]);
        assert_eq!(
            std::fs::read_dir(&temporary)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<BTreeSet<std::ffi::OsString>>(),
            expected
        );
        assert!(!_native::process_owner::cleanup_pending(&data));
        log.finish(0);
        drop(log);
        assert!(
            Instant::now() < media.deadline,
            "{route}: shared collision deadline exhausted"
        );
    }
    refused_downloads(root, project, media, runtime, &inputs[0]);
    for (path, bytes) in sources.iter().zip(inputs) {
        assert_eq!(std::fs::read(path).unwrap(), *bytes);
    }
}

struct Occupied<'a> {
    inner: super::bounded_media::BoundedCodec,
    data: PathBuf,
    prepared: &'a Mutex<Option<(Vec<u8>, PathBuf, PathBuf)>>,
}
impl MediaCodec for Occupied<'_> {
    fn with_scratch(&self, scratch: PathBuf) -> Self {
        Self {
            inner: self.inner.with_scratch(scratch),
            data: self.data.clone(),
            prepared: self.prepared,
        }
    }

    async fn call(&self, request: Value) -> Result<Value, String> {
        let response = self.inner.call(request.clone()).await?;
        if request["operation"] == "prepare" {
            assert_eq!(response["fmt"], "video");
            let source = Path::new(request["source"].as_str().unwrap());
            let bytes = std::fs::read(source).map_err(|error| error.to_string())?;
            assert!(!bytes.is_empty());
            let key = response["content_key"].as_str().unwrap();
            let destination = self
                .data
                .join("media/video")
                .join(format!("{}.webm", key.replace(':', "_")));
            let collision = _native::codec_format::collision_key(source, key)?;
            let alternate =
                destination.with_file_name(format!("{}.webm", collision.replace(':', "_")));
            std::fs::create_dir_all(destination.parent().unwrap())
                .map_err(|error| error.to_string())?;
            std::fs::write(&destination, b"existing unrelated bytes")
                .map_err(|error| error.to_string())?;
            std::fs::write(&alternate, &bytes[..bytes.len() / 2])
                .map_err(|error| error.to_string())?;
            let mut prepared = self
                .prepared
                .lock()
                .map_err(|_| "retention observation poisoned")?;
            assert!(
                prepared.is_none(),
                "fixture must observe exactly one real prepare"
            );
            *prepared = Some((bytes, destination, alternate));
        }
        Ok(response)
    }
}

fn refused_downloads(
    root: &Path,
    project: &Path,
    media: &super::bounded_media::BoundedCodec,
    runtime: &tokio::runtime::Runtime,
    input: &[u8],
) {
    for route in ["pack", "ids"] {
        let data = root.join(format!("refused-download-{route}"));
        let temporary = data.join("tmp");
        let prepared = Mutex::new(None);
        let codec = Occupied {
            inner: media.with_scratch(temporary.join("codecs")),
            data: data.clone(),
            prepared: &prepared,
        };
        let log = RunLog::open(&data, "refused-download", vec![]);
        let mut item = sticker("12345", "fuid-12345", "x", "retention");
        item["custom_emoji_id"] = json!("12345");
        item["is_video"] = json!(true);
        let first = if route == "pack" {
            pack(&[item])
        } else {
            super::read_only_api::Reply::api(
                "getCustomEmojiStickers",
                "custom_emoji_ids",
                "[\"12345\"]",
                json!([item]),
            )
        };
        let mut script = vec![first];
        script.extend(download("12345", "retention.webm", input));
        let mut server = super::read_only_api::Fixture::new(script);
        let tg = Telegram::new(super::read_only_api::TOKEN.into(), server.base.clone()).unwrap();
        let mut catalog = Catalog::open(&data.join("catalog.db"), "fixture", project).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut collector = Collector {
                cat: &mut catalog,
                media: &codec,
                data: data.clone(),
                threshold: -1,
            };
            if route == "pack" {
                collector
                    .fetch_pack(&tg, runtime, "fixture", 0, "keep")
                    .unwrap()
            } else {
                fetch_ids_cli::fetch_ids(
                    &mut collector,
                    &tg,
                    runtime,
                    &["12345".into()],
                    "keep",
                    None,
                    &log,
                )
                .unwrap()
            }
        }));
        let requests = server.finish();
        let counts = result.unwrap_or_else(|error| std::panic::resume_unwind(error));
        assert_eq!(requests.unwrap().len(), 3);
        assert_eq!(counts["new"], 0);
        assert_eq!(counts["dedup"], 0);
        assert_eq!(counts["failed"], 1);
        assert!(catalog.all(None).unwrap().is_empty());
        assert!(
            catalog
                .dispatch(json!({"operation":"seen","file_unique_id":"fuid-12345"}))
                .unwrap()
                .is_null()
        );
        let (bytes, destination, alternate) = prepared
            .lock()
            .unwrap()
            .take()
            .expect("real reencode was not reached");
        let quarantine = destination.parent().unwrap().join("quarantine");
        let retained = std::fs::read_dir(&quarantine)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "webm"))
            .collect::<Vec<_>>();
        assert_eq!(retained.len(), 1);
        assert_eq!(std::fs::read(&retained[0]).unwrap(), bytes);
        let record: Value =
            serde_json::from_slice(&std::fs::read(retained[0].with_extension("json")).unwrap())
                .unwrap();
        assert_eq!(
            record["sha256"],
            _native::tooling_manifest::digest(&retained[0]).unwrap()
        );
        assert_eq!(record["size"], bytes.len());
        assert_eq!(record["provenance"]["file_unique_id"], "fuid-12345");
        let retry = temporary.join("retry.webm");
        std::fs::write(&retry, &bytes).unwrap();
        let error = runtime
            .block_on(_native::media_store::store(
                &codec.inner,
                &retry,
                &destination,
                "video",
                record["content_key"].as_str().unwrap(),
                &record["provenance"],
            ))
            .unwrap_err();
        assert!(error.contains("retained at"), "{error}");
        assert!(!retry.exists());
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"existing unrelated bytes"
        );
        assert_eq!(std::fs::read(&alternate).unwrap(), bytes[..bytes.len() / 2]);
        let after = std::fs::read_dir(&quarantine)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "webm"))
            .collect::<Vec<_>>();
        assert_eq!(after, retained);
        assert_eq!(std::fs::read(&retained[0]).unwrap(), bytes);
        let names = std::fs::read_dir(&temporary)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<BTreeSet<_>>();
        assert_eq!(names, BTreeSet::from([std::ffi::OsString::from("codecs")]));
        assert_eq!(
            std::fs::read_dir(temporary.join("codecs")).unwrap().count(),
            0
        );
        assert!(!_native::process_owner::cleanup_pending(&data));
        drop(catalog);
        let reopened = Catalog::open(&data.join("catalog.db"), "reopen", project).unwrap();
        assert!(reopened.all(None).unwrap().is_empty());
        drop(reopened);
        log.finish(0);
        drop(log);
        assert!(
            Instant::now() < media.deadline,
            "retention fixture deadline exhausted"
        );
    }
}
