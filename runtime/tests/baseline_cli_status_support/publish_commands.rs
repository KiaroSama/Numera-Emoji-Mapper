use crate::{
    identity::{art, frozen_art, insert},
    support::{
        http::{Reply, Server},
        *,
    },
};
use numera_runtime as native;
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
const FIRST: &str = "fixture1_by_YourEmojiBot";
const SECOND: &str = "fixture2_by_YourEmojiBot";
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Ordinary,
    Branded,
    Lost,
    Foreign,
    Into,
    New,
    ForeignTail,
    Masked,
}
#[derive(Default)]
struct Wire {
    sets: BTreeMap<String, Vec<Value>>,
    messages: Vec<String>,
    workers: Vec<Value>,
}
fn sticker(file: &str, fuid: &str, cid: &str) -> Value {
    json!({"file_id":file,"file_unique_id":fuid,"custom_emoji_id":cid})
}
fn server(
    f: &Fixture,
    mode: Mode,
    ticker: bool,
    payload: Vec<u8>,
    logo: Vec<u8>,
    wire: Arc<Mutex<Wire>>,
) -> Server {
    let root = f.root.clone();
    let data = f.data();
    let logo_rgba =
        native::codec_image::decoded(&frozen_art("numera-emoji-mapper-logo-circle.png"))
            .unwrap()
            .rgba;
    Server::new(move |r| {
        if r.path == "/publish" {
            wire.lock()
                .unwrap()
                .workers
                .push(serde_json::from_slice(&r.body).unwrap());
            return Reply::json(json!({"ok":true}));
        }
        if r.verb == "GET" {
            return Reply::Bytes(
                200,
                if r.method() == "logo.png" {
                    logo.clone()
                } else {
                    payload.clone()
                },
            );
        }
        let mut w = wire.lock().unwrap();
        match r.method() {
            "getMe" => Reply::ok(json!({"username":"YourEmojiBot"})),
            "getStickerSet" => {
                let name = &r.form()["name"];
                match w.sets.get(name).filter(|v| !v.is_empty()) {
                    Some(live) => Reply::ok(json!({"name":name,"stickers":live})),
                    None => Reply::json(json!({"ok":false,"description":"STICKERSET_INVALID"})),
                }
            }
            "createNewStickerSet" => {
                let name = String::from_utf8(r.field("name")).unwrap();
                assert_eq!(name, if mode == Mode::New { SECOND } else { FIRST });
                assert!(
                    r.body
                        .windows(b"custom_emoji".len())
                        .any(|v| v == b"custom_emoji")
                );
                let file = r.field("file0");
                if mode == Mode::Branded {
                    let im = image::load_from_memory(&file).unwrap();
                    assert_eq!(im.color(), image::ColorType::Rgba8);
                    assert_eq!(im.to_rgba8(), logo_rgba);
                    let folder = if ticker {
                        std::fs::read_dir(root.join("logs"))
                            .unwrap()
                            .map(|e| e.unwrap().path())
                            .find(|p| {
                                p.file_name()
                                    .unwrap()
                                    .to_string_lossy()
                                    .starts_with("ticker-codec-")
                            })
                            .unwrap()
                            .join("brand")
                    } else {
                        data.join("brand")
                    };
                    let cached = std::fs::read_dir(folder)
                        .unwrap()
                        .map(|e| e.unwrap().path())
                        .filter(|p| p.extension().is_some_and(|e| e == "png"))
                        .collect::<Vec<_>>();
                    assert_eq!(cached.len(), 1);
                    assert_eq!(std::fs::read(&cached[0]).unwrap(), file);
                } else {
                    assert_eq!(file, payload);
                }
                if ticker {
                    let intent = &read_json(&root.join("state_fixture.json"))["in_flight"];
                    assert_eq!(intent["operation"], "create");
                    assert_eq!(
                        intent["key"],
                        if mode == Mode::Branded {
                            "__brand_logo__"
                        } else {
                            "fixture"
                        }
                    );
                }
                w.sets.entry(name).or_default().push(sticker(
                    "new-file",
                    "new-unique",
                    if mode == Mode::Branded && !ticker {
                        "222222222"
                    } else {
                        "111111111"
                    },
                ));
                if mode == Mode::Lost {
                    Reply::Disconnect
                } else {
                    Reply::ok(json!(true))
                }
            }
            "addStickerToSet" => {
                assert!(mode == Mode::Branded || mode == Mode::Into);
                let name = String::from_utf8(r.field("name")).unwrap();
                assert_eq!(name, FIRST);
                assert_eq!(r.field("file0"), payload);
                if ticker {
                    let intent = &read_json(&root.join("state_fixture.json"))["in_flight"];
                    assert_eq!(intent["operation"], "add");
                    assert_eq!(intent["expected_before"], 1);
                }
                w.sets.entry(name).or_default().push(sticker(
                    "item-file",
                    "item-unique",
                    if ticker { "222222222" } else { "111111111" },
                ));
                Reply::ok(json!(true))
            }
            "getFile" => {
                let file = &r.form()["file_id"];
                Reply::ok(
                    json!({"file_path":if file.starts_with("foreign-")||file=="old-file"||(mode==Mode::Branded&&file=="new-file"){"logo.png"}else{"fixture.png"}}),
                )
            }
            "sendMessage" => {
                w.messages.push(r.form()["text"].clone());
                Reply::ok(json!({"message_id":1}))
            }
            _ => panic!("unexpected {}", r.method()),
        }
    })
}
fn collection(mode: Mode) {
    let f = Fixture::new();
    let image = art(&f, "numera-emoji-mapper-logo.png", "source.png");
    let payload = std::fs::read(&image).unwrap();
    let logo = art(&f, "numera-emoji-mapper-logo-circle.png", "logo.png");
    let logo_payload = std::fs::read(&logo).unwrap();
    let key = {
        let mut cat = f.catalog();
        insert(
            &mut cat,
            &image,
            json!({"emojis":["😀"],"keywords":["public | artwork"],"sources":["fixture"]}),
        )
    };
    let wire = Arc::new(Mutex::new(Wire::default()));
    let mut old_key = None;
    let mut later_key = None;
    let mut sentinel = Value::Null;
    if mode == Mode::Foreign {
        let mut cat = f.catalog();
        old_key = Some(insert(
            &mut cat,
            &logo,
            json!({"emojis":["⭐"],"keywords":["foreign"]}),
        ));
        wire.lock().unwrap().sets.insert(
            FIRST.into(),
            vec![sticker("foreign-file", "foreign-unique", "222222222")],
        );
        document(
            &f.data().join("publish_fixture.json"),
            &json!({"base":"fixture","sets":[],"sent":[],"sent_full":[],"skipped":[],"in_flight":{"key":key,"operation":"create","set_name":FIRST,"set_index":1,"title":"Fixture 1"}}),
        );
    }
    if mode == Mode::Into || mode == Mode::New {
        let mut cat = f.catalog();
        let old = insert(&mut cat, &logo, json!({"emojis":["⭐"]}));
        cat.record_seen("old-unique", &old).unwrap();
        cat.uploaded(&json!({"content_key":old,"custom_emoji_id":"222222222","base":"fixture","set_name":FIRST}),UTC).unwrap();
        wire.lock().unwrap().sets.insert(
            FIRST.into(),
            vec![sticker("old-file", "old-unique", "222222222")],
        );
        let mut state = json!({"base":"fixture","sets":[{"name":FIRST,"fmt":"mixed","index":1,"keys":[old],"live":1,"logo":false}],"sent":[FIRST],"sent_full":[],"skipped":[],"in_flight":null});
        old_key = Some(old);
        if mode == Mode::Into {
            let later = art(&f, "numera-coin-emoji-mapper-logo.png", "later.png");
            let later = insert(&mut cat, &later, json!({"emojis":["🪙"]}));
            assert_ne!(later, key);
            assert_ne!(Some(&later), old_key.as_ref());
            cat.record_seen("later-unique", &later).unwrap();
            cat.uploaded(&json!({"content_key":later,"custom_emoji_id":"333333333","base":"fixture","set_name":SECOND}),UTC).unwrap();
            wire.lock().unwrap().sets.insert(
                SECOND.into(),
                vec![sticker("later-file", "later-unique", "333333333")],
            );
            sentinel = json!({"name":SECOND,"title":"Fixture 2","fmt":"mixed","index":2,"keys":[later],"live":1,"logo":false});
            state["sets"].as_array_mut().unwrap().push(sentinel.clone());
            state["sent"].as_array_mut().unwrap().push(json!(SECOND));
            later_key = Some(later);
        }
        document(&f.data().join("publish_fixture.json"), &state);
    }
    let bot = server(&f, mode, false, payload, logo_payload, wire.clone());
    let mut env = bot.environment();
    let logo_source = project().join("assets/numera-emoji-mapper-logo-circle.png");
    let mut args = vec![
        "build-collection",
        "--base",
        "fixture",
        "--title",
        "Fixture",
        "--mixed",
        "--user-id",
        "111111111",
    ];
    if mode == Mode::Branded {
        args.extend(["--per-set", "2"]);
        env.extend([
            ("BRAND_LOGO_BOTS", "YourEmojiBot".into()),
            ("BRAND_LOGO_PATH", logo_source.display().to_string()),
        ]);
    } else {
        args.push("--no-brand-logo");
    }
    if mode == Mode::Into {
        args.extend(["--into-pack", "1"]);
    }
    if mode == Mode::New {
        args.push("--new-set");
    }
    if mode == Mode::Foreign {
        let r = f.run(&args, &env);
        assert_ne!(r.code, 0);
        assert!(r.err.contains("not proven"), "{}", r.err);
        let state = read_json(&f.data().join("publish_fixture.json"));
        assert_eq!(state["sets"], json!([]));
        assert_eq!(state["in_flight"]["key"], key);
        let mut cat = f.catalog();
        for k in [&key, old_key.as_ref().unwrap()] {
            assert_eq!(
                cat.dispatch(json!({"operation":"published","base":"fixture","content_key":k}))
                    .unwrap(),
                false
            );
        }
        assert!(
            !bot.methods()
                .iter()
                .any(|m| ["createNewStickerSet", "sendMessage"].contains(&m.as_str()))
        );
        return;
    }
    let repeat = if mode == Mode::Into || mode == Mode::New {
        1
    } else {
        2
    };
    for _ in 0..repeat {
        let r = f.run(&args, &env);
        r.code(0);
        if repeat == 2 {
            assert!(r.out.contains("\nDONE.\n"));
            assert!(
                r.out
                    .ends_with("  https://t.me/addemoji/fixture1_by_YourEmojiBot  [mixed]\n"),
                "{}",
                r.out
            );
        }
    }
    let state = read_json(&f.data().join("publish_fixture.json"));
    assert!(state["in_flight"].is_null());
    let mut cat = f.catalog();
    let cid = |cat: &mut native::catalog::Catalog, key: &str| {
        cat.dispatch(json!({"operation":"custom_id","base":"fixture","content_key":key}))
            .unwrap()
    };
    assert_eq!(cid(&mut cat, &key), "111111111");
    let methods = bot.methods();
    for text in &wire.lock().unwrap().messages {
        assert!(
            text.contains(if mode == Mode::New {
                "Fixture 2"
            } else {
                "Fixture 1"
            }),
            "{text}"
        );
    }
    if mode == Mode::Into || mode == Mode::New {
        let old = old_key.unwrap();
        assert_eq!(
            state["sets"][0]["keys"],
            if mode == Mode::Into {
                json!([old, key])
            } else {
                json!([old])
            }
        );
        assert_eq!(
            state["sets"][0]["live"],
            if mode == Mode::Into { 2 } else { 1 }
        );
        assert_eq!(cid(&mut cat, &old), "222222222");
        if mode == Mode::New {
            assert_eq!(state["sets"][1]["keys"], json!([key]));
            assert_eq!(state["sets"][1]["name"], SECOND);
        } else {
            assert_eq!(state["sets"].as_array().unwrap().len(), 2);
            assert_eq!(state["sets"][1], sentinel);
            let later = later_key.unwrap();
            assert_eq!(cid(&mut cat, &later), "333333333");
            assert_eq!(
                cat.dispatch(json!({"operation":"published_sets"})).unwrap()[later],
                SECOND
            );
            assert_eq!(
                wire.lock().unwrap().sets[SECOND],
                vec![sticker("later-file", "later-unique", "333333333")]
            );
        }
        assert_eq!(
            methods
                .iter()
                .filter(|m| *m == "createNewStickerSet")
                .count(),
            usize::from(mode == Mode::New)
        );
        assert_eq!(
            methods.iter().filter(|m| *m == "addStickerToSet").count(),
            usize::from(mode == Mode::Into)
        );
        assert_eq!(
            wire.lock().unwrap().sets[FIRST][0]["custom_emoji_id"],
            "222222222"
        );
        return;
    }
    assert_eq!(
        methods
            .iter()
            .filter(|m| *m == "createNewStickerSet")
            .count(),
        1
    );
    assert_eq!(
        methods.iter().filter(|m| *m == "addStickerToSet").count(),
        usize::from(mode == Mode::Branded)
    );
    assert_eq!(
        wire.lock().unwrap().messages.len(),
        if mode == Mode::Branded { 2 } else { 1 }
    );
    assert_eq!(state["sets"][0]["keys"], json!([key]));
    assert_eq!(
        state["sets"][0]["live"],
        if mode == Mode::Branded { 2 } else { 1 }
    );
    assert_eq!(state["sets"][0]["logo"], mode == Mode::Branded);
    assert_eq!(
        state["sent_full"],
        if mode == Mode::Branded {
            json!([FIRST])
        } else {
            json!([])
        }
    );
    assert_eq!(state["sent"], json!([FIRST]));
    assert!(
        std::fs::read_to_string(f.data().join(format!("manifests/{FIRST}.md")))
            .unwrap()
            .contains("public \\| artwork")
    );
    let codec = f.data().join("tmp/native-codec");
    if codec.exists() {
        assert_eq!(std::fs::read_dir(codec).unwrap().count(), 0);
    }
}
// tests.test_native_publication.NativePublicationFlow.test_create_confirm_record_manifest_and_repeat_without_upload
#[test]
fn create_confirm_record_manifest_and_repeat() {
    collection(Mode::Ordinary);
}
// tests.test_native_publication.NativePublicationFlow.test_configured_logo_leads_pack_and_full_milestone_announces_once
#[test]
fn configured_logo_leads_and_full_milestone_announces_once() {
    collection(Mode::Branded);
}
// tests.test_native_publication.NativePublicationFlow.test_applied_create_with_lost_response_is_verified_not_retried
#[test]
fn applied_create_lost_response_is_verified_without_retry() {
    collection(Mode::Lost);
}
// tests.test_native_publication.NativePublicationFlow.test_restart_rejects_foreign_unrecorded_create_without_attributing_it
#[test]
fn restart_refuses_foreign_unrecorded_create() {
    collection(Mode::Foreign);
}
// tests.test_native_publication.NativePublicationFlow.test_explicit_into_pack_appends_without_recreating_the_family
#[test]
fn explicit_into_pack_appends_and_preserves_later_pack() {
    collection(Mode::Into);
}
// tests.test_native_publication.NativePublicationFlow.test_explicit_new_set_preserves_the_previous_pack
#[test]
fn explicit_new_set_preserves_previous_pack() {
    collection(Mode::New);
}
fn ticker(mode: Mode, announce: Option<(&str, &str)>) {
    let f = Fixture::new();
    let image = art(&f, "numera-emoji-mapper-logo.png", "source/fixture.png");
    let payload = std::fs::read(image).unwrap();
    let logo = art(&f, "numera-emoji-mapper-logo-circle.png", "logo.png");
    let logo_payload = std::fs::read(logo).unwrap();
    let wire = Arc::new(Mutex::new(Wire::default()));
    let recovery = mode == Mode::ForeignTail || mode == Mode::Masked;
    if recovery {
        let mut live = Vec::new();
        if mode == Mode::ForeignTail {
            live.push(sticker("foreign-file", "foreign-unique", "333333333"));
        }
        live.push(sticker("item-file", "item-unique", "222222222"));
        if mode == Mode::ForeignTail {
            live.push(sticker("foreign-tail", "tail-unique", "444444444"));
        }
        wire.lock().unwrap().sets.insert(FIRST.into(), live);
        document(
            &f.root.join("state_fixture.json"),
            &json!({"base":"fixture","per_set":200,"sets":[{"name":FIRST,"index":1,"count":1,"title":"Fixture 1"}],"done":[],"sent":[FIRST],"in_flight":{"key":"fixture","operation":"add","set_name":FIRST,"set_index":1,"expected_before":1}}),
        );
    }
    let bot = server(&f, mode, true, payload, logo_payload, wire.clone());
    let source = f.root.join("source");
    let token = announce.map_or("GENERAL_BOT_TOKEN", |(token, _)| token);
    let args = [
        "build-pack",
        "--base",
        "fixture",
        "--title",
        "Fixture",
        "--source-dir",
        source.to_str().unwrap(),
        "--token-env",
        token,
        "--user-id",
        "111111111",
    ];
    let mut env = bot.environment();
    env.extend([
        (
            "BRAND_LOGO_BOTS",
            if mode == Mode::Branded {
                "YourEmojiBot"
            } else {
                ""
            }
            .into(),
        ),
        (
            "BRAND_LOGO_PATH",
            project()
                .join("assets/numera-emoji-mapper-logo-circle.png")
                .display()
                .to_string(),
        ),
    ]);
    if announce.is_some() {
        env.extend([
            ("WORKER_PUBLISH_URL", format!("{}/publish", bot.base)),
            ("WORKER_PUBLISH_SECRET", "fixture-secret".into()),
        ]);
    }
    let count = if recovery {
        wire.lock().unwrap().sets[FIRST].len()
    } else if mode == Mode::Branded {
        2
    } else {
        1
    };
    for _ in 0..if announce.is_some() { 1 } else { 2 } {
        let r = f.run(&args, &env);
        r.code(0);
        assert!(
            r.out.contains("\nDONE. 1 total emojis across 1 set(s)."),
            "{}",
            r.out
        );
        assert!(
            r.out
                .ends_with(&format!("  https://t.me/addemoji/{FIRST}  ({count})\n")),
            "{}",
            r.out
        );
    }
    let methods = bot.methods();
    assert_eq!(
        methods
            .iter()
            .filter(|m| *m == "createNewStickerSet")
            .count(),
        usize::from(!recovery)
    );
    assert_eq!(
        methods.iter().filter(|m| *m == "addStickerToSet").count(),
        usize::from(mode == Mode::Branded)
    );
    let w = wire.lock().unwrap();
    if let Some((_, expected)) = announce {
        assert_eq!(w.workers.len(), 1);
        assert_eq!(w.workers[0]["bot"], expected);
        assert!(w.messages.is_empty());
    } else {
        assert_eq!(w.messages.len(), usize::from(!recovery));
    }
    drop(w);
    let state = read_json(&f.root.join("state_fixture.json"));
    assert_eq!(state["done"], json!(["fixture"]));
    assert!(state["in_flight"].is_null());
    assert_eq!(state["sets"][0]["count"], count);
    assert_eq!(state["sent"], json!([FIRST]));
    assert!(!f.data().exists());
    assert!(!std::fs::read_dir(f.root.join("logs")).unwrap().any(|p| {
        p.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("ticker-codec-")
    }));
}
// tests.test_native_ticker_cli.NativeTickerCLI.test_unbranded_create_resume_and_repeat
#[test]
fn ticker_unbranded_create_resume_repeat() {
    ticker(Mode::Ordinary, None);
}
// tests.test_native_ticker_cli.NativeTickerCLI.test_configured_logo_first_and_restart_without_duplicate
#[test]
fn ticker_configured_logo_first_and_restart_without_duplicate() {
    ticker(Mode::Branded, None);
}
// tests.test_native_ticker_cli.NativeTickerCLI.test_interrupted_add_finds_ours_before_a_foreign_tail
#[test]
fn ticker_interrupted_add_finds_ours_before_foreign_tail() {
    ticker(Mode::ForeignTail, None);
}
// tests.test_native_ticker_cli.NativeTickerCLI.test_removal_masking_interrupted_add_does_not_upload_again
#[test]
fn ticker_masked_growth_does_not_upload_again() {
    ticker(Mode::Masked, None);
}
// tests.test_build_pack_announce.TheAnnouncerMatchesTheToken.test_the_coin_token_announces_as_the_coin_bot
#[test]
fn coin_token_announces_as_coin_bot() {
    ticker(Mode::Ordinary, Some(("TELEGRAM_BOT_TOKEN", "coin")));
}
// tests.test_build_pack_announce.TheAnnouncerMatchesTheToken.test_any_other_token_announces_as_the_general_bot
#[test]
fn other_token_announces_as_general_bot() {
    ticker(Mode::Ordinary, Some(("GENERAL_BOT_TOKEN", "general")));
}
// tests.test_native_ticker_cli.NativeTickerCLI.test_dry_run_text_matches_source_without_network_or_state
#[test]
fn ticker_dry_run_exact_text_without_network_or_state() {
    let f = Fixture::new();
    write(
        &f.root.join("source/fixture.png"),
        std::fs::read(project().join("assets/numera-emoji-mapper-logo.png")).unwrap(),
    );
    write(
        &f.root.join("keywords.csv"),
        b"ticker,keywords\nfixture,\"fixture, public art\"\n",
    );
    let source = f.root.join("source");
    let state = f.root.join("resume.json");
    let keywords = f.root.join("keywords.csv");
    let r = f.run(
        &[
            "build-pack",
            "--base",
            "fixture",
            "--title",
            "Fixture",
            "--source-dir",
            source.to_str().unwrap(),
            "--token-env",
            "GENERAL_BOT_TOKEN",
            "--user-id",
            "111111111",
            "--keywords",
            keywords.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
            "--dry-run",
        ],
        &[],
    );
    r.code(0);
    assert_eq!(
        r.out,
        format!(
            "DRY RUN: token-env=GENERAL_BOT_TOKEN  owner_user_id=[set]  source={}  images=1  keywords=1\nDRY RUN: 1 images -> 1 set(s) of up to 200, named fixture1_by_<bot> ...  state=resume.json\n",
            source.display()
        )
    );
    assert!(!state.exists());
}
// tests.test_native_publish_cli.NativePublishCommand.test_corrupt_resume_refuses_before_catalog_migration
#[test]
fn corrupt_collection_resume_refuses_before_catalog_migration() {
    let f = Fixture::new();
    std::fs::create_dir_all(f.data()).unwrap();
    let path = f.data().join("catalog.db");
    Connection::open(&path)
        .unwrap()
        .execute("CREATE TABLE untouched(value TEXT)", [])
        .unwrap();
    let state = f.data().join("publish_fixture.json");
    write(&state, b"{truncated");
    let before = std::fs::read(&path).unwrap();
    let r = f.run(
        &[
            "build-collection",
            "--base",
            "fixture",
            "--title",
            "Fixture",
            "--mixed",
            "--no-brand-logo",
            "--dry-run",
        ],
        &[],
    );
    r.code(4);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read(&state).unwrap(), b"{truncated");
}
// tests.test_native_publish_cli.NativePublishCommand.test_dry_run_preserves_plan_and_uses_current_inclusion_and_family
#[test]
fn collection_dry_run_preserves_plan_and_uses_inclusion_and_family() {
    let f = Fixture::new();
    {
        let mut cat = f.catalog();
        for key in ["a", "b", "c"] {
            cat.insert(&json!({"content_key":key,"fmt":"static","file_path":f.root.join(format!("{key}.png"))}),UTC).unwrap();
        }
        cat.inclusion(&["b".into()]).unwrap();
        cat.uploaded(&json!({"content_key":"a","custom_emoji_id":"111111111","base":"fixture","set_name":"fixture1"}),UTC).unwrap();
    }
    let plan = f.data().join("publish_plan_fixture.json");
    let saved = b"{\"mixed\":[\"a\",\"b\"]}\n";
    write(&plan, saved);
    let r = f.run(
        &[
            "build-collection",
            "--base",
            "fixture",
            "--title",
            "Fixture",
            "--mixed",
            "--no-brand-logo",
            "--dry-run",
        ],
        &[],
    );
    r.code(0);
    assert!(r.out.contains("mixed: 1 emoji -> 1 set(s) of up to 200"));
    assert_eq!(std::fs::read(&plan).unwrap(), saved);
    assert!(!f.data().join("publish_fixture.json").exists());
    let mut cat = f.catalog();
    assert_eq!(
        cat.dispatch(json!({"operation":"published","base":"fixture","content_key":"a"}))
            .unwrap(),
        true
    );
    assert_eq!(
        cat.dispatch(json!({"operation":"published","base":"fixture","content_key":"c"}))
            .unwrap(),
        false
    );
    assert_eq!(
        serde_json::from_slice::<Value>(saved).unwrap(),
        json!({"mixed":["a","b"]})
    );
}
// tests.test_native_publish_cli.NativePublishCommand.test_preflight_reports_missing_file_without_publishing_or_saving_plan
#[test]
fn real_preflight_missing_excluded_accepted_rejected_unknown_no_plan() {
    let f = Fixture::new();
    let source = f.root.join("missing.png");
    {
        let mut cat = f.catalog();
        cat.insert(
            &json!({"content_key":"s:missing","fmt":"static","file_path":source}),
            UTC,
        )
        .unwrap();
    }
    let answer = Arc::new(Mutex::new(
        json!({"ok":true,"result":{"file_id":"fixture-upload"}}),
    ));
    let shared = answer.clone();
    let bot = Server::new(move |r| {
        if r.method() == "getMe" {
            Reply::ok(json!({"username":"YourEmojiBot"}))
        } else {
            assert_eq!(r.method(), "uploadStickerFile");
            Reply::json(shared.lock().unwrap().clone())
        }
    });
    let args = [
        "build-collection",
        "--base",
        "fixture",
        "--title",
        "Fixture",
        "--mixed",
        "--no-brand-logo",
        "--user-id",
        "111111111",
        "--preflight",
    ];
    for phase in ["missing", "excluded", "accepted", "rejected", "unknown"] {
        if phase == "excluded" {
            f.catalog().inclusion(&["s:missing".into()]).unwrap();
        }
        if phase == "accepted" {
            write(
                &source,
                std::fs::read(project().join("assets/numera-emoji-mapper-logo.png")).unwrap(),
            );
            f.catalog().inclusion(&[]).unwrap();
        }
        if phase == "rejected" {
            *answer.lock().unwrap() =
                json!({"ok":false,"description":"Bad Request: wrong file type"});
        }
        if phase == "unknown" {
            *answer.lock().unwrap() = json!({"ok":true});
        }
        bot.observed.lock().unwrap().clear();
        let r = f.run(&args, &bot.environment());
        let (code, expected) = match phase {
            "missing" => (
                4,
                "\nPREFLIGHT: 1 file(s) queued -- 0 accepted, 0 refused, 1 missing, 0 not checked.\n  MISSING s:missing  (missing.png) is not on disk\n\nNothing was published. Fix or deselect these, then publish.\n\n",
            ),
            "excluded" => (
                0,
                "\nPREFLIGHT: 0 file(s) queued -- 0 accepted, 0 refused, 0 missing, 0 not checked.\nNothing was queued, so nothing was validated.\n\n",
            ),
            "accepted" => (
                0,
                "\nPREFLIGHT: 1 file(s) queued -- 1 accepted, 0 refused, 0 missing, 0 not checked.\nAll 1 queued file(s) are acceptable to Telegram.\n\n",
            ),
            "rejected" => (
                4,
                "\nPREFLIGHT: 1 file(s) queued -- 0 accepted, 1 refused, 0 missing, 0 not checked.\n  REFUSED s:missing  (missing.png)\n          uploadStickerFile failed: Bad Request: wrong file type\n\nNothing was published. Fix or deselect these, then publish.\n\n",
            ),
            _ => (
                3,
                "\nPREFLIGHT: 1 file(s) queued -- 0 accepted, 0 refused, 0 missing, 1 not checked.\n  UNCHECKED s:missing  (missing.png)\n            Bot API success has no result\n\nNothing was published. Telegram could not be reached for 1 file(s), so this run proves nothing about them. Re-run the preflight once the connection is back.\n\n",
            ),
        };
        r.code(code);
        assert_eq!(r.out, expected, "{phase}");
        assert_eq!(
            bot.methods(),
            if ["missing", "excluded"].contains(&phase) {
                vec!["getMe"]
            } else {
                vec!["getMe", "uploadStickerFile"]
            }
        );
        assert!(!f.data().join("publish_fixture.json").exists());
        assert!(!f.data().join("publish_plan_fixture.json").exists());
    }
}
