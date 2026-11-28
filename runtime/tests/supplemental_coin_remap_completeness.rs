//! Durable file roles and complete local artwork gate real remap publication.
extern crate numera_runtime as rt;

use rt::coin_remap::{Files, persist, run};
use rt::coin_remap_policy::{Decision, Match, Options, decide};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[path = "baseline_coin_providers_support/server.rs"]
mod server;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

const OLD: &[u8] = b"{\"aaa\":\"old-aaa\",\"broken\":\"old-broken\"}\n";
const PRIOR_BACKUP: &[u8] = b"{\"prior\":\"backup-id\"}\n";
const PRIOR_CANDIDATES: &[u8] = b"{\"prior\":\"candidate-id\"}\n";
const ART: &[u8] = include_bytes!("../../tests/fixtures/native/static-codec/alpha-edge.png");

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
    std::fs::write(
        &files.state,
        b"{\"sets\":[{\"index\":1,\"name\":\"fixture1\"}]}\n",
    )
    .unwrap();
    std::fs::write(&files.cache, b"{}\n").unwrap();
    std::fs::write(&files.output, OLD).unwrap();
    std::fs::write(files.images.join("aaa.png"), ART).unwrap();
    files
}

fn options(apply: bool) -> Options {
    Options {
        apply,
        max_distance: Some(1.0),
        min_margin: None,
        incomplete_sets: 0,
        failed_images: 0,
    }
}

fn decision(candidate: bool) -> Decision {
    let mut options = options(true);
    options.incomplete_sets = usize::from(candidate);
    decide(
        &[Match {
            ticker: "aaa".into(),
            cid: "new-a".into(),
            distance: 0.0,
            margin: f64::INFINITY,
        }],
        options,
    )
    .unwrap()
}

fn roles(files: &Files) -> BTreeMap<&'static str, PathBuf> {
    BTreeMap::from([
        ("state", files.state.clone()),
        ("cache", files.cache.clone()),
        ("output", files.output.clone()),
        ("candidates", files.candidates.clone()),
        ("backup", files.output.with_extension("prebroken.json")),
    ])
}

fn snapshot(files: &Files) -> BTreeMap<&'static str, Option<Vec<u8>>> {
    roles(files)
        .into_iter()
        .map(|(role, path)| (role, path.is_file().then(|| std::fs::read(path).unwrap())))
        .collect()
}

fn hardlink_roles(files: &Files, linked: &[&str]) {
    let paths = roles(files);
    std::fs::write(&files.candidates, PRIOR_CANDIDATES).unwrap();
    std::fs::write(&paths["backup"], PRIOR_BACKUP).unwrap();
    for role in &linked[1..] {
        std::fs::remove_file(&paths[role]).unwrap();
        std::fs::hard_link(&paths[linked[0]], &paths[role]).unwrap();
    }
}

fn assert_run_refuses(linked: &[&str], apply: bool) {
    let root = tooling_fixture::Root::new("remap durable inode refusal");
    let files = files(&root.0);
    hardlink_roles(&files, linked);
    let before = snapshot(&files);
    let server = server::Server::new(vec![]);
    let log = rt::logging::RunLog::open(&root.0, "remap-completeness", vec![]);
    let result = run(&files, &server.telegram(), options(apply), &log);
    assert!(
        server.finish().is_empty(),
        "preflight must not contact a provider"
    );
    assert!(
        result.unwrap_err().contains("must be distinct"),
        "hardlinked durable roles must be refused: {linked:?}"
    );
    assert_eq!(
        snapshot(&files),
        before,
        "all initial role bytes must survive"
    );
    assert!(
        !root.0.join(".locks").exists(),
        "no lease may precede refusal"
    );
    drop(log);
    root.clean().unwrap();
}

#[test]
fn full_run_rejects_every_forbidden_hardlink_pair_before_all_effects() {
    // These nine forbidden pairs plus the positive output/backup case cover all ten pairs.
    for linked in [
        ["state", "cache"],
        ["state", "output"],
        ["state", "candidates"],
        ["state", "backup"],
        ["cache", "output"],
        ["cache", "candidates"],
        ["cache", "backup"],
        ["output", "candidates"],
        ["candidates", "backup"],
    ] {
        for apply in [false, true] {
            assert_run_refuses(&linked, apply);
        }
    }
}

#[test]
fn full_run_rejects_three_way_hardlinks_even_with_the_valid_backup_pair() {
    for third in ["state", "cache", "candidates"] {
        for apply in [false, true] {
            assert_run_refuses(&["output", "backup", third], apply);
        }
    }
}

#[test]
fn direct_persistence_rejects_candidate_hardlinks_for_both_destinations() {
    for linked in [
        vec!["output", "candidates"],
        vec!["candidates", "backup"],
        vec!["output", "backup", "candidates"],
    ] {
        for candidate in [false, true] {
            let root = tooling_fixture::Root::new("remap direct inode refusal");
            let files = files(&root.0);
            hardlink_roles(&files, &linked);
            let before = snapshot(&files);
            let error = persist(
                &root.0,
                &files.output,
                &files.candidates,
                &decision(candidate),
            )
            .unwrap_err();
            assert!(error.contains("must be distinct"));
            assert_eq!(snapshot(&files), before);
            assert!(!root.0.join(".locks").exists());
            root.clean().unwrap();
        }
    }
}

#[test]
fn direct_persistence_allows_independent_paths_and_valid_backup_hardlinks() {
    for linked_backup in [false, true] {
        for candidate in [false, true] {
            let root = tooling_fixture::Root::new("remap independent roles");
            let files = files(&root.0);
            let backup = files.output.with_extension("prebroken.json");
            if linked_backup {
                std::fs::hard_link(&files.output, &backup).unwrap();
            }
            persist(
                &root.0,
                &files.output,
                &files.candidates,
                &decision(candidate),
            )
            .unwrap();
            if candidate {
                assert_eq!(std::fs::read(&files.output).unwrap(), OLD);
                assert_eq!(read_json(&files.candidates), json!({"aaa":"new-a"}));
                assert_eq!(backup.exists(), linked_backup);
            } else {
                assert_eq!(read_json(&files.output), json!({"aaa":"new-a"}));
                assert!(!files.candidates.exists());
                assert!(backup.exists());
            }
            if backup.exists() {
                assert_eq!(std::fs::read(&backup).unwrap(), OLD);
            }
            root.clean().unwrap();
        }
    }
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn remote(corrupt: bool) -> server::Server {
    let mut members = vec![json!({"custom_emoji_id":"new-a","file_unique_id":"u-a","file_id":"a"})];
    if corrupt {
        members.push(json!({"custom_emoji_id":"new-b","file_unique_id":"u-b","file_id":"b"}));
    }
    let mut replies = vec![server::reply(
        "getStickerSet",
        server::ok(json!({"stickers":members})),
    )];
    for (file, bytes) in if corrupt {
        vec![("a", ART), ("b", b"not an image".as_slice())]
    } else {
        vec![("a", ART)]
    } {
        replies.push(server::reply(
            "getFile",
            server::ok(json!({"file_path":format!("{file}.png")})),
        ));
        let mut reply = server::reply(&format!("GET {file}.png"), Value::Null);
        reply.body = server::Body::Bytes(bytes.into());
        replies.push(reply);
    }
    server::Server::new(replies)
}

#[test]
fn every_unreadable_local_png_blocks_apply_and_preserves_canonical_and_prior_backup() {
    for count in [1, 2] {
        for existing_backup in [false, true] {
            let root = tooling_fixture::Root::new("remap incomplete local artwork");
            let files = files(&root.0);
            let backup = files.output.with_extension("prebroken.json");
            if existing_backup {
                std::fs::write(&backup, PRIOR_BACKUP).unwrap();
            }
            for index in 0..count {
                std::fs::write(
                    files.images.join(format!("broken-{index}.png")),
                    b"not an image",
                )
                .unwrap();
            }
            let server = remote(false);
            let log = rt::logging::RunLog::open(&root.0, "remap-completeness", vec![]);
            assert_eq!(
                run(&files, &server.telegram(), options(true), &log).unwrap(),
                3
            );
            assert_eq!(server.finish().len(), 3);
            assert_eq!(std::fs::read(&files.output).unwrap(), OLD);
            assert_eq!(read_json(&files.candidates), json!({"aaa":"new-a"}));
            assert_eq!(backup.exists(), existing_backup);
            if existing_backup {
                assert_eq!(std::fs::read(&backup).unwrap(), PRIOR_BACKUP);
            }
            let events = std::fs::read_to_string(log.path.as_ref().unwrap()).unwrap();
            assert!(events.contains(&format!("local_failed={count}")));
            assert!(events.contains(&format!("failed_images={count}")));
            drop(log);
            root.clean().unwrap();
        }
    }
}

#[test]
fn local_completeness_preserves_dry_empty_remote_failure_and_unique_match_contracts() {
    for scenario in ["dry", "empty", "remote", "complete", "linked-backup"] {
        let root = tooling_fixture::Root::new("remap completeness controls");
        let files = files(&root.0);
        let backup = files.output.with_extension("prebroken.json");
        if scenario == "dry" {
            std::fs::write(files.images.join("broken.png"), b"not an image").unwrap();
        } else if scenario == "empty" {
            std::fs::write(files.images.join("aaa.png"), b"not an image").unwrap();
        } else if scenario == "linked-backup" {
            std::fs::hard_link(&files.output, &backup).unwrap();
        }
        let server = remote(scenario == "remote");
        let log = rt::logging::RunLog::open(&root.0, "remap-completeness", vec![]);
        let code = run(&files, &server.telegram(), options(scenario != "dry"), &log).unwrap();
        assert_eq!(
            server.finish().len(),
            if scenario == "remote" { 5 } else { 3 }
        );
        match scenario {
            "dry" | "empty" => {
                assert_eq!(code, if scenario == "dry" { 0 } else { 4 });
                assert_eq!(std::fs::read(&files.output).unwrap(), OLD);
                assert!(!files.candidates.exists());
                assert!(!backup.exists());
            }
            "remote" => {
                assert_eq!(code, 3);
                assert_eq!(std::fs::read(&files.output).unwrap(), OLD);
                assert_eq!(read_json(&files.candidates), json!({"aaa":"new-a"}));
                assert!(!backup.exists());
            }
            _ => {
                assert_eq!(code, 0);
                assert_eq!(read_json(&files.output), json!({"aaa":"new-a"}));
                assert_eq!(std::fs::read(&backup).unwrap(), OLD);
                assert!(!files.candidates.exists());
            }
        }
        drop(log);
        root.clean().unwrap();
    }
}

#[path = "support/test_admission.rs"]
mod test_admission;
