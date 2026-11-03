//! Original live-set assertions through the real publisher, catalog and bounded loopback reads.
use _native::{
    catalog::Catalog, media_adapter::MediaAdapter, media_native_adapter::NativeMediaAdapter,
    publication::Publisher, reconcile, telegram::Telegram,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/bounded_media.rs"]
mod bounded_media;
#[path = "support/publication_identity.rs"]
mod publication_identity;
#[path = "support/read_only_api.rs"]
mod read_only_api;
#[path = "support/ticker_bookkeeping.rs"]
mod ticker_bookkeeping;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use bounded_media::BoundedCodec;
use read_only_api::{Fixture, Reply, TOKEN};
use tooling_fixture::Root;

const SET: &str = "pks1_by_YourEmojiBot";
fn sticker(fuid: &str, cid: &str) -> Value {
    json!({"file_unique_id":fuid,"custom_emoji_id":cid,"file_id":format!("f-{fuid}")})
}
fn pack(stickers: &[Value]) -> Reply {
    Reply::api("getStickerSet", "name", SET, json!({"stickers":stickers}))
}
fn state(keys: &[&str], live: usize) -> Value {
    json!({"name":SET,"fmt":"static","index":1,"title":"Pack1","keys":keys,"live":live,"logo":false})
}
fn published(catalog: &mut Catalog, key: &str) -> Value {
    catalog
        .dispatch(json!({"operation":"custom_id","base":"pk","content_key":key}))
        .unwrap()
}
fn catalog(path: &Path, project: &Path, keys: &[&str]) -> Catalog {
    let mut cat = Catalog::open(&path.join("catalog.db"), "fixture", project).unwrap();
    for (index, key) in keys.iter().enumerate() {
        cat.insert(&json!({"content_key":key,"fmt":"static","file_path":path.join(format!("unused-{index}.png"))}), "fixture").unwrap();
        cat.record_seen(&format!("UP-item{index}"), key).unwrap();
    }
    cat
}

#[test]
fn original_live_manifest_refusals_never_guess_or_reassign_recorded_ids() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let root = Root::new("live manifest café");
    let a = sticker("UP-item0", "c0");
    let b = sticker("UP-item1", "c1");
    let alien = sticker("NEVER-SEEN", "foreign-cid");
    let cases = [
        (
            "unknown",
            vec![],
            vec!["s:first", "s:second"],
            false,
            "unknown",
        ),
        (
            "missing",
            vec![],
            vec!["s:first", "s:second"],
            false,
            "no longer exists",
        ),
        (
            "shrunk",
            vec![a.clone()],
            vec!["s:first", "s:second"],
            false,
            "smaller",
        ),
        (
            "reordered",
            vec![b.clone(), a.clone()],
            vec!["s:first", "s:second"],
            false,
            "position 0",
        ),
        (
            "replaced",
            vec![a.clone(), sticker("UP-item0", "cX")],
            vec!["s:first", "s:second"],
            false,
            "position 1",
        ),
        (
            "duplicate",
            vec![a.clone(), b.clone(), sticker("UP-item0", "c2")],
            vec!["s:first", "s:second"],
            false,
            "twice",
        ),
        (
            "matching-tail",
            vec![a.clone(), b.clone()],
            vec!["s:first"],
            false,
            "",
        ),
        (
            "matching-prefix",
            vec![a.clone(), b.clone()],
            vec!["s:first", "s:second"],
            false,
            "",
        ),
        (
            "known-cid",
            vec![alien.clone(), b.clone()],
            vec!["s:first", "s:second"],
            true,
            "position 0",
        ),
        (
            "known-replaced",
            vec![a.clone(), alien.clone()],
            vec!["s:first", "s:second"],
            true,
            "position 1",
        ),
        (
            "unidentifiable",
            vec![sticker("FRESH-1", "c1"), sticker("FRESH-0", "c0")],
            vec!["s:first", "s:second"],
            false,
            "could not be examined",
        ),
        (
            "fresh-known",
            vec![sticker("BRAND-NEW-0", "c0"), sticker("BRAND-NEW-1", "c1")],
            vec!["s:first", "s:second"],
            false,
            "",
        ),
        (
            "earlier-commit",
            vec![a.clone(), sticker("UP-item0", "c2")],
            vec![],
            false,
            "twice",
        ),
    ];
    for (label, live, keys, seed_cids, error) in cases {
        let path = root.0.join(label);
        std::fs::create_dir(&path).unwrap();
        let mut cat = catalog(&path, project, &["s:first", "s:second"]);
        if seed_cids {
            for (key, cid) in [("s:first", "c0"), ("s:second", "c1")] {
                cat.uploaded(
                    &json!({"content_key":key,"custom_emoji_id":cid,"base":"pk","set_name":SET}),
                    "fixture",
                )
                .unwrap();
            }
        }
        if label == "fresh-known" {
            cat.record_seen("BRAND-NEW-0", "s:first").unwrap();
            cat.record_seen("BRAND-NEW-1", "s:second").unwrap();
        }
        let mut replies = vec![match label {
            "unknown" => Reply::envelope("getStickerSet", "name", SET, json!([])),
            "missing" => Reply::envelope(
                "getStickerSet",
                "name",
                SET,
                json!({"ok":false,"description":"STICKERSET_INVALID"}),
            ),
            _ => pack(&live),
        }];
        if matches!(label, "known-cid" | "known-replaced" | "unidentifiable") {
            let file = live[usize::from(label == "known-replaced")]["file_id"]
                .as_str()
                .unwrap();
            replies.push(Reply::envelope(
                "getFile",
                "file_id",
                file,
                json!({"ok":false,"description":"fixture download refused"}),
            ));
        }
        let mut server = Fixture::new(replies);
        let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
        let media = MediaAdapter {
            python: "must-not-run".into(),
            root: project.into(),
            scratch: path.join("codec"),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut set = state(
            &keys,
            if label == "matching-tail" {
                1
            } else {
                keys.len()
            },
        );
        let original = set.clone();
        let result = Publisher {
            tg: &tg,
            cat: &mut cat,
            media: &media,
            data: path.clone(),
            runtime: &runtime,
            config: None,
        }
        .reconcile(&mut set, "pk");
        if error.is_empty() {
            result.unwrap();
            assert_eq!(set["keys"], json!(["s:first", "s:second"]), "{label}");
            assert_eq!(set["live"], 2);
            assert!(reconcile::open(&set));
        } else {
            assert!(result.unwrap_err().contains(error), "{label}");
            assert_eq!(set, original, "{label}");
        }
        assert!(
            !media.scratch.exists(),
            "known identity invoked codec: {label}"
        );
        let requests = server.finish().unwrap();
        assert_eq!(
            requests.len(),
            if matches!(label, "known-cid" | "known-replaced" | "unidentifiable") {
                2
            } else {
                1
            }
        );
        drop(cat);
        let mut cat = Catalog::open(&path.join("catalog.db"), "reopen", project).unwrap();
        if error.is_empty() {
            assert_eq!(published(&mut cat, "s:first"), "c0");
            assert_eq!(published(&mut cat, "s:second"), "c1");
        } else if label == "earlier-commit" {
            assert_eq!(
                published(&mut cat, "s:first"),
                "c0",
                "earlier proven commit was lost"
            );
            assert_eq!(cat.get("s:first").unwrap().unwrap()["uploaded"], true);
            assert!(published(&mut cat, "s:second").is_null());
        } else if seed_cids {
            assert_eq!(published(&mut cat, "s:first"), "c0");
            assert_eq!(published(&mut cat, "s:second"), "c1");
        } else {
            assert!(published(&mut cat, "s:first").is_null());
            assert!(published(&mut cat, "s:second").is_null());
        }
    }
    root.clean().unwrap();
}

#[test]
fn original_reconciliation_sources_refuse_before_runtime_or_workspace_access() {
    use _native::source_git_inventory::Source;
    let paths = [
        "tests/reference/collection_reconcile.py",
        "tests/test_build_collection_state.py",
        "tests/_bc_fixtures.py",
        "tests/_media_fixtures.py",
        "emojikit/media.py",
        "emojikit/identity.py",
        "tests/reference/catalog.py",
        "emojikit/media_paths.py",
        "tests/reference/ingest.py",
        "tests/_similarity_oracle.py",
        "tests/reference/collection_state.py",
        "tests/reference/build_collection.py",
        "emojikit/telegram_api.py",
        "emojikit/errors.py",
        "tests/test_reconcile_identity.py",
        "tests/test_reconcile_uniqueness.py",
        "tests/test_plan_apply.py",
        "tests/reference/plan_apply.py",
        "tests/reference/panel_plan.py",
        "tests/reference/sync_order.py",
        "tests/reference/packstate.py",
        "emojikit/state_artifacts.py",
        "tests/oracles/telegram_api.py",
        "tests/reference/collection_media_check.py",
        "tests/reference/collection_names.py",
        "emojikit/operator_config.py",
        "emojikit/cli_env.py",
        "tests/test_telegram_client.py",
        "tests/test_telegram_body_shape.py",
        "tests/_pack_fixtures.py",
        "tests/reference/build_pack.py",
        "tests/test_resume_safety.py",
        "tests/test_unresolved_mutation.py",
        "tests/test_build_pack_announce.py",
    ];
    let baseline: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-baseline.json"
    ))
    .unwrap();
    let sources = paths.map(|path| Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: baseline["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["source"] == path)
            .unwrap()["sha256"]
            .as_str()
            .unwrap()
            .into(),
        text: "raise AssertionError('must not execute')".into(),
    });
    let refs = sources.iter().collect::<Vec<_>>();
    let missing = Path::new("must-not-be-inspected");
    assert!(
        _native::source_reconcile_tests::observe(missing, missing, &refs)
            .unwrap_err()
            .contains("source identity drift")
    );
    assert!(_native::source_reconcile_tests::source_identity(&[]).is_err());
}

fn download(file: &str, name: &str, bytes: &[u8]) -> [Reply; 2] {
    [
        Reply::api("getFile", "file_id", file, json!({"file_path":name})),
        Reply::file(name, bytes),
    ]
}

#[test]
fn original_foreign_tail_and_reupload_use_real_content_without_positional_fallback() {
    ticker_bookkeeping::run();
    publication_identity::run();
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let project = _native::tooling_environment::physical_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
    )
    .unwrap();
    let root = Root::new("live content café");
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    let inputs = ["opaque", "alpha-edge", "transparent-rgb"].map(|name| {
        let row = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == name)
            .unwrap();
        let path = project.join(row["input"].as_str().unwrap());
        assert_eq!(
            _native::tooling_manifest::digest(&path).unwrap(),
            row["input_sha256"]
        );
        (
            path.clone(),
            std::fs::read(&path).unwrap(),
            row["expected"].clone(),
        )
    });
    let keys = [
        inputs[0].2["content_key"].as_str().unwrap(),
        inputs[1].2["content_key"].as_str().unwrap(),
    ];
    let native = BoundedCodec {
        native: NativeMediaAdapter {
            executable: PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji")),
            root: project.clone(),
            scratch: root.0.join("envelopes"),
        },
        deadline: Instant::now() + Duration::from_secs(60),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for label in [
        "reupload",
        "foreign-end",
        "foreign-short",
        "inserted",
        "failed-tail",
        "unreadable-behind",
        "ours-behind",
        "stop-at-first",
    ] {
        let path = root.0.join(label);
        std::fs::create_dir(&path).unwrap();
        let mut cat = Catalog::open(&path.join("catalog.db"), "fixture", &project).unwrap();
        for (index, (source, _, expected)) in inputs[..2].iter().enumerate() {
            cat.insert(&json!({"content_key":keys[index],"fmt":"static","file_path":source,"phash":expected["phash"]}), "fixture").unwrap();
            cat.record_seen(&format!("UP-item{index}"), keys[index])
                .unwrap();
        }
        if label == "reupload" {
            for (key, cid) in [(keys[0], "c0"), (keys[1], "c1")] {
                cat.uploaded(
                    &json!({"content_key":key,"custom_emoji_id":cid,"base":"pk","set_name":SET}),
                    "fixture",
                )
                .unwrap();
            }
        }
        let a = sticker("UP-item0", "c0");
        let b = sticker("UP-item1", "c1");
        let foreign = sticker("OWNER", "owner-cid");
        let fresh = sticker("FRESH-item1", "fresh-c1");
        let live = match label {
            "reupload" => vec![sticker("RE-UPLOADED", "c9"), b.clone()],
            "foreign-end" => vec![a.clone(), b.clone(), foreign.clone()],
            "foreign-short" => vec![a.clone(), foreign.clone()],
            "inserted" => vec![a.clone(), foreign.clone(), b.clone()],
            "failed-tail" => vec![a.clone(), sticker("UNSEEN", "cx")],
            "unreadable-behind" | "ours-behind" => vec![a.clone(), foreign.clone(), fresh.clone()],
            "stop-at-first" => vec![
                a.clone(),
                foreign.clone(),
                fresh.clone(),
                sticker("OWNER2", "owner-cid2"),
            ],
            _ => unreachable!(),
        };
        let mut replies = vec![pack(&live)];
        if label == "reupload" {
            replies.extend(download("f-RE-UPLOADED", "reupload.png", &inputs[0].1));
        } else if label == "failed-tail" {
            replies.push(Reply::envelope(
                "getFile",
                "file_id",
                "f-UNSEEN",
                json!({"ok":false,"description":"fixture download refused"}),
            ));
        } else {
            replies.extend(download("f-OWNER", "foreign.png", &inputs[2].1));
            if label == "unreadable-behind" {
                replies.push(Reply::envelope(
                    "getFile",
                    "file_id",
                    "f-FRESH-item1",
                    json!({"ok":false,"description":"fixture download refused"}),
                ));
            } else if matches!(label, "ours-behind" | "stop-at-first") {
                replies.extend(download("f-FRESH-item1", "fresh.png", &inputs[1].1));
            }
        }
        let expected_requests = replies.len();
        let mut server = Fixture::new(replies);
        let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
        let recorded = if matches!(label, "reupload" | "foreign-end") {
            &keys[..]
        } else {
            &keys[..1]
        };
        let mut set = state(recorded, recorded.len());
        let original = set.clone();
        let result = Publisher {
            tg: &tg,
            cat: &mut cat,
            media: &native,
            data: path.clone(),
            runtime: &runtime,
            config: None,
        }
        .reconcile(&mut set, "pk");
        if matches!(label, "reupload" | "foreign-end" | "foreign-short") {
            result.unwrap();
            assert_eq!(set["keys"], json!(recorded));
            assert_eq!(set["live"], live.len());
            assert_eq!(reconcile::open(&set), label == "reupload");
            assert_eq!(
                published(&mut cat, keys[0]),
                if label == "reupload" { "c9" } else { "c0" }
            );
            if label == "foreign-short" {
                assert!(published(&mut cat, keys[1]).is_null());
            } else {
                assert_eq!(published(&mut cat, keys[1]), "c1");
            }
        } else {
            let error = result.unwrap_err();
            if matches!(label, "failed-tail" | "unreadable-behind") {
                assert!(error.contains("could not be examined"), "{label}: {error}");
            }
            if matches!(label, "ours-behind" | "stop-at-first") {
                assert!(
                    error.contains(keys[1]) && error.contains("position 2"),
                    "{error}"
                );
            }
            assert_eq!(set, original);
            assert!(published(&mut cat, keys[0]).is_null());
            assert!(published(&mut cat, keys[1]).is_null());
        }
        assert_eq!(server.finish().unwrap().len(), expected_requests);
        if path.join("tmp").exists() {
            assert_eq!(std::fs::read_dir(path.join("tmp")).unwrap().count(), 0);
        }
    }
    assert!(
        !native.native.scratch.exists(),
        "publisher ignored invocation-owned codec storage"
    );
    for (source, bytes, _) in &inputs {
        assert_eq!(&std::fs::read(source).unwrap(), bytes);
    }
    runtime.shutdown_timeout(Duration::from_secs(1));
    root.clean().unwrap();
}
