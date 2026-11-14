//! One admitted original report checks actual local orchestration with native codec IPC.
use _native::{catalog::Catalog, config::Config, local_cli, logging::RunLog};
use serde_json::{Value, json};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[path = "bounded_media.rs"]
mod bounded_media;

fn arguments(input: &Path, data: &Path) -> Vec<String> {
    vec![
        "--in".into(),
        input.to_str().unwrap().into(),
        "--data-dir".into(),
        data.to_str().unwrap().into(),
    ]
}
fn empty_temporary(data: &Path) {
    assert_eq!(std::fs::read_dir(data.join("tmp")).unwrap().count(), 0);
}
fn original_bytes(value: &Value) -> Vec<u8> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|byte| u8::try_from(byte.as_u64().unwrap()).unwrap())
        .collect()
}

pub fn check(project: &Path, root: &Path, original: &Value, log: &RunLog) {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let directory = root.join("native local engine");
    std::fs::create_dir(&directory).unwrap();
    let directory = _native::tooling_environment::physical_root(&directory).unwrap();
    let config = Config::load_with_environment(
        directory.clone(),
        &BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let calls = Cell::new(0);
    let make_media = |scratch| {
        calls.set(calls.get() + 1);
        Ok(bounded_media::BoundedCodec {
            native: _native::media_native_adapter::NativeMediaAdapter {
                executable: PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji")),
                root: project.to_path_buf(),
                scratch,
            },
            deadline,
        })
    };
    let refused = directory.join("invalid threshold must not exist");
    let error = local_cli::run_with_codec(
        &config,
        &[
            "--phash-threshold".into(),
            "999".into(),
            "--data-dir".into(),
            refused.to_str().unwrap().into(),
        ],
        log,
        make_media,
    )
    .unwrap_err();
    assert!(error.contains("out of range"));
    assert!(!refused.exists());
    assert_eq!(calls.get(), 0);
    let unavailable = directory.join("unavailable codec");
    let error = local_cli::run_with_codec::<bounded_media::BoundedCodec>(
        &config,
        &[
            root.join("good.png").to_str().unwrap().into(),
            "--data-dir".into(),
            unavailable.to_str().unwrap().into(),
        ],
        log,
        |_scratch| Err("controlled codec prerequisite refusal".into()),
    )
    .unwrap_err();
    assert_eq!(error, "controlled codec prerequisite refusal");
    assert!(!unavailable.join("catalog.db").exists());
    empty_temporary(&unavailable);
    assert_eq!(calls.get(), 0);

    let good = std::fs::read(root.join("good.png")).unwrap();
    let blank = std::fs::read(root.join("blank.png")).unwrap();
    let mut clean = None;
    for (case, inputs, code, method) in [
        (
            "failed",
            vec![
                ("broken.png", b"not a png".as_slice()),
                ("also.webp", b"garbage".as_slice()),
            ],
            4,
            Some("AddMediaExitCode.test_all_inputs_unusable_exits_failed"),
        ),
        ("blank", vec![("blank.png", blank.as_slice())], 4, None),
        (
            "partial",
            vec![
                ("broken.png", b"not a png".as_slice()),
                ("good.png", good.as_slice()),
            ],
            3,
            Some("AddMediaExitCode.test_some_inputs_usable_exits_partial"),
        ),
        (
            "clean",
            vec![("good.png", good.as_slice())],
            0,
            Some("AddMediaExitCode.test_all_inputs_usable_exits_ok"),
        ),
        ("repeat", vec![("good.png", good.as_slice())], 0, None),
    ] {
        let input = directory.join(case).join("input");
        std::fs::create_dir_all(&input).unwrap();
        for (name, bytes) in &inputs {
            std::fs::write(input.join(name), bytes).unwrap();
        }
        let data = directory
            .join(if case == "repeat" { "clean" } else { case })
            .join("data");
        let result =
            local_cli::run_with_codec(&config, &arguments(&input, &data), log, make_media).unwrap();
        assert_eq!(result, code, "{case}");
        empty_temporary(&data);
        let catalog = Catalog::open(&data.join("catalog.db"), "fixture", &directory).unwrap();
        let rows = catalog.all(None).unwrap();
        if let Some(method) = method {
            let observations = original["observations"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|value| value["source_case"] == method)
                .collect::<Vec<_>>();
            assert_eq!(observations.len(), 1, "{method}");
            let observation = observations[0];
            assert_eq!(json!(result), observation["code"], "{case}");
            let expected = observation["items"].as_array().unwrap();
            assert_eq!(rows.len(), expected.len(), "{case}");
            for (actual, expected) in rows.iter().zip(expected) {
                for field in [
                    "content_key",
                    "fmt",
                    "phash",
                    "emojis",
                    "keywords",
                    "sources",
                    "uploaded",
                    "included",
                ] {
                    assert_eq!(actual[field], expected[field], "{case}: {field}");
                }
                super::assert_observed_media(
                    &std::fs::read(actual["file_path"].as_str().unwrap()).unwrap(),
                    &original_bytes(&expected["media_bytes"]),
                    case,
                );
            }
        }
        if matches!(case, "failed" | "blank") {
            assert!(rows.is_empty());
            assert_eq!(catalog.stats().unwrap(), json!({}));
            assert!(!data.join("media").exists());
        } else {
            assert_eq!(rows.len(), 1);
            let retained = (
                rows[0].clone(),
                std::fs::read(rows[0]["file_path"].as_str().unwrap()).unwrap(),
            );
            if case == "clean" {
                clean = Some(retained);
            } else if case == "repeat" {
                assert_eq!(Some(retained), clean);
            }
        }
        drop(catalog);
        for (name, bytes) in inputs {
            assert_eq!(std::fs::read(input.join(name)).unwrap(), bytes, "{case}");
        }
    }

    let artwork = &original["artwork"];
    let source = root.join("assets/numera-emoji-mapper-logo.png");
    let before = std::fs::read(&source).unwrap();
    assert_eq!(
        _native::tooling_manifest::digest(&source).unwrap(),
        artwork["input_sha256"]
    );
    let expected = original_bytes(&artwork["media_bytes"]);
    let clean_data = directory.join("artwork/data");
    let unknown = directory.join("unknown/data");
    std::fs::create_dir_all(&unknown).unwrap();
    let rival = unknown.join("rival.png");
    let invalid = b"undecodable retained catalog media";
    std::fs::write(&rival, invalid).unwrap();
    let mut catalog = Catalog::open(&unknown.join("catalog.db"), "fixture", &directory).unwrap();
    assert!(
        catalog
            .insert(
                &json!({
                    "content_key":artwork["content_key"],"fmt":"static","file_path":rival,
                    "phash":artwork["phash"],"emojis":["⭐"],"keywords":["original"],"sources":[]
                }),
                "fixture"
            )
            .unwrap()
    );
    let key = artwork["content_key"].as_str().unwrap();
    catalog.record_seen("prior-unique", key).unwrap();
    let prior = catalog.all(None).unwrap();
    drop(catalog);
    let mut clean_row = None;
    for (case, data, code) in [
        ("first", &clean_data, 0),
        ("repeat", &clean_data, 0),
        ("unknown", &unknown, 4),
    ] {
        assert_eq!(
            local_cli::run_with_codec(
                &config,
                &[
                    source.to_str().unwrap().into(),
                    "--as".into(),
                    "static".into(),
                    "--data-dir".into(),
                    data.to_str().unwrap().into(),
                    "--emoji".into(),
                    "😀".into(),
                    "--phash-threshold".into(),
                    "-1".into(),
                ],
                log,
                make_media,
            )
            .unwrap(),
            code,
            "{case}"
        );
        empty_temporary(data);
        let mut catalog = Catalog::open(&data.join("catalog.db"), "fixture", &directory).unwrap();
        let rows = catalog.all(None).unwrap();
        assert_eq!(rows.len(), 1, "{case}");
        assert_eq!(rows[0]["content_key"], artwork["content_key"]);
        assert_eq!(rows[0]["phash"], artwork["phash"]);
        if case == "unknown" {
            assert_eq!(rows, prior);
            assert_eq!(
                catalog
                    .dispatch(json!({"operation":"seen","file_unique_id":"prior-unique"}))
                    .unwrap(),
                key
            );
            assert_eq!(std::fs::read(&rival).unwrap(), invalid);
        } else {
            assert_eq!(rows[0]["emojis"], json!(["😀"]));
            assert_eq!(rows[0]["keywords"], json!(["numera-emoji-mapper-logo"]));
            assert_eq!(
                rows[0]["sources"],
                json!(["local:numera-emoji-mapper-logo.png"])
            );
            assert_eq!(rows[0]["uploaded"], false);
            assert_eq!(rows[0]["included"], true);
            if case == "first" {
                clean_row = Some(rows[0].clone());
            } else {
                assert_eq!(Some(rows[0].clone()), clean_row);
            }
        }
        drop(catalog);
        let stored = std::fs::read_dir(data.join("media/static"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "png"))
            .collect::<Vec<_>>();
        assert_eq!(stored.len(), 1, "{case}");
        super::assert_observed_media(&std::fs::read(&stored[0]).unwrap(), &expected, case);
        assert_eq!(std::fs::read(&source).unwrap(), before);
    }
    assert_eq!(calls.get(), 8);
    assert_eq!(std::fs::read(root.join("good.png")).unwrap(), good);
    assert_eq!(std::fs::read(root.join("blank.png")).unwrap(), blank);
    assert!(
        Instant::now() < deadline,
        "native ingest deadline elapsed; evidence retained"
    );
    assert!(!_native::process_owner::cleanup_pending(&directory));
    std::fs::remove_dir_all(directory).expect("clean verified native local engine fixture");
}
