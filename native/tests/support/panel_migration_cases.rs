//! A served plan follows real native video rekeying and returns intact after verified restore.
use super::{
    bounded_media::BoundedCodec,
    panel_persistence_cases::{key, page, post},
};
use _native::{catalog::Catalog, config::Config, media_native_adapter::NativeMediaAdapter};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

pub fn check(
    client: &reqwest::blocking::Client,
    url: &str,
    db: &Path,
    root: &Path,
    project: &Path,
    deadline: Instant,
) {
    const OLD: &str = "v:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    // Source _norm_pixels is the exact solid RGBA pixel repeated at64x64, not target output.
    let statics = [
        "s:d6336ced254b869e10469dbc66aad18a",
        "s:7b4d49f95f1b1438a85e0421049e6e0b",
        "s:2c22a4af24b27a2cdfd2aebdadfe3e54",
        "s:70165fc3a9de8deb2ff2b565b41647d1",
    ];
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/video-fingerprint.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let case = &fixture["cases"][2];
    let source = project.join(case["input"].as_str().unwrap());
    assert_eq!(
        _native::tooling_manifest::digest(&source).unwrap(),
        case["input_sha256"]
    );
    let bytes = std::fs::read(&source).unwrap();
    let video = root.join("legacy-video.webm");
    std::fs::write(&video, &bytes).unwrap();
    let plan_path = db.with_file_name("pack_plan.json");
    let _previous = _native::panel_save::read_plan(&plan_path).unwrap();
    std::fs::remove_file(&plan_path).unwrap();
    {
        let mut cat = Catalog::open(db, "video fixture", root).unwrap();
        let connection = rusqlite::Connection::open(db).unwrap();
        connection
            .busy_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        for (i, new) in statics.iter().enumerate() {
            connection
                .execute(
                    "UPDATE items SET content_key=? WHERE content_key=?",
                    rusqlite::params![new, key(i)],
                )
                .unwrap();
        }
        connection.execute("UPDATE items SET content_key=?,format='video',file_path=?,phash=0 WHERE content_key=?", rusqlite::params![OLD,video.to_str().unwrap(),key(4)]).unwrap();
        connection.execute("DELETE FROM publications", []).unwrap();
        connection
            .execute("UPDATE items SET uploaded=0,custom_emoji_id=NULL", [])
            .unwrap();
        cat.inclusion(&[]).unwrap();
        cat.uploaded(&json!({"content_key":OLD,"base":"fixture","set_name":"one","custom_emoji_id":"VIDEO-CID"}), "fixture").unwrap();
    }
    let known = statics.iter().copied().chain([OLD]).collect::<Vec<_>>();
    assert!(
        page(client, url, deadline)
            .iter()
            .any(|card| card["key"] == OLD)
    );
    assert_eq!(
        post(
            client,
            url,
            json!({"known":known,"excluded":[],"packs":[[OLD,2]]}),
            deadline
        )
        .0,
        200
    );
    let before = _native::panel_save::read_plan(&plan_path).unwrap();
    let signature = _native::migration_signature::signature(db, &[]).unwrap();
    let config = Config::load_with_environment(
        root.into(),
        &BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]),
    )
    .unwrap();
    let codec = BoundedCodec {
        native: NativeMediaAdapter {
            executable: env!("CARGO_BIN_EXE_numera-emoji").into(),
            root: project.into(),
            scratch: root.join("envelopes"),
        },
        deadline,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let applied = _native::migration_apply::apply(&config, root, &codec, &runtime, None).unwrap();
    let new = case["expected"]["content_key"].as_str().unwrap();
    assert_ne!(new, OLD);
    assert_eq!(applied["moved"], 1);
    assert_eq!(applied["key_map"], json!({OLD:new}));
    {
        let mut cat = Catalog::open(db, "reopen migrated", root).unwrap();
        assert!(cat.get(OLD).unwrap().is_none());
        assert_eq!(cat.get(new).unwrap().unwrap()["fmt"], "video");
        assert_eq!(
            cat.dispatch(json!({"operation":"custom_id","base":"fixture","content_key":new}))
                .unwrap(),
            "VIDEO-CID"
        );
    }
    let cards = page(client, url, deadline);
    assert_eq!(
        cards.iter().find(|card| card["key"] == new).unwrap()["pack"],
        2
    );
    assert!(cards.iter().all(|card| card["key"] != OLD));
    let migrated = _native::panel_save::read_plan(&plan_path).unwrap();
    assert_eq!(migrated["targets"], json!([[new, 2]]));
    assert_eq!(
        migrated["moves"],
        json!([{"key":new,"label":before["moves"][0]["label"],"from_pack":1,"to_pack":2}])
    );
    let plan_bytes = std::fs::read(&plan_path).unwrap();
    assert_eq!(
        _native::migration_apply::apply(&config, root, &codec, &runtime, None).unwrap(),
        json!({"noop":true,"moved":0,"state":[],"renamed":[]})
    );
    assert_eq!(std::fs::read(&plan_path).unwrap(), plan_bytes);
    let bundle = Path::new(applied["bundle"].as_str().unwrap());
    _native::migration_restore::restore(root, root, bundle, true).unwrap();
    assert_eq!(
        _native::migration_signature::signature(db, &[]).unwrap(),
        signature
    );
    assert_eq!(_native::panel_save::read_plan(&plan_path).unwrap(), before);
    assert_eq!(
        page(client, url, deadline)
            .iter()
            .find(|card| card["key"] == OLD)
            .unwrap()["pack"],
        2
    );
    assert_eq!(std::fs::read(&video).unwrap(), bytes);
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    assert!(!root.join("identity-migration.journal.json").exists());
    assert_eq!(std::fs::read_dir(&codec.native.scratch).unwrap().count(), 0);
    assert!(!_native::process_owner::cleanup_pending(root));
    assert!(Instant::now() < deadline);
}
