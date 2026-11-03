//! Original ZIP export outcomes through real roster freshness, SQLite and the public exporter.
use _native::{config::Config, pack_export};
use pyo3::prelude::*;
use rusqlite::{Connection, params};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

fn png(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    Python::initialize();
    Python::attach(|py| {
        let image = py
            .import("PIL.Image")
            .unwrap()
            .call_method1("new", ("RGBA", (100, 100), (0, 0, 0, 0)))
            .unwrap();
        let result = image
            .call_method1("paste", ((200, 30, 30, 255), (20, 20, 80, 80)))
            .and_then(|_| image.call_method1("save", (path, "PNG")));
        let closed = image.call_method0("close");
        result.unwrap();
        closed.unwrap();
    });
}
fn fixture(root: &Path) -> (Config, Vec<PathBuf>) {
    let packs = root.join("packs");
    let data = root.join("collection");
    std::fs::create_dir_all(&packs).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    let logo = root.join("logo.png");
    let a = data.join("media/a.webp");
    let b = data.join("media/b.webm");
    png(&logo);
    png(&a);
    std::fs::write(&b, b"\x1a\x45\xdf\xa3 a video").unwrap();
    let ka = format!("s:{}", "a".repeat(32));
    let kb = format!("v:{}", "b".repeat(32));
    let db = Connection::open(data.join("catalog.db")).unwrap();
    db.execute_batch(
        "CREATE TABLE items(content_key TEXT,file_path TEXT,format TEXT,keywords TEXT);
        CREATE TABLE publications(base TEXT,content_key TEXT,set_name TEXT,custom_emoji_id TEXT);",
    )
    .unwrap();
    for (key, path, format, keyword, id) in [
        (&ka, &a, "static", "alpha", "902"),
        (&kb, &b, "video", "beta", "901"),
    ] {
        db.execute(
            "INSERT INTO items VALUES(?,?,?,?)",
            params![
                key,
                path.to_str().unwrap(),
                format,
                json!([keyword]).to_string()
            ],
        )
        .unwrap();
        db.execute(
            "INSERT INTO publications VALUES(?,?,?,?)",
            params!["pk", key, "mine2_by_bot", id],
        )
        .unwrap();
    }
    drop(db);
    let document = json!({"set_name":"mine2_by_bot","title":"Mine 2","family":"general","pack_index":2,"emoji":[
        {"slot":1,"role":"brand-logo","custom_emoji_id":"900","history_key":"logo:mine2_by_bot"},
        {"slot":2,"role":"emoji","custom_emoji_id":"901","history_key":format!("ck:{kb}")},
        {"slot":3,"role":"emoji","custom_emoji_id":"902","history_key":format!("ck:{ka}")}]});
    _native::atomic::write_json(&packs.join("mine2_by_bot.json"), &document).unwrap();
    let config = Config::load_with_environment(
        root.into(),
        &BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("COLLECTION_PACK_BASE".into(), "pk".into()),
            ("BRAND_LOGO_PATH".into(), "logo.png".into()),
        ]),
    )
    .unwrap();
    _native::atomic::write_json(
        &packs.join("index.json"),
        &json!({"inputs":_native::roster_cli::fingerprint(&config).unwrap()}),
    )
    .unwrap();
    (config, vec![logo, a, b])
}
fn read_zip(path: &Path) -> Vec<(String, Vec<u8>)> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let mut member = archive.by_index(index).unwrap();
        assert!(member.size() <= 16384);
        let name = member.name().to_owned();
        let mut bytes = Vec::new();
        member.read_to_end(&mut bytes).unwrap();
        entries.push((name, bytes));
    }
    entries
}

#[test]
fn original_export_keeps_slot_order_manifest_and_source_bytes_for_number_and_name() {
    let root = Root::new("pack export café");
    let (config, files) = fixture(&root.0);
    let originals = files
        .iter()
        .map(|path| std::fs::read(path).unwrap())
        .collect::<Vec<_>>();
    let source_rows = _native::archive_catalog::read(&config).unwrap();
    let roster = std::fs::read(root.0.join("packs/mine2_by_bot.json")).unwrap();
    let index = std::fs::read(root.0.join("packs/index.json")).unwrap();
    for (selector, filename) in [("2", "number.zip"), ("mine2_by_bot", "name.zip")] {
        let output = root.0.join(filename);
        assert_eq!(pack_export::run(&config, selector, &output).unwrap(), 0);
        let entries = read_zip(&output);
        assert_eq!(
            entries
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            [
                "001_logo.png",
                "002_video_bbbbbbbbbbbb.webm",
                "003_static_aaaaaaaaaaaa.webp",
                "_manifest.md"
            ]
        );
        assert_eq!(entries[0].1, originals[0]);
        assert_eq!(entries[1].1, originals[2]);
        assert_eq!(entries[2].1, originals[1]);
        let manifest = std::str::from_utf8(&entries[3].1).unwrap();
        assert!(manifest.contains("beta") && manifest.contains("alpha"));
        assert!(manifest.find("beta").unwrap() < manifest.find("alpha").unwrap());
        assert!(manifest.contains("901") && manifest.contains("902"));
        assert!(!manifest.contains("900") && !manifest.contains("001_logo.png"));
        for (path, bytes) in files.iter().zip(&originals) {
            assert_eq!(std::fs::read(path).unwrap(), *bytes);
        }
        assert_eq!(
            _native::archive_catalog::read(&config).unwrap(),
            source_rows
        );
        assert_eq!(
            std::fs::read(root.0.join("packs/mine2_by_bot.json")).unwrap(),
            roster
        );
        assert_eq!(
            std::fs::read(root.0.join("packs/index.json")).unwrap(),
            index
        );
        assert!(!std::fs::read_dir(&root.0).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));
    }
    root.clean().unwrap();
}

#[test]
fn original_stale_or_missing_slot_refuses_without_partial_zip_or_source_changes() {
    let root = Root::new("export refusals café");
    for label in [
        "stale",
        "missing",
        "unknown",
        "source-output",
        "foreign-stage",
    ] {
        let directory = root.0.join(label);
        let (config, files) = fixture(&directory);
        if label == "stale" {
            std::fs::remove_file(directory.join("packs/index.json")).unwrap();
        }
        if label == "missing" {
            std::fs::remove_file(&files[2]).unwrap();
        }
        let originals = files
            .iter()
            .filter(|path| path.is_file())
            .map(|path| (path, std::fs::read(path).unwrap()))
            .collect::<Vec<_>>();
        let output = if label == "source-output" {
            files[1].clone()
        } else {
            directory.join("out.zip")
        };
        let foreign = directory.join("out.zip.tmp");
        std::fs::write(&foreign, b"caller staging bytes").unwrap();
        let selector = if label == "unknown" { "9" } else { "2" };
        let result = pack_export::run(&config, selector, &output);
        match label {
            "stale" => assert_eq!(result.unwrap(), 3),
            "missing" | "unknown" => assert_eq!(result.unwrap(), 1),
            "source-output" => assert!(result.unwrap_err().contains("replace a source file")),
            "foreign-stage" => {
                assert_eq!(result.unwrap(), 0);
                assert_eq!(read_zip(&output).len(), 4);
            }
            _ => unreachable!(),
        }
        if ["stale", "missing", "unknown"].contains(&label) {
            assert!(!output.exists());
        }
        assert_eq!(std::fs::read(&foreign).unwrap(), b"caller staging bytes");
        for (path, bytes) in originals {
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
        assert_eq!(
            std::fs::read_dir(&directory)
                .unwrap()
                .filter(|entry| entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".tmp"))
                .count(),
            1
        );
    }
    root.clean().unwrap();
}

#[test]
fn original_export_sources_refuse_before_runtime_or_workspace_access() {
    use _native::source_git_inventory::Source;
    let absent = PathBuf::from("must-not-be-inspected");
    let source = Source {
        baseline_revision: "0".repeat(40),
        path: "unverified.py".into(),
        sha256: "0".repeat(64),
        text: "raise AssertionError('must not execute')".into(),
    };
    assert!(
        _native::source_export_tests::observe(&absent, &absent, &[&source; 9])
            .unwrap_err()
            .contains("source identity drift")
    );
    assert!(_native::source_export_tests::source_identity(&[]).is_err());
}
