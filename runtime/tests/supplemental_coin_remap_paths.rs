//! Remap reserves independent durable data, backup and lock-storage roles before effects.
extern crate numera_runtime as production;

use production::coin_remap::{Files, persist, run};
use production::coin_remap_policy::{Match, Options, decide};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

const OLD: &[u8] = b"{\"btc\":\"old-id\"}\n";

fn files(root: &Path) -> Files {
    let images = root.join("images");
    std::fs::create_dir(&images).unwrap();
    let files = Files {
        root: root.into(),
        state: root.join("state.json"),
        cache: root.join("cache.json"),
        images,
        output: root.join("map.json"),
        candidates: root.join("candidate.json"),
        base: "fixture".into(),
    };
    std::fs::write(&files.state, b"{\"sets\":[]}\n").unwrap();
    std::fs::write(&files.output, OLD).unwrap();
    files
}

fn options(incomplete_sets: usize) -> Options {
    Options {
        apply: true,
        max_distance: Some(1.0),
        min_margin: None,
        incomplete_sets,
        failed_images: 0,
    }
}

fn decision(incomplete_sets: usize) -> production::coin_remap_policy::Decision {
    decide(
        &[Match {
            ticker: "btc".into(),
            cid: "new-id".into(),
            distance: 0.0,
            margin: f64::INFINITY,
        }],
        options(incomplete_sets),
    )
    .unwrap()
}

fn snapshot(files: &Files) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    [
        files.state.clone(),
        files.cache.clone(),
        files.output.clone(),
        files.candidates.clone(),
        files.output.with_extension("prebroken.json"),
    ]
    .into_iter()
    .map(|path| {
        let bytes = path.is_file().then(|| std::fs::read(&path).unwrap());
        (path, bytes)
    })
    .collect()
}

fn refusal(files: &Files) -> String {
    let tg = production::telegram::Telegram::new(
        "synthetic-fixture".into(),
        "http://127.0.0.1:1".into(),
    )
    .unwrap();
    let log = production::logging::RunLog::open(&files.root, "remap-paths", vec![]);
    run(files, &tg, options(0), &log).unwrap_err()
}

#[test]
fn backup_aliases_are_rejected_before_any_cache_state_or_canonical_write() {
    for role in ["cache", "state", "candidates"] {
        let root = tooling_fixture::Root::new("remap backup role");
        let mut files = files(&root.0);
        let backup = files.output.with_extension("prebroken.json");
        match role {
            "cache" => files.cache = backup.clone(),
            "state" => {
                std::fs::write(&backup, std::fs::read(&files.state).unwrap()).unwrap();
                files.state = backup.clone();
            }
            _ => {
                std::fs::write(&backup, OLD).unwrap();
                files.candidates = backup.clone();
            }
        }
        let before = snapshot(&files);
        assert!(refusal(&files).contains("must be distinct"));
        assert_eq!(snapshot(&files), before);
        assert_eq!(std::fs::read(&files.output).unwrap(), OLD);
        assert!(!root.0.join(".locks").exists());
        root.clean().unwrap();
    }
}

#[test]
fn every_durable_role_refuses_lock_storage_before_lease_metadata_can_overwrite_it() {
    for role in ["state", "cache", "output", "candidates"] {
        for lock in ["canonical_map.lock", "pack_fixture.lock"] {
            let root = tooling_fixture::Root::new("remap lock role");
            let mut files = files(&root.0);
            let path = root.0.join(".locks").join(lock);
            std::fs::create_dir(root.0.join(".locks")).unwrap();
            std::fs::write(&path, b"{\"sets\":[]}\n").unwrap();
            match role {
                "state" => files.state = path.clone(),
                "cache" => files.cache = path.clone(),
                "output" => files.output = path.clone(),
                _ => files.candidates = path.clone(),
            }
            let before = snapshot(&files);
            let lock_before = std::fs::read(&path).unwrap();
            assert!(refusal(&files).contains("must not use lock storage"));
            assert_eq!(snapshot(&files), before);
            assert_eq!(std::fs::read(&path).unwrap(), lock_before);
            assert_eq!(std::fs::read_dir(root.0.join(".locks")).unwrap().count(), 1);
            root.clean().unwrap();
        }
    }
}

#[test]
fn hardlinked_lock_aliases_refuse_before_in_place_lease_writes_touch_durable_data() {
    for role in ["state", "cache", "output", "candidates", "backup"] {
        for lock in ["canonical_map.lock", "pack_fixture.lock"] {
            let root = tooling_fixture::Root::new("remap hardlinked lock role");
            let files = files(&root.0);
            let backup = files.output.with_extension("prebroken.json");
            let path = match role {
                "state" => &files.state,
                "cache" => &files.cache,
                "output" => &files.output,
                "candidates" => &files.candidates,
                _ => &backup,
            };
            if !path.exists() {
                std::fs::write(path, b"{}\n").unwrap();
            }
            std::fs::create_dir(root.0.join(".locks")).unwrap();
            let lock = root.0.join(".locks").join(lock);
            std::fs::hard_link(path, &lock).unwrap();
            let before = snapshot(&files);
            let lock_before = std::fs::read(&lock).unwrap();
            assert!(refusal(&files).contains("must not use lock storage"));
            assert_eq!(snapshot(&files), before);
            assert_eq!(std::fs::read(&lock).unwrap(), lock_before);
            if ["output", "candidates", "backup"].contains(&role) {
                for incomplete in [0, 1] {
                    assert!(
                        persist(
                            &root.0,
                            &files.output,
                            &files.candidates,
                            &decision(incomplete)
                        )
                        .unwrap_err()
                        .contains("must not use lock storage")
                    );
                    assert_eq!(snapshot(&files), before);
                    assert_eq!(std::fs::read(&lock).unwrap(), lock_before);
                }
            }
            root.clean().unwrap();
        }
    }
}

#[test]
fn direct_persistence_protects_canonical_backup_and_lock_roles_for_both_destinations() {
    let root = tooling_fixture::Root::new("remap direct persistence");
    let files = files(&root.0);
    let backup = files.output.with_extension("prebroken.json");
    std::fs::write(&backup, OLD).unwrap();
    for candidate in [
        files.output.clone(),
        backup.clone(),
        root.0.join(".locks/canonical_map.lock"),
        root.0.join(".locks/pack_fixture.lock"),
    ] {
        for incomplete in [0, 1] {
            assert!(persist(&root.0, &files.output, &candidate, &decision(incomplete)).is_err());
            assert_eq!(std::fs::read(&files.output).unwrap(), OLD);
            assert_eq!(std::fs::read(&backup).unwrap(), OLD);
            assert!(!root.0.join(".locks").exists());
        }
    }
    root.clean().unwrap();
}

#[test]
fn invalid_existing_backup_refuses_and_a_valid_hardlinked_backup_remains_unchanged() {
    let root = tooling_fixture::Root::new("remap existing backup");
    let files = files(&root.0);
    let backup = files.output.with_extension("prebroken.json");
    std::fs::create_dir(&backup).unwrap();
    assert!(refusal(&files).contains("regular"));
    assert_eq!(std::fs::read(&files.output).unwrap(), OLD);
    assert!(!files.cache.exists());
    assert!(!root.0.join(".locks").exists());
    std::fs::remove_dir(&backup).unwrap();
    for invalid in [b"not-json".as_slice(), b"[]".as_slice()] {
        std::fs::write(&backup, invalid).unwrap();
        assert!(persist(&root.0, &files.output, &files.candidates, &decision(0)).is_err());
        assert_eq!(std::fs::read(&files.output).unwrap(), OLD);
        assert_eq!(std::fs::read(&backup).unwrap(), invalid);
    }
    std::fs::remove_file(&backup).unwrap();
    std::fs::hard_link(&files.output, &backup).unwrap();
    persist(&root.0, &files.output, &files.candidates, &decision(0)).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&files.output).unwrap()).unwrap(),
        json!({"btc":"new-id"})
    );
    assert_eq!(std::fs::read(&backup).unwrap(), OLD);
    persist(&root.0, &files.output, &files.candidates, &decision(0)).unwrap();
    assert_eq!(std::fs::read(&backup).unwrap(), OLD);
    root.clean().unwrap();
}

#[test]
fn alternate_path_spellings_and_links_cannot_hide_backup_aliases() {
    let root = tooling_fixture::Root::new("remap physical aliases");
    let files = files(&root.0);
    let backup = files.output.with_extension("prebroken.json");
    std::fs::write(&backup, OLD).unwrap();
    let mut aliases = vec![root.0.join(".").join("map.prebroken.json")];
    if cfg!(windows) {
        aliases.push(root.0.join("MAP.PREBROKEN.JSON"));
        aliases.push(root.0.canonicalize().unwrap().join("map.prebroken.json"));
    }
    for alias in aliases {
        assert!(persist(&root.0, &files.output, &alias, &decision(1)).is_err());
        assert_eq!(std::fs::read(&files.output).unwrap(), OLD);
        assert_eq!(std::fs::read(&backup).unwrap(), OLD);
    }
    #[cfg(unix)]
    {
        let linked = root.0.join("linked-backup.json");
        std::os::unix::fs::symlink(&backup, &linked).unwrap();
        assert!(persist(&root.0, &files.output, &linked, &decision(1)).is_err());
        assert_eq!(std::fs::read(&backup).unwrap(), OLD);
        std::fs::remove_file(linked).unwrap();
    }
    assert!(!root.0.join(".locks").exists());
    root.clean().unwrap();
}

#[test]
fn missing_or_malformed_set_inventory_is_a_regular_error_and_preserves_durable_data() {
    for state in [json!({}), json!({"sets":null}), json!({"sets":42})] {
        let root = tooling_fixture::Root::new("remap missing inventory");
        let files = files(&root.0);
        std::fs::write(&files.state, serde_json::to_vec(&state).unwrap()).unwrap();
        let before = snapshot(&files);
        assert_eq!(refusal(&files), "remap state has no sets");
        assert_eq!(snapshot(&files), before);
        root.clean().unwrap();
    }
}

#[path = "support/test_admission.rs"]
mod test_admission;
