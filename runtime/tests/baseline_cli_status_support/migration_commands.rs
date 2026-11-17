use crate::{
    identity::fingerprint,
    support::*,
    video_collection::{BLUE, RED, encode, media},
};
use numera_runtime as native;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
fn db(f: &Fixture) -> Connection {
    std::fs::create_dir_all(f.data()).unwrap();
    let db = Connection::open(f.data().join("catalog.db")).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT,format TEXT,phash INTEGER);CREATE TABLE publications(content_key TEXT,custom_emoji_id TEXT);CREATE TABLE seen_files(content_key TEXT,file_unique_id TEXT);").unwrap();
    db
}
fn command(f: &Fixture, args: &[&str]) -> Run {
    let mut cmd = vec!["identity-repair"];
    cmd.extend_from_slice(args);
    f.run(&cmd, &[])
}
fn signature(f: &Fixture) -> String {
    native::migration_signature::signature(&f.data().join("catalog.db"), &[]).unwrap()
}
// tests.test_native_identity_cli.NativeIdentityCLI.test_report_preserves_database_and_refuses_missing_media
#[test]
fn identity_report_preserves_database_and_refuses_missing_media_and_corrupt_evidence() {
    let f = Fixture::new();
    let source = f.data().join("source.png");
    write(
        &source,
        std::fs::read(project().join("assets/numera-emoji-mapper-logo.png")).unwrap(),
    );
    let fp = fingerprint(&source);
    let key = fp["content_key"].as_str().unwrap();
    let db = db(&f);
    db.execute(
        "INSERT INTO items VALUES(?,'./source.png','static',?)",
        params![key, fp["phash"].as_u64().map(|h| h as i64)],
    )
    .unwrap();
    drop(db);
    let plan = f.data().join("publish_plan_fixture.json");
    document(&plan, &json!({"static":[key,"s:obsolete"]}));
    let database = f.data().join("catalog.db");
    let before = std::fs::read(&database).unwrap();
    {
        let _lease = native::ownership::Ownership::acquire(
            &f.data(),
            native::ownership::Mode::Maintenance,
            UTC,
        )
        .unwrap();
        command(&f, &["report"]).code(4);
        assert!(!f.data().join("tmp/identity-codec").exists());
    }
    let r = command(&f, &["report"]);
    r.code(0);
    assert!(r.out.contains("Clean: identities, media"));
    assert!(r.out.contains("s:obsolete"));
    let expected = format!(
        "catalog: {}\n\nVIDEO IDENTITY: 0 row(s) checked\n  keys that would change : 0\n  hash-only corrections  : 0\n  already correct        : 0\n  media missing on disk  : 0\n  undecodable            : 0\n  collisions             : 0\n\n  state/plan files naming unknown keys: 1\n    publish_plan_fixture.json: 1 stale key(s), e.g. s:obsolete\n\nSUSPECT MAPPINGS: 0 (heuristic only)\n  Rows with a recorded Telegram id AND a look-alike of the same\n  format. The old recovery rule compared grayscale structure alone,\n  so these are the rows it COULD have confused. This is NOT a\n  validation of historical id mappings, and zero here would not\n  prove none was ever wrong: the handle a past download was\n  attributed by is not stored, so it cannot be re-checked.\n\nClean: identities, media and all required references agree.\nObsolete frozen-plan/skipped-history entries are retained; they are not live references.\n",
        database.display()
    );
    assert_eq!(r.out, expected);
    assert_eq!(std::fs::read(&database).unwrap(), before);
    std::fs::remove_file(&plan).unwrap();
    let preview = command(&f, &["migrate-video-keys"]);
    preview.code(0);
    assert_eq!(
        preview.out,
        "Nothing to do: identities, hashes and required references are consistent.\n"
    );
    document(&plan, &json!({"static":[key,"s:obsolete"]}));
    let journal = f.data().join("identity-migration.journal.json");
    for corrupt in ["{truncated", "{}", "[]"] {
        write(&journal, corrupt.as_bytes());
        command(&f, &["report"]).code(4);
        assert_eq!(std::fs::read(&database).unwrap(), before);
        assert_eq!(std::fs::read_to_string(&journal).unwrap(), corrupt);
    }
    std::fs::remove_file(&journal).unwrap();
    command(&f, &["report", "--data-dir", "missing-catalog"]).code(2);
    assert!(!f.root.join("missing-catalog").exists());
    for apply in [false, true] {
        let mut args = vec!["migrate-video-keys", "--from-backup", "missing.db"];
        if apply {
            args.push("--apply");
        }
        command(&f, &args).code(2);
        assert!(!f.data().join("missing.db").exists());
    }
    let bad_plan = f.data().join("pack_plan.json");
    document(&bad_plan, &json!({"version":1,"moves":{},"held":[]}));
    command(&f, &["report"]).code(4);
    assert_eq!(std::fs::read(&database).unwrap(), before);
    std::fs::remove_file(bad_plan).unwrap();
    std::fs::remove_file(&source).unwrap();
    let r = command(&f, &["report"]);
    r.code(4);
    assert!(r.out.contains("missing catalog media"));
    assert_eq!(std::fs::read(&database).unwrap(), before);
    document(
        &f.data().join("publish_fixture.json"),
        &json!({"sets":[{"keys":["s:missing"]}]}),
    );
    command(&f, &["report"]).code(3);
    assert_eq!(std::fs::read(database).unwrap(), before);
}
fn bundles(f: &Fixture) -> Vec<PathBuf> {
    std::fs::read_dir(f.data())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(".rollback.json")
        })
        .collect()
}
fn migration(mode: &str) {
    let f = Fixture::new();
    std::fs::create_dir_all(f.data()).unwrap();
    let source = if mode == "static" {
        let p = f.data().join("s_fixture.png");
        write(
            &p,
            std::fs::read(project().join("assets/numera-emoji-mapper-logo.png")).unwrap(),
        );
        p
    } else {
        let p = f.data().join("001_video_aaaaaaaaaaaa.webm");
        if mode == "generated" {
            let generated = encode(&f.data(), "migration-clip", &[RED, BLUE, RED]);
            std::fs::rename(generated, &p).unwrap();
        } else {
            let raw = std::env::var_os("NUMERA_REAL_VIDEO_FIXTURE")
                .or_else(|| option_env!("NUMERA_REAL_VIDEO_FIXTURE").map(std::ffi::OsString::from))
                .expect(
                    "ignored operator replay requires NUMERA_REAL_VIDEO_FIXTURE at compile time",
                );
            std::fs::copy(raw, &p).unwrap();
        }
        p
    };
    let fmt = if mode == "static" { "static" } else { "video" };
    let fp = if fmt == "static" {
        fingerprint(&source)
    } else {
        runtime()
            .block_on(
                media(&f).call(json!({"operation":"fingerprint","source":source,"format":fmt})),
            )
            .unwrap()
    };
    let key = if fmt == "static" {
        fp["content_key"].as_str().unwrap().to_owned()
    } else {
        format!("v:{}", "a".repeat(32))
    };
    let phash = fp["phash"].as_u64().unwrap();
    let original = std::fs::read(&source).unwrap();
    let db = db(&f);
    db.execute(
        "INSERT INTO items VALUES(?,?,?,?)",
        params![
            key,
            format!(
                "collection/{}",
                source.file_name().unwrap().to_str().unwrap()
            ),
            fmt,
            phash as i64
        ],
    )
    .unwrap();
    db.execute("INSERT INTO publications VALUES(?,'111111111')", [&key])
        .unwrap();
    db.execute("INSERT INTO seen_files VALUES(?,'fixture-unique')", [&key])
        .unwrap();
    drop(db);
    let state = json!({"sets":[{"keys":[key]}],"skipped":[]});
    document(&f.data().join("publish_fixture.json"), &state);
    let before = std::fs::read(f.data().join("catalog.db")).unwrap();
    let r = command(&f, &["migrate-video-keys", "--apply"]);
    r.code(0);
    if mode == "static" {
        assert_eq!(std::fs::read(f.data().join("catalog.db")).unwrap(), before);
        assert!(bundles(&f).is_empty());
        assert_eq!(std::fs::read(&source).unwrap(), original);
        let family = f.data().join("publish_x.y.json");
        document(&family, &state);
        {
            let _lease = native::locks::Lease::acquire(
                &native::locks::pack_family_path(&f.root, "x.y"),
                UTC,
            )
            .unwrap();
            command(&f, &["migrate-video-keys", "--apply"]).code(4);
            assert_eq!(std::fs::read(f.data().join("catalog.db")).unwrap(), before);
        }
        std::fs::remove_file(family).unwrap();
        let frozen = f.data().join("publish_plan_fixture.json");
        write(&frozen, b"{truncated");
        command(&f, &["migrate-video-keys", "--apply"]).code(4);
        assert_eq!(std::fs::read(f.data().join("catalog.db")).unwrap(), before);
        assert!(bundles(&f).is_empty());
        assert_eq!(std::fs::read(frozen).unwrap(), b"{truncated");
        return;
    }
    let manifests = bundles(&f);
    assert_eq!(manifests.len(), 1);
    let manifest = &manifests[0];
    let doc = read_json(manifest);
    assert_eq!(doc["version"], 2);
    assert!(!f.data().join("identity-migration.journal.json").exists());
    assert_eq!(
        native::migration_bundle::current(&f.data(), &f.root, &doc).unwrap(),
        doc["database_signature"]
    );
    assert!(doc.get("phash").is_some());
    assert_eq!(doc["direction"], "forward");
    let migrated = Path::new(doc["files"][0]["destination"].as_str().unwrap());
    assert_eq!(std::fs::read(migrated).unwrap(), original);
    assert!(!source.exists());
    command(&f, &["restore", "--bundle", manifest.to_str().unwrap()]).code(3);
    assert!(!source.exists());
    command(
        &f,
        &["restore", "--bundle", manifest.to_str().unwrap(), "--apply"],
    )
    .code(0);
    assert_eq!(signature(&f), doc["before_signature"]);
    assert_eq!(read_json(&f.data().join("publish_fixture.json")), state);
    let new = doc["key_map"][&key].as_str().unwrap();
    let db = Connection::open(f.data().join("catalog.db")).unwrap();
    for table in ["items", "publications", "seen_files"] {
        db.execute(
            &format!("UPDATE {table} SET content_key=? WHERE content_key=?"),
            [new, &key],
        )
        .unwrap();
    }
    drop(db);
    let partial = std::fs::read(f.data().join("catalog.db")).unwrap();
    let backup = doc["backup"].as_str().unwrap();
    let r = command(&f, &["migrate-video-keys", "--from-backup", backup]);
    r.code(3);
    assert!(r.out.contains("recovered 1 old->new pair(s)"));
    assert_eq!(std::fs::read(f.data().join("catalog.db")).unwrap(), partial);
    assert!(!f.data().join("identity-migration.journal.json").exists());
    let r = command(
        &f,
        &["migrate-video-keys", "--apply", "--from-backup", backup],
    );
    r.code(0);
    let legacy: Value = serde_json::from_str(r.out.trim()).unwrap();
    native::migration_restore::restore(
        &f.data(),
        &f.root,
        Path::new(legacy["bundle"].as_str().unwrap()),
        true,
    )
    .unwrap();
    assert_eq!(std::fs::read(&source).unwrap(), original);
    if mode == "generated" {
        command(
            &f,
            &["restore", "--bundle", manifest.to_str().unwrap(), "--apply"],
        )
        .code(0);
        assert_eq!(signature(&f), doc["before_signature"]);
        replay(&f, &source, &original, &state, &key, phash);
    }
    let db = Connection::open(f.data().join("catalog.db")).unwrap();
    db.execute("UPDATE publications SET custom_emoji_id='222222222'", [])
        .unwrap();
    drop(db);
    let changed = std::fs::read(f.data().join("catalog.db")).unwrap();
    let r = command(
        &f,
        &["restore", "--bundle", manifest.to_str().unwrap(), "--apply"],
    );
    r.code(4);
    assert!(r.err.contains("catalog changed outside migration"));
    assert_eq!(std::fs::read(f.data().join("catalog.db")).unwrap(), changed);
    assert!(!f.data().join("identity-migration.journal.json").exists());
}
fn rewrite(f: &Fixture, map: &BTreeMap<String, String>, hashes: &BTreeMap<String, Option<u64>>) {
    let mut db = Connection::open(f.data().join("catalog.db")).unwrap();
    native::migration_database::rewrite(&mut db, map, hashes).unwrap();
}
fn replay(f: &Fixture, source: &Path, original: &[u8], state: &Value, old: &str, phash: u64) {
    let new = runtime()
        .block_on(
            media(f).call(json!({"operation":"fingerprint","source":source,"format":"video"})),
        )
        .unwrap()["content_key"]
        .as_str()
        .unwrap()
        .to_owned();
    let map = BTreeMap::from([(old.to_owned(), new.clone())]);
    let hashes = BTreeMap::from([(new.clone(), Some(phash))]);
    let journal = f.data().join("identity-migration.journal.json");
    let backup = f.data().join("replay-before.db");
    native::sqlite_snapshot::backup(&native::identity_survey::read(&f.data()).unwrap(), &backup)
        .unwrap();
    let files = native::migration_files::plan(&f.data(), &f.root, &map).unwrap();
    let after = native::state_artifacts::rewrite("publish_fixture.json", state, &map).unwrap();
    let mut doc = native::migration_bundle::make(
        &f.data(),
        &backup,
        &map,
        &hashes,
        &files,
        &json!({"publish_fixture.json":{"before":state,"after":after}}),
    )
    .unwrap();
    let manifest = backup.with_extension("rollback.json");
    doc["bundle"] = json!(manifest);
    document(&manifest, &doc);
    let before = std::fs::read(f.data().join("catalog.db")).unwrap();
    let protected = f.root.join("assets/panel.html");
    let protected_bytes = std::fs::read(&protected).unwrap();
    for field in ["phash", "bundle"] {
        let mut tampered = doc.clone();
        if field == "phash" {
            tampered["phash"][&new] = json!((phash as i64) ^ 1);
        } else {
            tampered["bundle"] = json!(protected);
        }
        document(&journal, &tampered);
        command(f, &["migrate-video-keys", "--apply"]).code(4);
        assert_eq!(
            std::fs::read(f.data().join("catalog.db")).unwrap(),
            before,
            "inconsistent intent must refuse before SQL"
        );
        assert_eq!(read_json(&journal), tampered);
        assert_eq!(std::fs::read(source).unwrap(), original);
        assert_eq!(std::fs::read(&protected).unwrap(), protected_bytes);
    }
    document(&journal, &doc);
    rewrite(f, &map, &hashes);
    let intent = &files[0];
    let destination = Path::new(intent["destination"].as_str().unwrap());
    native::archive_move::transfer(source, destination, true).unwrap();
    let expected =
        native::migration_apply::apply(&f.config(&[]), &f.data(), &media(f), &runtime(), None)
            .unwrap();
    native::migration_restore::restore(&f.data(), &f.root, &manifest, true).unwrap();
    document(&manifest, &doc);
    document(&journal, &doc);
    rewrite(f, &map, &hashes);
    native::archive_move::transfer(source, destination, true).unwrap();
    let r = command(f, &["migrate-video-keys", "--apply"]);
    r.code(0);
    assert_eq!(
        serde_json::from_str::<Value>(r.out.trim()).unwrap(),
        expected
    );
    assert!(!journal.exists());
    let mut applied = read_json(&manifest);
    native::migration_bundle::applied(&f.data(), &f.root, &applied).unwrap();
    assert_eq!(std::fs::read(destination).unwrap(), original);
    assert_eq!(read_json(&f.data().join("publish_fixture.json")), after);
    applied["direction"] = json!("restore");
    applied["stage"] = json!("restore");
    document(&journal, &applied);
    let alternate = f.data().join("other.rollback.json");
    document(&alternate, &applied);
    let before_refusal = (
        std::fs::read(f.data().join("catalog.db")).unwrap(),
        std::fs::read(&journal).unwrap(),
    );
    let r = command(
        f,
        &[
            "restore",
            "--bundle",
            alternate.to_str().unwrap(),
            "--apply",
        ],
    );
    r.code(4);
    assert!(r.err.contains("requested bundle differs"));
    assert_eq!(
        (
            std::fs::read(f.data().join("catalog.db")).unwrap(),
            std::fs::read(&journal).unwrap()
        ),
        before_refusal
    );
    native::archive_move::transfer(destination, source, intent["destination_existed"] != true)
        .unwrap();
    document(&f.data().join("publish_fixture.json"), state);
    command(
        f,
        &["restore", "--bundle", manifest.to_str().unwrap(), "--apply"],
    )
    .code(0);
    assert_eq!(signature(f), doc["before_signature"]);
    assert_eq!(std::fs::read(source).unwrap(), original);
    assert!(!journal.exists());
    let r = command(f, &["migrate-video-keys", "--apply"]);
    r.code(0);
    let out: Value = serde_json::from_str(r.out.trim()).unwrap();
    let native_manifest = Path::new(out["bundle"].as_str().unwrap());
    let mut native_doc = read_json(native_manifest);
    native::migration_restore::restore(&f.data(), &f.root, native_manifest, true).unwrap();
    native_doc["direction"] = json!("forward");
    native_doc["stage"] = json!("backup");
    document(native_manifest, &native_doc);
    document(&journal, &native_doc);
    let map = serde_json::from_value(native_doc["key_map"].clone()).unwrap();
    let signed: BTreeMap<String, Option<i64>> =
        serde_json::from_value(native_doc["phash"].clone()).unwrap();
    let hashes = signed
        .into_iter()
        .map(|(k, h)| (k, h.map(|v| v as u64)))
        .collect();
    rewrite(f, &map, &hashes);
    native::migration_apply::apply(&f.config(&[]), &f.data(), &media(f), &runtime(), None).unwrap();
    native::migration_bundle::applied(&f.data(), &f.root, &read_json(native_manifest)).unwrap();
    assert!(!journal.exists());
    native::migration_restore::restore(&f.data(), &f.root, native_manifest, true).unwrap();
    assert_eq!(std::fs::read(source).unwrap(), original);
    assert_eq!(signature(f), doc["before_signature"]);
}
// tests.test_native_migration_cli.NativeMigrationCLI.test_version_two_apply_restore_and_late_edit_refusal
#[test]
fn static_identity_apply_is_noop_and_refuses_family_or_corrupt_evidence() {
    migration("static");
}
// tests.test_native_migration_cli.NativeMigrationCLI.test_real_operator_video_copy_migrates_and_source_can_restore_bundle
#[test]
#[ignore = "private real-video replay requires NUMERA_REAL_VIDEO_FIXTURE; generated CI coverage runs separately"]
fn optional_operator_video_copy_migrates_and_library_restores_bundle() {
    migration("operator");
}
// tests.test_native_migration_cli.NativeMigrationCLI.test_generated_video_bundle_replays_between_backends
#[test]
fn generated_video_bundle_replays_between_actual_cli_and_library() {
    migration("generated");
}
