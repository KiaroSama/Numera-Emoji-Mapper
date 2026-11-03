//! Immutable provenance and legacy state repair share the real native identity fixture.
use super::{database, empty_codecs, identifiers, item, read_json, recovery_outputs};
use crate::bounded_media::BoundedCodec;
use _native::{
    atomic, config::Config, migration_apply, migration_provenance, migration_restore,
    migration_signature,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

const OLD: &str = "v:0123456789abcdef";
const OTHER: &str = "v:cccccccccccccccc";

pub fn check(
    root: &Path,
    config: &Config,
    media: &BoundedCodec,
    runtime: &tokio::runtime::Runtime,
    bytes: &[u8],
    key: &str,
    hash: u64,
) {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/video-fingerprint.json"
    ))
    .unwrap();
    let rival = &fixture["cases"][1];
    let rival_path = media.native.root.join(rival["input"].as_str().unwrap());
    assert_eq!(
        _native::tooling_manifest::digest(&rival_path).unwrap(),
        rival["input_sha256"]
    );
    let rival_bytes = std::fs::read(&rival_path).unwrap();
    let rival_key = rival["expected"]["content_key"].as_str().unwrap();
    let rival_hash = rival["expected"]["phash"].as_u64().unwrap();
    assert_ne!(rival_key, key);
    let parent = root.join("provenance");
    std::fs::create_dir(&parent).unwrap();
    for case in [
        "fuid",
        "cid",
        "both",
        "path-only",
        "equal-path",
        "same-slot",
        "conflict",
        "duplicate-current",
        "duplicate-backup",
        "undecodable",
    ] {
        let data = parent.join(case);
        std::fs::create_dir(&data).unwrap();
        let name = if case == "same-slot" {
            format!("001_video_{}.webm", &key[2..14])
        } else {
            "unrelated-new-position.webm".into()
        };
        let current_path = data.join(&name);
        let current_bytes: &[u8] = if case == "undecodable" {
            b"not a video"
        } else {
            bytes
        };
        std::fs::write(&current_path, current_bytes).unwrap();
        let old_path = if matches!(case, "path-only" | "equal-path") {
            format!("./{name}")
        } else {
            "./001_video_0123456789ab.webm".into()
        };
        let current_db = data.join("catalog.db");
        let old_db = data.join("catalog.old.db");
        let current = database(&current_db);
        let old = database(&old_db);
        item(&current, key, &format!("./{name}"), "video", Some(hash));
        item(&old, OLD, &old_path, "video", Some(0));
        let old_ids = match case {
            "fuid" | "undecodable" => (Some("shared-fuid"), None),
            "cid" => (None, Some("shared-cid")),
            "path-only" => (None, None),
            _ => (Some("shared-fuid"), Some("shared-cid")),
        };
        let current_ids = match case {
            "equal-path" | "same-slot" => (Some("other-fuid"), Some("other-cid")),
            "conflict" => (Some("shared-fuid"), Some("other-cid")),
            _ => old_ids,
        };
        identifiers(&old, OLD, old_ids.0, old_ids.1);
        identifiers(&current, key, current_ids.0, current_ids.1);
        let second_path = data.join("different-current-artwork.webm");
        if matches!(case, "both" | "conflict" | "duplicate-current") {
            std::fs::write(&second_path, &rival_bytes).unwrap();
            item(
                &current,
                rival_key,
                "./different-current-artwork.webm",
                "video",
                Some(rival_hash),
            );
            if case == "both" {
                item(&old, OTHER, "./loose-historical.webm", "video", Some(0));
                identifiers(&old, OTHER, Some("second-fuid"), Some("second-cid"));
                identifiers(&current, rival_key, Some("second-fuid"), Some("second-cid"));
            } else {
                identifiers(&current, rival_key, Some("rival-fuid"), Some("shared-cid"));
            }
        }
        if case == "duplicate-backup" {
            item(&old, OTHER, "./other-old-file.webm", "video", Some(0));
            identifiers(&old, OTHER, Some("other-old-fuid"), Some("shared-cid"));
        }
        current.close().unwrap();
        old.close().unwrap();
        let before_keys = if case == "both" {
            vec![OLD, OTHER]
        } else {
            vec![OLD]
        };
        let state = json!({"sets":[{"keys":before_keys}]});
        let plan = json!({"mixed":before_keys});
        let obsolete = json!({"mixed":["v:ghostghostghost"]});
        for (name, doc) in [
            ("publish_fixture.json", &state),
            ("publish_plan_fixture.json", &plan),
            ("publish_plan_old.json", &obsolete),
        ] {
            atomic::write_json(&data.join(name), doc).unwrap();
        }
        let state_bytes = std::fs::read(data.join("publish_fixture.json")).unwrap();
        let plan_bytes = std::fs::read(data.join("publish_plan_fixture.json")).unwrap();
        let obsolete_bytes = std::fs::read(data.join("publish_plan_old.json")).unwrap();
        let signatures =
            [&current_db, &old_db].map(|path| migration_signature::signature(path, &[]).unwrap());
        let success = matches!(case, "fuid" | "cid" | "both");
        let expected = if success {
            let mut expected = BTreeMap::from([(OLD.into(), key.into())]);
            if case == "both" {
                expected.insert(OTHER.into(), rival_key.into());
            }
            expected
        } else {
            BTreeMap::new()
        };
        assert_eq!(
            migration_provenance::recover(&data, &config.root, &old_db, media, runtime).unwrap(),
            expected,
            "{case}"
        );
        assert_eq!(
            [&current_db, &old_db].map(|path| migration_signature::signature(path, &[]).unwrap()),
            signatures,
            "read-only recovery: {case}"
        );
        if case == "fuid" {
            let insufficient = data.join("already-current-backup.db");
            let current = rusqlite::Connection::open_with_flags(
                &current_db,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .unwrap();
            _native::sqlite_snapshot::backup(&current, &insufficient).unwrap();
            current.close().unwrap();
            assert_eq!(
                migration_signature::signature(&insufficient, &[]).unwrap(),
                signatures[0]
            );
            let backup_bytes = std::fs::read(&insufficient).unwrap();
            let error = migration_apply::apply(config, &data, media, runtime, Some(&insufficient))
                .unwrap_err();
            assert!(
                error.contains("required state references have no trusted mapping"),
                "{error}"
            );
            assert_eq!(std::fs::read(&insufficient).unwrap(), backup_bytes);
            assert_eq!(
                [&current_db, &old_db]
                    .map(|path| migration_signature::signature(path, &[]).unwrap()),
                signatures
            );
            assert_eq!(
                std::fs::read(data.join("publish_fixture.json")).unwrap(),
                state_bytes
            );
            assert_eq!(
                std::fs::read(data.join("publish_plan_fixture.json")).unwrap(),
                plan_bytes
            );
            assert_eq!(std::fs::read(&current_path).unwrap(), current_bytes);
            assert!(recovery_outputs(&data).is_empty());
            assert!(!data.join("identity-migration.journal.json").exists());
            empty_codecs(media);
        }
        let result = migration_apply::apply(config, &data, media, runtime, Some(&old_db));
        if success {
            let result = result.unwrap_or_else(|error| panic!("{case}: {error}"));
            assert_eq!(result["moved"], 0);
            assert_eq!(result["key_map"], json!(expected));
            let after_keys = if case == "both" {
                vec![key, rival_key]
            } else {
                vec![key]
            };
            assert_eq!(
                read_json(&data.join("publish_fixture.json")),
                json!({"sets":[{"keys":after_keys}]})
            );
            assert_eq!(
                read_json(&data.join("publish_plan_fixture.json")),
                json!({"mixed":after_keys})
            );
            assert_eq!(
                recovery_outputs(&data).len(),
                2,
                "one new backup and bundle: {case}"
            );
            let before_repeat = recovery_outputs(&data);
            assert_eq!(
                migration_apply::apply(config, &data, media, runtime, None).unwrap(),
                json!({"noop":true,"moved":0,"state":[],"renamed":[]})
            );
            assert_eq!(recovery_outputs(&data), before_repeat);
            assert_eq!(
                [&current_db, &old_db]
                    .map(|path| migration_signature::signature(path, &[]).unwrap()),
                signatures,
                "legacy row/CID/FUID preservation: {case}"
            );
            migration_restore::restore(
                &data,
                &config.root,
                Path::new(result["bundle"].as_str().unwrap()),
                true,
            )
            .unwrap();
        } else {
            let error = result.unwrap_err();
            assert!(
                error.contains(if case == "undecodable" {
                    "survey incomplete"
                } else {
                    "required state references have no trusted mapping"
                }),
                "{case}: {error}"
            );
            assert!(recovery_outputs(&data).is_empty(), "{case}");
        }
        assert_eq!(
            [&current_db, &old_db].map(|path| migration_signature::signature(path, &[]).unwrap()),
            signatures,
            "{case}"
        );
        assert_eq!(
            std::fs::read(data.join("publish_fixture.json")).unwrap(),
            state_bytes,
            "{case}"
        );
        assert_eq!(
            std::fs::read(data.join("publish_plan_fixture.json")).unwrap(),
            plan_bytes,
            "{case}"
        );
        assert_eq!(
            std::fs::read(data.join("publish_plan_old.json")).unwrap(),
            obsolete_bytes,
            "{case}"
        );
        assert_eq!(
            std::fs::read(&current_path).unwrap(),
            current_bytes,
            "{case}"
        );
        if second_path.exists() {
            assert_eq!(std::fs::read(second_path).unwrap(), rival_bytes, "{case}");
        }
        assert!(
            !data.join("identity-migration.journal.json").exists(),
            "{case}"
        );
        empty_codecs(media);
    }
    assert_eq!(std::fs::read(rival_path).unwrap(), rival_bytes);
}
