//! Original native ticker success/repeat scenarios reuse the existing command and converted artwork.
use super::super::{Commands, SET, download, listing};
use crate::bot_api::{Reply, Server, rgba_signature};
use _native::atomic;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

fn me() -> Reply {
    Reply::new(
        "getMe",
        &[],
        json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
    )
}
fn member(file: &str, cid: &str) -> Value {
    json!({"file_id":file,"file_unique_id":format!("unique-{file}"),"custom_emoji_id":cid})
}
fn mutation(path: &Path, logo: bool, create: bool, bytes: &[u8]) -> Reply {
    let input = json!({"sticker":"attach://file0","format":"static",
        "emoji_list":[if logo {"✅"} else {"🪙"}],
        "keywords":[if logo {"logo"} else {"fixture"}]});
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
        ("name".into(), SET.into()),
    ]);
    if create {
        reply.fields.insert("title".into(), "T 1".into());
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
        // The caller's converted logo is the independent historical Pillow
        // reference. Retain exact request-time pixel/mode/size proof while
        // permitting the production PNG encoder's different compression bytes.
        let expected_logo = rgba_signature(bytes).unwrap();
        assert_eq!(expected_logo["size"], json!([100, 100]));
        assert_eq!(expected_logo["mode"], "RGBA");
        reply.upload_rgba.insert("file0".into(), expected_logo);
    } else {
        reply.files.insert("file0".into(), bytes.into());
    }
    reply.observation = Some((
        path.into(),
        "/in_flight/key".into(),
        json!(if logo { "__brand_logo__" } else { "fixture" }),
    ));
    reply
}

pub(super) fn run(commands: &Commands<'_>, bytes: &[Vec<u8>; 3], logo_source: &Path) {
    dry_run(commands, &bytes[0]);
    for variant in ["plain", "branded", "foreign-tail", "masked-growth"] {
        let directory = commands.root.join(format!("ticker-source-{variant}"));
        let images = directory.join("source");
        std::fs::create_dir_all(&images).unwrap();
        let source = images.join("fixture.png");
        std::fs::write(&source, &bytes[0]).unwrap();
        let state = directory.join("state.json");
        let ours = member("ticker-ours", "222222222");
        let foreign = member("ticker-foreign", "333333333");
        let tail = member("ticker-tail", "444444444");
        let logo = member("ticker-logo", "111111111");
        let branded = variant == "branded";
        let recovery = matches!(variant, "foreign-tail" | "masked-growth");
        let live = if branded {
            vec![logo.clone(), ours.clone()]
        } else if variant == "foreign-tail" {
            vec![foreign, ours.clone(), tail]
        } else {
            vec![ours.clone()]
        };
        if recovery {
            atomic::write_json(
                &state,
                &json!({"base":"t","per_set":200,
                "sets":[{"name":SET,"title":"T 1","index":1,"count":1}],
                "done":[],"sent":[SET],
                "in_flight":{"key":"fixture","operation":"add","set_name":SET,
                    "set_index":1,"expected_before":1}}),
            )
            .unwrap();
        }
        let mut replies = vec![me()];
        if recovery {
            replies.push(listing(&live));
            download(&mut replies, "ticker-ours", &bytes[0]);
            replies.push(listing(&live));
        } else {
            replies.push(mutation(
                &state,
                branded,
                true,
                &bytes[usize::from(branded)],
            ));
            replies.push(listing(if branded {
                std::slice::from_ref(&logo)
            } else {
                &live
            }));
            download(
                &mut replies,
                if branded {
                    "ticker-logo"
                } else {
                    "ticker-ours"
                },
                &bytes[usize::from(branded)],
            );
            if branded {
                replies.push(listing(std::slice::from_ref(&logo)));
                replies.push(mutation(&state, false, false, &bytes[0]));
            }
            let mut note = Reply::new(
                "sendMessage",
                &[
                    ("chat_id", "111111111"),
                    ("text", "✅ T 1\nhttps://t.me/addemoji/t1_by_YourEmojiBot"),
                    ("disable_web_page_preview", "true"),
                ],
                json!({"ok":true,"result":{"message_id":1}}),
            );
            note.observation = Some((state.clone(), "/in_flight".into(), Value::Null));
            replies.push(note);
        }
        replies.push(me());
        replies.push(listing(&live));
        let expected = replies
            .iter()
            .map(|reply| (reply.method.clone(), reply.fields.clone()))
            .collect::<Vec<_>>();
        let mut server = Server::new(replies);
        let mut environment = commands.environment.clone();
        environment.insert(
            "BRAND_LOGO_BOTS".into(),
            if branded { "YourEmojiBot" } else { "" }.into(),
        );
        environment.insert(
            "BRAND_LOGO_PATH".into(),
            logo_source.to_str().unwrap().into(),
        );
        environment.insert("BRAND_LOGO_KEYWORDS".into(), "logo".into());
        for key in [
            "WORKER_PUBLISH_URL",
            "WORKER_PUBLISH_SECRET",
            "PACK_LINKS_CHAT_ID",
        ] {
            environment.remove(key);
        }
        let command = Commands {
            environment: &environment,
            ..*commands
        };
        println!("original native ticker success and repeat: {variant}");
        let results = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let first = command.call(&images, &state, &server.base, &[]);
            let committed = std::fs::read(&state).unwrap();
            let repeated = command.call(&images, &state, &server.base, &[]);
            (first, repeated, committed)
        }));
        let observed = server.finish();
        let (first, repeated, committed) = match results {
            Ok(value) => value,
            Err(panic) => std::panic::resume_unwind(panic),
        };
        let observed = observed.unwrap();
        assert_eq!(observed.len(), expected.len());
        for (request, (method, fields)) in observed.iter().zip(expected) {
            assert_eq!(request.method, method);
            assert_eq!(request.form, fields);
        }
        for result in [first, repeated] {
            assert_eq!(
                result.code, 0,
                "{variant}: {} {}",
                result.stdout, result.stderr
            );
            assert!(
                result
                    .stdout
                    .contains("\nDONE. 1 total emojis across 1 set(s).")
            );
            assert!(result.stdout.ends_with(&format!(
                "  https://t.me/addemoji/{SET}  ({})\n",
                live.len()
            )));
        }
        assert_eq!(
            std::fs::read(&state).unwrap(),
            committed,
            "repeat changed durable state"
        );
        let saved: Value = serde_json::from_slice(&committed).unwrap();
        assert_eq!(saved["done"], json!(["fixture"]));
        assert!(saved["in_flight"].is_null());
        assert_eq!(saved["sets"][0]["count"], live.len());
        assert_eq!(saved["sent"], json!([SET]));
        for (method, count) in [
            ("createNewStickerSet", usize::from(!recovery)),
            ("addStickerToSet", usize::from(branded)),
            ("sendMessage", usize::from(!recovery)),
        ] {
            assert_eq!(
                observed
                    .iter()
                    .filter(|request| request.method == method)
                    .count(),
                count
            );
        }
        let mutations = observed
            .iter()
            .filter(|request| {
                matches!(
                    request.method.as_str(),
                    "createNewStickerSet" | "addStickerToSet"
                )
            })
            .collect::<Vec<_>>();
        for request in mutations {
            let intent = &request.observation.as_ref().unwrap()["in_flight"];
            assert_eq!(intent["set_name"], SET);
            assert_eq!(
                intent["operation"],
                if request.method == "createNewStickerSet" {
                    "create"
                } else {
                    "add"
                }
            );
            assert_eq!(
                intent["expected_before"],
                usize::from(request.method == "addStickerToSet")
            );
        }
        assert_eq!(std::fs::read(source).unwrap(), bytes[0]);
        assert!(!commands.root.join("collection").exists());
        assert!(!_native::process_owner::cleanup_pending(commands.root));
        assert!(
            !std::fs::read_dir(commands.root.join("logs"))
                .unwrap()
                .any(|entry| entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("ticker-codec-"))
        );
    }
}

fn dry_run(commands: &Commands<'_>, bytes: &[u8]) {
    let directory = commands.root.join("ticker-source-dryrun");
    let images = directory.join("source");
    std::fs::create_dir_all(&images).unwrap();
    std::fs::write(images.join("fixture.png"), bytes).unwrap();
    let keywords = directory.join("keywords.csv");
    let csv = b"ticker,keywords\nfixture,\"fixture, public art\"\n";
    std::fs::write(&keywords, csv).unwrap();
    let state = directory.join("resume.json");
    let mut server = Server::new(vec![]);
    let result = commands.call(
        &images,
        &state,
        &server.base,
        &["--keywords", keywords.to_str().unwrap(), "--dry-run"],
    );
    let requests = server.finish().unwrap();
    assert!(requests.is_empty());
    assert_eq!(result.code, 0, "{} {}", result.stdout, result.stderr);
    assert_eq!(
        result.stdout,
        format!(
            "DRY RUN: token-env=GENERAL_BOT_TOKEN  owner_user_id=[set]  source={}  images=1  keywords=1\nDRY RUN: 1 images -> 1 set(s) of up to 200, named t1_by_<bot> ...  state=resume.json\n",
            images.display()
        )
    );
    assert!(!state.exists());
    assert_eq!(std::fs::read(images.join("fixture.png")).unwrap(), bytes);
    assert_eq!(std::fs::read(keywords).unwrap(), csv);
}
