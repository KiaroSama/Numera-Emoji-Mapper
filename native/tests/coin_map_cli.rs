//! Additive native coin commands exercise actual entry points on owned fixtures only.
use _native::process_owner::OwnedProcess;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
fn native_alias_and_chain_commands_refuse_unknown_options_and_write_locked_maps() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let scratch = root
        .join("logs/tmp")
        .join(format!("coin map cli {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
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
        let input = if index >= 16 {
            "## Bitcoin - Bitcoin\nticker: btc\npremium-id:\n"
        } else {
            inventory
        };
        std::fs::write(coins.join("currency-emoji-inventory.md"), input.as_bytes()).unwrap();
        let work = scratch.join(format!("run{index}"));
        std::fs::create_dir(&work).unwrap();
        let output = scratch.join(format!("stdout{index}.txt"));
        let errors = scratch.join(format!("stderr{index}.txt"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_numera-emoji"));
        cmd.arg(command)
            .args(arguments)
            .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
            .env("COIN_PACK_BASE", "cryptoemoji")
            .env("COIN_PACK_TITLE", "Fixture")
            .env_remove("TELEGRAM_BOT_TOKEN")
            .env_remove("CMC_API_KEY")
            .stdin(Stdio::null())
            .stdout(Stdio::from(std::fs::File::create(&output).unwrap()))
            .stderr(Stdio::from(std::fs::File::create(&errors).unwrap()));
        if index == 17 {
            cmd.env("CMC_API_KEY", "synthetic-cli-key");
        }
        let mut owner = OwnedProcess::spawn(&mut cmd, work).unwrap();
        let code = runtime
            .block_on(owner.wait(Duration::from_secs(10), Duration::from_secs(5)))
            .unwrap()
            .code()
            .unwrap();
        owner.finish().unwrap();
        assert_eq!(
            code,
            expected,
            "{}",
            std::fs::read_to_string(&errors).unwrap()
        );
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
    std::fs::remove_dir_all(&scratch).unwrap();
}
