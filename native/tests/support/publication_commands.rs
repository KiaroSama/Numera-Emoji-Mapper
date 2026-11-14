//! Original native publication commands reuse the ticker's copied artifact and descendant owner.
use super::{Commands, download};
use crate::bot_api::{Reply, Server, rgba_signature};
use _native::{atomic, catalog::Catalog};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[path = "publication_brand.rs"]
mod brand;
#[path = "publication_capacity.rs"]
mod capacity;
#[path = "publication_dedup.rs"]
mod dedup;
#[path = "publication_notify.rs"]
mod notify;
#[path = "publication_order.rs"]
mod order;
#[path = "publication_preflight.rs"]
mod preflight;
#[path = "publication_progress.rs"]
mod progress;
#[path = "publication_refusal.rs"]
mod refusal;
#[path = "publication_retry.rs"]
mod retry;
#[path = "ticker_success.rs"]
mod ticker_success;
#[path = "publication_validation.rs"]
mod validation;

const UTC: &str = "2026-10-07 00:00:00 UTC";
const FIRST: &str = "fixture1_by_YourEmojiBot";
const SECOND: &str = "fixture2_by_YourEmojiBot";

fn member(file: &str, fuid: &str, cid: &str) -> Value {
    json!({"file_id":file,"file_unique_id":fuid,"custom_emoji_id":cid})
}
fn listing(name: &str, members: &[Value]) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", name)],
        json!({"ok":true,"result":{"name":name,"stickers":members}}),
    )
}
fn me() -> Reply {
    Reply::new(
        "getMe",
        &[],
        json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
    )
}
fn mutation(state: &Path, name: &str, key: &str, artwork: (&[u8], bool), create: bool) -> Reply {
    let (bytes, logo) = artwork;
    let input = json!({"sticker":"attach://file0","format":"static", "emoji_list":[if logo {"✅"} else {"😀"}], "keywords":if logo {json!(["logo"])} else {json!(["public | artwork"])}});
    let mut reply = Reply::new(
        if create {
            "createNewStickerSet"
        } else {
            "addStickerToSet"
        },
        &[],
        json!({"ok":true,"result":true}),
    );
    reply.fields = BTreeMap::from([
        ("user_id".into(), "111111111".into()),
        ("name".into(), name.into()),
    ]);
    if create {
        reply.fields.insert(
            "title".into(),
            if name == SECOND {
                "Fixture 2"
            } else {
                "Fixture 1"
            }
            .into(),
        );
        reply
            .fields
            .insert("sticker_type".into(), "custom_emoji".into());
        reply
            .fields
            .insert("stickers".into(), json!([input]).to_string());
    } else {
        reply.fields.insert("sticker".into(), input.to_string());
    }
    if logo {
        // Logo inputs come from the hash-admitted artwork converted by the
        // historical Pillow codec above. Only the generated PNG compression may
        // differ; the actual multipart body must retain its size, mode and RGBA.
        let expected_logo = rgba_signature(bytes).unwrap();
        assert_eq!(expected_logo["size"], json!([100, 100]));
        assert_eq!(expected_logo["mode"], "RGBA");
        reply.upload_rgba.insert("file0".into(), expected_logo);
    } else {
        reply.files.insert("file0".into(), bytes.to_vec());
    }
    reply.observation = Some((state.into(), "/in_flight/key".into(), json!(key)));
    reply
}
fn notification(name: &str, full: bool, state: &Path) -> Reply {
    let title = if name == SECOND {
        "Fixture 2"
    } else {
        "Fixture 1"
    };
    let mut reply = Reply::new(
        "sendMessage",
        &[],
        json!({"ok":true,"result":{"message_id":1}}),
    );
    reply.fields = BTreeMap::from([
        ("chat_id".into(), "111111111".into()),
        (
            "text".into(),
            format!("✅ {title}\nhttps://t.me/addemoji/{name}"),
        ),
        ("disable_web_page_preview".into(), "true".into()),
    ]);
    reply.observation = Some((
        state.into(),
        if full { "/sent_full" } else { "/sent" }.into(),
        if name == SECOND {
            json!([FIRST])
        } else {
            json!([])
        },
    ));
    reply
}
fn set(name: &str, index: u64, key: &str) -> Value {
    json!({"name":name,"title":format!("Fixture {index}"),"fmt":"mixed","index":index,"keys":[key],"live":1,"logo":false})
}
fn execute(
    commands: &Commands<'_>,
    data: &Path,
    options: &[&str],
    server: &Server,
    branded: Option<&Path>,
) -> _native::tooling_process::Completed {
    let mut environment = commands.environment.clone();
    environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
    environment.insert("PACK_LINKS_CHAT_ID".into(), String::new());
    environment.insert("WORKER_PUBLISH_URL".into(), String::new());
    environment.insert("WORKER_PUBLISH_SECRET".into(), String::new());
    if let Some(logo) = branded {
        environment.insert("BRAND_LOGO_BOTS".into(), "YourEmojiBot".into());
        environment.insert("BRAND_LOGO_PATH".into(), logo.to_str().unwrap().into());
        environment.insert("BRAND_LOGO_KEYWORDS".into(), "logo".into());
    }
    let mut argv = vec![
        commands.executable.as_os_str().to_owned(),
        "build-collection".into(),
        "--base".into(),
        "fixture".into(),
        "--title".into(),
        "Fixture".into(),
        "--user-id".into(),
        "111111111".into(),
        "--data-dir".into(),
        data.as_os_str().to_owned(),
    ];
    if !options.contains(&"--formats") {
        argv.push("--mixed".into());
    }
    if branded.is_some() {
        argv.extend(["--per-set".into(), "2".into()]);
    } else {
        argv.push("--no-brand-logo".into());
    }
    argv.extend(
        options
            .iter()
            .map(|option| std::ffi::OsString::from(*option)),
    );
    commands.capture(&argv, &environment)
}

pub(super) fn run(commands: &Commands<'_>, project: &Path) {
    let directory = commands.root.join("publication-command-art");
    std::fs::create_dir(&directory).unwrap();
    let originals = [
        (
            "numera-emoji-mapper-logo.png",
            "6f85376de3893dfa1e78045ce264103b0055acec400dd8a35222405d5c4f6951",
        ),
        (
            "numera-emoji-mapper-logo-circle.png",
            "8c278a42ab70a8cff3eaec0c7a6ed6cbc2d0ead11791cbadaf0aa8e06df3909c",
        ),
        (
            "numera-coin-emoji-mapper-logo.png",
            "f1ed763b31339e5dbb398abaa8e3870572acf0bed87d512279a84a88a23059a3",
        ),
    ]
    .map(|(name, hash)| {
        let source = project.join("assets").join(name);
        assert_eq!(_native::tooling_manifest::digest(&source).unwrap(), hash);
        (source.clone(), std::fs::read(source).unwrap())
    });
    let images = std::array::from_fn::<_, 3, _>(|index| {
        let path = directory.join(format!("image-{index}.png"));
        _native::codec_image::to_static_png(&originals[index].0, &path).unwrap();
        path
    });
    // Independent original conversion fixture, not an expected hash computed from target output.
    assert_eq!(
        _native::tooling_manifest::digest(&images[0]).unwrap(),
        "6516c02d3e209bb2567b471e198a469a867e4834cc08d20674ca65df3e3182bf"
    );
    let bytes = images.each_ref().map(|path| std::fs::read(path).unwrap());
    assert!(
        bytes
            .iter()
            .all(|bytes| !bytes.is_empty() && bytes.len() < 60 * 1024)
    );
    let identities = images
        .each_ref()
        .map(|path| _native::codec_static_identity::fingerprint(path).unwrap());
    let keys = identities
        .each_ref()
        .map(|identity| identity["content_key"].as_str().unwrap().to_owned());
    assert_eq!(keys.iter().collect::<BTreeSet<_>>().len(), 3);
    ticker_success::run(commands, &bytes, &originals[1].0);
    for variant in ["normal", "branded", "lost", "foreign", "into", "new"] {
        let data = commands.root.join(format!("publication-command-{variant}"));
        let state_path = data.join("publish_fixture.json");
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
        let total = if variant == "into" {
            3
        } else if matches!(variant, "foreign" | "new") {
            2
        } else {
            1
        };
        for index in 0..total {
            assert!(cat.insert(&json!({"content_key":keys[index],"fmt":"static","file_path":images[index],"phash":identities[index]["phash"],"emojis":[if index==0 {"😀"} else {"⭐"}],"keywords":[if index==0 {"public | artwork"} else {"foreign"}]}),UTC).unwrap());
        }
        let mut initial = json!({"base":"fixture","sets":[],"sent":[],"sent_full":[],"skipped":[],"in_flight":null});
        if variant == "foreign" {
            initial["in_flight"] = json!({"key":keys[0],"operation":"create","set_name":FIRST,"set_index":1,"title":"Fixture 1"});
        }
        if matches!(variant, "into" | "new") {
            let mut sets = vec![set(FIRST, 1, &keys[1])];
            let mut sent = vec![FIRST];
            for (index, name, fuid, cid) in [
                (1, FIRST, "old-unique", "222222222"),
                (2, SECOND, "later-unique", "333333333"),
            ] {
                if index == 2 && variant != "into" {
                    continue;
                }
                cat.record_seen(fuid, &keys[index]).unwrap();
                cat.uploaded(&json!({"content_key":keys[index],"custom_emoji_id":cid,"base":"fixture","set_name":name}),UTC).unwrap();
            }
            if variant == "into" {
                sets.push(set(SECOND, 2, &keys[2]));
                sent.push(SECOND);
            }
            initial["sets"] = json!(sets);
            initial["sent"] = json!(sent);
        }
        if variant != "normal" && variant != "branded" && variant != "lost" {
            atomic::write_json(&state_path, &initial).unwrap();
        }
        let before = cat.all(None).unwrap();
        drop(cat);
        let initial_bytes = std::fs::read(&state_path).ok();
        let first = member(
            "new-file",
            "new-unique",
            if variant == "branded" {
                "222222222"
            } else {
                "111111111"
            },
        );
        let item = member("item-file", "item-unique", "111111111");
        let old = member("old-file", "old-unique", "222222222");
        let later = member("later-file", "later-unique", "333333333");
        let foreign = member("foreign-file", "foreign-unique", "222222222");
        let mut replies = vec![me()];
        if variant == "foreign" {
            replies.push(listing(FIRST, std::slice::from_ref(&foreign)));
            download(&mut replies, "foreign-file", &bytes[1]);
        } else {
            let target = if variant == "new" { SECOND } else { FIRST };
            if matches!(variant, "into" | "new") {
                replies.push(listing(FIRST, std::slice::from_ref(&old)));
            }
            if variant == "into" {
                replies.push(listing(FIRST, std::slice::from_ref(&old)));
            }
            let mut upload = mutation(
                &state_path,
                target,
                &keys[0],
                (
                    &bytes[usize::from(variant == "branded")],
                    variant == "branded",
                ),
                variant != "into",
            );
            if variant == "lost" {
                upload.payload = json!([]);
            }
            replies.push(upload);
            if variant == "lost" {
                replies.push(listing(FIRST, std::slice::from_ref(&first)));
                download(&mut replies, "new-file", &bytes[0]);
            }
            let uploaded = if variant == "into" {
                item.clone()
            } else {
                first.clone()
            };
            let mut live = if variant == "into" {
                vec![old.clone(), item.clone()]
            } else {
                vec![first.clone()]
            };
            replies.push(listing(target, &live));
            download(
                &mut replies,
                uploaded["file_id"].as_str().unwrap(),
                &bytes[usize::from(variant == "branded")],
            );
            if variant == "branded" {
                replies.push(listing(FIRST, &live));
                replies.push(mutation(
                    &state_path,
                    FIRST,
                    &keys[0],
                    (&bytes[0], false),
                    false,
                ));
                live.push(item.clone());
                replies.push(listing(FIRST, &live));
                download(&mut replies, "item-file", &bytes[0]);
                replies.push(notification(FIRST, true, &state_path));
            }
            if variant == "new" {
                replies.push(listing(FIRST, std::slice::from_ref(&old)));
            }
            replies.push(listing(target, &live));
            if variant == "into" {
                replies.push(listing(SECOND, std::slice::from_ref(&later)));
            }
            if variant != "into" {
                replies.push(notification(target, false, &state_path));
            }
            if matches!(variant, "normal" | "branded" | "lost") {
                replies.extend([me(), listing(FIRST, &live), listing(FIRST, &live)]);
            }
        }
        let expected = replies
            .iter()
            .map(|reply| (reply.method.clone(), reply.fields.clone()))
            .collect::<Vec<_>>();
        let mut server = Server::new(replies);
        let extra = match variant {
            "into" => vec!["--into-pack", "1"],
            "new" => vec!["--new-set"],
            _ => vec![],
        };
        println!("native collection publication command: {variant}");
        let result = execute(
            commands,
            &data,
            &extra,
            &server,
            if variant == "branded" {
                Some(&originals[1].0)
            } else {
                None
            },
        );
        if variant == "foreign" {
            assert_ne!(result.code, 0);
            assert!(result.stderr.contains("not proven"), "{}", result.stderr);
            assert_eq!(std::fs::read(&state_path).unwrap(), initial_bytes.unwrap());
        } else {
            assert_eq!(
                result.code, 0,
                "{variant}: {} {}",
                result.stdout, result.stderr
            );
            assert!(result.stdout.contains("\nDONE.\n"));
            assert!(result.stdout.ends_with(&format!(
                "  https://t.me/addemoji/{}  [mixed]\n",
                if matches!(variant, "into" | "new") {
                    SECOND
                } else {
                    FIRST
                }
            )));
            if matches!(variant, "normal" | "branded" | "lost") {
                let repeated = execute(
                    commands,
                    &data,
                    &extra,
                    &server,
                    if variant == "branded" {
                        Some(&originals[1].0)
                    } else {
                        None
                    },
                );
                assert_eq!(
                    repeated.code, 0,
                    "{variant} repeat: {} {}",
                    repeated.stdout, repeated.stderr
                );
                assert!(repeated.stdout.contains("\nDONE.\n"));
                assert!(
                    repeated
                        .stdout
                        .ends_with(&format!("  https://t.me/addemoji/{FIRST}  [mixed]\n"))
                );
            }
        }
        let requests = server.finish().unwrap();
        assert_eq!(requests.len(), expected.len());
        for (actual, (method, fields)) in requests.iter().zip(expected) {
            assert_eq!(actual.method, method);
            assert_eq!(actual.form, fields);
        }
        let saved: Value = serde_json::from_slice(&std::fs::read(&state_path).unwrap()).unwrap();
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
        if variant == "foreign" {
            assert_eq!(cat.all(None).unwrap(), before);
            for key in &keys[..2] {
                assert_eq!(
                    cat.dispatch(
                        json!({"operation":"published","base":"fixture","content_key":key})
                    )
                    .unwrap(),
                    false
                );
            }
            assert_eq!(saved["sets"], json!([]));
            assert_eq!(saved["in_flight"]["key"], keys[0]);
            assert!(!requests.iter().any(|request| {
                ["createNewStickerSet", "addStickerToSet", "sendMessage"]
                    .contains(&request.method.as_str())
            }));
        } else {
            assert!(saved["in_flight"].is_null());
            assert_eq!(saved["skipped"], json!([]));
            let target = usize::from(variant == "new");
            assert_eq!(
                saved["sets"][target]["keys"],
                if variant == "into" {
                    json!([keys[1], keys[0]])
                } else {
                    json!([keys[0]])
                }
            );
            assert_eq!(
                saved["sets"][target]["live"],
                if matches!(variant, "into" | "branded") {
                    2
                } else {
                    1
                }
            );
            assert_eq!(saved["sets"][target]["logo"], variant == "branded");
            assert_eq!(
                saved["sent_full"],
                if variant == "branded" {
                    json!([FIRST])
                } else {
                    json!([])
                }
            );
            assert_eq!(
                saved["sent"],
                if matches!(variant, "into" | "new") {
                    json!([FIRST, SECOND])
                } else {
                    json!([FIRST])
                }
            );
            assert_eq!(
                cat.dispatch(
                    json!({"operation":"custom_id","base":"fixture","content_key":keys[0]})
                )
                .unwrap(),
                "111111111"
            );
            if matches!(variant, "into" | "new") {
                assert_eq!(
                    cat.dispatch(
                        json!({"operation":"custom_id","base":"fixture","content_key":keys[1]})
                    )
                    .unwrap(),
                    "222222222"
                );
                let untouched = if variant == "into" { 1 } else { 0 };
                assert_eq!(saved["sets"][untouched], initial["sets"][untouched]);
                if variant == "into" {
                    assert_eq!(
                        cat.dispatch(
                            json!({"operation":"custom_id","base":"fixture","content_key":keys[2]})
                        )
                        .unwrap(),
                        "333333333"
                    );
                    assert_eq!(
                        cat.dispatch(json!({"operation":"published_sets"})).unwrap()[&keys[2]],
                        SECOND
                    );
                }
            }
            let manifest = data.join("manifests").join(format!(
                "{}.md",
                if variant == "new" { SECOND } else { FIRST }
            ));
            let manifest = std::fs::read_to_string(manifest).unwrap();
            assert!(manifest.contains("public \\| artwork"));
            if matches!(variant, "normal" | "branded") {
                let branded = variant == "branded";
                assert!(manifest.contains(if branded {
                    "|  2 emoji\n"
                } else {
                    "|  1 emoji\n"
                }));
                assert!(!manifest.contains(if branded {
                    "|  1 emoji\n"
                } else {
                    "brand logo"
                }));
                if branded {
                    assert!(manifest.contains("| 1 | brand logo | — |"));
                    assert!(manifest.contains("| 2 | public \\| artwork | 111111111 |"));
                } else {
                    assert!(manifest.contains("| 1 | public \\| artwork | 111111111 |"));
                }
            }
            assert!(
                std::fs::read_dir(data.join("tmp/native-codec"))
                    .unwrap()
                    .next()
                    .is_none()
            );
            let mutations = requests
                .iter()
                .filter_map(|request| {
                    request
                        .observation
                        .as_ref()
                        .map(|observed| (request, observed))
                })
                .collect::<Vec<_>>();
            for (request, observed) in mutations {
                if ["createNewStickerSet", "addStickerToSet"].contains(&request.method.as_str()) {
                    let intent = &observed["in_flight"];
                    let creating = request.method == "createNewStickerSet";
                    assert_eq!(intent["key"], keys[0]);
                    assert_eq!(intent["operation"], if creating { "create" } else { "add" });
                    assert_eq!(
                        intent["set_name"],
                        if variant == "new" { SECOND } else { FIRST }
                    );
                    assert_eq!(intent["set_index"], if variant == "new" { 2 } else { 1 });
                    assert_eq!(
                        intent["expected_before"],
                        if creating { Value::Null } else { json!(1) }
                    );
                    assert_eq!(intent["format"], "static");
                    assert!(!intent["started_utc"].as_str().unwrap().is_empty());
                }
            }
        }
        drop(cat);
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.method == "createNewStickerSet")
                .count(),
            usize::from(!matches!(variant, "foreign" | "into"))
        );
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.method == "addStickerToSet")
                .count(),
            usize::from(matches!(variant, "branded" | "into"))
        );
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.method == "sendMessage")
                .count(),
            if variant == "branded" {
                2
            } else {
                usize::from(!matches!(variant, "foreign" | "into"))
            }
        );
        assert!(!_native::process_owner::cleanup_pending(commands.root));
    }
    refusal::run(commands, &images, &keys);
    progress::run(commands, &images, &bytes, &keys);
    validation::run(commands, &images);
    dedup::run(commands, &images, &bytes, &keys);
    preflight::run(commands, &images, &bytes, &keys);
    notify::run(commands);
    order::run(commands, &images, &bytes, &keys);
    brand::run(commands, &images, &bytes, &keys, &originals[1].0);
    retry::run(commands, &images, &bytes);
    capacity::run(commands, &images, &bytes, &keys, &originals[1].0);
    for (source, bytes) in &originals {
        assert_eq!(&std::fs::read(source).unwrap(), bytes);
    }
    for (source, bytes) in images.iter().zip(&bytes) {
        assert_eq!(&std::fs::read(source).unwrap(), bytes);
    }
}
