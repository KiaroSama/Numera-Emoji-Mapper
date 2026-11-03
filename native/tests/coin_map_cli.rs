//! Additive native coin commands exercise actual entry points on owned fixtures only.
use _native::{logging::RunLog, tooling_process};
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

#[test]
fn native_alias_and_chain_commands_refuse_unknown_options_and_write_locked_maps() {
    let fixture = tooling_fixture::Root::new("coin map original commands");
    let scratch = &fixture.0;
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
    let log = RunLog::open(scratch, "coin-map-command-owner", Vec::new());
    let coins = scratch.join("coins");
    std::fs::create_dir(&coins).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let inventory = "## Bitcoin - Bitcoin\nticker: btc\npremium-id:\n## Wrapped - Bitcoin (Token)\nticker: btctrc20\npremium-id:\n";
    std::fs::write(
        coins.join("currency-emoji-inventory.md"),
        inventory.as_bytes(),
    )
    .unwrap();
    std::fs::write(
        coins.join("keywords.csv"),
        b"ticker,name,format,file,keywords\r\nbtc,Bitcoin,png,btc.png,btc\r\n",
    )
    .unwrap();
    for (index, command, arguments, expected) in [
        (0, "coin-alias-map", vec!["--typo".into()], 2),
        (1, "coin-alias-map", vec!["--help".into()], 0),
        (4, "coin-build-keywords", vec!["--help".into()], 0),
        (5, "coin-build-keywords", vec!["--typo".into()], 2),
        (6, "coin-fetch-logos", vec!["--help".into()], 0),
        (7, "coin-fetch-logos", vec!["0".into()], 2),
        (8, "coin-fetch-logos", vec!["notanumber".into()], 2),
        (9, "coin-fetch-logos", vec!["--typo".into()], 2),
        (36, "coin-rebuild", vec!["--help".into()], 0),
        (37, "coin-rebuild", vec!["buid".into()], 2),
        (38, "coin-rebuild", vec!["--token-env".into()], 2),
        (39, "coin-rebuild", vec!["build".into()], 2),
        (33, "coin-rebuild-map", vec!["--help".into()], 0),
        (34, "coin-rebuild-map", vec!["--typo".into()], 2),
        (35, "coin-rebuild-map", vec!["--coin-dir".into()], 2),
        (28, "coin-verify-logos", vec!["--help".into()], 0),
        (29, "coin-verify-logos", vec!["--typo".into()], 2),
        (30, "coin-verify-logos", vec!["--fix".into()], 2),
        (
            31,
            "coin-verify-logos",
            vec![
                "--emoji-dir".into(),
                coins.to_string_lossy().into_owned(),
                "--fix".into(),
            ],
            2,
        ),
        (
            32,
            "coin-verify-logos",
            vec![
                "--emoji-dir".into(),
                coins.to_string_lossy().into_owned(),
                "--top".into(),
                "0".into(),
            ],
            0,
        ),
        (24, "coin-remap-ids", vec!["--help".into()], 0),
        (25, "coin-remap-ids", vec!["--apply".into()], 2),
        (
            26,
            "coin-remap-ids",
            vec!["--emoji-dir".into(), ".".into(), "--apply".into()],
            2,
        ),
        (27, "coin-remap-ids", vec!["--typo".into()], 2),
        (21, "coin-check-packs", vec!["--help".into()], 0),
        (22, "coin-check-packs", vec!["--typo".into()], 2),
        (23, "coin-check-packs", vec!["--coin-dir".into()], 2),
        (18, "coin-write-manifests", vec!["--help".into()], 0),
        (19, "coin-write-manifests", vec!["--typo".into()], 2),
        (20, "coin-write-manifests", vec!["--map".into()], 2),
        (10, "coin-fetch-paprika", vec!["--help".into()], 0),
        (11, "coin-fetch-cmc", vec!["--help".into()], 0),
        (12, "coin-fetch-paprika", vec!["--dryy".into()], 2),
        (13, "coin-fetch-cmc", vec!["--dr".into()], 2),
        (14, "coin-fetch-cmc", vec!["--retry-unmatched".into()], 2),
        (15, "coin-fetch-paprika", vec![], 2),
        (
            16,
            "coin-fetch-paprika",
            vec![
                "--dry".into(),
                "--coin-dir".into(),
                coins.to_string_lossy().into_owned(),
            ],
            0,
        ),
        (
            17,
            "coin-fetch-cmc",
            vec![
                "--dry".into(),
                "--coin-dir".into(),
                coins.to_string_lossy().into_owned(),
            ],
            0,
        ),
        (
            2,
            "coin-alias-map",
            vec!["--coin-dir".into(), coins.to_string_lossy().into_owned()],
            0,
        ),
        (
            3,
            "coin-enhance-map",
            vec!["--coin-dir".into(), coins.to_string_lossy().into_owned()],
            0,
        ),
    ] {
        _native::atomic::write_json(
            &coins.join("ticker_to_id.json"),
            &json!({"btc":"111111111","other":"preserved"}),
        )
        .unwrap();
        let arguments: Vec<String> = arguments;
        let inventory_text = if index >= 16 {
            "## Bitcoin - Bitcoin\nticker: btc\npremium-id:\n"
        } else {
            inventory
        };
        std::fs::write(
            coins.join("currency-emoji-inventory.md"),
            inventory_text.as_bytes(),
        )
        .unwrap();
        let mut observer = bot_api::Server::new(vec![]);
        let mut environment =
            _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
        environment.insert("NUMERA_TEST_API_BASE".into(), observer.base.clone());
        environment.insert("NUMERA_TEST_PROVIDER_BASE".into(), observer.base.clone());
        if index == 17 {
            environment.insert("CMC_API_KEY".into(), "synthetic-cli-key".into());
        }
        let mut argv = vec![executable.as_os_str().to_owned(), command.into()];
        argv.extend(arguments.into_iter().map(std::ffi::OsString::from));
        println!("native coin map command admission: {index} {command}");
        let result = runtime
            .block_on(tooling_process::capture_with_input(
                scratch,
                &argv,
                &environment,
                &input,
                Duration::from_secs(10),
                Duration::from_secs(5),
                &log,
            ))
            .unwrap();
        assert_eq!(result.code, expected, "{} {}", result.stdout, result.stderr);
        assert!(!result.stderr.contains("Traceback"));
        assert!(observer.finish().unwrap().is_empty());
        assert!(!_native::process_owner::cleanup_pending(scratch));
        let mapping: Value =
            serde_json::from_slice(&std::fs::read(coins.join("ticker_to_id.json")).unwrap())
                .unwrap();
        assert_eq!(mapping["btc"], "111111111");
        assert_eq!(mapping["other"], "preserved");
        if index == 2 || index == 3 {
            assert_eq!(mapping["btctrc20"], "111111111");
            let filled =
                std::fs::read_to_string(coins.join("currency-emoji-inventory.filled.md")).unwrap();
            assert_eq!(filled.matches("premium-id: 111111111").count(), 2);
        } else {
            assert!(mapping.get("btctrc20").is_none());
        }
    }
    original_map_assertions(scratch, &executable, &input, &runtime, &log);
    log.finish(0);
    assert!(!_native::process_owner::cleanup_pending(scratch));
    assert!(
        std::fs::read_dir(scratch.join("logs/tmp/tooling"))
            .unwrap()
            .next()
            .is_none()
    );
    fixture.clean().unwrap();
}

fn original_map_assertions(
    root: &Path,
    executable: &Path,
    input: &Path,
    runtime: &tokio::runtime::Runtime,
    log: &RunLog,
) {
    for (index, command, held) in [
        (0, "coin-alias-map", false),
        (1, "coin-alias-map", true),
        (2, "coin-enhance-map", false),
        (3, "coin-enhance-map", true),
    ] {
        let data = root.join(format!("original-map-{index}"));
        std::fs::create_dir(&data).unwrap();
        let path = data.join("ticker_to_id.json");
        let alias = command == "coin-alias-map";
        let initial = if alias {
            json!({"aaa":"111","bbb":"222","ddd":"333","zzz":"written-by-the-other-tool"})
        } else {
            json!({"btc":"111","zzz":"written-by-the-other-tool"})
        };
        _native::atomic::write_json(&path, &initial).unwrap();
        let inventory = if alias {
            "## 🟡 — Foo Protocol\n   ticker: ccc\n   premium-id:\n\n## 🟢 — Bar Coin\n   ticker: eee\n   premium-id:\n"
        } else {
            "   ticker: btcbsc\n   premium-id:\n"
        };
        std::fs::write(
            data.join("currency-emoji-inventory.md"),
            inventory.as_bytes(),
        )
        .unwrap();
        std::fs::write(data.join("keywords.csv"), if alias {
            b"ticker,name,format,file,keywords\naaa,Foo Network,svg,logos/svg/aaa.svg,aaa\nbbb,Foo Token,svg,logos/svg/bbb.svg,bbb\nddd,Bar Labs,svg,logos/svg/ddd.svg,ddd\n".as_slice()
        } else { b"ticker,name\nbtc,Bitcoin\n".as_slice() }).unwrap();
        let before = std::fs::read(&path).unwrap();
        let lock_path = root.join(".locks/canonical_map.lock");
        let lease =
            held.then(|| _native::locks::Lease::acquire(&lock_path, "original held map").unwrap());
        let mut observer = bot_api::Server::new(vec![]);
        let mut environment =
            _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
        environment.insert("NUMERA_TEST_API_BASE".into(), observer.base.clone());
        environment.insert("NUMERA_TEST_PROVIDER_BASE".into(), observer.base.clone());
        let argv = vec![
            executable.as_os_str().to_owned(),
            command.into(),
            "--coin-dir".into(),
            data.as_os_str().to_owned(),
        ];
        let result = runtime
            .block_on(tooling_process::capture_with_input(
                root,
                &argv,
                &environment,
                input,
                Duration::from_secs(6),
                Duration::from_secs(3),
                log,
            ))
            .unwrap();
        assert_eq!(
            result.code,
            if held { 4 } else { 0 },
            "{} {}",
            result.stdout,
            result.stderr
        );
        assert!(!result.stderr.contains("Traceback"));
        assert!(observer.finish().unwrap().is_empty());
        let filled = data.join("currency-emoji-inventory.filled.md");
        if held {
            assert_eq!(std::fs::read(&path).unwrap(), before);
            assert!(!filled.exists());
            assert!(_native::locks::Lease::acquire(&lock_path, "still held").is_err());
        } else {
            let mut expected = initial;
            expected[if alias { "eee" } else { "btcbsc" }] =
                json!(if alias { "333" } else { "111" });
            let mapping: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(mapping, expected);
            let text = std::fs::read_to_string(&filled)
                .unwrap()
                .replace("\r\n", "\n");
            if alias {
                assert!(mapping.get("ccc").is_none());
                assert!(result.stdout.contains("AMBIGUOUS ccc"));
                assert!(result.stdout.contains("aaa, bbb"));
                assert!(text.contains("ticker: ccc\n   premium-id:\n"));
                assert!(text.contains("ticker: eee\n   premium-id: 333\n"));
            } else {
                assert_eq!(text, "   ticker: btcbsc\n   premium-id: 111\n");
            }
        }
        drop(lease);
        assert!(lock_path.is_file());
        drop(_native::locks::Lease::acquire(&lock_path, "original map release").unwrap());
        assert_eq!(
            std::fs::read(data.join("currency-emoji-inventory.md")).unwrap(),
            inventory.as_bytes()
        );
        assert_eq!(
            std::fs::read_dir(&data).unwrap().count(),
            if held { 3 } else { 4 }
        );
        assert!(!_native::process_owner::cleanup_pending(root));
    }
}
