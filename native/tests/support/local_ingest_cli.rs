//! Actual local-ingest CLI comparisons against independently replayed original observations.
use _native::catalog::Catalog;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[path = "local_ingest_native.rs"]
mod local_ingest_native;
use crate::retained_cli;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("Ingest parity fixture retained: {}", self.0.display());
            return;
        }
        let children = self.0.join("logs/tmp/tooling");
        match std::fs::read_dir(&children) {
            Ok(mut entries) => {
                assert!(entries.next().is_none(), "owned ingest children unresolved")
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("cannot inspect ingest child cleanup: {error}"),
        }
        std::fs::remove_dir_all(&self.0).expect("clean owned ingest parity catalog");
    }
}

pub fn check(project: &Path, runtime: &tokio::runtime::Runtime, original: &Value) {
    use _native::{logging::RunLog, tooling_environment, tooling_process};
    use pyo3::prelude::*;
    use pyo3::types::PyBytes;
    use std::ffi::OsString;
    use std::time::Duration;

    let inventory: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/source-baseline.json"
    ))
    .unwrap();
    assert_eq!(
        inventory["baseline_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    for (path, expected) in [
        (
            "tests/test_resume_safety.py",
            "0af175ef5368ed7a6613ce487f65ff5b8f42e1751fcfa9e787b17faea956b3d4",
        ),
        (
            "tests/_pack_fixtures.py",
            "bbac616f05c178647206528c015f4ba85df29a36c93fb69de52f2a0d505368d8",
        ),
        (
            "tests/reference/add_media.py",
            "8d865761219c77a459e5e37c45f15aeb05f41228a847dce6d9ab0eedc214aac6",
        ),
    ] {
        let source = inventory["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|source| source["source"] == path)
            .unwrap();
        assert_eq!(source["sha256"], expected, "{path}");
    }
    let parent = tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
    let root = parent.join(format!("local ingest café {}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let scratch = Scratch(tooling_environment::physical_root(&root).unwrap());
    let (executable, environment) = retained_cli::install(project, &scratch.0);
    let log = RunLog::open(&scratch.0, "local-ingest-test", vec![]);
    assert!(log.path.is_some());
    let good = scratch.0.join("good.png");
    let blank = scratch.0.join("blank.png");
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        for (path, size, color) in [
            (&good, (100, 100), (0, 180, 90, 255)),
            (&blank, (120, 90), (0, 0, 0, 0)),
        ] {
            let image = py
                .import("PIL.Image")?
                .call_method1("new", ("RGBA", size, color))?;
            let saved = image.call_method1("save", (path, "PNG"));
            let closed = image.call_method0("close");
            saved?;
            closed?;
        }
        Ok(())
    })
    .unwrap();
    let good_bytes = std::fs::read(&good).unwrap();
    let blank_bytes = std::fs::read(&blank).unwrap();
    let mut clean_media = None;
    for (case, inputs, expected, summary) in [
        (
            "failed",
            vec![
                ("broken.png", b"not a png".as_slice()),
                ("also.webp", b"garbage".as_slice()),
            ],
            4,
            "Done. new=0 dedup=0 failed=2\n",
        ),
        (
            "blank",
            vec![("blank.png", blank_bytes.as_slice())],
            4,
            "Done. new=0 dedup=0 failed=1\n",
        ),
        (
            "partial",
            vec![
                ("broken.png", b"not a png".as_slice()),
                ("good.png", good_bytes.as_slice()),
            ],
            3,
            "Done. new=1 dedup=0 failed=1\n  catalog static: 1 total (1 pending upload)\n",
        ),
        (
            "clean",
            vec![("good.png", good_bytes.as_slice())],
            0,
            "Done. new=1 dedup=0 failed=0\n  catalog static: 1 total (1 pending upload)\n",
        ),
        (
            "repeat",
            vec![("good.png", good_bytes.as_slice())],
            0,
            "Done. new=0 dedup=1 failed=0\n  catalog static: 1 total (1 pending upload)\n",
        ),
    ] {
        let source = scratch.0.join(case).join("in");
        std::fs::create_dir_all(&source).unwrap();
        for (name, bytes) in &inputs {
            std::fs::write(source.join(name), bytes).unwrap();
        }
        let data = scratch
            .0
            .join(if case == "repeat" { "clean" } else { case })
            .join("data");
        let argv = [
            executable.as_os_str().to_owned(),
            OsString::from("add-media"),
            "--in".into(),
            source.as_os_str().to_owned(),
            "--data-dir".into(),
            data.as_os_str().to_owned(),
        ];
        let completed = runtime
            .block_on(tooling_process::capture(
                &scratch.0,
                &argv,
                &environment,
                Duration::from_secs(20),
                Duration::from_secs(10),
                &log,
            ))
            .unwrap();
        assert_eq!(completed.code, expected, "{case}: {}", completed.stderr);
        assert_eq!(completed.stdout, summary, "{case}");
        let source_case = match case {
            "failed" => Some("AddMediaExitCode.test_all_inputs_unusable_exits_failed"),
            "partial" => Some("AddMediaExitCode.test_some_inputs_usable_exits_partial"),
            "clean" => Some("AddMediaExitCode.test_all_inputs_usable_exits_ok"),
            "repeat" | "blank" => None,
            _ => unreachable!(),
        };
        let observed = source_case.map(|name| {
            let matches = original["observations"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|observation| observation["source_case"] == name)
                .collect::<Vec<_>>();
            assert_eq!(matches.len(), 1, "{name}");
            let observation = matches[0];
            assert_eq!(completed.code, observation["code"].as_i64().unwrap() as i32);
            assert_eq!(completed.stdout, observation["stdout"].as_str().unwrap());
            observation
        });
        assert!(
            std::fs::read_dir(data.join("tmp"))
                .unwrap()
                .next()
                .is_none(),
            "{case}: codec workspace remains"
        );
        assert!(
            std::fs::read_dir(scratch.0.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none()
        );
        for (name, bytes) in &inputs {
            assert_eq!(std::fs::read(source.join(name)).unwrap(), *bytes);
        }
        let catalog = Catalog::open(&data.join("catalog.db"), "fixture", &scratch.0).unwrap();
        let rows = catalog.all(None).unwrap();
        if let Some(observed) = observed {
            let source_rows = observed["items"].as_array().unwrap();
            assert_eq!(rows.len(), source_rows.len());
            for (row, source) in rows.iter().zip(source_rows) {
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
                    assert_eq!(row[field], source[field], "{case}: {field}");
                }
                let expected_bytes = source["media_bytes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|byte| u8::try_from(byte.as_u64().unwrap()).unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(
                    std::fs::read(row["file_path"].as_str().unwrap()).unwrap(),
                    expected_bytes
                );
            }
        }
        if matches!(case, "failed" | "blank") {
            assert!(rows.is_empty());
            assert_eq!(catalog.stats().unwrap(), json!({}));
            assert!(
                !data.join("media").exists(),
                "{case}: refused input was stored"
            );
        } else {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0]["fmt"], "static");
            assert_eq!(rows[0]["phash"], 0);
            assert_eq!(rows[0]["included"], true);
            assert_eq!(rows[0]["keywords"], json!(["good"]));
            assert_eq!(rows[0]["emojis"], json!(["😀"]));
            assert_eq!(rows[0]["sources"], json!(["local:good.png"]));
            assert_eq!(rows[0]["uploaded"], false);
            let path = Path::new(rows[0]["file_path"].as_str().unwrap());
            assert!(path.is_file());
            Python::attach(|py| -> PyResult<()> {
                let image = py.import("PIL.Image")?.call_method1("open", (path,))?;
                let observed = (|| -> PyResult<_> {
                    Ok((
                        image.getattr("size")?.extract::<(u32, u32)>()?,
                        image.getattr("mode")?.extract::<String>()?,
                        image
                            .call_method0("tobytes")?
                            .cast_into::<PyBytes>()?
                            .as_bytes()
                            .to_vec(),
                    ))
                })();
                let closed = image.call_method0("close");
                let (size, mode, pixels) = observed?;
                closed?;
                assert_eq!(size, (100, 100));
                assert_eq!(mode, "RGBA");
                assert_eq!(pixels, [0, 180, 90, 255].repeat(100 * 100));
                Ok(())
            })
            .unwrap();
            let retained = (rows[0].clone(), std::fs::read(path).unwrap());
            if case == "clean" {
                clean_media = Some(retained);
            } else if case == "repeat" {
                assert_eq!(Some(retained), clean_media);
            }
        }
        drop(catalog);
    }
    assert_eq!(std::fs::read(good).unwrap(), good_bytes);
    assert_eq!(std::fs::read(blank).unwrap(), blank_bytes);
    real_artwork(
        project,
        &scratch.0,
        &executable,
        runtime,
        &environment,
        &log,
        &original["artwork"],
    );
    let refused_data = scratch.0.join("invalid-threshold-must-not-create");
    let argv = [
        executable.as_os_str().to_owned(),
        OsString::from("add-media"),
        "--phash-threshold".into(),
        "999".into(),
        "--data-dir".into(),
        refused_data.as_os_str().to_owned(),
    ];
    let refused = runtime
        .block_on(tooling_process::capture(
            &scratch.0,
            &argv,
            &environment,
            Duration::from_secs(5),
            Duration::from_secs(3),
            &log,
        ))
        .unwrap();
    assert_eq!(refused.code, 2, "{}", refused.stderr);
    assert!(refused.stdout.is_empty());
    assert!(refused.stderr.contains("out of range"));
    assert!(!refused_data.exists());
    assert!(
        std::fs::read_dir(scratch.0.join("logs/tmp/tooling"))
            .unwrap()
            .next()
            .is_none()
    );
    let mut command_logs = Vec::new();
    for entry in std::fs::read_dir(scratch.0.join("logs")).unwrap() {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("add-media_")
        {
            command_logs.push(std::fs::read_to_string(path).unwrap());
        }
    }
    assert_eq!(command_logs.len(), 9);
    for (code, count) in [(4, 3), (3, 1), (0, 4), (2, 1)] {
        assert_eq!(
            command_logs
                .iter()
                .filter(|text| text.contains(&format!("exit={code} duration_ms=")))
                .count(),
            count
        );
    }
    local_ingest_native::check(project, &scratch.0, original, &log);
    log.finish(0);
    drop(log);
}

fn real_artwork(
    project: &Path,
    root: &Path,
    executable: &Path,
    runtime: &tokio::runtime::Runtime,
    environment: &BTreeMap<String, String>,
    log: &_native::logging::RunLog,
    original: &Value,
) {
    use pyo3::prelude::*;
    use pyo3::types::PyBytes;
    use std::ffi::OsString;
    use std::time::Duration;

    let source = project.join("assets/numera-emoji-mapper-logo.png");
    _native::tooling_environment::reject_links(&source, "artwork input cannot follow links")
        .unwrap();
    let input = std::fs::read(&source).unwrap();
    let input_hash = Sha256::digest(&input)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        input_hash,
        "6f85376de3893dfa1e78045ce264103b0055acec400dd8a35222405d5c4f6951"
    );
    assert_eq!(original["input_sha256"], input_hash);
    assert_eq!(original["size"], json!([100, 100]));
    assert_eq!(original["mode"], "RGBA");
    assert!(original["phash"].is_u64());
    let bytes = original["media_bytes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|byte| u8::try_from(byte.as_u64().unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert!(!bytes.is_empty());
    let copied = root.join("assets/numera-emoji-mapper-logo.png");
    std::fs::write(&copied, &input).unwrap();
    let clean = root.join("real-artwork/data");
    let unknown = root.join("unknown-artwork/data");
    std::fs::create_dir_all(&unknown).unwrap();
    let rival = unknown.join("rival.png");
    let invalid = b"undecodable retained catalog media";
    std::fs::write(&rival, invalid).unwrap();
    let mut catalog = Catalog::open(&unknown.join("catalog.db"), "fixture", root).unwrap();
    assert!(catalog.insert(&json!({
        "content_key":original["content_key"], "fmt":"static", "file_path":rival,
        "phash":original["phash"], "emojis":["⭐"], "keywords":["original"], "sources":[]
    }), "fixture").unwrap());
    let key = original["content_key"].as_str().unwrap();
    catalog.record_seen("prior-unique", key).unwrap();
    let before = catalog.all(None).unwrap();
    drop(catalog);
    let mut clean_row = None;
    for (case, data, code, summary) in [
        (
            "real-first",
            &clean,
            0,
            "Done. new=1 dedup=0 failed=0\n  catalog static: 1 total (1 pending upload)\n",
        ),
        (
            "real-repeat",
            &clean,
            0,
            "Done. new=0 dedup=1 failed=0\n  catalog static: 1 total (1 pending upload)\n",
        ),
        (
            "unknown-rival",
            &unknown,
            4,
            "Done. new=0 dedup=0 failed=1\n  catalog static: 1 total (1 pending upload)\n",
        ),
    ] {
        let argv = [
            executable.as_os_str().to_owned(),
            OsString::from("add-media"),
            copied.as_os_str().to_owned(),
            "--as".into(),
            "static".into(),
            "--data-dir".into(),
            data.as_os_str().to_owned(),
            "--emoji".into(),
            "😀".into(),
            "--phash-threshold".into(),
            "-1".into(),
        ];
        let completed = runtime
            .block_on(_native::tooling_process::capture(
                root,
                &argv,
                environment,
                Duration::from_secs(20),
                Duration::from_secs(10),
                log,
            ))
            .unwrap();
        assert_eq!(completed.code, code, "{case}: {}", completed.stderr);
        assert_eq!(completed.stdout, summary, "{case}");
        let mut catalog = Catalog::open(&data.join("catalog.db"), "fixture", root).unwrap();
        let rows = catalog.all(None).unwrap();
        assert_eq!(rows.len(), 1, "{case}");
        assert_eq!(rows[0]["content_key"], original["content_key"]);
        assert_eq!(rows[0]["phash"], original["phash"]);
        if case == "unknown-rival" {
            assert_eq!(rows, before);
            assert_eq!(
                catalog
                    .dispatch(json!({"operation":"seen", "file_unique_id":"prior-unique"}))
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
            if case == "real-first" {
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
        assert_eq!(std::fs::read(&stored[0]).unwrap(), bytes, "{case}");
        Python::attach(|py| -> PyResult<()> {
            let image = py
                .import("PIL.Image")?
                .call_method1("open", (&stored[0],))?;
            let observed = (|| -> PyResult<_> {
                let pixels = image.call_method0("tobytes")?.cast_into::<PyBytes>()?;
                Ok((
                    image.getattr("size")?.extract::<(u32, u32)>()?,
                    image.getattr("mode")?.extract::<String>()?,
                    Sha256::digest(pixels.as_bytes())
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>(),
                ))
            })();
            let closed = image.call_method0("close");
            let (size, mode, pixels) = observed?;
            closed?;
            assert_eq!(size, (100, 100));
            assert_eq!(mode, "RGBA");
            assert_eq!(pixels, original["rgba_sha256"].as_str().unwrap());
            Ok(())
        })
        .unwrap();
        assert!(
            std::fs::read_dir(data.join("tmp"))
                .unwrap()
                .next()
                .is_none(),
            "{case}"
        );
        assert!(
            std::fs::read_dir(root.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none(),
            "{case}"
        );
        assert_eq!(std::fs::read(&copied).unwrap(), input);
    }
    assert_eq!(std::fs::read(source).unwrap(), input);
}
