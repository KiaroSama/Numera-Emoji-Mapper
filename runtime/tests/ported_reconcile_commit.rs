// Ported behavioral subset of native/tests/reconcile_commit.rs at a39a049c02f8dcc90d18fe8771869f82dd655221.
// Original source SHA256: 3cbb18d90a3c653ae89b877b0e5b62f3a61ccfd907f7f25a856664cbea9e9bd9.
// Excluded compatibility-only cases remain in the original suite and migration inventory.
extern crate numera_runtime as _native;

// Original live-set assertions through the real publisher, catalog and bounded loopback reads.
use _native::{
    catalog::Catalog, media_adapter::MediaAdapter, publication::Publisher, reconcile,
    telegram::Telegram,
};
use serde_json::{Value, json};
use std::path::Path;
#[allow(dead_code)] // This cohort needs only the shared scripted read helpers.
#[path = "support/read_only_api.rs"]
mod read_only_api;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
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
            executable: "must-not-run".into(),
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

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
