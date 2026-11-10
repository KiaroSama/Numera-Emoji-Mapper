//! Real replacement effects preserve the original oracle until manifest AND artwork are proven.
use _native::{coin_replace_disk::Files, telegram::Telegram};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/coin_replace_restart.rs"]
mod coin_replace_restart;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use bot_api::{Reply, Server};

const SET: &str = "cryptoemoji1_by_YourEmojiBot";
const OLD: &str = "cid-old";
const NEW: &str = "cid-new";

fn document(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn member(cid: &str) -> Value {
    json!({"custom_emoji_id":cid,"file_id":format!("fid-{cid}")})
}
fn listing(ids: &[&str]) -> Reply {
    let members = ids.iter().map(|id| member(id)).collect::<Vec<_>>();
    Reply::new(
        "getStickerSet",
        &[("name", SET)],
        json!({"ok":true,"result":{"stickers":members}}),
    )
}
fn unavailable() -> Reply {
    // A definite read refusal is unknown LIVE state, not a missing/unchanged set.
    Reply::new(
        "getStickerSet",
        &[("name", SET)],
        json!({"ok":false,"description":"fixture read unavailable"}),
    )
}
fn download(replies: &mut Vec<Reply>, bytes: Option<&[u8]>) {
    replies.push(Reply::new(
        "getFile",
        &[("file_id", "fid-cid-new")],
        json!({"ok":true,"result":{"file_path":"replacement.png"}}),
    ));
    let mut reply = Reply::new("getFile", &[], Value::Null);
    reply.method = "GET replacement.png".into();
    if let Some(bytes) = bytes {
        reply.download = Some(bytes.to_vec());
    } else {
        // Readback echoes observed wire bytes; the independent PNG golden admits that upload.
        reply.download_upload = Some("file0".into());
    }
    replies.push(reply);
}
struct Art {
    original: Vec<u8>,
    foreign: Vec<u8>,
    foreign_hash: u64,
    prepared_hash: String,
}
impl Art {
    fn load(project: &Path) -> Self {
        let (pinned, provider) = _native::source_static_oracle::fixtures().unwrap();
        let fixture: Value = std::env::var_os("NUMERA_STATIC_CONVERT_ORACLE")
            .map(|path| document(Path::new(&path)))
            .unwrap_or(pinned);
        _native::source_static_oracle::fixture_identity(&fixture, &provider).unwrap();
        assert_eq!(
            fixture["source_revision"],
            "1a36a968b006236bd0665a1da65c541938c06061"
        );
        assert_eq!(
            fixture["source_sha256"],
            "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53"
        );
        let cases = fixture["cases"].as_array().unwrap();
        let art = cases
            .iter()
            .find(|case| case["name"] == "alpha-edge")
            .unwrap();
        let foreign = cases
            .iter()
            .find(|case| case["name"] == "public-logo")
            .unwrap();
        let original = std::fs::read(project.join(art["input"].as_str().unwrap())).unwrap();
        let foreign_bytes =
            std::fs::read(project.join(foreign["input"].as_str().unwrap())).unwrap();
        assert_eq!(hash(&original), art["input_sha256"]);
        assert_eq!(hash(&foreign_bytes), foreign["input_sha256"]);
        let identities: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/static-codec.json"
        ))
        .unwrap();
        let foreign_hash = identities["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == "public-logo")
            .unwrap()["expected"]["phash"]
            .as_u64()
            .unwrap();
        Self {
            original,
            foreign: foreign_bytes,
            foreign_hash,
            prepared_hash: art["expected"]["png_sha256"].as_str().unwrap().into(),
        }
    }
}
fn files(root: PathBuf, art: &Art) -> Files {
    let images = root.join("logos/emoji");
    std::fs::create_dir_all(&images).unwrap();
    std::fs::create_dir(root.join("logos/png")).unwrap();
    std::fs::write(images.join("btc.png"), &art.foreign).unwrap();
    std::fs::write(root.join("logos/png/btc.png"), &art.foreign).unwrap();
    let files = Files {
        map: root.join("map.json"),
        state: root.join("state.json"),
        intent: root.join("intent.json"),
        images,
        base: "cryptoemoji".into(),
        root,
    };
    _native::atomic::write_json(&files.map, &json!({"btc":OLD,"wbtc":OLD,"eth":"cid-eth"}))
        .unwrap();
    _native::atomic::write_json(&files.state, &json!({"sets":[{"name":SET,"index":1}]})).unwrap();
    files
}
fn mutation(files: &Files, art: &Art, payload: Value) -> Reply {
    let input = json!({"sticker":"attach://file0","format":"static","emoji_list":["🪙"],"keywords":["btc","bitcoin"]}).to_string();
    let mut reply = Reply::new(
        "replaceStickerInSet",
        &[
            ("name", SET),
            ("old_sticker", "fid-cid-old"),
            ("user_id", "111111111"),
            ("sticker", &input),
        ],
        payload,
    );
    reply
        .upload_sha256
        .insert("file0".into(), art.prepared_hash.clone());
    reply.observation = Some((
        files.intent.clone(),
        "/before".into(),
        json!(["a", OLD, "c"]),
    ));
    reply
}
fn untouched(files: &Files, art: &Art, map: &[u8], state: &[u8]) {
    assert_eq!(std::fs::read(&files.map).unwrap(), map);
    assert_eq!(std::fs::read(&files.state).unwrap(), state);
    assert_eq!(
        std::fs::read(files.images.join("btc.png")).unwrap(),
        art.foreign
    );
    assert_eq!(
        std::fs::read(files.root.join("logos/png/btc.png")).unwrap(),
        art.foreign
    );
}
fn released(files: &Files) {
    for path in [
        _native::locks::pack_family_path(&files.root, "cryptoemoji"),
        files.root.join(".locks/canonical_map.lock"),
    ] {
        drop(_native::locks::Lease::acquire(&path, "replacement released").unwrap());
        assert!(path.is_file());
    }
    assert!(!_native::process_owner::cleanup_pending(&files.root));
}
fn promoted(files: &Files, art: &Art) {
    assert_eq!(
        document(&files.map),
        json!({"btc":NEW,"wbtc":NEW,"eth":"cid-eth"})
    );
    assert_eq!(
        hash(&std::fs::read(files.images.join("btc.png")).unwrap()),
        art.prepared_hash
    );
    assert_eq!(
        std::fs::read(files.root.join("logos/png/btc.png")).unwrap(),
        art.original
    );
    assert!(!files.intent.exists());
    assert!(
        std::fs::read_dir(files.root.join("logos/.incoming"))
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn replacement_recovery_accepts_equivalent_inventory_paths_but_retains_foreign_and_traversing_art()
{
    let fixture = tooling_fixture::Root::new("coin replacement inventory paths");
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let art = Art::load(project);
    let files = files(fixture.0.join("recovery"), &art);
    let map = std::fs::read(&files.map).unwrap();
    let state = std::fs::read(&files.state).unwrap();
    let stage = files.root.join("logos/.incoming/recorded-run");
    std::fs::create_dir_all(stage.join("png")).unwrap();
    let raw = stage.join("png").join("btc.png");
    let prepared = stage.join("btc.png");
    std::fs::write(&raw, &art.original).unwrap();
    _native::codec_image::to_static_png(&raw, &prepared).unwrap();
    let uploaded = std::fs::read(&prepared).unwrap();
    assert_eq!(hash(&uploaded), art.prepared_hash);
    let target = |path: &Path| {
        let text = path.canonicalize().unwrap().to_string_lossy().into_owned();
        if cfg!(windows) {
            text.to_lowercase()
        } else {
            text
        }
    };
    let intent = json!({"key":"btc","set_name":SET,"set_index":1,"old_cid":OLD,
        "before":["a",OLD,"c"],"source_dhash":_native::codec_image::source_dhash(&prepared).unwrap(),
        "source":prepared,"stage":stage,"map_target":target(&files.map),"state_target":target(&files.state),
        "promote":[[raw,files.root.join("logos/png/btc.png")],[prepared,files.images.join("btc.png")]]});
    let proof = || {
        let mut replies = vec![listing(&["a", NEW, "c"])];
        download(&mut replies, Some(&uploaded));
        replies
    };
    let outside_source = files.root.join("outside-prepared.png");
    let outside_destination = files.root.join("outside-destination.png");
    std::fs::write(&outside_source, &uploaded).unwrap();
    std::fs::write(&outside_destination, &art.foreign).unwrap();
    for variant in ["outside-source", "outside-destination", "parent-traversal"] {
        let mut refused = intent.clone();
        match variant {
            "outside-source" => refused["promote"][1][0] = json!(outside_source),
            "outside-destination" => refused["promote"][1][1] = json!(outside_destination),
            _ => {
                refused["promote"][0][0] = json!(stage.join("png/../png/btc.png"));
            }
        }
        _native::atomic::write_json(&files.intent, &refused).unwrap();
        let recorded = std::fs::read(&files.intent).unwrap();
        assert!(
            coin_replace_restart::recover_result(&files, proof()).is_err(),
            "{variant}"
        );
        untouched(&files, &art, &map, &state);
        assert_eq!(std::fs::read(&files.intent).unwrap(), recorded, "{variant}");
        assert_eq!(std::fs::read(&raw).unwrap(), art.original, "{variant}");
        assert_eq!(std::fs::read(&prepared).unwrap(), uploaded, "{variant}");
        assert_eq!(
            std::fs::read(&outside_source).unwrap(),
            uploaded,
            "{variant}"
        );
        assert_eq!(
            std::fs::read(&outside_destination).unwrap(),
            art.foreign,
            "{variant}"
        );
        released(&files);
    }
    // Windows inventories may persist '/' where chained PathBuf joins use '\\'.
    // An interior current-directory component exercises equivalent native paths on every OS.
    let mut equivalent = intent;
    for row in equivalent["promote"].as_array_mut().unwrap() {
        for value in row.as_array_mut().unwrap() {
            let path = Path::new(value.as_str().unwrap());
            let spelling = format!(
                "{}/./{}",
                path.parent().unwrap().to_str().unwrap(),
                path.file_name().unwrap().to_str().unwrap()
            );
            assert_ne!(spelling, value.as_str().unwrap());
            *value = json!(spelling);
        }
    }
    _native::atomic::write_json(&files.intent, &equivalent).unwrap();
    assert!(coin_replace_restart::recover_result(&files, proof()).unwrap());
    promoted(&files, &art);
    assert!(!stage.exists());
    assert_eq!(std::fs::read(&files.state).unwrap(), state);
    assert_eq!(std::fs::read(outside_source).unwrap(), uploaded);
    assert_eq!(std::fs::read(outside_destination).unwrap(), art.foreign);
    released(&files);
    fixture.clean().unwrap();
}

#[test]
fn native_replacement_uploads_once_and_promotes_only_content_proven_art() {
    let fixture = tooling_fixture::Root::new("coin replacement original effects");
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let art = Art::load(project);
    // The PNG fixtures adapt the original seeded noise, not its exact random pixel recipe.
    for variant in [
        "accepted",
        "rejected",
        "unchanged",
        "shifted",
        "foreign",
        "unreadable",
        "verified-unknown",
        "unknown",
    ] {
        let files = files(fixture.0.join(variant), &art);
        let map = std::fs::read(&files.map).unwrap();
        let state = std::fs::read(&files.state).unwrap();
        let payload = match variant {
            "rejected" => json!({"ok":false,"description":"STICKER_INVALID"}),
            // A non-object reply enters transport proof, not a lost-socket reproduction.
            "verified-unknown" | "unknown" => Value::Null,
            _ => json!({"ok":true,"result":true}),
        };
        let (replacement, reached, release) = mutation(&files, &art, payload).hold();
        let mut replies = vec![listing(&["a", OLD, "c"]), replacement];
        if matches!(variant, "verified-unknown" | "unknown") {
            replies.push(if variant == "unknown" {
                unavailable()
            } else {
                listing(&["a", NEW, "c"])
            });
        }
        if variant != "rejected" {
            replies.push(match variant {
                "unchanged" => listing(&["a", OLD, "c"]),
                "shifted" => listing(&[NEW, "c"]),
                "unreadable" | "unknown" => unavailable(),
                _ => listing(&["a", NEW, "c"]),
            });
        }
        if matches!(variant, "accepted" | "foreign" | "verified-unknown") {
            download(
                &mut replies,
                (variant == "foreign").then_some(art.foreign.as_slice()),
            );
        }
        let mut server = Server::new(replies);
        let tg = Telegram::new(bot_api::TOKEN.into(), server.base.clone()).unwrap();
        let root = files.root.clone();
        let prepared_hash = art.prepared_hash.clone();
        let foreign_hash = art.foreign_hash;
        let observer = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                tokio::time::timeout(std::time::Duration::from_secs(5), reached)
                    .await
                    .unwrap()
                    .unwrap();
            });
            let intent = document(&root.join("intent.json"));
            assert_eq!(intent["operation"], "replace");
            assert_eq!(intent["key"], "btc");
            assert_eq!(intent["old_cid"], OLD);
            assert_eq!(intent["set_name"], SET);
            assert_eq!(intent["set_index"], 1);
            assert_eq!(intent["before"], json!(["a", OLD, "c"]));
            let staged = std::fs::read(intent["source"].as_str().unwrap()).unwrap();
            assert_eq!(hash(&staged), prepared_hash);
            assert!(staged.starts_with(b"\x89PNG\r\n\x1a\n"));
            // A fixture premise: wrong live art must be beyond the recorded proof threshold.
            assert!((intent["source_dhash"].as_u64().unwrap() ^ foreign_hash).count_ones() > 8);
            for path in [
                _native::locks::pack_family_path(&root, "cryptoemoji"),
                root.join(".locks/canonical_map.lock"),
            ] {
                assert!(
                    _native::locks::Lease::acquire(&path, "replacement competing writer").is_err()
                );
            }
            release.send(()).unwrap();
        });
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            _native::coin_replace_effects::replace(
                &files,
                &tg,
                "btc",
                "Bitcoin",
                &art.original,
                111111111,
            )
            .unwrap()
        }));
        let observed = observer.join();
        let requests = server.finish();
        observed.unwrap();
        let ok = outcome.unwrap_or_else(|error| std::panic::resume_unwind(error));
        let requests = requests.unwrap();
        assert_eq!(
            ok,
            matches!(variant, "accepted" | "verified-unknown"),
            "{variant}"
        );
        let writes = requests
            .iter()
            .filter(|request| request.method == "replaceStickerInSet")
            .collect::<Vec<_>>();
        assert_eq!(writes.len(), 1, "{variant}");
        assert_eq!(hash(&writes[0].files["file0"]), art.prepared_hash);
        let intent = writes[0].observation.as_ref().unwrap();
        assert_eq!(intent["old_cid"], OLD);
        if ok {
            assert_eq!(
                std::fs::read(files.images.join("btc.png")).unwrap(),
                writes[0].files["file0"]
            );
            promoted(&files, &art);
        } else {
            untouched(&files, &art, &map, &state);
            if matches!(variant, "rejected" | "unchanged") {
                assert!(!files.intent.exists());
                assert!(
                    std::fs::read_dir(files.root.join("logos/.incoming"))
                        .unwrap()
                        .next()
                        .is_none()
                );
            } else {
                assert_eq!(document(&files.intent), *intent);
                assert_eq!(
                    std::fs::read(intent["source"].as_str().unwrap()).unwrap(),
                    writes[0].files["file0"]
                );
                assert_eq!(
                    std::fs::read(Path::new(intent["stage"].as_str().unwrap()).join("png/btc.png"))
                        .unwrap(),
                    art.original
                );
            }
        }
        assert_eq!(std::fs::read(&files.state).unwrap(), state);
        assert!(!requests.iter().any(|request| matches!(
            request.method.as_str(),
            "addStickerToSet" | "createNewStickerSet"
        )));
        released(&files);
        if variant == "unreadable" {
            coin_replace_restart::check(&files, &art, &map, &state);
        }
    }
    coin_replace_restart::locked(&fixture.0, &art);
    fixture.clean().unwrap();
}
