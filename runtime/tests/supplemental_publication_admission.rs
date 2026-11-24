//! Reconciliation only commits usable identities while retaining earlier proven progress.
use numera_runtime::{
    catalog::Catalog, media_adapter::MediaAdapter, publication::Publisher, telegram::Telegram,
};
use serde_json::{Value, json};
use std::path::Path;

#[allow(dead_code)]
#[path = "support/read_only_api.rs"]
mod read_only_api;
#[path = "support/test_admission.rs"]
mod test_admission;
use read_only_api::{Fixture, Reply, TOKEN};

const BASE: &str = "admission";
const SET: &str = "admissions1_by_YourEmojiBot";
const FIRST: &str = "s:first";
const NEXT: &str = "s:next";

fn sticker(key: &str, cid: Value) -> Value {
    json!({"file_id":format!("unused-{key}"),"file_unique_id":format!("known-{key}"),
        "custom_emoji_id":cid})
}

fn observation(stickers: &[Value]) -> Reply {
    Reply::api(
        "getStickerSet",
        "name",
        SET,
        json!({"name":SET,"stickers":stickers}),
    )
}

fn published(catalog: &mut Catalog, key: &str) -> Value {
    catalog
        .dispatch(json!({"operation":"published","base":BASE,"content_key":key}))
        .unwrap()
}

fn reconcile(
    catalog: &mut Catalog,
    tg: &Telegram,
    media: &MediaAdapter,
    runtime: &tokio::runtime::Runtime,
    data: &Path,
    set: &mut Value,
) -> Result<(), String> {
    Publisher {
        tg,
        cat: catalog,
        media,
        data: data.into(),
        runtime,
        config: None,
    }
    .reconcile(set, BASE)
}

#[test]
fn malformed_branded_logo_never_records_downstream_items() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for logo in [json!({}), json!({"custom_emoji_id":""})] {
        let scratch = tempfile::tempdir().unwrap();
        let database = scratch.path().join("catalog.db");
        let mut catalog = Catalog::open(&database, "fixture UTC", project).unwrap();
        catalog
            .insert(
                &json!({"content_key":NEXT,"fmt":"static",
                    "file_path":scratch.path().join("next.png")}),
                "fixture UTC",
            )
            .unwrap();
        catalog.record_seen(&format!("known-{NEXT}"), NEXT).unwrap();
        let mut server = Fixture::new(vec![observation(&[
            logo,
            sticker(NEXT, json!("111111111")),
        ])]);
        let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
        let media = MediaAdapter {
            executable: "must-not-run".into(),
            root: project.into(),
            scratch: scratch.path().join("unused-codec"),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut set = json!({"name":SET,"fmt":"static","index":1,
            "keys":[],"live":0,"logo":true});
        let original = set.clone();
        let result = reconcile(
            &mut catalog,
            &tg,
            &media,
            &runtime,
            scratch.path(),
            &mut set,
        );
        drop(catalog);
        let mut catalog = Catalog::open(&database, "reopen UTC", project).unwrap();
        assert_eq!(published(&mut catalog, NEXT), false);
        assert!(result.is_err(), "malformed logo CID was accepted");
        assert_eq!(set, original);
        assert_eq!(server.finish().unwrap().len(), 1);
    }
}

fn refused_tail_keeps_pending(prior_valid: bool) {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for (label, cid) in [
        ("missing", None),
        ("null", Some(Value::Null)),
        ("empty", Some(json!(""))),
        ("number", Some(json!(123))),
        ("boolean", Some(json!(true))),
        ("array", Some(json!([]))),
        ("object", Some(json!({}))),
    ] {
        let scratch = tempfile::tempdir().unwrap();
        let database = scratch.path().join("catalog.db");
        let mut catalog = Catalog::open(&database, "fixture UTC", project).unwrap();
        for key in [FIRST, NEXT] {
            catalog
                .insert(
                    &json!({"content_key":key,"fmt":"static",
                        "file_path":scratch.path().join(format!("{}.png", key.replace(':', "_")))}),
                    "fixture UTC",
                )
                .unwrap();
            catalog.record_seen(&format!("known-{key}"), key).unwrap();
        }
        let first = sticker(FIRST, json!("111111111"));
        let mut invalid = sticker(NEXT, Value::Null);
        match cid {
            Some(cid) => invalid["custom_emoji_id"] = cid,
            None => {
                invalid.as_object_mut().unwrap().remove("custom_emoji_id");
            }
        }
        let settled = if prior_valid { vec![first] } else { vec![] };
        let mut malformed = settled.clone();
        malformed.push(invalid);
        let mut server = Fixture::new(vec![observation(&malformed), observation(&settled)]);
        let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
        let media = MediaAdapter {
            executable: "must-not-run".into(),
            root: project.into(),
            scratch: scratch.path().join("unused-codec"),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut set = json!({"name":SET,"fmt":"static","index":1,
            "keys":[],"live":0,"logo":false});
        let original = set.clone();
        assert!(
            reconcile(
                &mut catalog,
                &tg,
                &media,
                &runtime,
                scratch.path(),
                &mut set
            )
            .is_err(),
            "malformed CID was accepted: {label}"
        );
        assert_eq!(set, original, "refused membership changed: {label}");
        drop(catalog);
        let mut catalog = Catalog::open(&database, "reopen UTC", project).unwrap();
        assert_eq!(
            published(&mut catalog, NEXT),
            false,
            "refused CID suppressed a pending item: {label}"
        );
        assert_eq!(
            catalog.get(NEXT).unwrap().unwrap()["uploaded"],
            false,
            "{label}"
        );
        assert_eq!(published(&mut catalog, FIRST), prior_valid, "{label}");
        if prior_valid {
            assert_eq!(
                catalog
                    .dispatch(json!({"operation":"custom_id","base":BASE,"content_key":FIRST}))
                    .unwrap(),
                "111111111",
                "earlier proven progress was lost: {label}"
            );
        }
        reconcile(
            &mut catalog,
            &tg,
            &media,
            &runtime,
            scratch.path(),
            &mut set,
        )
        .unwrap();
        assert_eq!(
            set["keys"],
            if prior_valid {
                json!([FIRST])
            } else {
                json!([])
            },
            "{label}"
        );
        assert_eq!(published(&mut catalog, NEXT), false, "{label}");
        assert!(
            catalog
                .pending(Some("static"), Some(BASE))
                .unwrap()
                .iter()
                .any(|item| item["content_key"] == NEXT),
            "later successful reconciliation lost the refused pending item: {label}"
        );
        assert!(
            !media.scratch.exists(),
            "known identity invoked a codec: {label}"
        );
        assert_eq!(server.finish().unwrap().len(), 2, "{label}");
    }
}

#[test]
fn malformed_tail_cid_never_creates_a_durable_publication() {
    refused_tail_keeps_pending(false);
}

#[test]
fn malformed_tail_cid_preserves_earlier_identity_proven_progress() {
    refused_tail_keeps_pending(true);
}

#[test]
fn a_tail_cannot_reuse_an_earlier_custom_emoji_id_for_another_catalog_key() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for recorded_prefix in [false, true] {
        let scratch = tempfile::tempdir().unwrap();
        let database = scratch.path().join("catalog.db");
        let mut catalog = Catalog::open(&database, "fixture UTC", project).unwrap();
        for key in [FIRST, NEXT] {
            catalog
                .insert(
                    &json!({"content_key":key,"fmt":"static",
                        "file_path":scratch.path().join(format!("{}.png", key.replace(':', "_")))}),
                    "fixture UTC",
                )
                .unwrap();
            catalog.record_seen(&format!("known-{key}"), key).unwrap();
        }
        if recorded_prefix {
            catalog
                .uploaded(
                    &json!({"content_key":FIRST,"custom_emoji_id":"111111111","base":BASE,"set_name":SET}),
                    "fixture UTC",
                )
                .unwrap();
        }
        let mut server = Fixture::new(vec![observation(&[
            sticker(FIRST, json!("111111111")),
            sticker(NEXT, json!("111111111")),
        ])]);
        let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
        let media = MediaAdapter {
            executable: "must-not-run".into(),
            root: project.into(),
            scratch: scratch.path().join("unused-codec"),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut set = json!({"name":SET,"fmt":"static","index":1,
            "keys":if recorded_prefix {json!([FIRST])}else{json!([])},
            "live":usize::from(recorded_prefix),"logo":false});
        let original = set.clone();
        assert!(
            reconcile(
                &mut catalog,
                &tg,
                &media,
                &runtime,
                scratch.path(),
                &mut set
            )
            .is_err(),
            "one custom emoji ID was attributed to different catalog keys"
        );
        assert_eq!(set, original);
        drop(catalog);
        let mut catalog = Catalog::open(&database, "reopen UTC", project).unwrap();
        assert_eq!(published(&mut catalog, FIRST), true);
        assert_eq!(published(&mut catalog, NEXT), false);
        assert_eq!(catalog.get(NEXT).unwrap().unwrap()["uploaded"], false);
        assert_eq!(
            catalog
                .dispatch(json!({"operation":"custom_id","base":BASE,"content_key":FIRST}))
                .unwrap(),
            "111111111"
        );
        assert!(!media.scratch.exists());
        assert_eq!(server.finish().unwrap().len(), 1);
    }
}
