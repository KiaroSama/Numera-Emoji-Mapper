//! Real native codec evidence crosses the existing identity journal, provenance and rollback seams.
use _native::{
    atomic, config::Config, identity_invariants, identity_survey,
    media_native_adapter::NativeMediaAdapter, migration_apply, migration_restore,
    migration_signature,
};
use rusqlite::{Connection, OpenFlags, params};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[path = "support/bounded_media.rs"]
mod bounded_media;
use bounded_media::BoundedCodec;

#[path = "support/identity_catalog_cases.rs"]
mod identity_catalog_cases;
#[path = "support/identity_provenance_cases.rs"]
mod identity_provenance_cases;
#[allow(dead_code)] // This cohort needs the scripted read, not download replies.
#[path = "support/read_only_api.rs"]
mod read_only_api;

#[path = "support/identity_command_cases.rs"]
mod identity_command_cases;
#[path = "support/retained_cli.rs"]
mod retained_cli;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

const OLD: &str = "v:0123456789abcdef";
const SCHEMA: &str = "PRAGMA journal_mode=WAL;
    CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT,format TEXT,phash INTEGER);
    CREATE TABLE publications(base TEXT,content_key TEXT,set_name TEXT,custom_emoji_id TEXT);
    CREATE TABLE seen_files(file_unique_id TEXT PRIMARY KEY,content_key TEXT);";

struct Scratch(PathBuf);
impl Scratch {
    fn clean(self, media: &BoundedCodec) {
        empty_codecs(media);
        assert!(!_native::process_owner::cleanup_pending(&self.0));
        std::fs::remove_dir_all(&self.0).expect("clean verified native identity fixture");
    }
}
fn database(path: &Path) -> Connection {
    let db = Connection::open(path).unwrap();
    db.execute_batch(SCHEMA).unwrap();
    db
}
fn item(db: &Connection, key: &str, path: &str, format: &str, hash: Option<u64>) {
    db.execute(
        "INSERT INTO items VALUES(?,?,?,?)",
        params![key, path, format, hash.map(|hash| hash as i64)],
    )
    .unwrap();
}
fn identifiers(db: &Connection, key: &str, fuid: Option<&str>, cid: Option<&str>) {
    if let Some(fuid) = fuid {
        db.execute("INSERT INTO seen_files VALUES(?,?)", params![fuid, key])
            .unwrap();
    }
    if let Some(cid) = cid {
        db.execute(
            "INSERT INTO publications VALUES('fixture',?,'fixture1',?)",
            params![key, cid],
        )
        .unwrap();
    }
}
fn rows(path: &Path) -> Value {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut query = db
        .prepare("SELECT content_key,file_path,format,phash FROM items ORDER BY content_key")
        .unwrap();
    json!(
        query
            .query_map([], |row| Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<i64>>(3)?
            )))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    )
}
fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn empty_codecs(media: &BoundedCodec) {
    assert_eq!(std::fs::read_dir(&media.native.scratch).unwrap().count(), 0);
}
fn recovery_outputs(data: &Path) -> Vec<PathBuf> {
    let mut paths = std::fs::read_dir(data)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("catalog.before-video-identity-")
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

#[test]
fn native_identity_maintenance_preserves_every_reference_and_verified_rollback() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = _native::tooling_environment::physical_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
    )
    .unwrap();
    let parent = _native::tooling_environment::confined_directory(&root, "logs/tmp", true).unwrap();
    let path = parent.join(format!("native identity consumers {}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    let scratch = Scratch(_native::tooling_environment::physical_root(&path).unwrap());
    let media = BoundedCodec {
        native: NativeMediaAdapter {
            executable: PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji")),
            root: root.clone(),
            scratch: scratch.0.join("envelopes"),
        },
        deadline: Instant::now() + Duration::from_secs(60),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let config = Config::load_with_environment(
        scratch.0.clone(),
        &BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]),
    )
    .unwrap();
    let video: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/video-fingerprint.json"
    ))
    .unwrap();
    let still: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    assert_eq!(video["source_revision"], still["source_revision"]);
    assert_eq!(
        video["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let clip = &video["cases"][2];
    let image = &still["cases"][3];
    let original_video = root.join(clip["input"].as_str().unwrap());
    let original_static = root.join(image["input"].as_str().unwrap());
    for (path, expected) in [(&original_video, clip), (&original_static, image)] {
        assert_eq!(
            _native::tooling_manifest::digest(path).unwrap(),
            expected["input_sha256"]
        );
    }
    let video_bytes = std::fs::read(&original_video).unwrap();
    let static_bytes = std::fs::read(&original_static).unwrap();
    let key = clip["expected"]["content_key"].as_str().unwrap();
    let hash = clip["expected"]["phash"].as_u64().unwrap();
    let static_key = image["expected"]["content_key"].as_str().unwrap();
    let static_hash = image["expected"]["phash"].as_u64().unwrap();
    let data = scratch.0.join("collection");
    std::fs::create_dir(&data).unwrap();
    let source = data.join("007_video_0123456789ab.webm");
    let destination = data.join(format!("007_video_{}.webm", &key[2..14]));
    let static_path = data.join("still.png");
    std::fs::write(&source, &video_bytes).unwrap();
    std::fs::write(&static_path, &static_bytes).unwrap();
    let catalog = data.join("catalog.db");
    let writer = database(&catalog);
    item(
        &writer,
        OLD,
        "./007_video_0123456789ab.webm",
        "video",
        Some(0),
    );
    item(
        &writer,
        static_key,
        "./still.png",
        "static",
        Some(static_hash),
    );
    identifiers(&writer, OLD, Some("fixture-fuid"), Some("111111111"));
    let documents = [
        (
            "publish_fixture.json",
            json!({"sets":[{"keys":[OLD,static_key]}],"skipped":[OLD],"in_flight":{"key":OLD}}),
            json!({"sets":[{"keys":[key,static_key]}],"skipped":[key],"in_flight":{"key":key}}),
        ),
        (
            "publish_plan_fixture.json",
            json!({"mixed":[OLD,"v:obsolete"]}),
            json!({"mixed":[key,"v:obsolete"]}),
        ),
        (
            "publish_plan_beta.json",
            json!({"mixed":[OLD]}),
            json!({"mixed":[key]}),
        ),
        (
            "pack_plan.json",
            json!({"version":1,"moves":[{"key":OLD}],"held":[{"key":OLD}],
                "targets":[[OLD,2]],"known":[OLD,static_key],"excluded":[OLD],"note":OLD}),
            json!({"version":1,"moves":[{"key":key}],"held":[{"key":key}],
                "targets":[[key,2]],"known":[key,static_key],"excluded":[key],"note":OLD}),
        ),
    ];
    for (name, before, _) in &documents {
        atomic::write_json(&data.join(name), before).unwrap();
    }
    let original_rows = rows(&catalog);
    let signature = migration_signature::signature(&catalog, &[]).unwrap();
    assert!(data.join("catalog.db-wal").metadata().unwrap().len() > 0);
    let survey = identity_survey::survey(&data, &config.root, &media, &runtime).unwrap();
    assert!(survey.complete() && survey.pending());
    assert_eq!(survey.checked, 1);
    assert_eq!(survey.unchanged, 0);
    assert_eq!(
        survey.changed,
        vec![(OLD.into(), key.into(), "007_video_0123456789ab.webm".into())]
    );
    assert!(survey.hashes.is_empty() && survey.collisions.is_empty());
    assert_eq!(survey.phashes, BTreeMap::from([(key.into(), Some(hash))]));
    assert_eq!(
        migration_signature::signature(&catalog, &[]).unwrap(),
        signature
    );
    assert!(migration_apply::journal(&data).unwrap().is_none());
    let family = _native::locks::Lease::acquire(
        &_native::locks::pack_family_path(&config.root, "fixture"),
        "fixture UTC",
    )
    .unwrap();
    let held = migration_apply::apply(&config, &data, &media, &runtime, None).unwrap_err();
    assert!(held.contains("OS lock acquisition failed"), "{held}");
    assert_eq!(
        migration_signature::signature(&catalog, &[]).unwrap(),
        signature
    );
    assert!(!data.join("identity-migration.journal.json").exists());
    for (name, before, _) in &documents {
        assert_eq!(read_json(&data.join(name)), *before);
    }
    assert_eq!(std::fs::read(&source).unwrap(), video_bytes);
    drop(family);
    let result = migration_apply::apply(&config, &data, &media, &runtime, None).unwrap();
    assert_eq!(result["moved"], 1);
    assert_eq!(result["key_map"], json!({OLD:key}));
    assert_eq!(
        result["renamed"],
        json!([destination.file_name().unwrap().to_str().unwrap()])
    );
    assert!(!source.exists());
    assert_eq!(std::fs::read(&destination).unwrap(), video_bytes);
    assert_eq!(std::fs::read(&static_path).unwrap(), static_bytes);
    assert_eq!(
        rows(&catalog),
        json!([
            [static_key, "./still.png", "static", static_hash as i64],
            [
                key,
                format!("./{}", destination.file_name().unwrap().to_str().unwrap()),
                "video",
                hash as i64
            ]
        ])
    );
    for (name, _, after) in &documents {
        assert_eq!(read_json(&data.join(name)), *after);
    }
    assert_eq!(
        writer
            .query_row(
                "SELECT content_key,custom_emoji_id FROM publications",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap(),
        (key.into(), "111111111".into())
    );
    assert_eq!(
        writer
            .query_row(
                "SELECT content_key,file_unique_id FROM seen_files",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap(),
        (key.into(), "fixture-fuid".into())
    );
    let manifest = PathBuf::from(result["bundle"].as_str().unwrap());
    let backup = PathBuf::from(result["backup"].as_str().unwrap());
    let bundle_bytes = std::fs::read(&manifest).unwrap();
    let bundle = read_json(&manifest);
    assert_eq!(bundle["version"], 2);
    assert_eq!(bundle["stage"], "verified");
    assert_eq!(bundle["before_signature"], signature);
    assert_eq!(rows(&backup), original_rows);
    assert_eq!(
        migration_signature::signature(&backup, &[]).unwrap(),
        signature
    );
    let applied_signature = migration_signature::signature(&catalog, &[]).unwrap();
    let clean = identity_survey::survey(&data, &config.root, &media, &runtime).unwrap();
    assert!(clean.complete() && !clean.pending());
    assert_eq!(clean.checked, 1);
    assert_eq!(clean.unchanged, 1);
    writer
        .execute("UPDATE items SET phash=0 WHERE content_key=?", [key])
        .unwrap();
    let hash_only = identity_survey::survey(&data, &config.root, &media, &runtime).unwrap();
    assert!(hash_only.complete() && hash_only.pending());
    assert!(hash_only.changed.is_empty() && hash_only.collisions.is_empty());
    assert_eq!(hash_only.hashes, vec![(key.into(), Some(0), Some(hash))]);
    let corrected = migration_apply::apply(&config, &data, &media, &runtime, None).unwrap();
    assert_eq!(corrected["moved"], 0);
    assert_eq!(corrected["key_map"], json!({}));
    assert_eq!(corrected["state"], json!([]));
    assert_eq!(corrected["renamed"], json!([]));
    assert_eq!(
        migration_signature::signature(&catalog, &[]).unwrap(),
        applied_signature
    );
    assert_eq!(std::fs::read(&destination).unwrap(), video_bytes);
    assert_eq!(std::fs::read(&manifest).unwrap(), bundle_bytes);
    assert_ne!(corrected["backup"], result["backup"]);
    assert_ne!(corrected["bundle"], result["bundle"]);
    assert_eq!(
        migration_apply::apply(&config, &data, &media, &runtime, None).unwrap(),
        json!({"noop":true,"moved":0,"state":[],"renamed":[]})
    );
    for (name, _, after) in &documents {
        assert_eq!(read_json(&data.join(name)), *after);
    }
    assert_eq!(
        migration_signature::signature(&catalog, &[]).unwrap(),
        applied_signature
    );
    assert!(
        identity_invariants::issues(&data, &config.root, &media, &runtime)
            .unwrap()
            .is_empty()
    );
    std::fs::write(&static_path, b"not an image").unwrap();
    let incomplete = identity_invariants::issues(&data, &config.root, &media, &runtime).unwrap();
    assert_eq!(incomplete.len(), 1);
    assert!(incomplete[0].starts_with(&format!("non-video identity undecodable: {static_key}:")));
    assert_eq!(std::fs::read(&static_path).unwrap(), b"not an image");
    assert_eq!(
        migration_signature::signature(&catalog, &[]).unwrap(),
        applied_signature
    );
    std::fs::write(&static_path, &static_bytes).unwrap();
    migration_restore::restore(&data, &config.root, &manifest, false).unwrap();
    assert_eq!(
        migration_signature::signature(&catalog, &[]).unwrap(),
        applied_signature
    );
    assert_eq!(std::fs::read(&manifest).unwrap(), bundle_bytes);
    assert!(!source.exists());
    assert_eq!(std::fs::read(&destination).unwrap(), video_bytes);
    migration_restore::restore(&data, &config.root, &manifest, true).unwrap();
    assert_eq!(
        migration_signature::signature(&catalog, &[]).unwrap(),
        signature
    );
    assert_eq!(rows(&catalog), original_rows);
    for (name, before, _) in &documents {
        assert_eq!(read_json(&data.join(name)), *before);
    }
    assert!(!destination.exists());
    assert_eq!(std::fs::read(&source).unwrap(), video_bytes);
    assert_eq!(std::fs::read(&static_path).unwrap(), static_bytes);
    assert!(!data.join("identity-migration.journal.json").exists());
    empty_codecs(&media);
    drop(writer);

    for case in ["missing", "undecodable", "collision"] {
        let refused = scratch.0.join(case);
        std::fs::create_dir(&refused).unwrap();
        let database_path = refused.join("catalog.db");
        let db = database(&database_path);
        let input = refused.join("source.webm");
        item(&db, OLD, "./source.webm", "video", Some(0));
        if case != "missing" {
            let bytes: &[u8] = if case == "undecodable" {
                b"not a video"
            } else {
                &video_bytes
            };
            std::fs::write(&input, bytes).unwrap();
        }
        if case == "collision" {
            item(&db, "v:second", "./source.webm", "video", Some(0));
        }
        let state = json!({"sets":[{"keys":[OLD]}]});
        let state_path = refused.join("publish_fixture.json");
        atomic::write_json(&state_path, &state).unwrap();
        let state_bytes = std::fs::read(&state_path).unwrap();
        let before = migration_signature::signature(&database_path, &[]).unwrap();
        let survey = identity_survey::survey(&refused, &config.root, &media, &runtime).unwrap();
        assert_eq!(survey.checked, if case == "collision" { 2 } else { 1 });
        assert_eq!(survey.unchanged, 0);
        match case {
            "missing" => {
                assert!(!survey.complete());
                assert_eq!(survey.missing.len(), 1);
                assert!(survey.undecodable.is_empty());
            }
            "undecodable" => {
                assert!(!survey.complete());
                assert!(survey.missing.is_empty());
                assert_eq!(survey.undecodable.len(), 1);
            }
            "collision" => {
                assert!(survey.complete());
                assert_eq!(survey.collisions.len(), 1);
                let mut colliding = survey.collisions[key].clone();
                colliding.sort();
                assert_eq!(colliding, [OLD, "v:second"]);
            }
            _ => unreachable!(),
        }
        assert_eq!(
            migration_apply::apply(&config, &refused, &media, &runtime, None).unwrap_err(),
            "migration survey incomplete or colliding; no writes"
        );
        assert_eq!(
            migration_signature::signature(&database_path, &[]).unwrap(),
            before
        );
        assert_eq!(std::fs::read(&state_path).unwrap(), state_bytes);
        assert!(!refused.join("identity-migration.journal.json").exists());
        assert_eq!(
            std::fs::read_dir(&refused)
                .unwrap()
                .map(|entry| entry.unwrap())
                .filter(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("catalog.before-"))
                .count(),
            0
        );
        match case {
            "missing" => assert!(!input.exists()),
            "undecodable" => assert_eq!(std::fs::read(&input).unwrap(), b"not a video"),
            _ => assert_eq!(std::fs::read(&input).unwrap(), video_bytes),
        }
        empty_codecs(&media);
        drop(db);
    }

    identity_provenance_cases::check(
        &scratch.0,
        &config,
        &media,
        &runtime,
        &video_bytes,
        key,
        hash,
    );
    identity_catalog_cases::check(&scratch.0, &config, &media, &runtime);
    assert_eq!(std::fs::read(original_video).unwrap(), video_bytes);
    assert_eq!(std::fs::read(original_static).unwrap(), static_bytes);
    assert!(
        Instant::now() < media.deadline,
        "native identity fixture exceeded60s; evidence retained"
    );
    scratch.clean(&media);
}

#[test]
fn original_identity_report_and_incomplete_apply_preserve_storage_through_actual_commands() {
    let project = _native::tooling_environment::physical_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
    )
    .unwrap();
    let root = tooling_fixture::Root::new("identity report commands");
    let (executable, environment) = retained_cli::install(&project, &root.0);
    let log = _native::logging::RunLog::open(&root.0, "identity-command-fixture", Vec::new());
    identity_command_cases::check(&root.0, &executable, &environment, &log);
    log.finish(0);
    drop(log);
    root.clean().unwrap();
}
