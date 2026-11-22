use super::{FIRST, Mode, Wire, server, sticker};
use crate::{identity::art, support::*};
use serde_json::json;
use std::sync::{Arc, Mutex};

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
