// Individually traceable migrations of the original backend panel tests.
// Inherited MutationGuard methods are discovered from the original class AST and
// declared for each concrete class, retaining all 18 inherited executions.
use numera_runtime as runtime;
use serde_json::{Value, json};
use sha2::Digest;
use std::time::{Duration, Instant};
#[path = "baseline_panel_support/mod.rs"]
mod support;
use support::{Fixture, TOKEN, UTC};

#[path = "baseline_panel_support/mutation_guard.rs"]
mod mutation_guard;

// OriginalID: test_panel_logging.UILogContract.test_logs_actions_and_errors_but_rejects_private_or_unbounded_payloads
#[test]
fn logging_logs_actions_and_errors_but_rejects_private_or_unbounded_payloads() {
    let scratch = support::Scratch::new();
    let log = runtime::logging::RunLog::open(&scratch.0, "ui-test", vec![]);
    let mut sink = runtime::panel_events::Events::default();
    assert_eq!(
        sink.record(
            &json!({"events":[{"event":"hold","count":4},
        {"event":"error","name":"TypeError","source":"panel-grid.js","line":12}]}),
            &log
        )
        .unwrap(),
        204
    );
    let good = std::fs::read_to_string(log.path.as_ref().unwrap()).unwrap();
    assert!(good.contains("\"event\":\"hold\""));
    assert!(good.contains("\"name\":\"TypeError\""));
    for event in [
        json!({"event":"error","message":"private-test-value"}),
        json!({"event":[]}),
        json!({"event":"error","source":[]}),
        json!({"event":"hold","count":true}),
        // Non-finite JSON numbers are represented as null by serde_json and must be rejected.
        json!({"event":"hold","count":f64::NAN}),
    ] {
        assert!(sink.record(&json!({"events":[event]}), &log).is_err());
    }
    assert_eq!(
        std::fs::read_to_string(log.path.as_ref().unwrap()).unwrap(),
        good
    );
    assert!(
        sink.record(&json!({"events":vec![json!({"event":"hold"});33]}), &log)
            .is_err()
    );
}
// OriginalID: test_panel_logging.EveryScriptMayReportErrors.test_every_served_script_is_an_accepted_log_source
#[test]
fn logging_every_served_script_is_an_accepted_log_source() {
    let scratch = support::Scratch::new();
    let log = runtime::logging::RunLog::open(&scratch.0, "sources", vec![]);
    let holding =
        std::fs::read_to_string(support::project().join("assets/panel-holding.js")).unwrap();
    let html = std::fs::read_to_string(support::project().join("assets/panel.html")).unwrap();
    let mut served = std::collections::BTreeSet::new();
    for fragment in html.split("<script").skip(1) {
        let tag = fragment.split('>').next().unwrap();
        let Some((_, value)) = tag.split_once("src") else {
            continue;
        };
        let value = value.trim_start().strip_prefix('=').unwrap().trim_start();
        let quote = value.chars().next().unwrap();
        assert!(
            matches!(quote, '\'' | '"'),
            "unrecognized script URL: {tag}"
        );
        let url = value[1..].split(quote).next().unwrap();
        if let Some(name) = url.strip_prefix("/static/") {
            served.insert(name.split('?').next().unwrap().to_owned());
        }
    }
    assert!(
        !served.is_empty(),
        "the production panel has no script inventory"
    );
    assert_eq!(
        served,
        runtime::asset_version::PANEL_SCRIPTS
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
        "served panel scripts and their production cache inventory diverged"
    );
    let mut sink = runtime::panel_events::Events::default();
    for name in served {
        assert_eq!(
            sink.record(
                &json!({"events":[{"event":"error","source":name,"name":"Error"}]}),
                &log
            )
            .unwrap(),
            204
        );
        assert!(
            holding.contains(&format!("'{name}'")),
            "{name} missing from known scripts"
        );
    }
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_reload_then_save_preserves_intent_without_redefining_live_membership
#[test]
fn intent_reload_then_save_preserves_intent_without_redefining_live_membership() {
    let f = Fixture::intent();
    assert_eq!(f.save().0, 200);
    for _ in 0..2 {
        let page = f.page();
        let item = page.iter().find(|card| card["key"] == f.keys[0]).unwrap();
        assert_eq!(item["pack"], 2);
        let targets = page
            .iter()
            .filter(|v| v.get("pack").is_some())
            .map(|v| json!([v["key"], v["pack"]]))
            .collect::<Vec<_>>();
        assert_eq!(f.save_with(json!(targets), json!(f.keys), json!([])).0, 200);
        assert_eq!(
            f.plan()["moves"],
            json!([{"key":f.keys[0],"label":item["label"],"from_pack":1,"to_pack":2}])
        );
    }
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_unpublished_target_survives_reload_without_inventing_a_live_source
#[test]
fn intent_unpublished_target_survives_reload_without_inventing_a_live_source() {
    let f = Fixture::intent();
    assert_eq!(
        f.save_with(json!([[f.keys[4], 2]]), json!(f.keys), json!([]))
            .0,
        200
    );
    let page = f.page();
    assert_eq!(
        page.iter().find(|card| card["key"] == f.keys[4]).unwrap()["pack"],
        2
    );
    assert_eq!(f.plan()["moves"], json!([]));
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_partial_scope_keeps_other_tabs_moves_holds_and_targets
#[test]
fn intent_partial_scope_keeps_other_tabs_moves_holds_and_targets() {
    let f = Fixture::intent();
    assert_eq!(
        f.save_with(json!([[f.keys[0], 2]]), json!(f.keys), json!([f.keys[2]]))
            .0,
        200
    );
    let before = f.plan();
    assert_eq!(
        f.save_with(json!([[f.keys[1], 1]]), json!([f.keys[1]]), json!([]))
            .0,
        200
    );
    let after = f.plan();
    assert_eq!(after["moves"], before["moves"]);
    assert_eq!(after["held"], before["held"]);
    assert_eq!(runtime::intent::targets(&after).unwrap()[&f.keys[0]], 2);
    assert!(
        after["excluded"]
            .as_array()
            .unwrap()
            .contains(&json!(f.keys[2]))
    );
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_legacy_save_does_not_clear_target_intent
#[test]
fn intent_legacy_save_does_not_clear_target_intent() {
    let f = Fixture::intent();
    assert_eq!(f.save().0, 200);
    assert_eq!(
        f.save_with(Value::Null, json!(f.keys), json!([f.keys[1]]))
            .0,
        200
    );
    let plan = f.plan();
    assert_eq!(runtime::intent::targets(&plan).unwrap()[&f.keys[0]], 2);
    assert!(
        plan["excluded"]
            .as_array()
            .unwrap()
            .contains(&json!(f.keys[1]))
    );
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_invalid_targets_refuse_without_mutation
#[test]
fn intent_invalid_targets_refuse_without_mutation() {
    let f = Fixture::intent();
    assert_eq!(f.save().0, 200);
    let before = std::fs::read(&f.plan_path).unwrap();
    for targets in [
        json!([[f.keys[0], -1]]),
        json!([[f.keys[0], 0]]),
        json!([[f.keys[0], true]]),
        json!([[f.keys[0], 1.5]]),
        json!([[f.keys[0], 1_u64 << 54]]),
        json!([[f.keys[0], 1], [f.keys[0], 2]]),
        json!([["s:unknown", 1]]),
    ] {
        assert_eq!(
            f.save_with(targets, json!(f.keys), json!([f.keys[1]])).0,
            400
        );
        assert_eq!(std::fs::read(&f.plan_path).unwrap(), before);
        assert!(f.included(&f.keys[1]));
    }
    assert_eq!(
        f.save_with(json!([[f.keys[0], 2]]), json!([f.keys[1]]), json!([]))
            .0,
        400
    );
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_bad_saved_plan_is_preserved_instead_of_overwritten
#[test]
fn intent_bad_saved_plan_is_preserved_instead_of_overwritten() {
    let f = Fixture::intent();
    for text in [
        "null",
        "{broken",
        r#"{"version":99}"#,
        r#"{"version":1,"moves":[],"held":[],"targets":[["x",-1]]}"#,
    ] {
        std::fs::write(&f.plan_path, text).unwrap();
        assert_eq!(
            f.save_with(json!([[f.keys[0], 2]]), json!(f.keys), json!([f.keys[1]]))
                .0,
            409
        );
        assert_eq!(f.get("/", None).0, 409);
        assert_eq!(std::fs::read_to_string(&f.plan_path).unwrap(), text);
        assert!(f.included(&f.keys[1]));
    }
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_old_delta_only_plan_still_loads_and_upgrades_without_losing_moves
#[test]
fn intent_old_delta_only_plan_still_loads_and_upgrades_without_losing_moves() {
    let f = Fixture::intent();
    let old = json!({"version":1,"held":[],"moves":[{"key":f.keys[0],"label":"first","from_pack":1,"to_pack":2}]});
    std::fs::write(&f.plan_path, old.to_string()).unwrap();
    assert_eq!(f.page()[0]["pack"], 2);
    assert_eq!(
        f.save_with(json!([[f.keys[1], 1]]), json!([f.keys[1]]), json!([]))
            .0,
        200
    );
    assert_eq!(f.plan()["moves"], old["moves"]);
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_stale_server_view_after_rekey_cannot_acknowledge_old_keys
#[test]
fn intent_stale_server_view_after_rekey_cannot_acknowledge_old_keys() {
    let f = Fixture::intent();
    f.update(
        "UPDATE items SET content_key='s:changed' WHERE content_key=?",
        &f.keys[0],
    );
    assert_eq!(f.save().0, 409);
    assert!(!f.plan_path.exists());
    assert_eq!(f.post("/api/order", json!({"order":f.keys})).0, 409);
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_capacity_counts_the_logo_for_each_target_pack
#[test]
fn intent_capacity_counts_the_logo_for_each_target_pack() {
    let view = json!([{"key":"__logo__","isLogo":true,"included":true},
        {"key":"a","included":true,"pack":1},{"key":"b","included":true,"pack":1}]);
    let plan = runtime::intent::execute(json!({"operation":"merge","previous":null,"view":view,
        "targets":{"a":1,"b":1},"scope":["a","b"],"per_set":2,"written_utc":UTC}))
    .unwrap();
    assert_eq!(plan["counts"], json!({"1":2}));
    assert_eq!(plan["over_capacity"], json!({"1":3}));
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_refresh_does_not_hide_stale_identity_from_save_validation
#[test]
fn intent_refresh_does_not_hide_stale_identity_from_save_validation() {
    support::refuse_stale_scope("rekey", true);
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_refresh_does_not_acknowledge_a_draft_for_a_deleted_item
#[test]
fn intent_refresh_does_not_acknowledge_a_draft_for_a_deleted_item() {
    support::refuse_stale_scope("delete", true);
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_newly_hidden_item_is_not_intersected_out_of_a_save
#[test]
fn intent_newly_hidden_item_is_not_intersected_out_of_a_save() {
    support::refuse_stale_scope("published", true);
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_save_revalidates_visibility_even_without_another_page_load
#[test]
fn intent_save_revalidates_visibility_even_without_another_page_load() {
    support::refuse_stale_scope("published", false);
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_save_uses_current_live_pack_not_the_page_load_snapshot
#[test]
fn intent_save_uses_current_live_pack_not_the_page_load_snapshot() {
    let f = Fixture::intent();
    let key = &f.keys[0];
    f.page();
    f.cat().uploaded(&json!({"content_key":key,"custom_emoji_id":"MOVED-CID","base":"fixture","set_name":"two"}),UTC).unwrap();
    assert_eq!(
        f.save_with(json!([[key, 1]]), json!(f.keys), json!([])).0,
        200
    );
    let moves = f.plan()["moves"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| json!([row["from_pack"], row["to_pack"]]))
        .collect::<Vec<_>>();
    assert_eq!(
        moves,
        vec![json!([2, 1])],
        "a cached source pack erased the requested reverse move"
    );
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_explicit_exclusion_outside_known_scope_refuses_without_writes
#[test]
fn intent_explicit_exclusion_outside_known_scope_refuses_without_writes() {
    let f = Fixture::intent();
    assert_eq!(f.save().0, 200);
    let before = std::fs::read(&f.plan_path).unwrap();
    assert_eq!(
        f.save_with(json!([]), json!([f.keys[0]]), json!([f.keys[1]]))
            .0,
        400
    );
    assert_eq!(std::fs::read(&f.plan_path).unwrap(), before);
    assert!(f.included(&f.keys[1]));
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_actual_video_plan_survives_migration_reload_and_restore
#[test]
fn intent_actual_video_plan_survives_migration_reload_and_restore() {
    use std::collections::BTreeMap;
    let mut f = Fixture::intent();
    // Pre-encoded using the exact original _video_fixtures.encode recipe:
    // 100x100, three RED (220,20,20,255) frames at 30fps, lossless alpha VP9.
    // Fixture generation is not part of the behavior under test; migration still
    // decodes the real media through the standalone Rust codec process.
    let video = f.root().join("legacy-video.webm");
    std::fs::write(
        &video,
        include_bytes!("baseline_panel_support/legacy-red-3frames.webm"),
    )
    .unwrap();
    assert_eq!(
        sha2::Sha256::digest(std::fs::read(&video).unwrap())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        "20d43c242bba0933a8b5fc024cbbd95f83eb76074545203cec1cccef192ab405"
    );
    let old = "v:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    {
        let mut cat = f.cat();
        let db = rusqlite::Connection::open(&f.db).unwrap();
        db.execute(
            "UPDATE items SET content_key=?,format='video',file_path=? WHERE content_key=?",
            rusqlite::params![old, video.to_str().unwrap(), f.keys[4]],
        )
        .unwrap();
        cat.uploaded(&json!({"content_key":old,"custom_emoji_id":"VIDEO-CID","base":"fixture","set_name":"one"}),UTC).unwrap();
    }
    f.keys[4] = old.into();
    f.page();
    assert_eq!(
        f.save_with(json!([[old, 2]]), json!(f.keys), json!([])).0,
        200
    );
    let before = std::fs::read(&f.plan_path).unwrap();
    let config = runtime::config::Config::load_with_environment(
        f.root().to_path_buf(),
        &BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]),
    )
    .unwrap();
    let codec = support::bounded_codec::BoundedCodec {
        native: runtime::media_native_adapter::NativeMediaAdapter {
            executable: env!("CARGO_BIN_EXE_numera-emoji").into(),
            root: support::project(),
            scratch: f.root().join("codec-envelopes"),
        },
        deadline: Instant::now() + Duration::from_secs(60),
    };
    let clock = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let applied = runtime::migration_apply::apply(&config, f.root(), &codec, &clock, None).unwrap();
    let new = {
        let mut cat = f.cat();
        let video = cat
            .all(None)
            .unwrap()
            .into_iter()
            .find(|row| row["fmt"] == "video")
            .unwrap();
        let new = video["content_key"].as_str().unwrap().to_owned();
        assert_ne!(new, old);
        assert_eq!(
            cat.dispatch(json!({"operation":"custom_id","base":"fixture","content_key":new}))
                .unwrap(),
            "VIDEO-CID"
        );
        new
    };
    assert_eq!(
        f.page().iter().find(|card| card["key"] == new).unwrap()["pack"],
        2
    );
    assert_eq!(
        runtime::intent::targets(&f.plan()).unwrap(),
        BTreeMap::from([(new, 2)])
    );
    let repeated =
        runtime::migration_apply::apply(&config, f.root(), &codec, &clock, None).unwrap();
    assert_eq!(repeated["noop"], true);
    runtime::migration_restore::restore(
        f.root(),
        f.root(),
        std::path::Path::new(applied["bundle"].as_str().unwrap()),
        true,
    )
    .unwrap();
    assert_eq!(f.plan(), serde_json::from_slice::<Value>(&before).unwrap());
    assert_eq!(
        f.page().iter().find(|card| card["key"] == old).unwrap()["pack"],
        2
    );
}
// OriginalID: test_panel_startup.StartUpDoesNotWaitOnTelegram.test_a_hanging_bot_lookup_does_not_hold_the_start
#[test]
fn startup_a_hanging_bot_lookup_does_not_hold_the_start() {
    use std::net::TcpListener;
    use std::process::Stdio;
    let f = Fixture::new(1, false);
    let binary = support::copied_binary(f.root());
    let bot = support::HangingBot::new();
    let bot_port = bot.port;
    let reserve = TcpListener::bind("127.0.0.1:0").unwrap();
    let panel_port = reserve.local_addr().unwrap().port();
    drop(reserve);
    let started = Instant::now();
    let mut launch = support::command(&binary);
    launch
        .args(["panel", "--data-dir"])
        .arg(f.root())
        .args(["--port", &panel_port.to_string(), "--no-open"])
        .env("GENERAL_BOT_TOKEN", "not-secret-test-token")
        .env(
            "NUMERA_TEST_API_BASE",
            format!("http://127.0.0.1:{bot_port}"),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = support::Child(launch.spawn().unwrap());
    let deadline = started + Duration::from_secs(2);
    let url = format!("http://127.0.0.1:{panel_port}/api/ping");
    let mut ready = false;
    while Instant::now() < deadline {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "panel exited before binding"
        );
        if let Ok(response) = f
            .client
            .get(&url)
            .timeout(Duration::from_millis(100))
            .send()
            && response.status() == 200
        {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let elapsed = started.elapsed();
    let lookup_started = bot.seen.recv_timeout(Duration::from_secs(1));
    drop(bot);
    assert!(ready);
    assert!(elapsed < Duration::from_secs(2));
    assert!(lookup_started.is_ok(), "the bot lookup was never exercised");
}
// OriginalID: test_panel_startup.StartUpDoesNotWaitOnTelegram.test_a_running_panel_on_the_port_is_reopened_not_rebuilt
#[test]
fn startup_a_running_panel_on_the_port_is_reopened_not_rebuilt() {
    let f = Fixture::new(1, false);
    let binary = support::copied_binary(f.root());
    let before = f.signature();
    let mut launch = support::command(&binary);
    launch.args(["panel", "--data-dir"]).arg(f.root()).args([
        "--port",
        &f.port.to_string(),
        "--no-open",
    ]);
    let output = support::wait_output(&mut launch, Duration::from_secs(5));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Panel already running:"));
    assert_eq!(f.signature(), before);
    assert_eq!(f.get("/api/ping", None).0, 200);
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
