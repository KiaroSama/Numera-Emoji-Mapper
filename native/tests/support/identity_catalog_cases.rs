//! Full catalog order and recorded identity resume survive the public migration and restore.
use super::{
    bounded_media::BoundedCodec,
    empty_codecs,
    read_only_api::{Fixture, Reply, TOKEN},
};
use _native::{
    atomic, catalog::Catalog, config::Config, media_adapter::MediaAdapter, migration_apply,
    migration_restore, migration_signature, publication::Publisher, telegram::Telegram,
};
use serde_json::{Value, json};
use std::path::Path;
use std::time::Instant;

fn resume(data: &Path, project: &Path, runtime: &tokio::runtime::Runtime, record: &Value) {
    let db = data.join("catalog.db");
    let before = migration_signature::signature(&db, &[]).unwrap();
    let stickers = (0..3)
        .rev()
        .map(|i| {
            json!({
                "file_unique_id":format!("FUID{i}"),"custom_emoji_id":format!("CID{i}"),
                "file_id":format!("must-not-download-{i}"),"is_video":true
            })
        })
        .collect::<Vec<_>>();
    let mut server = Fixture::new(vec![Reply::api(
        "getStickerSet",
        "name",
        "audit1",
        json!({"stickers":stickers}),
    )]);
    let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
    let codec = MediaAdapter {
        python: "must-not-decode-known-migration-identities".into(),
        root: project.into(),
        scratch: data.join("must-not-decode"),
    };
    let mut catalog = Catalog::open(&db, "resume fixture", project).unwrap();
    let mut actual = record.clone();
    let result = Publisher {
        tg: &tg,
        cat: &mut catalog,
        media: &codec,
        data: data.into(),
        runtime,
        config: None,
    }
    .reconcile(&mut actual, "audit");
    let observed = server.finish();
    result.unwrap();
    assert_eq!(observed.unwrap(), [format!("/bot{TOKEN}/getStickerSet")]);
    assert_eq!(actual, *record);
    assert!(!codec.scratch.exists());
    drop(catalog);
    assert_eq!(migration_signature::signature(&db, &[]).unwrap(), before);
}

pub fn check(
    root: &Path,
    config: &Config,
    codec: &BoundedCodec,
    runtime: &tokio::runtime::Runtime,
) {
    let project = &codec.native.root;
    let data = root.join("full-catalog-resume");
    std::fs::create_dir(&data).unwrap();
    let db = data.join("catalog.db");
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/video-fingerprint.json"
    ))
    .unwrap();
    let old = (0..3)
        .map(|i| format!("v:{}", format!("{i:02x}").repeat(16)))
        .collect::<Vec<_>>();
    let mut fresh = Vec::new();
    let mut originals = Vec::new();
    let mut catalog = Catalog::open(&db, "fixture", project).unwrap();
    for (i, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let source = project.join(case["input"].as_str().unwrap());
        assert_eq!(
            _native::tooling_manifest::digest(&source).unwrap(),
            case["input_sha256"]
        );
        let bytes = std::fs::read(&source).unwrap();
        let path = data.join(format!("{i:03}_video_{}.webm", &old[i][2..14]));
        std::fs::write(&path, &bytes).unwrap();
        catalog
            .insert(
                &json!({"content_key":old[i],"fmt":"video","file_path":path,"phash":i}),
                "fixture",
            )
            .unwrap();
        catalog.uploaded(&json!({"content_key":old[i],"base":"audit","set_name":"audit1","custom_emoji_id":format!("CID{i}")}), "fixture").unwrap();
        catalog.record_seen(&format!("FUID{i}"), &old[i]).unwrap();
        fresh.push(case["expected"]["content_key"].as_str().unwrap().to_owned());
        originals.push((source, path, bytes));
    }
    assert_eq!(fresh.len(), 3);
    catalog
        .order(&old.iter().rev().cloned().collect::<Vec<_>>())
        .unwrap();
    catalog.inclusion(&[old[1].clone()]).unwrap();
    drop(catalog);
    let record = json!({"name":"audit1","index":1,"fmt":"video","live":3,"keys":old.iter().rev().collect::<Vec<_>>()});
    let state_path = data.join("publish_audit.json");
    let plan_path = data.join("publish_plan_audit.json");
    let state = json!({"base":"audit","sets":[record]});
    let plan = json!({"video":old.iter().rev().collect::<Vec<_>>()});
    atomic::write_json(&state_path, &state).unwrap();
    atomic::write_json(&plan_path, &plan).unwrap();
    let before = migration_signature::signature(&db, &[]).unwrap();
    let applied = migration_apply::apply(config, &data, codec, runtime, None).unwrap();
    assert_eq!(applied["moved"], 3);
    assert_eq!(
        applied["key_map"],
        json!(
            old.iter()
                .cloned()
                .zip(fresh.iter().cloned())
                .collect::<std::collections::BTreeMap<_, _>>()
        )
    );
    let migrated: Value = serde_json::from_slice(&std::fs::read(&state_path).unwrap()).unwrap();
    assert_eq!(
        migrated["sets"][0]["keys"],
        json!(fresh.iter().rev().collect::<Vec<_>>())
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&plan_path).unwrap()).unwrap(),
        json!({"video":fresh.iter().rev().collect::<Vec<_>>() })
    );
    for (keys, pack) in [(&fresh, &migrated["sets"][0]), (&old, &record)] {
        let mut catalog = Catalog::open(&db, "observe fixture", project).unwrap();
        let items = catalog.all(None).unwrap();
        assert_eq!(
            items
                .iter()
                .map(|v| v["content_key"].as_str().unwrap())
                .collect::<Vec<_>>(),
            keys.iter().rev().map(String::as_str).collect::<Vec<_>>()
        );
        assert_eq!(
            items
                .iter()
                .map(|v| v["included"].as_bool().unwrap())
                .collect::<Vec<_>>(),
            [true, false, true]
        );
        for i in 0..3 {
            assert_eq!(
                catalog
                    .dispatch(json!({"operation":"custom_id","base":"audit","content_key":keys[i]}))
                    .unwrap(),
                format!("CID{i}")
            );
            assert_eq!(
                catalog
                    .dispatch(json!({"operation":"seen","file_unique_id":format!("FUID{i}")}))
                    .unwrap(),
                keys[i]
            );
            let path = catalog.get(&keys[i]).unwrap().unwrap()["file_path"]
                .as_str()
                .unwrap()
                .to_owned();
            assert_eq!(std::fs::read(path).unwrap(), originals[i].2);
        }
        drop(catalog);
        let connection =
            rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap();
        for (i, key) in keys.iter().enumerate() {
            let stored: String = connection
                .query_row(
                    "SELECT file_path FROM items WHERE content_key=?",
                    [key],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(stored, format!("./{i:03}_video_{}.webm", &key[2..14]));
        }
        connection.close().unwrap();
        resume(&data, project, runtime, pack);
        if keys == &fresh {
            migration_restore::restore(
                &data,
                &config.root,
                Path::new(applied["bundle"].as_str().unwrap()),
                true,
            )
            .unwrap();
            assert_eq!(migration_signature::signature(&db, &[]).unwrap(), before);
            assert_eq!(
                serde_json::from_slice::<Value>(&std::fs::read(&state_path).unwrap()).unwrap(),
                state
            );
            assert_eq!(
                serde_json::from_slice::<Value>(&std::fs::read(&plan_path).unwrap()).unwrap(),
                plan
            );
        }
    }
    for (source, path, bytes) in originals {
        assert_eq!(std::fs::read(source).unwrap(), bytes);
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    assert!(!data.join("identity-migration.journal.json").exists());
    assert!(!_native::process_owner::cleanup_pending(&data));
    empty_codecs(codec);
    assert!(Instant::now() < codec.deadline);
}
