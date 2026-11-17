//! Original private publication boundaries and real CLI dispatch with lazy dependencies.
use crate::baseline_publication_support as support;
use crate::{atomic, config::Config, logging::RunLog, process_owner::OwnedProcess};
use serde_json::json;
use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use support::*;

// tests.test_brand_logo.AnUnusableLogoStopsThePublish.test_a_corrupt_logo_stops_before_any_set_is_created
#[test]
fn brand_a_corrupt_logo_stops_before_any_set_is_created() {
    let fixture = Fixture::new();
    let corrupt = fixture.data.join("corrupt.png");
    std::fs::write(&corrupt, b"not an image").unwrap();
    let bot = Bot::new(Model::ordinary());
    let config = fixture.config(&[("BRAND_LOGO_BOTS", "YourEmojiBot".into())]);
    let log = RunLog::open(&fixture.root.0, "corrupt-brand", vec![]);
    let args = [
        "--base",
        "pk",
        "--title",
        "Pack",
        "--formats",
        "static",
        "--user-id",
        "7",
        "--brand-logo",
        corrupt.to_str().unwrap(),
        "--data-dir",
        fixture.data.to_str().unwrap(),
    ]
    .map(str::to_owned);
    let current = std::env::current_exe().unwrap();
    let profile = current
        .ancestors()
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name == "debug" || name == "release")
        })
        .unwrap();
    let executable = profile.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    assert!(
        executable.is_file(),
        "standalone codec companion is required"
    );
    let code = super::run_with_factories(
        &config,
        &args,
        &log,
        |_, _| Ok(bot.telegram()),
        |config, scratch| {
            Ok(crate::media_native_adapter::NativeMediaAdapter {
                executable,
                root: config.root.clone(),
                scratch,
            })
        },
    )
    .unwrap();
    assert_eq!(code, 2);
    assert!(
        std::fs::read_to_string(log.path.unwrap())
            .unwrap()
            .contains("brand logo cannot be used")
    );
    bot.edit(|model| {
        assert!(model.sets.is_empty());
        assert!(
            model
                .requests
                .iter()
                .all(|request| request.path.ends_with("getMe"))
        );
    });
}
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Run {
    code: u8,
    out: String,
    log: String,
    receipt: serde_json::Value,
}
fn command(f: &Fixture, bot: &Bot, args: &[String], extra: &[(&str, Option<String>)]) -> Run {
    captured(f, bot, args, extra, serde_json::Value::Null)
}
fn captured(
    f: &Fixture,
    bot: &Bot,
    args: &[String],
    extra: &[(&str, Option<String>)],
    report: serde_json::Value,
) -> Run {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let spec = f.root.0.join(format!("publication-spec-{id}.json"));
    let result = f.root.0.join(format!("publication-result-{id}.json"));
    let workspace = f.root.0.join(format!("publication-worker-{id}"));
    std::fs::create_dir(&workspace).unwrap();
    atomic::write_json(&spec,&json!({"root":f.root.0,"data":f.data,"base":bot.base,"args":args,"extra":extra,"result":result,"report":report})).unwrap();
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args([
        "--exact",
        "publish_cli::baseline_publication_tests::publication_worker",
        "--nocapture",
        "--test-threads=1",
    ])
    .env("NUMERA_PUBLICATION_SPEC", &spec)
    .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
    .stdin(Stdio::null())
    .stdout(std::fs::File::create(workspace.join("stdout")).unwrap())
    .stderr(std::fs::File::create(workspace.join("stderr")).unwrap());
    if let Some(probe) = report.get("probe") {
        cmd.env("PATH", probe["path"].as_str().unwrap())
            .env("EMOJI_FFMPEG_TIMEOUT", "1")
            .env(
                "NUMERA_PUBLICATION_PROBE_CALLS",
                probe["calls"].as_str().unwrap(),
            )
            .env(
                "NUMERA_PUBLICATION_PROBE_RAW",
                probe["raw"].as_str().unwrap(),
            )
            .env(
                "NUMERA_PUBLICATION_PROBE_HANG",
                if probe["hang"] == true { "1" } else { "0" },
            );
    }
    let mut child = OwnedProcess::spawn_session(&mut cmd, workspace.clone()).unwrap();
    let status = runtime()
        .block_on(child.wait(Duration::from_secs(150), Duration::from_secs(140)))
        .unwrap();
    let out = std::fs::read_to_string(workspace.join("stdout")).unwrap();
    let err = std::fs::read_to_string(workspace.join("stderr")).unwrap();
    child.finish().unwrap();
    assert!(status.success(), "publication worker failed: {out}\n{err}");
    let doc = read_json(&result);
    Run {
        code: doc["code"].as_u64().unwrap() as u8,
        out,
        log: doc["log"].as_str().unwrap().into(),
        receipt: doc,
    }
}
fn cli(f: &Fixture, bot: &Bot, extra: &[&str]) -> Run {
    let mut args = [
        "--base",
        "pk",
        "--title",
        "Pack",
        "--formats",
        "static",
        "--user-id",
        "7",
        "--no-brand-logo",
        "--data-dir",
        f.data.to_str().unwrap(),
    ]
    .map(str::to_owned)
    .to_vec();
    args.extend(extra.iter().map(|s| s.to_string()));
    command(f, bot, &args, &[])
}
fn dry(f: &Fixture, extra: &[&str], config: &[(&str, Option<String>)]) -> Run {
    let bot = Bot::new(Model::ordinary());
    let mut args = [
        "--base",
        "pk",
        "--title",
        "T",
        "--data-dir",
        f.data.to_str().unwrap(),
        "--dry-run",
    ]
    .map(str::to_owned)
    .to_vec();
    args.extend(extra.iter().map(|s| s.to_string()));
    command(f, &bot, &args, config)
}
#[test]
fn publication_worker() {
    let Some(path) = std::env::var_os("NUMERA_PUBLICATION_SPEC") else {
        return;
    };
    let spec = read_json(std::path::Path::new(&path));
    let root = std::path::PathBuf::from(spec["root"].as_str().unwrap());
    let mut env = BTreeMap::from([
        ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
        ("BRAND_LOGO_BOTS".into(), "".into()),
        ("GENERAL_BOT_TOKEN".into(), "test-only-token".into()),
        ("PACK_OWNER_USER_ID".into(), "7".into()),
        ("PACK_LINKS_CHAT_ID".into(), "".into()),
    ]);
    for pair in spec["extra"].as_array().unwrap() {
        if let Some(value) = pair[1].as_str() {
            env.insert(pair[0].as_str().unwrap().into(), value.into());
        } else {
            env.remove(pair[0].as_str().unwrap());
        }
    }
    let config = Config::load_with_environment(root.clone(), &env).unwrap();
    let log = RunLog::open(&root, "publication-baseline", vec![]);
    let args: Vec<String> = serde_json::from_value(spec["args"].clone()).unwrap();
    let mut probe_receipt = serde_json::Value::Null;
    let telegram_calls = std::cell::Cell::new(0);
    let media_calls = std::cell::Cell::new(0);
    let result = if spec["report"]["probe"].is_object() {
        let probe = &spec["report"]["probe"];
        let started = std::time::Instant::now();
        let usable = runtime()
            .block_on(crate::publication_media::usable(
                &root,
                std::path::Path::new(probe["source"].as_str().unwrap()),
                "video",
            ))
            .unwrap();
        let actual_config = Config::load(root.clone()).unwrap();
        let budget =
            crate::codec_process::codec_budget(Some(actual_config.value("EMOJI_FFMPEG_TIMEOUT")))
                .unwrap();
        let captures = root.join("logs/tmp");
        if captures.exists() {
            assert!(!crate::process_owner::cleanup_pending(&captures));
            assert_eq!(
                std::fs::read_dir(&captures).unwrap().count(),
                0,
                "probe left owned capture files"
            );
        }
        probe_receipt = json!({"usable":usable,"elapsed_ms":started.elapsed().as_millis(),"wall_ms":budget.wall.as_millis(),"calls":std::fs::read_to_string(probe["calls"].as_str().unwrap()).unwrap()});
        Ok(if usable { 0 } else { 4 })
    } else if spec["report"].is_object() {
        let report = &spec["report"];
        let refused: Vec<(String, String, String)> =
            serde_json::from_value(report["refused"].clone()).unwrap();
        let missing: Vec<(String, String)> =
            serde_json::from_value(report["missing"].clone()).unwrap();
        let unknown: Vec<(String, String, String)> =
            serde_json::from_value(report["unknown"].clone()).unwrap();
        Ok(super::report_preflight(
            report["accepted"].as_u64().unwrap() as usize,
            &refused,
            &missing,
            &unknown,
        ))
    } else {
        super::run_with_factories(
            &config,
            &args,
            &log,
            |_, _| {
                telegram_calls.set(telegram_calls.get() + 1);
                Ok(telegram(spec["base"].as_str().unwrap()))
            },
            |_, _| {
                media_calls.set(media_calls.get() + 1);
                Ok(Images::default())
            },
        )
    };
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            log.event("ERROR", "build-collection", &error);
            if error.starts_with("FLOOD_WAIT") {
                3
            } else {
                2
            }
        }
    };
    let captured = log
        .path
        .as_ref()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .unwrap_or_default();
    atomic::write_json(
        std::path::Path::new(spec["result"].as_str().unwrap()),
        &json!({"code":code,"log":captured,"probe":probe_receipt,"factories":{"telegram":telegram_calls.get(),"media":media_calls.get()}}),
    )
    .unwrap();
}

// tests.test_build_collection_state.StateFileContract.test_absent_file_uses_the_default
#[test]
fn state_absent_file_uses_the_default() {
    let f = Fixture::new();
    assert_eq!(
        super::read(&f.data.join("nope.json"), json!({"d":1})).unwrap(),
        json!({"d":1})
    );
}
// tests.test_build_collection_state.StateFileContract.test_truncated_state_refuses_to_start_from_scratch
#[test]
fn state_truncated_state_refuses_to_start_from_scratch() {
    let f = Fixture::new();
    std::fs::write(f.state_path(), r#"{"base":"pk","sets":[{"name":"pks1"#).unwrap();
    assert!(
        super::read(&f.state_path(), json!({}))
            .and_then(|doc| crate::resume::collection(doc, "pk"))
            .is_err()
    );
}
// tests.test_build_collection_state.StateFileContract.test_truncated_plan_refuses_to_refreeze
#[test]
fn state_truncated_plan_refuses_to_refreeze() {
    let f = Fixture::new();
    std::fs::write(f.plan_path(), r#"{"static":["s:a""#).unwrap();
    assert!(super::read(&f.plan_path(), json!({})).is_err());
}
// tests.test_build_collection_state.StateFileContract.test_a_failed_write_leaves_the_previous_state_intact
#[test]
fn state_a_failed_write_leaves_the_previous_state_intact() {
    let f = Fixture::new();
    atomic::write_json(
        &f.state_path(),
        &json!({"base":"pk","sets":[],"sent":["first"]}),
    )
    .unwrap();
    let bytes = serde_json::to_vec(&json!({"base":"pk","sets":[],"sent":["second"]})).unwrap();
    assert!(
        atomic::write_bytes_with(
            &f.state_path(),
            &bytes,
            std::fs::File::sync_all,
            |_, _| Err(std::io::Error::other("boom"))
        )
        .is_err()
    );
    assert_eq!(read_json(&f.state_path())["sent"], json!(["first"]));
}
// tests.test_build_collection_state.StateFileContract.test_a_broken_state_stops_the_run_before_any_mutation
#[test]
fn state_a_broken_state_stops_the_run_before_any_mutation() {
    let f = Fixture::new();
    f.write_state(vec![f.state_set(None, 1, false)], json!({}));
    let bot = Bot::new(Model::ordinary());
    let run = cli(&f, &bot, &[]);
    assert_eq!(run.code, 4);
    assert_eq!(run.receipt["factories"], json!({"telegram":0,"media":0}));
    bot.edit(|m| {
        assert!(m.uploaded.is_empty());
        assert!(m.sets.is_empty());
        assert!(m.requests.is_empty());
    });
    assert!(!f.plan_path().exists());
}
// tests.test_build_collection_state.StateFileContract.test_main_stops_on_an_unreadable_plan
#[test]
fn state_main_stops_on_an_unreadable_plan() {
    let f = Fixture::new();
    std::fs::write(f.plan_path(), "{oops").unwrap();
    assert_eq!(dry(&f, &[], &[]).code, 4);
}

// tests.test_build_collection_state.FreshUploadIdentity.test_a_foreign_sticker_swapped_in_never_takes_our_key
#[test]
fn state_a_foreign_sticker_swapped_in_never_takes_our_key() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model.prefix = "FRESH-".into();
    model.final_edit = "foreign".into();
    let bot = Bot::new(model);
    assert_eq!(cli(&f, &bot, &[]).code, 4);
    assert_ne!(
        f.cid(&f.keys[0]),
        sticker("NEVER-SEEN", "foreign-cid")["custom_emoji_id"]
    );
    assert_eq!(f.cid(&f.keys[0]), format!("{SET}-0"));
}

// tests.test_build_collection_state.FreshUploadIdentity.test_a_same_length_reorder_before_read_back_fails_closed
#[test]
fn state_a_same_length_reorder_before_read_back_fails_closed() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model.prefix = "FRESH-".into();
    model.final_edit = "reverse".into();
    let bot = Bot::new(model);
    assert_eq!(cli(&f, &bot, &[]).code, 4);
    assert_eq!(f.cid(&f.keys[0]), format!("{SET}-0"));
    assert_eq!(f.cid(&f.keys[1]), format!("{SET}-1"));
}
// tests.test_build_collection_state.FreshUploadIdentity.test_the_uploaded_copys_identity_is_recorded_at_upload_time
#[test]
fn state_the_uploaded_copys_identity_is_recorded_at_upload_time() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model.prefix = "FRESH-".into();
    let bot = Bot::new(model);
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    assert_eq!(f.seen("FRESH-item0"), f.keys[0]);
    assert_eq!(f.seen("FRESH-item1"), f.keys[1]);
}
// tests.test_build_collection_state.FreshUploadIdentity.test_an_upload_that_adds_no_identity_is_never_recorded
#[test]
fn state_an_upload_that_adds_no_identity_is_never_recorded() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model.prefix = "FRESH-".into();
    model.mutation_mode = "silent".into();
    let bot = Bot::new(model);
    assert_eq!(cli(&f, &bot, &[]).code, 4);
    assert!(f.published(&f.keys[0]));
    assert!(!f.published(&f.keys[1]));
}
// tests.test_build_collection_state.FreshUploadIdentity.test_a_concurrent_writer_makes_the_position_ambiguous
#[test]
fn state_a_concurrent_writer_makes_the_position_ambiguous() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model.prefix = "FRESH-".into();
    model.mutation_mode = "race".into();
    let bot = Bot::new(model);
    assert_eq!(cli(&f, &bot, &[]).code, 4);
    assert!(!f.published(&f.keys[1]));
    assert!(f.seen("SOMEONE-ELSE").is_null());
}

// tests.test_publish_cli.CliContract.test_second_publisher_fails_fast_while_the_lock_is_held
#[test]
fn cli_second_publisher_fails_fast_while_the_lock_is_held() {
    let f = Fixture::new();
    let _held = crate::locks::Lease::acquire(&f.data.join("publish_pk.lock"), UTC).unwrap();
    assert_eq!(dry(&f, &[], &[]).code, 4);
}
// tests.test_publish_cli.CliContract.test_the_lock_is_released_again
#[test]
fn cli_the_lock_is_released_again() {
    let f = Fixture::new();
    assert_eq!(dry(&f, &[], &[]).code, 0);
    assert_eq!(dry(&f, &[], &[]).code, 0);
    let _held = crate::locks::Lease::acquire(&f.data.join("publish_pk.lock"), UTC).unwrap();
}
// tests.test_publish_cli.CliContract.test_unknown_format_is_a_usage_error
#[test]
fn cli_unknown_format_is_a_usage_error() {
    let f = Fixture::new();
    let run = dry(&f, &["--formats", "garbage"], &[]);
    assert_eq!(run.code, 2);
    assert!(run.log.contains("--formats"));
}
// tests.test_publish_cli.CliContract.test_duplicate_and_empty_formats_are_usage_errors
#[test]
fn cli_duplicate_and_empty_formats_are_usage_errors() {
    let f = Fixture::new();
    for raw in ["static,static", "static,", ""] {
        let run = dry(&f, &["--formats", raw], &[]);
        assert_eq!(run.code, 2, "{raw}");
        assert!(run.log.contains("--formats"));
    }
}
// tests.test_publish_cli.CliContract.test_per_set_must_be_within_telegrams_cap
#[test]
fn cli_per_set_must_be_within_telegrams_cap() {
    let f = Fixture::new();
    for raw in ["0", "-5", "201"] {
        assert_eq!(dry(&f, &["--per-set", raw], &[]).code, 2, "{raw}");
    }
}
// tests.test_publish_cli.CliContract.test_dry_run_counts_the_brand_logo_slot
#[test]
fn cli_dry_run_counts_the_brand_logo_slot() {
    let f = Fixture::new();
    {
        let mut cat = f.catalog();
        for i in 0..198 {
            let path = png(
                &f.data.join(format!("media/static/x{i}.png")),
                [(i % 200) as u8, 40, 60, 255],
            );
            cat.insert(&json!({"content_key":format!("s:x{i:030}"),"fmt":"static","file_path":path,"emojis":["😀"],"keywords":[format!("x{i}")]}),UTC).unwrap();
        }
    }
    let logo = png(&f.data.join("logo.png"), [200, 30, 30, 255]);
    let run = dry(
        &f,
        &[
            "--formats",
            "static",
            "--brand-logo",
            logo.to_str().unwrap(),
        ],
        &[("BRAND_LOGO_BOTS", Some("YourEmojiBot".into()))],
    );
    assert_eq!(run.code, 0);
    assert!(
        run.out.contains("200 emoji -> 2 set(s) of up to 199"),
        "{}",
        run.out
    );
    let run = dry(&f, &["--formats", "static", "--no-brand-logo"], &[]);
    assert_eq!(run.code, 0);
    assert!(run.out.contains("200 emoji -> 1 set(s) of up to 200"));
}
// tests.test_publish_cli.CliContract.test_per_set_1_with_a_logo_is_rejected_not_a_zero_division
#[test]
fn cli_per_set_1_with_a_logo_is_rejected_not_a_zero_division() {
    let f = Fixture::new();
    let logo = png(&f.data.join("logo.png"), [200, 30, 30, 255]);
    assert_eq!(
        dry(
            &f,
            &["--per-set", "1", "--brand-logo", logo.to_str().unwrap()],
            &[("BRAND_LOGO_BOTS", Some("YourEmojiBot".into()))]
        )
        .code,
        2
    );
}
// tests.test_publish_cli.CliContract.test_unset_logo_configuration_stops_the_publish_naming_the_key
#[test]
fn cli_unset_logo_configuration_stops_the_publish_naming_the_key() {
    let f = Fixture::new();
    let run = dry(&f, &[], &[("BRAND_LOGO_BOTS", None)]);
    assert_eq!(run.code, 2);
    assert!(run.log.contains("BRAND_LOGO_BOTS"));
}
// tests.test_publish_cli.CliContract.test_a_listed_bot_without_its_logo_file_stops_the_publish
#[test]
fn cli_a_listed_bot_without_its_logo_file_stops_the_publish() {
    let f = Fixture::new();
    let run = dry(
        &f,
        &[],
        &[
            ("BRAND_LOGO_BOTS", Some("YourEmojiBot".into())),
            (
                "BRAND_LOGO_PATH",
                Some(f.data.join("missing.png").to_str().unwrap().into()),
            ),
        ],
    );
    assert_eq!(run.code, 2);
    assert!(run.log.contains("logo"));
}
// tests.test_publish_cli.CliContract.test_no_brand_logo_needs_no_logo_configuration
#[test]
fn cli_no_brand_logo_needs_no_logo_configuration() {
    let f = Fixture::new();
    assert_eq!(
        dry(&f, &["--no-brand-logo"], &[("BRAND_LOGO_BOTS", None)]).code,
        0
    );
}

// tests.test_publish_cli.PublishThroughMain.test_publish_then_resume_uploads_each_emoji_once
#[test]
fn cli_publish_then_resume_uploads_each_emoji_once() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    assert_eq!(bot.edit(|m| m.uploaded.clone()), ["item0", "item1"]);
    assert_eq!(read_json(&f.state_path())["sets"][0]["keys"], json!(f.keys));
    for i in 0..2 {
        assert_eq!(
            f.catalog().get(&f.keys[i]).unwrap().unwrap()["custom_emoji_id"],
            format!("{SET}-{i}")
        );
    }
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    bot.edit(|m| {
        assert_eq!(m.uploaded, ["item0", "item1"]);
        assert_eq!(m.sets[SET].len(), 2);
    });
}
// tests.test_publish_cli.PublishThroughMain.test_a_set_emptied_between_runs_stops_the_resume
#[test]
fn cli_a_set_emptied_between_runs_stops_the_resume() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    bot.edit(|m| m.sets.get_mut(SET).unwrap().clear());
    assert_eq!(cli(&f, &bot, &[]).code, 4);
    assert_eq!(bot.edit(|m| m.uploaded.clone()), ["item0", "item1"]);
}
// tests.test_publish_cli.PublishThroughMain.test_unreachable_telegram_is_retryable_not_a_failure
#[test]
fn cli_unreachable_telegram_is_retryable_not_a_failure() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    bot.edit(|m| {
        m.unknown.insert(SET.into());
    });
    assert_eq!(cli(&f, &bot, &[]).code, 3);
    assert_eq!(bot.edit(|m| m.uploaded.clone()), ["item0", "item1"]);
}
// tests.test_publish_cli.PublishThroughMain.test_a_foreign_tail_sticker_sends_the_next_emoji_to_a_new_set
#[test]
fn cli_a_foreign_tail_sticker_sends_the_next_emoji_to_a_new_set() {
    let mut f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    bot.edit(|m| {
        m.sets
            .get_mut(SET)
            .unwrap()
            .push(sticker("OWNER", "owner-cid"))
    });
    let key = f.add(2);
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    assert_eq!(f.cid(&key), format!("{SET2}-0"));
    bot.edit(|m| {
        assert_eq!(m.sets[SET].len(), 3);
        assert_eq!(m.sets[SET2].len(), 1);
    });
}
// tests.test_publish_cli.PublishThroughMain.test_new_set_starts_the_next_pack_while_this_one_is_half_empty
#[test]
fn cli_new_set_starts_the_next_pack_while_this_one_is_half_empty() {
    let mut f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    assert_eq!(bot.edit(|m| m.sets[SET].len()), 2);
    let key = f.add(2);
    assert_eq!(cli(&f, &bot, &["--new-set"]).code, 0);
    bot.edit(|m| {
        assert_eq!(m.sets[SET].len(), 2);
        assert_eq!(m.sets[SET2].len(), 1);
    });
    assert_eq!(f.cid(&key), format!("{SET2}-0"));
    let state = read_json(&f.state_path());
    assert_eq!(
        state["sets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["index"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(state["sets"][1]["keys"], json!([key]));
}
// tests.test_publish_cli.PublishThroughMain.test_without_new_set_the_next_emoji_still_fills_the_current_pack
#[test]
fn cli_without_new_set_the_next_emoji_still_fills_the_current_pack() {
    let mut f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    f.add(2);
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    bot.edit(|m| {
        assert_eq!(m.sets[SET].len(), 3);
        assert!(!m.sets.contains_key(SET2));
    });
}
fn two_packs(f: &mut Fixture, bot: &Bot) {
    assert_eq!(cli(f, bot, &[]).code, 0);
    f.add(2);
    assert_eq!(cli(f, bot, &["--new-set"]).code, 0);
}
// tests.test_publish_cli.PublishThroughMain.test_into_pack_tops_up_an_older_pack_that_still_has_room
#[test]
fn cli_into_pack_tops_up_an_older_pack_that_still_has_room() {
    let mut f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    two_packs(&mut f, &bot);
    bot.edit(|m| {
        assert_eq!(m.sets[SET].len(), 2);
        assert_eq!(m.sets[SET2].len(), 1);
    });
    let key = f.add(3);
    assert_eq!(cli(&f, &bot, &["--into-pack", "1"]).code, 0);
    bot.edit(|m| {
        assert_eq!(m.sets[SET].len(), 3);
        assert_eq!(m.sets[SET2].len(), 1);
    });
    assert_eq!(f.cid(&key), format!("{SET}-2"));
}
// tests.test_publish_cli.PublishThroughMain.test_the_record_written_is_the_pack_actually_uploaded_to
#[test]
fn cli_the_record_written_is_the_pack_actually_uploaded_to() {
    let mut f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    two_packs(&mut f, &bot);
    let key = f.add(3);
    assert_eq!(cli(&f, &bot, &["--into-pack", "1"]).code, 0);
    let state = read_json(&f.state_path());
    let first = &state["sets"][0];
    let second = &state["sets"][1];
    assert!(first["keys"].as_array().unwrap().contains(&json!(key)));
    assert!(!second["keys"].as_array().unwrap().contains(&json!(key)));
    bot.edit(|m| {
        assert_eq!(first["live"], m.sets[SET].len());
        assert_eq!(second["live"], m.sets[SET2].len());
    });
}
// tests.test_publish_cli.PublishThroughMain.test_into_pack_refuses_a_number_that_is_not_there
#[test]
fn cli_into_pack_refuses_a_number_that_is_not_there() {
    let mut f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    two_packs(&mut f, &bot);
    f.add(3);
    assert_eq!(cli(&f, &bot, &["--into-pack", "9"]).code, 4);
    bot.edit(|m| {
        assert_eq!(m.sets[SET].len(), 2);
        assert_eq!(m.sets[SET2].len(), 1);
    });
}
// tests.test_publish_cli.PublishThroughMain.test_into_pack_refuses_a_full_pack
#[test]
fn cli_into_pack_refuses_a_full_pack() {
    let mut f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &["--per-set", "2"]).code, 0);
    f.add(2);
    assert_eq!(
        cli(&f, &bot, &["--per-set", "2", "--into-pack", "1"]).code,
        4
    );
}
// tests.test_publish_cli.PublishThroughMain.test_into_pack_and_new_set_together_are_rejected
#[test]
fn cli_into_pack_and_new_set_together_are_rejected() {
    let mut f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    two_packs(&mut f, &bot);
    assert_eq!(cli(&f, &bot, &["--new-set", "--into-pack", "1"]).code, 2);
}
// tests.test_publish_cli.PublishThroughMain.test_an_older_recorded_set_that_disappeared_fails_closed
#[test]
fn cli_an_older_recorded_set_that_disappeared_fails_closed() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &["--per-set", "1"]).code, 0);
    bot.edit(|m| {
        assert_eq!(m.sets.keys().cloned().collect::<Vec<_>>(), [SET, SET2]);
        m.sets.remove(SET);
    });
    assert_eq!(cli(&f, &bot, &["--per-set", "1"]).code, 4);
}
// tests.test_publish_cli.PublishThroughMain.test_an_older_set_that_cannot_be_read_is_retryable
#[test]
fn cli_an_older_set_that_cannot_be_read_is_retryable() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &["--per-set", "1"]).code, 0);
    bot.edit(|m| {
        m.unknown.insert(SET.into());
    });
    assert_eq!(cli(&f, &bot, &["--per-set", "1"]).code, 3);
}
// tests.test_publish_cli.PublishThroughMain.test_each_successful_upload_is_logged
#[test]
fn cli_each_successful_upload_is_logged() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    let run = cli(&f, &bot, &[]);
    assert_eq!(run.code, 0);
    let uploads = run
        .log
        .lines()
        .filter(|line| line.contains("] uploaded "))
        .collect::<Vec<_>>();
    assert_eq!(uploads.len(), 2, "{}", run.log);
    assert!(uploads[0].contains("1/2"));
    assert!(uploads[1].contains("2/2"));
    assert!(uploads.iter().all(|line| line.contains("pk")));
}
// tests.test_publish_cli.PublishThroughMain.test_a_file_telegram_will_never_accept_stops_being_retried
#[test]
fn cli_a_file_telegram_will_never_accept_stops_being_retried() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model.reject_adds = 1;
    let bot = Bot::new(model);
    cli(&f, &bot, &[]);
    assert_eq!(
        read_json(&f.state_path())["skipped"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let before = bot.edit(|m| m.adds.len());
    cli(&f, &bot, &[]);
    assert_eq!(bot.edit(|m| m.adds.len()), before);
}
// tests.test_publish_cli.PublishThroughMain.test_the_dry_run_counts_what_will_publish_not_the_plan
#[test]
fn cli_the_dry_run_counts_what_will_publish_not_the_plan() {
    let f = Fixture::new();
    let mut cat = f.catalog();
    let keys = cat
        .all(None)
        .unwrap()
        .iter()
        .map(|row| row["content_key"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    cat.inclusion(&[keys[0].clone()]).unwrap();
    let plan =
        crate::resume::freeze(json!({}), &cat.all(None).unwrap(), &["static".into()]).unwrap();
    let queued = super::pending(
        &mut cat,
        &plan,
        "static",
        "pk",
        &std::collections::BTreeSet::new(),
    )
    .unwrap();
    assert!(!queued.contains(&keys[0]));
    assert_eq!(queued.len(), keys.len() - 1);
    cat.uploaded(
        &json!({"content_key":queued[0],"custom_emoji_id":"1","base":"pk","set_name":"pks1"}),
        UTC,
    )
    .unwrap();
    assert!(
        !super::pending(
            &mut cat,
            &plan,
            "static",
            "pk",
            &std::collections::BTreeSet::new()
        )
        .unwrap()
        .contains(&queued[0])
    );
}
// tests.test_publish_cli.PublishThroughMain.test_preflight_refuses_early_and_publishes_nothing
#[test]
fn cli_preflight_refuses_early_and_publishes_nothing() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model.refuse.insert("item0.png".into());
    let bot = Bot::new(model);
    let run = cli(&f, &bot, &["--preflight"]);
    assert_ne!(run.code, 0);
    assert!(run.out.contains("REFUSED"));
    bot.edit(|m| {
        assert!(m.uploaded.is_empty());
        assert!(m.sets.is_empty());
    });
}
// tests.test_publish_cli.PublishThroughMain.test_preflight_passes_a_clean_queue_and_still_uploads_nothing
#[test]
fn cli_preflight_passes_a_clean_queue_and_still_uploads_nothing() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    let run = cli(&f, &bot, &["--preflight"]);
    assert_eq!(run.code, 0);
    assert_eq!(run.receipt["factories"], json!({"telegram":1,"media":0}));
    assert!(run.out.contains("acceptable"));
    bot.edit(|m| {
        assert_eq!(m.checked.len(), 2);
        assert!(m.uploaded.is_empty());
        assert!(m.sets.is_empty());
    });
}
// tests.test_publish_cli.PublishThroughMain.test_a_transport_error_is_not_reported_as_a_bad_file
#[test]
fn cli_a_transport_error_is_not_reported_as_a_bad_file() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model
        .unknown_upload
        .extend(["item0.png".into(), "item1.png".into()]);
    let bot = Bot::new(model);
    let run = cli(&f, &bot, &["--preflight"]);
    assert_eq!(run.code, 3);
    assert!(!run.out.contains("REFUSED"));
}
// tests.test_publish_cli.PublishThroughMain.test_a_run_that_lost_an_emoji_does_not_announce_the_pack
#[test]
fn cli_a_run_that_lost_an_emoji_does_not_announce_the_pack() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model.reject_adds = 1;
    let bot = Bot::new(model);
    cli(&f, &bot, &[]);
    assert_eq!(read_json(&f.state_path())["sent"], json!([]));
    cli(&f, &bot, &[]);
    assert_eq!(
        read_json(&f.state_path())["sent"].as_array().unwrap().len(),
        1
    );
}
// tests.test_publish_cli.PublishThroughMain.test_a_clean_run_announces_normally
#[test]
fn cli_a_clean_run_announces_normally() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    assert_eq!(
        read_json(&f.state_path())["sent"].as_array().unwrap().len(),
        1
    );
}
// tests.test_publish_cli.PublishThroughMain.test_a_transient_failure_is_still_retried
#[test]
fn cli_a_transient_failure_is_still_retried() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model.transport_adds = 5;
    let bot = Bot::new(model);
    cli(&f, &bot, &[]);
    assert_eq!(read_json(&f.state_path())["skipped"], json!([]));
}
// tests.test_publish_cli.PublishThroughMain.test_a_run_where_every_upload_failed_exits_non_zero
#[test]
fn cli_a_run_where_every_upload_failed_exits_non_zero() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model.fail_after = Some(0);
    let bot = Bot::new(model);
    assert_eq!(cli(&f, &bot, &[]).code, 4);
    assert!(bot.edit(|m| m.uploaded.is_empty()));
}
// tests.test_publish_cli.PublishThroughMain.test_a_partly_failed_run_is_partial
#[test]
fn cli_a_partly_failed_run_is_partial() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model.fail_after = Some(1);
    let bot = Bot::new(model);
    assert_eq!(cli(&f, &bot, &[]).code, 3);
    assert_eq!(bot.edit(|m| m.uploaded.clone()), ["item0"]);
}
// tests.test_publish_cli.PublishThroughMain.test_drift_while_adopting_a_set_stops_the_run_instead_of_retrying
#[test]
fn cli_drift_while_adopting_a_set_stops_the_run_instead_of_retrying() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model
        .sets
        .insert(SET.into(), vec![sticker("GHOST", "ghost-cid")]);
    model.undownloadable.insert("GHOST".into());
    let bot = Bot::new(model);
    let run = cli(&f, &bot, &[]);
    assert_eq!(run.code, 4);
    assert!(run.log.contains("could not be examined"), "{}", run.log);
    assert!(!run.log.contains("will retry"));
    assert!(!run.out.contains("DONE"));
    let state = read_json(&f.state_path());
    assert_eq!(
        state["sets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|set| set["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [SET]
    );
    crate::resume::collection(state, "pk").unwrap();
}
// tests.test_publish_cli.PublishThroughMain.test_skipped_blank_media_is_not_counted_as_a_failure
#[test]
fn cli_skipped_blank_media_is_not_counted_as_a_failure() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    let path = png(&f.data.join("media/static/blank.png"), [0, 0, 0, 0]);
    let key = format!("s:blank{}", "0".repeat(25));
    f.catalog().insert(&json!({"content_key":key,"fmt":"static","file_path":path,"emojis":["😀"],"keywords":["blank"]}),UTC).unwrap();
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    assert_eq!(bot.edit(|m| m.uploaded.clone()), ["item0", "item1"]);
}
// tests.test_publish_contracts.ThePublishFollowsThePanelOrderNow.test_a_reorder_after_the_plan_was_frozen_is_what_gets_uploaded
#[test]
fn contract_a_reorder_after_the_plan_was_frozen_is_what_gets_uploaded() {
    let f = Fixture::new();
    {
        let mut cat = f.catalog();
        let plan =
            crate::resume::freeze(json!({}), &cat.all(None).unwrap(), &["static".into()]).unwrap();
        atomic::write_json(&f.plan_path(), &plan).unwrap();
        cat.order(&f.keys.iter().rev().cloned().collect::<Vec<_>>())
            .unwrap();
    }
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    assert_eq!(bot.edit(|m| m.uploaded.clone()), ["item1", "item0"]);
}
// tests.test_publish_contracts.ThePublishFollowsThePanelOrderNow.test_a_dry_run_writes_no_plan
#[test]
fn contract_a_dry_run_writes_no_plan() {
    let f = Fixture::new();
    let run = dry(&f, &[], &[]);
    assert_eq!(run.code, 0);
    assert_eq!(run.receipt["factories"], json!({"telegram":0,"media":0}));
    assert!(!f.plan_path().exists());
}

fn preflight_fixture() -> (Fixture, serde_json::Value) {
    let mut f = Fixture::empty();
    for i in 0..3 {
        let path = png(&f.data.join(format!("media/i{i}.png")), [200, 30, 30, 255]);
        let key = format!("s:item{i}");
        f.catalog()
            .insert(
                &json!({"content_key":key,"fmt":"static","file_path":path}),
                UTC,
            )
            .unwrap();
        f.keys.push(key);
    }
    let plan = json!({"static":f.keys});
    (f, plan)
}
fn checked_preflight(
    f: &Fixture,
    plan: &serde_json::Value,
    answers: &serde_json::Value,
) -> (u8, Vec<String>) {
    let mut asked = Vec::new();
    let log = RunLog::open(&f.root.0, "preflight-original", vec![]);
    let code = super::preflight_with_check(
        &mut f.catalog(),
        plan,
        &["static".into()],
        "testbase",
        &std::collections::BTreeSet::new(),
        (42, &log),
        |user, path, fmt, upload| {
            assert_eq!(user, 42);
            assert_eq!(fmt, "static");
            assert_eq!(upload.field, "sticker");
            assert_eq!(upload.bytes, std::fs::read(path).unwrap());
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            asked.push(name.clone());
            let answer = &answers[&name];
            match answer["kind"].as_str() {
                Some("refused") => Err(crate::telegram::ApiError::Rejected(
                    answer["why"].as_str().unwrap().into(),
                )),
                Some("unknown") => Err(crate::telegram::ApiError::Transport(
                    answer["why"].as_str().unwrap().into(),
                )),
                _ => Ok(()),
            }
        },
    )
    .unwrap();
    (code, asked)
}
// tests.test_preflight_outcomes.PreflightOutcomes.test_every_file_accepted_is_the_only_success
#[test]
fn preflight_every_file_accepted_is_the_only_success() {
    let (f, plan) = preflight_fixture();
    let (code, asked) = checked_preflight(&f, &plan, &json!({}));
    assert_eq!(code, 0);
    assert_eq!(asked.len(), 3);
}
// tests.test_preflight_outcomes.PreflightOutcomes.test_a_total_transport_failure_is_never_success
#[test]
fn preflight_a_total_transport_failure_is_never_success() {
    let (f, plan) = preflight_fixture();
    let answers = json!({"i0.png":{"kind":"unknown","why":"connection reset by peer"},"i1.png":{"kind":"unknown","why":"connection reset by peer"},"i2.png":{"kind":"unknown","why":"connection reset by peer"}});
    let (code, asked) = checked_preflight(&f, &plan, &answers);
    assert_eq!(asked.len(), 3);
    assert_ne!(code, 0);
    assert_eq!(code, 3);
}
// tests.test_preflight_outcomes.PreflightOutcomes.test_a_partial_transport_failure_is_not_success_either
#[test]
fn preflight_a_partial_transport_failure_is_not_success_either() {
    let (f, plan) = preflight_fixture();
    let (code, _) = checked_preflight(
        &f,
        &plan,
        &json!({"i1.png":{"kind":"unknown","why":"timed out"}}),
    );
    assert_eq!(code, 3);
}
// tests.test_preflight_outcomes.PreflightOutcomes.test_a_telegram_refusal_fails
#[test]
fn preflight_a_telegram_refusal_fails() {
    let (f, plan) = preflight_fixture();
    let (code, _) = checked_preflight(
        &f,
        &plan,
        &json!({"i2.png":{"kind":"refused","why":"Bad Request: wrong file type"}}),
    );
    assert_eq!(code, 4);
}
// tests.test_preflight_outcomes.PreflightOutcomes.test_a_missing_file_fails_and_is_never_sent
#[test]
fn preflight_a_missing_file_fails_and_is_never_sent() {
    let (f, plan) = preflight_fixture();
    std::fs::remove_file(f.data.join("media/i0.png")).unwrap();
    let (code, asked) = checked_preflight(&f, &plan, &json!({}));
    assert_eq!(code, 4);
    assert!(!asked.contains(&"i0.png".into()));
}
// tests.test_preflight_outcomes.PreflightOutcomes.test_a_refusal_outranks_an_unknown
#[test]
fn preflight_a_refusal_outranks_an_unknown() {
    let (f, plan) = preflight_fixture();
    let (code, _) = checked_preflight(
        &f,
        &plan,
        &json!({"i0.png":{"kind":"refused","why":"Bad Request: STICKER_VIDEO_LONG"},"i1.png":{"kind":"unknown","why":"connection reset"}}),
    );
    assert_eq!(code, 4);
}
// tests.test_preflight_outcomes.PreflightOutcomes.test_an_empty_queue_is_not_a_validation
#[test]
fn preflight_an_empty_queue_is_not_a_validation() {
    let (f, _) = preflight_fixture();
    let (code, asked) = checked_preflight(&f, &json!({"static":[]}), &json!({}));
    assert_eq!(code, 0);
    assert!(asked.is_empty());
}
// tests.test_preflight_outcomes.PreflightOutcomes.test_it_publishes_nothing_whatever_the_outcome
#[test]
fn preflight_it_publishes_nothing_whatever_the_outcome() {
    let (f, plan) = preflight_fixture();
    let before = f.catalog().all(None).unwrap();
    for answers in [
        json!({}),
        json!({"i0.png":{"kind":"unknown","why":"x"}}),
        json!({"i0.png":{"kind":"refused","why":"no"}}),
    ] {
        let (_, asked) = checked_preflight(&f, &plan, &answers);
        assert!(
            asked
                .iter()
                .all(|name| ["i0.png", "i1.png", "i2.png"].contains(&name.as_str()))
        );
        assert_eq!(f.catalog().all(None).unwrap(), before);
        assert!(!f.state_path().exists());
        assert!(!f.data.join("publish_testbase.json").exists());
    }
}
fn summary(
    accepted: usize,
    refused: serde_json::Value,
    missing: serde_json::Value,
    unknown: serde_json::Value,
) -> Run {
    let f = Fixture::empty();
    let bot = Bot::new(Model::ordinary());
    captured(
        &f,
        &bot,
        &[],
        &[],
        json!({"accepted":accepted,"refused":refused,"missing":missing,"unknown":unknown}),
    )
}
// tests.test_preflight_outcomes.TheSummaryMatchesTheOutcome.test_success_names_the_number_actually_accepted
#[test]
fn preflight_summary_success_names_the_number_actually_accepted() {
    let run = summary(3, json!([]), json!([]), json!([]));
    assert_eq!(run.code, 0);
    assert!(run.out.contains("3 accepted"));
    assert!(run.out.contains("All 3 queued file(s) are acceptable"));
}
// tests.test_preflight_outcomes.TheSummaryMatchesTheOutcome.test_an_unknown_never_prints_an_acceptance_claim
#[test]
fn preflight_summary_an_unknown_never_prints_an_acceptance_claim() {
    let run = summary(
        0,
        json!([]),
        json!([]),
        json!([["k", "n.png", "timed out"]]),
    );
    assert_eq!(run.code, 3);
    assert!(!run.out.contains("acceptable to Telegram"));
    assert!(run.out.contains("1 not checked"));
    assert!(run.out.contains("proves nothing about them"));
}
// tests.test_preflight_outcomes.TheSummaryMatchesTheOutcome.test_each_category_is_reported_separately
#[test]
fn preflight_summary_each_category_is_reported_separately() {
    let run = summary(
        1,
        json!([["a", "a.png", "refused"]]),
        json!([["b", "b.png"]]),
        json!([["c", "c.png", "timeout"]]),
    );
    assert_eq!(run.code, 4);
    for text in [
        "1 accepted",
        "1 refused",
        "1 missing",
        "1 not checked",
        "REFUSED a",
        "MISSING b",
        "UNCHECKED c",
    ] {
        assert!(run.out.contains(text), "missing {text}: {}", run.out);
    }
}

fn frame(visible: usize) -> Vec<u8> {
    let mut raw = vec![0; crate::codec_video_timeline::FRAME_BYTES];
    for i in 0..visible {
        raw[i * 4 + 3] = 255;
    }
    raw
}
fn probe(raw: &[u8], hang: bool) -> Run {
    let f = Fixture::empty();
    let bin = f.root.0.join("probe-bin");
    std::fs::create_dir(&bin).unwrap();
    let ffmpeg = bin.join(if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    });
    let build = f.root.0.join("probe-build");
    std::fs::create_dir(&build).unwrap();
    let mut cmd = Command::new(option_env!("NUMERA_BASELINE_RUSTC").unwrap_or("rustc"));
    cmd.arg("--edition=2024")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/baseline_publication_support/probe_fixture.rs"),
        )
        .arg("-o")
        .arg(&ffmpeg)
        .stdout(Stdio::from(
            std::fs::File::create(build.join("stdout")).unwrap(),
        ))
        .stderr(Stdio::from(
            std::fs::File::create(build.join("stderr")).unwrap(),
        ));
    let mut compiler = OwnedProcess::spawn_session(&mut cmd, build.clone()).unwrap();
    let status = runtime()
        .block_on(compiler.wait(Duration::from_secs(60), Duration::from_secs(60)))
        .unwrap();
    let error = std::fs::read_to_string(build.join("stderr")).unwrap();
    compiler.finish().unwrap();
    assert!(status.success(), "probe fixture compile failed: {error}");
    std::fs::copy(
        &ffmpeg,
        bin.join(if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        }),
    )
    .unwrap();
    let source = f.data.join("clip.webm");
    std::fs::write(&source, b"\x1a\x45\xdf\xa3publication probe fixture").unwrap();
    let frames = f.root.0.join("frames.rgba");
    std::fs::write(&frames, raw).unwrap();
    let calls = f.root.0.join("probe-calls.txt");
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&inherited)))
        .unwrap();
    let bot = Bot::new(Model::ordinary());
    captured(
        &f,
        &bot,
        &[],
        &[],
        json!({"probe":{"path":path.to_str().unwrap(),"raw":frames,"calls":calls,"hang":hang,"source":source}}),
    )
}
// tests.test_publish_contracts.VideoBlankCheck.test_the_probe_runs_under_a_finite_wall_limit
#[test]
fn video_the_probe_runs_under_a_finite_wall_limit() {
    let raw = frame(900).repeat(2);
    let run = probe(&raw, false);
    assert_eq!(run.code, 0);
    let receipt = &run.receipt["probe"];
    assert!(
        receipt["calls"]
            .as_str()
            .unwrap()
            .lines()
            .any(|line| line.contains("|ffmpeg|"))
    );
    assert!(receipt["wall_ms"].as_u64().unwrap() > 0);
    let hanging = probe(&raw, true);
    assert_eq!(hanging.code, 0);
    assert!(
        hanging.receipt["probe"]["elapsed_ms"].as_u64().unwrap() < 5000,
        "real hanging control exceeded finite wall bound"
    );
}
// tests.test_publish_contracts.VideoBlankCheck.test_a_hanging_ffmpeg_is_killed_and_lets_the_upload_decide
#[test]
fn video_a_hanging_ffmpeg_is_killed_and_lets_the_upload_decide() {
    let run = probe(&[], true);
    assert_eq!(run.code, 0);
    assert!(
        run.receipt["probe"]["calls"]
            .as_str()
            .unwrap()
            .contains("|ffmpeg|")
    );
    assert!(run.receipt["probe"]["elapsed_ms"].as_u64().unwrap() < 5000);
}
// tests.test_publish_contracts.VideoBlankCheck.test_fade_in_video_is_accepted
#[test]
fn video_fade_in_video_is_accepted() {
    let mut raw = frame(0).repeat(2);
    raw.extend(frame(500));
    raw.extend(frame(900));
    assert_eq!(probe(&raw, false).code, 0);
}
// tests.test_publish_contracts.VideoBlankCheck.test_fully_blank_video_is_still_rejected
#[test]
fn video_fully_blank_video_is_still_rejected() {
    assert_eq!(probe(&frame(0).repeat(4), false).code, 4);
}
// tests.test_publish_contracts.VideoBlankCheck.test_a_handful_of_stray_pixels_is_still_blank
#[test]
fn video_a_handful_of_stray_pixels_is_still_blank() {
    assert_eq!(probe(&frame(3).repeat(3), false).code, 4);
}
// tests.test_publish_contracts.VideoBlankCheck.test_undecodable_output_lets_the_upload_decide
#[test]
fn video_undecodable_output_lets_the_upload_decide() {
    assert_eq!(probe(&[], false).code, 0);
}
