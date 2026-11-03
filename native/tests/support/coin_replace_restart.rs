//! Restart the actual durable replacement, never synthesize a successful map or content proof.
use super::bot_api::{Reply, Server};
use super::{
    Art, NEW, document, files as create_files, hash, listing, promoted, released, unavailable,
    untouched,
};
use _native::{
    coin_replace_disk::{Files, run},
    telegram::Telegram,
};
use serde_json::json;
use std::path::Path;

fn proof(bytes: &[u8]) -> Vec<Reply> {
    let mut replies = vec![listing(&["a", NEW, "c"])];
    super::download(&mut replies, Some(bytes));
    replies
}
fn recover(files: &Files, replies: Vec<Reply>) -> bool {
    let mut server = Server::new(replies);
    let tg = Telegram::new(super::bot_api::TOKEN.into(), server.base.clone()).unwrap();
    let outcome =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(files, &tg).unwrap()));
    let requests = server.finish();
    let ok = outcome.unwrap_or_else(|error| std::panic::resume_unwind(error));
    assert!(!requests.unwrap().iter().any(|request| matches!(
        request.method.as_str(),
        "replaceStickerInSet" | "addStickerToSet" | "createNewStickerSet"
    )));
    ok
}
pub(super) fn check(files: &Files, art: &Art, map: &[u8], state: &[u8]) {
    let intent_bytes = std::fs::read(&files.intent).unwrap();
    let intent = document(&files.intent);
    let stage = Path::new(intent["stage"].as_str().unwrap());
    let source = Path::new(intent["source"].as_str().unwrap());
    let uploaded = std::fs::read(source).unwrap();
    assert_eq!(hash(&uploaded), art.prepared_hash);

    // Wrong targets refuse BEFORE a live read, preserving both the selected and recorded files.
    for field in ["map", "state"] {
        let alternate = files.root.join(format!("other-{field}.json"));
        std::fs::write(&alternate, if field == "map" { map } else { state }).unwrap();
        let selected = Files {
            root: files.root.clone(),
            map: if field == "map" {
                alternate.clone()
            } else {
                files.map.clone()
            },
            state: if field == "state" {
                alternate.clone()
            } else {
                files.state.clone()
            },
            images: files.images.clone(),
            intent: files.intent.clone(),
            base: files.base.clone(),
        };
        assert!(!recover(&selected, vec![]));
        assert_eq!(
            std::fs::read(alternate).unwrap(),
            if field == "map" { map } else { state }
        );
        untouched(files, art, map, state);
        assert_eq!(std::fs::read(&files.intent).unwrap(), intent_bytes);
        assert_eq!(std::fs::read(source).unwrap(), uploaded);
        released(files);
    }
    let mut legacy = intent.clone();
    legacy.as_object_mut().unwrap().remove("map_target");
    legacy.as_object_mut().unwrap().remove("state_target");
    _native::atomic::write_json(&files.intent, &legacy).unwrap();
    let legacy_bytes = std::fs::read(&files.intent).unwrap();
    assert!(!recover(files, vec![]));
    assert_eq!(std::fs::read(&files.intent).unwrap(), legacy_bytes);
    untouched(files, art, map, state);
    // Restore only this test's deliberately edited intent, not a production recovery fallback.
    _native::atomic::write_bytes(&files.intent, &intent_bytes).unwrap();

    for replies in [vec![unavailable()], proof(&art.foreign)] {
        assert!(!recover(files, replies));
        untouched(files, art, map, state);
        assert_eq!(std::fs::read(&files.intent).unwrap(), intent_bytes);
        assert_eq!(std::fs::read(source).unwrap(), uploaded);
        assert_eq!(
            std::fs::read(stage.join("png/btc.png")).unwrap(),
            art.original
        );
        released(files);
    }
    assert!(recover(files, proof(&uploaded)));
    promoted(files, art);
    assert!(!stage.exists());
    assert_eq!(std::fs::read(&files.state).unwrap(), state);
    assert!(recover(files, vec![]));
    promoted(files, art);
    released(files);

    // A separate persisted copy models a crash after map repoint, before intent clear.
    let replay = create_files(files.root.join("already-repointed"), art);
    let replay_stage = replay.root.join("logos/.incoming/previous-run");
    std::fs::create_dir_all(replay_stage.join("png")).unwrap();
    let replay_source = replay_stage.join("btc.png");
    let raw = replay_stage.join("png/btc.png");
    std::fs::write(&replay_source, &uploaded).unwrap();
    std::fs::write(&raw, &art.original).unwrap();
    let target = |path: &Path| {
        let text = path.canonicalize().unwrap().to_string_lossy().into_owned();
        if cfg!(windows) {
            text.to_lowercase()
        } else {
            text
        }
    };
    let mut pending = intent;
    pending["source"] = json!(replay_source);
    pending["stage"] = json!(replay_stage);
    pending["map_target"] = json!(target(&replay.map));
    pending["state_target"] = json!(target(&replay.state));
    pending["promote"] = json!([
        [raw, replay.root.join("logos/png/btc.png")],
        [replay_source, replay.images.join("btc.png")]
    ]);
    _native::atomic::write_json(&replay.intent, &pending).unwrap();
    _native::atomic::write_json(&replay.map, &json!({"btc":NEW,"wbtc":NEW,"eth":"cid-eth"}))
        .unwrap();
    let settled = std::fs::read(&replay.map).unwrap();
    assert!(recover(&replay, proof(&uploaded)));
    assert_eq!(std::fs::read(&replay.map).unwrap(), settled);
    promoted(&replay, art);
    assert!(!replay_stage.exists());
    assert!(recover(&replay, vec![]));
    promoted(&replay, art);
    released(&replay);
}

pub(super) fn locked(root: &Path, art: &Art) {
    for variant in ["family-held", "map-held"] {
        let files = create_files(root.join(variant), art);
        let map = std::fs::read(&files.map).unwrap();
        let state = std::fs::read(&files.state).unwrap();
        let lock = if variant == "family-held" {
            _native::locks::pack_family_path(&files.root, "cryptoemoji")
        } else {
            let path = files.root.join(".locks/canonical_map.lock");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            path
        };
        let lease = _native::locks::Lease::acquire(&lock, "held replacement writer").unwrap();
        let mut server = Server::new(vec![]);
        let tg = Telegram::new(super::bot_api::TOKEN.into(), server.base.clone()).unwrap();
        assert!(
            _native::coin_replace_effects::replace(
                &files,
                &tg,
                "btc",
                "Bitcoin",
                &art.original,
                111111111
            )
            .is_err()
        );
        assert!(server.finish().unwrap().is_empty());
        untouched(&files, art, &map, &state);
        assert!(!files.intent.exists());
        assert!(!files.root.join("logos/.incoming").exists());
        drop(lease);
        released(&files);
    }
}
