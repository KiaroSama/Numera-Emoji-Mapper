//! Rebuild completion at the actual family-acquire boundary, followed by real HTTP effects.
use crate as rt;
use crate::baseline_coin_provider_support::*;
use serde_json::json;
use std::{collections::BTreeMap, time::Duration};
const TEST: &str = "coin_verify_cli::baseline_verify_tests::test_the_set_list_is_read_after_the_pack_lock_not_before";
// tests.test_verify_logos.VerifyLogosMainContracts.test_the_set_list_is_read_after_the_pack_lock_not_before
#[test]
fn test_the_set_list_is_read_after_the_pack_lock_not_before() {
    if let Some(root) = std::env::var_os("NUMERA_VERIFY_FAMILY_CHILD") {
        let root = std::path::PathBuf::from(root);
        let config = config(
            &root,
            &[
                (
                    "NUMERA_TEST_API_BASE",
                    &std::env::var("NUMERA_TEST_API_BASE").unwrap(),
                ),
                ("TELEGRAM_BOT_TOKEN", TOKEN),
                ("PACK_OWNER_USER_ID", "7"),
            ],
        );
        let args = ids(&[
            "--emoji-dir",
            root.join("emoji").to_str().unwrap(),
            "--map",
            root.join("ticker_to_id.json").to_str().unwrap(),
            "--state",
            root.join("state.json").to_str().unwrap(),
            "--fix",
            "--only",
            "btc",
        ]);
        let mut reached = false;
        let result = super::run_with_family(
            &config,
            &args,
            &rt::logging::RunLog::open(&root, "verify-family", vec![TOKEN.into()]),
            |path| {
                assert_eq!(path, rt::locks::pack_family_path(&root, "cryptoemoji"));
                let lease = rt::locks::Lease::acquire(path, "actual family lease after rebuild")?;
                write(
                    &root.join("state.json"),
                    &json!({"sets":[{"name":"rebuilt_pack_1","index":1}]}),
                );
                reached = true;
                Ok(lease)
            },
        )
        .unwrap();
        assert!(reached);
        assert_eq!(result, 0);
        return;
    }
    let f = Verify::new();
    write(&f.files.map, &json!({"btc":OLD}));
    write(
        &f.files.state,
        &json!({"sets":[{"name":"old_pack_1","index":1}]}),
    );
    let mut listed = listing(&f.before());
    listed.action = Some(Box::new(|r| assert_eq!(r.form["name"], "rebuilt_pack_1")));
    let mut mutate = reply("replaceStickerInSet", ok(json!(true)));
    mutate.action = Some(Box::new(|r| assert_eq!(r.form["name"], "rebuilt_pack_1")));
    let mut script = vec![
        json_reply(
            "/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page=250&page=1&sparkline=false",
            200,
            json!([{"symbol":"btc","name":"Bitcoin","image":"https://fixture.invalid/btc.png"}]),
        ),
        provider_reply("/btc.png", 200, &noise("official-btc")),
        listed,
        mutate,
        listing(&f.after()),
    ];
    uploaded_download(&mut script, &format!("fid-{NEW}"));
    let server = Server::new(script);
    let mut env = BTreeMap::new();
    for key in [
        "NUMERA_RUNTIME_TEST_ADMISSION",
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "LD_LIBRARY_PATH",
    ] {
        if let Ok(v) = std::env::var(key) {
            env.insert(key.into(), v);
        }
    }
    env.extend([
        (
            "NUMERA_VERIFY_FAMILY_CHILD".into(),
            f.root.0.to_string_lossy().into_owned(),
        ),
        ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
        ("NUMERA_TEST_API_BASE".into(), server.base.clone()),
        ("NUMERA_TEST_PROVIDER_BASE".into(), server.base.clone()),
    ]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let completed = runtime
        .block_on(rt::tooling_process::capture(
            &f.root.0,
            &[
                std::env::current_exe().unwrap().into_os_string(),
                "--exact".into(),
                TEST.into(),
                "--nocapture".into(),
            ],
            &env,
            Duration::from_secs(20),
            Duration::from_secs(12),
            &f.root.log(),
        ))
        .unwrap();
    assert_eq!(
        completed.code, 0,
        "{}{}",
        completed.stdout, completed.stderr
    );
    assert!(!completed.stdout.contains(TOKEN));
    assert!(!completed.stderr.contains(TOKEN));
    let requests = server.finish();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method == "replaceStickerInSet")
            .count(),
        1
    );
    assert!(
        requests
            .iter()
            .filter(|r| r.method == "getStickerSet")
            .all(|r| r.form["name"] == "rebuilt_pack_1")
    );
    assert_eq!(doc(&f.files.map)["btc"], NEW);
    assert!(!f.files.intent.exists());
}
