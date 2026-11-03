//! Original catalog identity assertions use the real canonical catalog and pinned comparison evidence.
use _native::catalog::Catalog;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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

#[test]
fn original_batch_exit_codes_distinguish_success_partial_failure_and_usage() {
    use _native::collector::{EXIT_FAILED, EXIT_OK, EXIT_PARTIAL, EXIT_USAGE, ingest_exit_code};
    for (succeeded, failed, expected) in [
        (5, 0, EXIT_OK),
        (3, 2, EXIT_PARTIAL),
        (0, 4, EXIT_FAILED),
        (0, 0, EXIT_OK),
    ] {
        assert_eq!(ingest_exit_code(succeeded, failed), expected);
    }
    assert_eq!(
        [EXIT_OK, EXIT_USAGE, EXIT_PARTIAL, EXIT_FAILED],
        [0, 2, 3, 4]
    );
    assert_eq!(
        std::collections::BTreeSet::from([EXIT_OK, EXIT_USAGE, EXIT_PARTIAL, EXIT_FAILED]).len(),
        4
    );
    assert_eq!(ingest_exit_code(u64::MAX, 0), EXIT_OK);
    assert_eq!(ingest_exit_code(u64::MAX, u64::MAX), EXIT_PARTIAL);
}

#[test]
fn original_nominations_unknown_ambiguity_and_collision_keep_results_and_call_order() {
    let bytes = include_bytes!("../../tests/fixtures/backend/ingest.json");
    let digest = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        digest,
        "2ec808a2a1210be99d3d161f3a14df5c8a3ab2b0b6895571cd44ec8fec646818"
    );
    let fixture: Value = serde_json::from_slice(bytes).unwrap();
    let cases =
        _native::source_fixture::cases(&fixture, "d173116:ingest.catalog_identity", 8).unwrap();
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let root = project
        .join("logs/tmp")
        .join(format!("ingest parity {}", std::process::id()));
    std::fs::create_dir_all(root.parent().unwrap()).unwrap();
    std::fs::create_dir(&root).unwrap();
    let scratch = Scratch(root);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    for (index, case) in cases.iter().enumerate() {
        let mut catalog = Catalog::open(
            &scratch.0.join(index.to_string()).join("catalog.db"),
            "fixture UTC",
            project,
        )
        .unwrap();
        let mut names = BTreeMap::new();
        for (rival, original) in case["items"].as_array().unwrap().iter().enumerate() {
            let mut item = original.clone();
            let filename = format!("rival-{rival}");
            names.insert(
                filename.clone(),
                original["content_key"].as_str().unwrap().to_owned(),
            );
            item["file_path"] = json!(scratch.0.join(filename));
            assert!(catalog.insert(&item, "fixture UTC").unwrap());
        }
        let before = catalog.all(None).unwrap();
        let mut calls = Vec::new();
        let result = runtime.block_on(_native::ingest::identify(
            &catalog,
            &case["incoming"],
            case["threshold"].as_i64().unwrap().try_into().unwrap(),
            async |a: &Path, _b: &Path, _fmt: &str| {
                let filename = a
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or("invalid rival path")?;
                let key = names.get(filename).ok_or("unexpected rival path")?;
                calls.push(key.clone());
                match case["comparison"].get(key).ok_or("unexpected comparison")? {
                    Value::Bool(value) => Ok(Some(*value)),
                    Value::Null => Ok(None),
                    _ => Err("invalid comparison fixture".into()),
                }
            },
            async |_path: &Path, _key: &str| {
                Ok(case["collision"]
                    .as_str()
                    .ok_or("missing collision fixture")?
                    .to_owned())
            },
        ));
        if let Some(error) = case.get("error") {
            assert_eq!(
                result.unwrap_err(),
                error.as_str().unwrap(),
                "{}",
                case["name"]
            );
        } else {
            assert_eq!(
                serde_json::to_value(result.unwrap()).unwrap(),
                case["expected"],
                "{}",
                case["name"]
            );
        }
        assert_eq!(json!(calls), case["calls"], "{}", case["name"]);
        assert_eq!(
            catalog.all(None).unwrap(),
            before,
            "identity must not insert or rewrite rows"
        );
        drop(catalog);
    }
    original_threshold_bounds(&scratch.0, project, &runtime);
}

fn original_threshold_bounds(root: &Path, project: &Path, runtime: &tokio::runtime::Runtime) {
    use pyo3::prelude::*;
    use pyo3::types::PyBytes;

    let directory = root.join("thresholds");
    let mut catalog = Catalog::open(&directory.join("catalog.db"), "fixture UTC", project).unwrap();
    let incoming = json!({"content_key":"s:threshold","fmt":"static",
        "file_path":directory.join("not-opened.png"),"phash":0});
    for threshold in [-1, 0, 8, 16, -2, 17, 64, 999] {
        let before = catalog.all(None).unwrap();
        let mut compared = false;
        let mut collided = false;
        let result = runtime.block_on(_native::ingest::identify(
            &catalog,
            &incoming,
            threshold,
            async |_a: &Path, _b: &Path, _format: &str| {
                compared = true;
                Err("unexpected threshold comparison".into())
            },
            async |_path: &Path, _key: &str| {
                collided = true;
                Err("unexpected threshold collision".into())
            },
        ));
        if threshold == -1 || (0..=16).contains(&threshold) {
            assert_eq!(result.unwrap(), ("s:threshold".into(), true), "{threshold}");
        } else {
            assert!(
                result.is_err(),
                "invalid threshold {threshold} was accepted"
            );
        }
        assert!(
            !compared && !collided,
            "{threshold} invoked an identity callback"
        );
        assert_eq!(
            catalog.all(None).unwrap(),
            before,
            "{threshold} changed rows"
        );
        assert!(!directory.join("not-opened.png").exists());
    }

    Python::initialize();
    let mut hashes = Vec::new();
    for seed in [1_u64, 2] {
        let path = directory.join(format!("noise-{seed}.png"));
        let pixels = (0..64 * 64_u64)
            .map(|index| (((index * 2_654_435_761 + seed * 40_503) >> 7) & 255) as u8)
            .collect::<Vec<_>>();
        Python::attach(|py| -> PyResult<()> {
            let module = py.import("PIL.Image")?;
            assert_eq!(
                module.getattr("__version__")?.extract::<String>()?,
                "12.3.0"
            );
            let gray =
                module.call_method1("frombytes", ("L", (64, 64), PyBytes::new(py, &pixels)))?;
            let saved = gray.call_method1("convert", ("RGBA",)).and_then(|rgba| {
                let saved = rgba.call_method1("save", (&path, "PNG"));
                let closed = rgba.call_method0("close");
                saved?;
                closed?;
                Ok(())
            });
            let closed = gray.call_method0("close");
            saved?;
            closed?;
            Ok(())
        })
        .unwrap();
        let before = std::fs::read(&path).unwrap();
        let fingerprint = _native::codec_static_identity::fingerprint(&path).unwrap();
        let phash = fingerprint["phash"]
            .as_u64()
            .expect("original noise must have a dHash");
        hashes.push(phash);
        let key = if seed == 1 { "s:noise-a" } else { "s:noise-b" };
        let result = runtime.block_on(_native::ingest::add_verified(
            &mut catalog,
            json!({"content_key":key,"fmt":"static","file_path":path,"phash":phash}),
            "fixture UTC",
            16,
            async |_a: &Path, _b: &Path, _format: &str| {
                panic!("unrelated original noise was nominated as a near duplicate")
            },
            async |_path: &Path, _key: &str| panic!("unrelated original noise collided"),
        ));
        assert_eq!(result.unwrap(), (key.into(), true));
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
    let distance = (hashes[0] ^ hashes[1]).count_ones();
    assert!(
        distance > 16 && distance <= 64,
        "original noise distance: {distance}"
    );
    let rows = catalog.all(None).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["content_key"], "s:noise-a");
    assert_eq!(rows[1]["content_key"], "s:noise-b");
    drop(catalog);
}
