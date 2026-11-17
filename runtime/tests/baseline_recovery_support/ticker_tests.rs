use super::run_with_factories;
use crate::baseline_recovery_support::*;
use crate::{
    atomic, collector,
    logging::RunLog,
    telegram::{ApiError, Telegram},
    ticker_state,
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
struct Ticker {
    root: Scratch,
    src: PathBuf,
    state: PathBuf,
}
impl Ticker {
    fn new(skipped: bool, n: u8) -> Self {
        let root = Scratch::new();
        let src = root.0.join("src");
        for i in 0..n {
            let mut c = [0, 0, 0, 255];
            c[usize::from(i) % 3] = 200;
            png(&src.join(format!("{}.png", char::from(b'a' + i))), c);
        }
        if skipped {
            std::fs::write(src.join("a.png"), b"").unwrap();
        }
        let state = root.0.join("state.json");
        Self { root, src, state }
    }
    fn initial(&self, cap: u64) -> Value {
        ticker_state::initial("t", cap)
    }
    fn save(&self, state: Value) {
        atomic::write_json(&self.state, &state).unwrap();
    }
    fn read(&self) -> Value {
        read(&self.state)
    }
    fn model(&self) -> Model {
        let mut model = Model::ordinary();
        model.username = Some("bot".into());
        model
    }
    fn set(&self, count: usize, matches: &[Option<bool>]) -> Model {
        let mut m = self.model();
        let mut stickers = vec![];
        for i in 0..count {
            let id = format!("f{i}");
            stickers.push(sticker(&id, &format!("cid{i}")));
            m.bodies.insert(
                id,
                match matches.get(i).copied().flatten() {
                    Some(true) => std::fs::read(self.src.join("b.png")).unwrap(),
                    Some(false) => {
                        let p = png(
                            &self.root.0.join(format!("foreign-{i}.png")),
                            [0, 0, 255, 255],
                        );
                        std::fs::read(p).unwrap()
                    }
                    None => b"undecodable".to_vec(),
                },
            );
        }
        m.sets.insert("t1_by_bot".into(), stickers);
        m
    }
    fn args(&self, extra: &[&str]) -> Vec<String> {
        let mut a = vec![
            "--base".into(),
            "t".into(),
            "--title".into(),
            "T".into(),
            "--user-id".into(),
            "1".into(),
            "--source-dir".into(),
            self.src.to_str().unwrap().into(),
            "--token-env".into(),
            "FAKE_TOKEN".into(),
            "--state".into(),
            self.state.to_str().unwrap().into(),
        ];
        a.extend(extra.iter().map(|s| s.to_string()));
        a
    }
    fn run(
        &self,
        bot: &Bot,
        extra: &[&str],
        fault: Option<&str>,
        intents: Arc<Mutex<Vec<Value>>>,
    ) -> Result<u8, String> {
        let cfg = config(&self.root.0, &[]);
        let log = RunLog::open(&cfg.root, "baseline-recovery", cfg.secret_values());
        let state = self.state.clone();
        let model = bot.model.clone();
        let fault = fault.map(str::to_owned);
        run_with_factories(
            &cfg,
            &self.args(extra),
            &log,
            |_, _| {
                let tg = Telegram::new("test-only-token".into(), bot.base.clone())
                    .map_err(|e| e.to_string())?;
                Ok(tg.with_call_fault(Arc::new(move |method, data, files| {
                    if ["createNewStickerSet", "addStickerToSet"].contains(&method) {
                        let intent = read(&state)["in_flight"].clone();
                        assert!(
                            !intent.is_null(),
                            "mutation reached wire before durable intent"
                        );
                        intents.lock().unwrap().push(intent);
                    }
                    let injecting = match fault.as_deref() {
                        Some(
                            "foreign-create" | "unknown-create" | "own-create" | "missing-create",
                        ) => method == "createNewStickerSet",
                        Some("ambiguous-add") => method == "addStickerToSet",
                        _ => false,
                    };
                    if injecting {
                        let mut m = model.lock().unwrap();
                        m.requests.push(server::Request {
                            path: format!("fault/{method}"),
                            form: data.clone(),
                            files: files
                                .iter()
                                .map(|f| (f.field.clone(), (f.filename.clone(), f.bytes.clone())))
                                .collect(),
                            authorization: String::new(),
                            raw: vec![],
                        });
                        if method == "createNewStickerSet"
                            && fault.as_deref() != Some("missing-create")
                        {
                            let id = "ambiguous";
                            m.sets
                                .insert(data["name"].clone(), vec![sticker(id, "created-cid")]);
                            let bytes = if fault.as_deref() == Some("own-create") {
                                files[0].bytes.clone()
                            } else if fault.as_deref() == Some("unknown-create") {
                                b"unreadable".to_vec()
                            } else {
                                let mut out = std::io::Cursor::new(Vec::new());
                                image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                                    100,
                                    100,
                                    image::Rgba([90, 90, 90, 255]),
                                ))
                                .write_to(&mut out, image::ImageFormat::Png)
                                .unwrap();
                                out.into_inner()
                            };
                            m.bodies.insert(id.into(), bytes);
                        }
                        return Some(ApiError::Ambiguous("timed out after apply".into()));
                    }
                    None
                })))
            },
            |_, _| Ok(Images),
        )
    }
    fn ordinary(&self, bot: &Bot, extra: &[&str]) -> u8 {
        self.run(bot, extra, None, Arc::default()).unwrap()
    }
}
fn set(count: u64, index: u64) -> Value {
    json!({"name":format!("t{index}_by_bot"),"title":format!("T {index}"),"count":count,"index":index})
}
fn intent(key: &str, operation: &str, index: u64, before: u64) -> Value {
    json!({"key":key,"operation":operation,"set_name":format!("t{index}_by_bot"),"set_index":index,"expected_before":before,"title":format!("T {index}")})
}
fn resume_state(f: &Ticker, count: u64, pending: Value) {
    let mut s = f.initial(200);
    s["sets"] = json!([set(count, 1)]);
    s["in_flight"] = pending;
    f.save(s);
}
fn mutations(bot: &Bot) -> Vec<String> {
    bot.edit(|m| {
        m.requests
            .iter()
            .filter(|r| {
                r.path.ends_with("createNewStickerSet") || r.path.ends_with("addStickerToSet")
            })
            .map(|r| r.files.values().next().unwrap().0.clone())
            .collect()
    })
}
fn usage(f: &Ticker, extra: &[&str]) -> u8 {
    use std::{
        process::{Command, Stdio},
        time::Duration,
    };
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    std::fs::create_dir(f.root.0.join("assets")).unwrap();
    std::fs::copy(
        project.join("assets/panel.html"),
        f.root.0.join("assets/panel.html"),
    )
    .unwrap();
    let name = if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    };
    let test_executable = std::env::current_exe().unwrap();
    let source = test_executable
        .ancestors()
        .skip(1)
        .map(|p| p.join(name))
        .find(|p| p.is_file())
        .expect("Cargo must build the actual application beside this test profile");
    let binary = f.root.0.join(name);
    if std::fs::hard_link(&source, &binary).is_err() {
        std::fs::copy(&source, &binary).unwrap();
    }
    let workspace = f.root.0.join("usage-process");
    std::fs::create_dir(&workspace).unwrap();
    let mut cmd = Command::new(binary);
    cmd.arg("build-pack")
        .args(f.args(extra))
        .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
        .env("FAKE_TOKEN", "test-only-token")
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(workspace.join("stdout")).unwrap())
        .stderr(std::fs::File::create(workspace.join("stderr")).unwrap());
    let mut child =
        crate::process_owner::OwnedProcess::spawn_session(&mut cmd, workspace.clone()).unwrap();
    let status = runtime()
        .block_on(child.wait(Duration::from_secs(15), Duration::from_secs(10)))
        .unwrap();
    let out = std::fs::read_to_string(workspace.join("stdout")).unwrap();
    let err = std::fs::read_to_string(workspace.join("stderr")).unwrap();
    child.finish().unwrap();
    assert!(
        out.contains("title and cap1..200 required")
            || err.contains("title and cap1..200 required"),
        "actual usage boundary was not reached: {out} {err}"
    );
    assert!(!f.state.exists(), "usage refusal wrote state");
    u8::try_from(status.code().unwrap()).unwrap()
}
fn add_media(f: &Ticker) -> u8 {
    let cfg = config(&f.root.0, &[]);
    let log = RunLog::open(&cfg.root, "recovery-add-media", vec![]);
    crate::local_cli::run_with_codec(
        &cfg,
        &[
            "--in".into(),
            f.src.to_str().unwrap().into(),
            "--data-dir".into(),
            f.root.0.join("data").to_str().unwrap().into(),
        ],
        &log,
        |_| Ok(Images),
    )
    .unwrap()
}

// Original: tests.test_resume_safety.AddMediaExitCode.test_all_inputs_unusable_exits_failed
#[test]
fn addmediaexitcode_test_all_inputs_unusable_exits_failed() {
    let f = Ticker::new(false, 0);
    std::fs::create_dir(&f.src).unwrap();
    std::fs::write(f.src.join("broken.png"), "not a png").unwrap();
    std::fs::write(f.src.join("also.webp"), "garbage").unwrap();
    assert_eq!(add_media(&f), 4);
}

// Original: tests.test_resume_safety.AddMediaExitCode.test_all_inputs_usable_exits_ok
#[test]
fn addmediaexitcode_test_all_inputs_usable_exits_ok() {
    let f = Ticker::new(false, 0);
    std::fs::create_dir(&f.src).unwrap();
    png(&f.src.join("good.png"), [0, 180, 90, 255]);
    assert_eq!(add_media(&f), 0);
}

// Original: tests.test_resume_safety.AddMediaExitCode.test_some_inputs_usable_exits_partial
#[test]
fn addmediaexitcode_test_some_inputs_usable_exits_partial() {
    let f = Ticker::new(false, 0);
    std::fs::create_dir(&f.src).unwrap();
    std::fs::write(f.src.join("broken.png"), "not a png").unwrap();
    png(&f.src.join("good.png"), [0, 180, 90, 255]);
    assert_eq!(add_media(&f), 3);
}

// Original: tests.test_resume_safety.AtomicJsonWrite.test_overwrites_in_place
#[test]
fn atomicjsonwrite_test_overwrites_in_place() {
    let s = Scratch::new();
    let p = s.0.join("state.json");
    atomic::write_json(&p, &json!({"n":1})).unwrap();
    atomic::write_json(&p, &json!({"n":2})).unwrap();
    assert_eq!(read(&p), json!({"n":2}));
}

// Original: tests.test_resume_safety.AtomicJsonWrite.test_previous_content_survives_a_failed_write
#[test]
fn atomicjsonwrite_test_previous_content_survives_a_failed_write() {
    struct Bad;
    impl serde::Serialize for Bad {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("not JSON serializable"))
        }
    }
    let s = Scratch::new();
    let p = s.0.join("state.json");
    atomic::write_json(&p, &json!({"done":["a"]})).unwrap();
    let e = atomic::write_json(&p, &Bad).unwrap_err();
    assert!(e.to_string().contains("not JSON serializable"));
    assert_eq!(read(&p), json!({"done":["a"]}));
    assert_eq!(files(&s.0), vec![p]);
}

// Original: tests.test_resume_safety.AtomicJsonWrite.test_writes_and_leaves_no_temp_file
#[test]
fn atomicjsonwrite_test_writes_and_leaves_no_temp_file() {
    let s = Scratch::new();
    let p = s.0.join("state.json");
    atomic::write_json(&p, &json!({"done":["a","b"]})).unwrap();
    assert_eq!(read(&p), json!({"done":["a","b"]}));
    assert_eq!(files(&s.0), vec![p]);
}

// Original: tests.test_resume_safety.ExitCodes.test_clean_run_is_zero
#[test]
fn exitcodes_test_clean_run_is_zero() {
    assert_eq!(collector::ingest_exit_code(5, 0), 0);
}

// Original: tests.test_resume_safety.ExitCodes.test_codes_are_distinct
#[test]
fn exitcodes_test_codes_are_distinct() {
    let actual = [
        collector::ingest_exit_code(5, 0),
        collector::ingest_exit_code(3, 2),
        collector::ingest_exit_code(0, 4),
        usage(&Ticker::new(false, 1), &["--per-set", "0", "--dry-run"]),
    ]
    .into_iter()
    .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(actual.len(), 4);
}

// Original: tests.test_resume_safety.ExitCodes.test_empty_run_is_not_a_failure
#[test]
fn exitcodes_test_empty_run_is_not_a_failure() {
    assert_eq!(collector::ingest_exit_code(0, 0), 0);
}

// Original: tests.test_resume_safety.ExitCodes.test_partial_run_is_retryable
#[test]
fn exitcodes_test_partial_run_is_retryable() {
    assert_eq!(collector::ingest_exit_code(3, 2), 3);
}

// Original: tests.test_resume_safety.ExitCodes.test_total_failure_is_terminal
#[test]
fn exitcodes_test_total_failure_is_terminal() {
    assert_eq!(collector::ingest_exit_code(0, 4), 4);
}

// Original: tests.test_resume_safety.LinksDestination.test_at_username_is_passed_through
#[test]
fn linksdestination_test_at_username_is_passed_through() {
    let s = Scratch::new();
    let cfg = config(&s.0, &[("PACK_LINKS_CHAT_ID", "@packlinks")]);
    assert_eq!(
        crate::announce::links_chat_id(&cfg, 111111111).unwrap(),
        "@packlinks"
    );
}

// Original: tests.test_resume_safety.LinksDestination.test_every_publisher_resolves_the_same_destination
#[test]
fn linksdestination_test_every_publisher_resolves_the_same_destination() {
    for source in [
        include_str!("../../src/ticker_build.rs"),
        include_str!("../../src/collection_notify.rs"),
        include_str!("../../src/coin_rebuild_links.rs"),
    ] {
        let calls = order::call_paths(source);
        assert!(calls.contains(&vec!["crate".into(), "announce".into(), "packs".into()]));
        assert!(
            !calls
                .iter()
                .any(|p| p.last().is_some_and(|n| n == "links_chat_id"))
        );
    }
    let s = Scratch::new();
    let bot = Bot::new(Model::ordinary());
    let cfg = config(&s.0, &[("PACK_LINKS_CHAT_ID", "@packlinks")]);
    assert_eq!(
        crate::announce::packs(
            &cfg,
            &bot.telegram(),
            111111111,
            &[json!({"name":"t1_by_bot","title":"T"})],
            "general",
            "",
            "cards"
        )
        .unwrap(),
        "@packlinks"
    );
    assert_eq!(
        bot.edit(|m| m.messages[0]["chat_id"].clone()),
        json!("@packlinks")
    );
}

// Original: tests.test_resume_safety.LinksDestination.test_numeric_channel_id_is_used_as_an_int
#[test]
fn linksdestination_test_numeric_channel_id_is_used_as_an_int() {
    let s = Scratch::new();
    let cfg = config(&s.0, &[("PACK_LINKS_CHAT_ID", "-1001111111111")]);
    assert_eq!(
        crate::announce::links_chat_id(&cfg, 111111111).unwrap(),
        "-1001111111111"
    );
}

// Original: tests.test_resume_safety.LinksDestination.test_surrounding_whitespace_is_tolerated
#[test]
fn linksdestination_test_surrounding_whitespace_is_tolerated() {
    let s = Scratch::new();
    let cfg = config(&s.0, &[("PACK_LINKS_CHAT_ID", "  -1001111111111  ")]);
    assert_eq!(
        crate::announce::links_chat_id(&cfg, 111111111).unwrap(),
        "-1001111111111"
    );
}

// Original: tests.test_resume_safety.LinksDestination.test_unset_falls_back_to_the_owner
#[test]
fn linksdestination_test_unset_falls_back_to_the_owner() {
    let s = Scratch::new();
    let cfg = config(&s.0, &[("PACK_LINKS_CHAT_ID", "")]);
    assert_eq!(
        crate::announce::links_chat_id(&cfg, 111111111).unwrap(),
        "111111111"
    );
}

// Original: tests.test_resume_safety.PerSetLimit.test_above_the_cap_is_rejected
#[test]
fn persetlimit_test_above_the_cap_is_rejected() {
    let f = Ticker::new(false, 1);
    let bot = Bot::new(f.model());
    assert!(
        f.run(
            &bot,
            &["--per-set", "400", "--dry-run"],
            None,
            Arc::default()
        )
        .unwrap_err()
        .contains("cap1..200")
    );
    assert!(bot.edit(|m| m.requests.is_empty()));
}

// Original: tests.test_resume_safety.PerSetLimit.test_default_matches_telegram_cap
#[test]
fn persetlimit_test_default_matches_telegram_cap() {
    let f = Ticker::new(false, 1);
    let bot = Bot::new(f.model());
    let code = f.ordinary(&bot, &[]);
    assert_eq!(code, 0);
    assert_eq!(f.read()["per_set"], 200);
    assert!(
        ticker_state::validate(&json!({"base":"t","done":[],"sets":[set(200,1)]}), "t", 200)
            .is_ok()
    );
    assert!(
        ticker_state::validate(&json!({"base":"t","done":[],"sets":[set(201,1)]}), "t", 200)
            .is_err()
    );
}

// Original: tests.test_resume_safety.PerSetLimit.test_negative_start_is_rejected
#[test]
fn persetlimit_test_negative_start_is_rejected() {
    let f = Ticker::new(false, 1);
    let bot = Bot::new(f.model());
    assert!(
        f.run(&bot, &["--start", "-5", "--dry-run"], None, Arc::default())
            .unwrap_err()
            .contains("invalid start")
    );
    assert!(bot.edit(|m| m.requests.is_empty()));
}

// Original: tests.test_resume_safety.PerSetLimit.test_valid_run_succeeds
#[test]
fn persetlimit_test_valid_run_succeeds() {
    let f = Ticker::new(false, 1);
    let bot = Bot::new(f.model());
    assert_eq!(f.ordinary(&bot, &["--per-set", "200", "--dry-run"]), 0);
    assert!(bot.edit(|m| m.requests.is_empty()));
}

// Original: tests.test_resume_safety.PerSetLimit.test_zero_is_rejected_instead_of_dividing_by_zero
#[test]
fn persetlimit_test_zero_is_rejected_instead_of_dividing_by_zero() {
    let f = Ticker::new(false, 1);
    let bot = Bot::new(f.model());
    assert!(
        f.run(&bot, &["--per-set", "0", "--dry-run"], None, Arc::default())
            .unwrap_err()
            .contains("cap1..200")
    );
    assert!(bot.edit(|m| m.requests.is_empty()));
}

// Original: tests.test_resume_safety.ResumeAfterSkippedImage.test_a_foreign_tail_sticker_does_not_resolve_our_in_flight_add
#[test]
fn resumeafterskippedimage_test_a_foreign_tail_sticker_does_not_resolve_our_in_flight_add() {
    let f = Ticker::new(true, 3);
    resume_state(&f, 0, intent("b", "add", 1, 0));
    let bot = Bot::new(f.set(1, &[Some(false)]));
    assert_eq!(f.ordinary(&bot, &[]), 4);
    assert!(!f.read()["done"].as_array().unwrap().contains(&json!("b")));
    assert!(mutations(&bot).is_empty());
}

// Original: tests.test_resume_safety.ResumeAfterSkippedImage.test_a_removal_masking_our_add_does_not_re_upload_a_live_image
#[test]
fn resumeafterskippedimage_test_a_removal_masking_our_add_does_not_re_upload_a_live_image() {
    let f = Ticker::new(true, 3);
    resume_state(&f, 2, intent("b", "add", 1, 2));
    let bot = Bot::new(f.set(2, &[Some(false), Some(true)]));
    assert_ne!(f.ordinary(&bot, &[]), 4);
    assert!(!mutations(&bot).iter().any(|s| s == "b.png"));
    assert!(f.read()["done"].as_array().unwrap().contains(&json!("b")));
    assert!(f.read()["in_flight"].is_null());
}

// Original: tests.test_resume_safety.ResumeAfterSkippedImage.test_corrupt_state_does_not_restart_from_zero
#[test]
fn resumeafterskippedimage_test_corrupt_state_does_not_restart_from_zero() {
    let f = Ticker::new(true, 3);
    let bytes = b"{\"base\": \"t\", \"done\": [trunca";
    std::fs::write(&f.state, bytes).unwrap();
    let bot = Bot::new(f.model());
    assert_eq!(f.ordinary(&bot, &[]), 4);
    assert!(mutations(&bot).is_empty());
    assert_eq!(std::fs::read(&f.state).unwrap(), bytes);
}

// Original: tests.test_resume_safety.ResumeAfterSkippedImage.test_in_flight_record_attributes_the_upload_exactly
#[test]
fn resumeafterskippedimage_test_in_flight_record_attributes_the_upload_exactly() {
    let f = Ticker::new(true, 3);
    resume_state(&f, 0, json!("b"));
    let bot = Bot::new(f.set(1, &[Some(true)]));
    assert_eq!(f.ordinary(&bot, &[]), 3);
    let uploads = mutations(&bot);
    assert!(!uploads.iter().any(|s| s == "b.png"));
    assert!(uploads.iter().any(|s| s == "c.png"));
    assert!(f.read()["done"].as_array().unwrap().contains(&json!("b")));
    assert!(f.read()["in_flight"].is_null());
}

// Original: tests.test_resume_safety.ResumeAfterSkippedImage.test_intent_is_recorded_before_each_upload
#[test]
fn resumeafterskippedimage_test_intent_is_recorded_before_each_upload() {
    let f = Ticker::new(true, 3);
    let bot = Bot::new(f.model());
    let observed = Arc::new(Mutex::new(vec![]));
    assert_eq!(f.run(&bot, &[], None, observed.clone()).unwrap(), 3);
    let seen = observed.lock().unwrap();
    assert_eq!(
        seen.iter()
            .map(|v| v["key"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["b", "c"]
    );
    assert_eq!(
        seen.iter()
            .map(|v| v["operation"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["create", "add"]
    );
    assert!(
        seen.iter()
            .all(|v| v["set_name"].as_str().is_some_and(|s| !s.is_empty()))
    );
}

// Original: tests.test_resume_safety.ResumeAfterSkippedImage.test_ours_behind_a_foreign_sticker_is_reconciled_not_stranded
#[test]
fn resumeafterskippedimage_test_ours_behind_a_foreign_sticker_is_reconciled_not_stranded() {
    let f = Ticker::new(true, 3);
    resume_state(&f, 0, intent("b", "add", 1, 0));
    let bot = Bot::new(f.set(2, &[Some(false), Some(true)]));
    assert_ne!(f.ordinary(&bot, &[]), 4);
    let saved = f.read();
    assert!(saved["done"].as_array().unwrap().contains(&json!("b")));
    assert!(saved["sets"][0]["count"].as_u64().unwrap() >= 2);
    assert!(!mutations(&bot).iter().any(|s| s == "b.png"));
}

// Original: tests.test_resume_safety.ResumeAfterSkippedImage.test_unexplained_drift_refuses_to_guess
#[test]
fn resumeafterskippedimage_test_unexplained_drift_refuses_to_guess() {
    let f = Ticker::new(true, 3);
    resume_state(&f, 0, Value::Null);
    let bot = Bot::new(f.set(2, &[Some(true), Some(true)]));
    assert_eq!(f.ordinary(&bot, &[]), 4);
    assert!(mutations(&bot).is_empty());
}

// Original: tests.test_resume_safety.SafeConfigParsing.test_absent_and_blank_use_the_default
#[test]
fn safeconfigparsing_test_absent_and_blank_use_the_default() {
    let s = Scratch::new();
    for cfg in [config(&s.0, &[]), config(&s.0, &[("X_TEST_NUM", "   ")])] {
        assert_eq!(cfg.integer("X_TEST_NUM", 7, i64::MIN, i64::MAX), 7);
    }
}

// Original: tests.test_resume_safety.SafeConfigParsing.test_garbage_falls_back_instead_of_raising
#[test]
fn safeconfigparsing_test_garbage_falls_back_instead_of_raising() {
    let s = Scratch::new();
    assert_eq!(
        config(&s.0, &[("X_TEST_NUM", "not-a-number")]).integer(
            "X_TEST_NUM",
            7,
            i64::MIN,
            i64::MAX
        ),
        7
    );
}

// Original: tests.test_resume_safety.SafeConfigParsing.test_valid_value
#[test]
fn safeconfigparsing_test_valid_value() {
    let s = Scratch::new();
    assert_eq!(
        config(&s.0, &[("X_TEST_NUM", "42")]).integer("X_TEST_NUM", 7, i64::MIN, i64::MAX),
        42
    );
}

// Original: tests.test_resume_safety.SafeConfigParsing.test_values_are_clamped
#[test]
fn safeconfigparsing_test_values_are_clamped() {
    let s = Scratch::new();
    assert_eq!(
        config(&s.0, &[("X_TEST_NUM", "999")]).integer("X_TEST_NUM", 1, i64::MIN, 64),
        64
    );
    assert_eq!(
        config(&s.0, &[("X_TEST_NUM", "-5")]).integer("X_TEST_NUM", 1, 0, i64::MAX),
        0
    );
}

// Original: tests.test_resume_safety.StateShapeValidation.test_a_sound_state_passes
#[test]
fn stateshapevalidation_test_a_sound_state_passes() {
    let s = json!({"base":"t","per_set":200,"done":["a"],"sets":[set(3,1)],"in_flight":null});
    ticker_state::validate(&s, "t", 200).unwrap();
}

// Original: tests.test_resume_safety.StateShapeValidation.test_done_must_hold_strings
#[test]
fn stateshapevalidation_test_done_must_hold_strings() {
    let s = json!({"base":"t","per_set":200,"done":[1,2],"sets":[set(3,1)],"in_flight":null});
    assert!(
        ticker_state::validate(&s, "t", 200)
            .unwrap_err()
            .contains("list of item keys")
    );
}

// Original: tests.test_resume_safety.StateShapeValidation.test_duplicate_and_backwards_indexes_are_rejected
#[test]
fn stateshapevalidation_test_duplicate_and_backwards_indexes_are_rejected() {
    for indices in [[1, 1], [2, 1]] {
        let s = json!({"base":"t","per_set":200,"done":["a"],"sets":[set(1,indices[0]),set(1,indices[1])],"in_flight":null});
        assert!(
            ticker_state::validate(&s, "t", 200)
                .unwrap_err()
                .contains("repeat or go backwards")
        );
    }
}

// Original: tests.test_resume_safety.StateShapeValidation.test_malformed_intent_is_rejected
#[test]
fn stateshapevalidation_test_malformed_intent_is_rejected() {
    for bad in [
        json!({"key":"a"}),
        json!({"key":"a","operation":"add"}),
        json!({"key":"a","operation":"wat","set_name":"s"}),
    ] {
        let s = json!({"base":"t","per_set":200,"done":["a"],"sets":[set(3,1)],"in_flight":bad});
        assert!(ticker_state::validate(&s, "t", 200).is_err());
    }
}

// Original: tests.test_resume_safety.StateShapeValidation.test_negative_and_oversized_counts_are_rejected
#[test]
fn stateshapevalidation_test_negative_and_oversized_counts_are_rejected() {
    for bad in [-1, 201] {
        let s = json!({"base":"t","per_set":200,"done":["a"],"sets":[{"name":"n","title":"","count":bad,"index":1}],"in_flight":null});
        assert!(
            ticker_state::validate(&s, "t", 200)
                .unwrap_err()
                .contains("count outside")
        );
    }
}

// Original: tests.test_resume_safety.StateShapeValidation.test_wrong_base_is_rejected
#[test]
fn stateshapevalidation_test_wrong_base_is_rejected() {
    let s = json!({"base":"other","per_set":200,"done":["a"],"sets":[set(3,1)],"in_flight":null});
    assert!(
        ticker_state::validate(&s, "t", 200)
            .unwrap_err()
            .contains("another base")
    );
}

fn later() -> Ticker {
    let f = Ticker::new(false, 2);
    let mut s = f.initial(1);
    s["done"] = json!(["a"]);
    s["sets"] = json!([set(1, 1)]);
    s["in_flight"] = intent("b", "create", 2, 0);
    f.save(s);
    f
}
fn later_model(f: &Ticker) -> Model {
    let mut m = f.model();
    m.sets
        .insert("t1_by_bot".into(), vec![sticker("f1", "cid1")]);
    m.bodies
        .insert("f1".into(), std::fs::read(f.src.join("a.png")).unwrap());
    m
}
fn recorded() -> Ticker {
    let f = Ticker::new(false, 1);
    let mut s = f.initial(200);
    s["done"] = json!(["x"]);
    s["sets"] = json!([set(5, 1)]);
    f.save(s);
    f
}

// Original: tests.test_unresolved_mutation.AmbiguousCreateForLaterSets.test_a_create_target_holding_EXTRA_stickers_is_never_adopted
#[test]
fn ambiguouscreateforlatersets_test_a_create_target_holding_extra_stickers_is_never_adopted() {
    let f = later();
    let mut m = later_model(&f);
    m.sets.insert(
        "t2_by_bot".into(),
        vec![sticker("f2", "cid2"), sticker("extra", "cid-extra")],
    );
    m.bodies
        .insert("f2".into(), std::fs::read(f.src.join("b.png")).unwrap());
    let bot = Bot::new(m);
    assert_eq!(f.ordinary(&bot, &["--per-set", "1"]), 4);
    assert!(
        !f.read()["sets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "t2_by_bot")
    );
    assert!(!f.read()["done"].as_array().unwrap().contains(&json!("b")));
    assert!(mutations(&bot).is_empty());
}

// Original: tests.test_unresolved_mutation.AmbiguousCreateForLaterSets.test_a_same_named_set_holding_a_FOREIGN_image_is_never_adopted
#[test]
fn ambiguouscreateforlatersets_test_a_same_named_set_holding_a_foreign_image_is_never_adopted() {
    let f = later();
    let mut m = later_model(&f);
    m.sets
        .insert("t2_by_bot".into(), vec![sticker("foreign", "cid2")]);
    m.bodies.insert(
        "foreign".into(),
        std::fs::read(f.src.join("a.png")).unwrap(),
    );
    let bot = Bot::new(m);
    assert_eq!(f.ordinary(&bot, &["--per-set", "1"]), 4);
    assert!(
        !f.read()["sets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "t2_by_bot")
    );
    assert!(mutations(&bot).is_empty());
}

// Original: tests.test_unresolved_mutation.AmbiguousCreateForLaterSets.test_an_unverifiable_create_target_stops_retryably
#[test]
fn ambiguouscreateforlatersets_test_an_unverifiable_create_target_stops_retryably() {
    let f = later();
    let mut m = later_model(&f);
    m.sets
        .insert("t2_by_bot".into(), vec![sticker("x", "cid2")]);
    m.bodies.insert("x".into(), b"undecodable".to_vec());
    let bot = Bot::new(m);
    assert_eq!(f.ordinary(&bot, &["--per-set", "1"]), 3);
    assert!(!f.read()["in_flight"].is_null());
}

// Original: tests.test_unresolved_mutation.AmbiguousCreateForLaterSets.test_landed_create_of_set_two_is_adopted
#[test]
fn ambiguouscreateforlatersets_test_landed_create_of_set_two_is_adopted() {
    let f = later();
    let mut m = later_model(&f);
    m.sets
        .insert("t2_by_bot".into(), vec![sticker("f2", "cid2")]);
    m.bodies
        .insert("f2".into(), std::fs::read(f.src.join("b.png")).unwrap());
    let bot = Bot::new(m);
    f.ordinary(&bot, &["--per-set", "1"]);
    assert!(mutations(&bot).is_empty());
    let s = f.read();
    assert_eq!(
        s["sets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["t1_by_bot", "t2_by_bot"]
    );
    assert!(s["done"].as_array().unwrap().contains(&json!("b")));
    assert!(s["in_flight"].is_null());
}

// Original: tests.test_unresolved_mutation.AmbiguousCreateForLaterSets.test_missing_create_of_set_two_leaves_the_item_pending
#[test]
fn ambiguouscreateforlatersets_test_missing_create_of_set_two_leaves_the_item_pending() {
    let f = later();
    let bot = Bot::new(later_model(&f));
    f.ordinary(&bot, &["--per-set", "1"]);
    assert!(f.read()["done"].as_array().unwrap().contains(&json!("b")));
    assert!(bot.edit(|m| {
        m.requests
            .iter()
            .any(|r| r.path.ends_with("createNewStickerSet") && r.form["name"] == "t2_by_bot")
    }));
}

// Original: tests.test_unresolved_mutation.AmbiguousCreateForLaterSets.test_unknown_create_stops_retryably
#[test]
fn ambiguouscreateforlatersets_test_unknown_create_stops_retryably() {
    let f = later();
    let mut m = later_model(&f);
    m.unknown.insert("t2_by_bot".into());
    let bot = Bot::new(m);
    assert_eq!(f.ordinary(&bot, &["--per-set", "1"]), 3);
    assert!(mutations(&bot).is_empty());
    assert!(!f.read()["in_flight"].is_null());
}

// Original: tests.test_unresolved_mutation.RecordedSetIntegrity.test_deleted_recorded_set_is_an_integrity_stop
#[test]
fn recordedsetintegrity_test_deleted_recorded_set_is_an_integrity_stop() {
    let f = recorded();
    let bot = Bot::new(f.model());
    assert_eq!(f.ordinary(&bot, &[]), 4);
    assert!(mutations(&bot).is_empty());
}

// Original: tests.test_unresolved_mutation.RecordedSetIntegrity.test_shrunken_set_is_an_integrity_stop
#[test]
fn recordedsetintegrity_test_shrunken_set_is_an_integrity_stop() {
    let f = recorded();
    let mut m = f.model();
    m.sets
        .insert("t1_by_bot".into(), vec![sticker("f", "c"); 3]);
    let bot = Bot::new(m);
    assert_eq!(f.ordinary(&bot, &[]), 4);
    assert!(mutations(&bot).is_empty());
}

// Original: tests.test_unresolved_mutation.RecordedSetIntegrity.test_unknown_recorded_set_is_retryable
#[test]
fn recordedsetintegrity_test_unknown_recorded_set_is_retryable() {
    let f = recorded();
    let mut m = f.model();
    m.unknown.insert("t1_by_bot".into());
    let bot = Bot::new(m);
    assert_eq!(f.ordinary(&bot, &[]), 3);
    assert!(mutations(&bot).is_empty());
}

// Original: tests.test_unresolved_mutation.UnresolvedMutationStopsTheRun.test_a_strangers_set_is_not_adopted_after_an_ambiguous_create
#[test]
fn unresolvedmutationstopstherun_test_a_strangers_set_is_not_adopted_after_an_ambiguous_create() {
    let f = Ticker::new(false, 3);
    let bot = Bot::new(f.model());
    assert_eq!(
        f.run(&bot, &[], Some("foreign-create"), Arc::default())
            .unwrap(),
        3
    );
    let s = f.read();
    assert_eq!(s["sets"], json!([]));
    assert_eq!(s["done"], json!([]));
    assert!(!s["in_flight"].is_null());
    assert!(bot.edit(|m| m.messages.is_empty()));
}

// Original: tests.test_unresolved_mutation.UnresolvedMutationStopsTheRun.test_ambiguous_add_stops_before_the_next_item
#[test]
fn unresolvedmutationstopstherun_test_ambiguous_add_stops_before_the_next_item() {
    let f = Ticker::new(false, 3);
    let bot = Bot::new(f.model());
    assert_eq!(
        f.run(&bot, &[], Some("ambiguous-add"), Arc::default())
            .unwrap(),
        3
    );
    let s = f.read();
    assert_eq!(s["in_flight"]["operation"], "add");
    assert_eq!(s["in_flight"]["key"], "b");
    assert_eq!(
        bot.edit(|m| m
            .requests
            .iter()
            .filter(|r| r.path.ends_with("addStickerToSet"))
            .count()),
        1
    );
}

// Original: tests.test_unresolved_mutation.UnresolvedMutationStopsTheRun.test_ambiguous_create_records_the_target_set
#[test]
fn unresolvedmutationstopstherun_test_ambiguous_create_records_the_target_set() {
    let f = Ticker::new(false, 3);
    let bot = Bot::new(f.model());
    assert_eq!(
        f.run(&bot, &[], Some("missing-create"), Arc::default())
            .unwrap(),
        3
    );
    let s = f.read();
    assert_eq!(s["in_flight"]["operation"], "create");
    assert!(!s["in_flight"]["set_name"].as_str().unwrap().is_empty());
}

// Original: tests.test_unresolved_mutation.UnresolvedMutationStopsTheRun.test_an_unverifiable_set_is_not_adopted_after_an_ambiguous_create
#[test]
fn unresolvedmutationstopstherun_test_an_unverifiable_set_is_not_adopted_after_an_ambiguous_create()
{
    let f = Ticker::new(false, 3);
    let bot = Bot::new(f.model());
    assert_eq!(
        f.run(&bot, &[], Some("unknown-create"), Arc::default())
            .unwrap(),
        3
    );
    let s = f.read();
    assert_eq!(s["sets"], json!([]));
    assert!(!s["in_flight"].is_null());
}

// Original: tests.test_unresolved_mutation.UnresolvedMutationStopsTheRun.test_our_own_set_is_still_adopted_after_an_ambiguous_create
#[test]
fn unresolvedmutationstopstherun_test_our_own_set_is_still_adopted_after_an_ambiguous_create() {
    let f = Ticker::new(false, 3);
    let bot = Bot::new(f.model());
    f.run(&bot, &[], Some("own-create"), Arc::default())
        .unwrap();
    let s = f.read();
    assert_eq!(
        s["sets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["t1_by_bot"]
    );
    assert!(s["in_flight"].is_null());
}

// Original: tests.test_unresolved_mutation.UnresolvedMutationStopsTheRun.test_state_for_a_different_base_is_refused
#[test]
fn unresolvedmutationstopstherun_test_state_for_a_different_base_is_refused() {
    let f = Ticker::new(false, 3);
    let mut s = f.initial(200);
    s["base"] = json!("OTHER");
    f.save(s);
    let bot = Bot::new(f.model());
    assert_eq!(f.ordinary(&bot, &[]), 4);
    assert!(mutations(&bot).is_empty());
}

// Original: tests.test_unresolved_mutation.UnresolvedMutationStopsTheRun.test_unknown_live_state_on_resume_keeps_the_intent
#[test]
fn unresolvedmutationstopstherun_test_unknown_live_state_on_resume_keeps_the_intent() {
    let f = Ticker::new(false, 3);
    resume_state(&f, 0, intent("a", "add", 1, 0));
    let mut m = f.model();
    m.unknown.insert("t1_by_bot".into());
    let bot = Bot::new(m);
    assert_eq!(f.ordinary(&bot, &[]), 3);
    assert!(mutations(&bot).is_empty());
    assert!(!f.read()["in_flight"].is_null());
}

// Original: tests.test_unresolved_mutation.UnresolvedMutationStopsTheRun.test_unusable_images_make_the_run_partial
#[test]
fn unresolvedmutationstopstherun_test_unusable_images_make_the_run_partial() {
    let f = Ticker::new(false, 3);
    std::fs::write(f.src.join("bad.png"), b"").unwrap();
    let bot = Bot::new(f.model());
    assert_eq!(f.ordinary(&bot, &[]), 3);
}
