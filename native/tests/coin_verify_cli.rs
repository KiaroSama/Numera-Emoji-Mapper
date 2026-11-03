//! Actual logo-repair restart settles target-bound intent before any new provider lookup.
use _native::{logging::RunLog, tooling_process};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;

#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/coin_verify_commands.rs"]
mod coin_verify_commands;
#[path = "support/coin_verify_map_race.rs"]
mod coin_verify_map_race;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use bot_api::{Reply, Server};

fn document(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn target(path: &Path) -> String {
    let path = path.canonicalize().unwrap().to_string_lossy().into_owned();
    if cfg!(windows) {
        path.to_lowercase()
    } else {
        path
    }
}
struct Harness<'a> {
    root: &'a Path,
    executable: &'a Path,
    input: &'a Path,
    runtime: &'a tokio::runtime::Runtime,
    log: &'a RunLog,
}
impl Harness<'_> {
    fn capture(
        &self,
        server: &Server,
        arguments: &[std::ffi::OsString],
        overrides: &[(&str, &str)],
    ) -> tooling_process::Completed {
        let mut environment =
            _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
        environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
        environment.insert("NUMERA_TEST_PROVIDER_BASE".into(), server.base.clone());
        environment.insert("COIN_PACK_BASE".into(), "cryptoemoji".into());
        environment.insert("PACK_OWNER_USER_ID".into(), "111111111".into());
        environment.insert("TELEGRAM_BOT_TOKEN".into(), bot_api::TOKEN.into());
        for (key, value) in overrides {
            environment.insert((*key).into(), (*value).into());
        }
        let mut argv = vec![
            self.executable.as_os_str().to_owned(),
            "coin-verify-logos".into(),
        ];
        argv.extend_from_slice(arguments);
        self.runtime
            .block_on(tooling_process::capture_with_input(
                self.root,
                &argv,
                &environment,
                self.input,
                Duration::from_secs(10),
                Duration::from_secs(5),
                self.log,
            ))
            .unwrap()
    }
}
fn released(root: &Path) {
    for path in [
        _native::locks::pack_family_path(root, "cryptoemoji"),
        root.join(".locks/canonical_map.lock"),
    ] {
        drop(_native::locks::Lease::acquire(&path, "verify released").unwrap());
        assert!(path.is_file());
    }
    assert!(!_native::process_owner::cleanup_pending(root));
    assert!(
        std::fs::read_dir(root.join("logs/tmp/tooling"))
            .unwrap()
            .next()
            .is_none()
    );
}
#[test]
fn verify_command_refuses_foreign_intent_then_recovers_original_art_before_provider_work() {
    let fixture = tooling_fixture::Root::new("coin verify original commands");
    let scratch = &fixture.0;
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::fs::create_dir(scratch.join("assets")).unwrap();
    std::fs::write(scratch.join("assets/panel.html"), b"fixture marker").unwrap();
    std::fs::write(scratch.join("pyproject.toml"), b"# fixture marker\n").unwrap();
    let executable = scratch.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let input = scratch.join("command-eof");
    std::fs::write(&input, b"").unwrap();
    let log = RunLog::open(
        scratch,
        "coin-verify-command-owner",
        vec![bot_api::TOKEN.into()],
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let harness = Harness {
        root: scratch,
        executable: &executable,
        input: &input,
        runtime: &runtime,
        log: &log,
    };
    let coins = scratch.join("coins");
    let images = coins.join("logos/emoji");
    let raw_images = coins.join("logos/png");
    let stage = coins.join("logos/.incoming/original-run");
    for path in [&images, &raw_images, &stage.join("png")] {
        std::fs::create_dir_all(path).unwrap();
    }
    let source_fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    let case = |name: &str| {
        source_fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap()
    };
    let art_case = case("alpha-edge");
    let foreign_case = case("public-logo");
    let art = std::fs::read(project.join(art_case["input"].as_str().unwrap())).unwrap();
    let foreign = std::fs::read(project.join(foreign_case["input"].as_str().unwrap())).unwrap();
    assert_eq!(hash(&art), art_case["input_sha256"]);
    assert_eq!(hash(&foreign), foreign_case["input_sha256"]);
    let want = art_case["expected"]["phash"].as_u64().unwrap();
    assert!((want ^ foreign_case["expected"]["phash"].as_u64().unwrap()).count_ones() > 8);
    let prepared = stage.join("aaa.png");
    let raw = stage.join("png/aaa.png");
    std::fs::write(&prepared, &art).unwrap();
    std::fs::write(&raw, &art).unwrap();
    std::fs::write(images.join("aaa.png"), &foreign).unwrap();
    std::fs::write(raw_images.join("aaa.png"), &foreign).unwrap();
    let recovery: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-replace-recovery.json"
    ))
    .unwrap();
    let observation = &recovery["cases"][0];
    let gate: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-verify-gate.json"
    ))
    .unwrap();
    assert_eq!(gate["source_runtime"], "CPython 3.11.9");
    assert_eq!(gate["source_revision"], source_fixture["source_revision"]);
    assert_eq!(gate["source_revision"], recovery["source_revision"]);
    assert_eq!(
        gate["source_hashes"]["coins/verify_logos.py"],
        recovery["source_sha256"]
    );
    for (path, expected) in [
        (
            "coins/verify_logos.py",
            "86f17862ebe5019e05965fa7aa30380a20226b3632f192f051525290ae7f7031",
        ),
        (
            "tests/test_verify_logos.py",
            "aa1a7ae193adf684062d4318182c94f5b6a6962fb8bb0c13313bac3868690136",
        ),
        (
            "emojikit/cli_env.py",
            "9e4c0a045f031bffd23fe5ff9e51a132363e9952a1cdde47d071bfc81234e887",
        ),
    ] {
        assert_eq!(gate["source_hashes"][path], expected);
    }
    assert_eq!(gate["cases"].as_array().unwrap().len(), 3);
    let state = coins.join("rebuild_dedup_state.json");
    let mapping = coins.join("ticker_to_id.json");
    let intent_path = coins.join("verify_logos_intent.json");
    _native::atomic::write_json(&state, &json!({"sets":[{"index":1,"name":"s1"}]})).unwrap();
    _native::atomic::write_json(&mapping, &observation["initial"]).unwrap();
    let state_before = std::fs::read(&state).unwrap();
    let mapping_before = std::fs::read(&mapping).unwrap();
    let mut intent = observation["intent"].clone();
    intent["key"] = json!("aaa");
    intent["operation"] = json!("replace");
    intent["source_dhash"] = json!(want);
    intent["source"] = json!(prepared);
    intent["stage"] = json!(stage);
    intent["map_target"] = json!("different-map");
    intent["state_target"] = json!(target(&state));
    intent["promote"] = json!([
        [raw, raw_images.join("aaa.png")],
        [prepared, images.join("aaa.png")]
    ]);
    _native::atomic::write_json(&intent_path, &intent).unwrap();
    for iteration in 0..3 {
        if iteration == 1 {
            intent["map_target"] = json!(target(&mapping));
            _native::atomic::write_json(&intent_path, &intent).unwrap();
        }
        let intent_before = std::fs::read(&intent_path).unwrap();
        let mut replies = Vec::new();
        if iteration > 0 {
            replies.push(Reply::new("getStickerSet",&[("name","s1")],json!({"ok":true,"result":{"stickers":[{"custom_emoji_id":"a","file_id":"unchanged"},{"custom_emoji_id":"new","file_id":"replacement"}]}})));
            replies.push(Reply::new(
                "getFile",
                &[("file_id", "replacement")],
                json!({"ok":true,"result":{"file_path":"fixture.png"}}),
            ));
            let mut download = Reply::new("getFile", &[], Value::Null);
            download.method = "GET fixture.png".into();
            download.download = Some(if iteration == 1 {
                foreign.clone()
            } else {
                art.clone()
            });
            replies.push(download);
        }
        if iteration == 2 {
            let mut markets = Reply::new(
                "GET /api/v3/coins/markets",
                &[(
                    "query",
                    "vs_currency=usd&order=market_cap_desc&per_page=250&page=1&sparkline=false",
                )],
                json!([]),
            );
            markets.observation = Some((
                mapping.clone(),
                "".into(),
                observation["expected"]["mapping"].clone(),
            ));
            replies.push(markets);
        }
        // Hold every response so the observer checks ownership at each real request.
        let mut signals = Vec::new();
        let replies = replies
            .into_iter()
            .map(|reply| {
                let method = reply.method.clone();
                let (reply, ready, release) = reply.hold();
                signals.push((method, ready, release));
                reply
            })
            .collect();
        let mut server = Server::new(replies);
        let family = _native::locks::pack_family_path(scratch, "cryptoemoji");
        let observed_mapping = mapping.clone();
        let observed_intent = intent_path.clone();
        let observed_stage = stage.clone();
        let observed_images = images.clone();
        let before = mapping_before.clone();
        let expected = observation["expected"]["mapping"].clone();
        let expected_art = art.clone();
        let observer = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            for (method, ready, release) in signals {
                runtime.block_on(async {
                    tokio::time::timeout(Duration::from_secs(5), ready)
                        .await
                        .unwrap()
                        .unwrap();
                });
                assert!(
                    _native::locks::Lease::acquire(&family, "verify ownership observer").is_err()
                );
                if method.starts_with("GET /api/") {
                    assert_eq!(document(&observed_mapping), expected);
                    assert_eq!(
                        std::fs::read(observed_images.join("aaa.png")).unwrap(),
                        expected_art
                    );
                    assert!(!observed_intent.exists() && !observed_stage.exists());
                } else {
                    assert_eq!(std::fs::read(&observed_mapping).unwrap(), before);
                    assert!(observed_intent.is_file() && observed_stage.is_dir());
                }
                release.send(()).unwrap();
            }
        });
        let arguments = vec![
            "--emoji-dir".into(),
            images.as_os_str().to_owned(),
            "--fix".into(),
            "--only".into(),
            "aaa".into(),
        ];
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            harness.capture(&server, &arguments, &[])
        }));
        let observed = observer.join();
        let requests = server.finish();
        let result = match result {
            Ok(result) => result,
            Err(error) => std::panic::resume_unwind(error),
        };
        observed.unwrap();
        let requests = requests.unwrap();
        assert_eq!(requests.len(), [0, 3, 4][iteration]);
        assert!(requests.iter().all(|request| matches!(
            request.method.as_str(),
            "getStickerSet" | "getFile" | "GET fixture.png" | "GET /api/v3/coins/markets"
        )));
        let expected = &gate["cases"][usize::from(iteration == 2)]["expected"];
        assert_eq!(
            i64::from(result.code),
            expected["code"].as_i64().unwrap(),
            "{} {}",
            result.stdout,
            result.stderr
        );
        assert!(!result.stderr.contains(bot_api::TOKEN));
        assert_eq!(std::fs::read(&state).unwrap(), state_before);
        if iteration < 2 {
            assert!(
                result
                    .stderr
                    .contains("previous logo replacement remains unresolved")
            );
            assert_eq!(std::fs::read(&intent_path).unwrap(), intent_before);
            assert_eq!(std::fs::read(&mapping).unwrap(), mapping_before);
            assert_eq!(std::fs::read(images.join("aaa.png")).unwrap(), foreign);
            assert_eq!(std::fs::read(raw_images.join("aaa.png")).unwrap(), foreign);
            assert!(prepared.is_file() && raw.is_file());
        } else {
            assert_eq!(document(&mapping), observation["expected"]["mapping"]);
            assert_eq!(std::fs::read(images.join("aaa.png")).unwrap(), art);
            assert_eq!(std::fs::read(raw_images.join("aaa.png")).unwrap(), art);
            assert!(!intent_path.exists() && !stage.exists());
        }
        released(scratch);
    }
    coin_verify_commands::check(&harness);
    coin_verify_map_race::map_writer_repoint_before_replacement_is_not_overwritten(&harness);
    coin_verify_map_race::repair_is_excluded_by_literal_pack_family_not_script_name(&harness);
    drop(runtime);
    drop(log);
    fixture.clean().unwrap();
}
