//! Shared family/map lock refusals are operational failures and preserve every durable artifact.
use _native::process_owner::OwnedProcess;
use serde_json::{Value, json};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
fn remap_and_logo_fix_busy_family_refuse_before_network_and_preserve_artifacts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let scratch = root
        .join("logs/tmp")
        .join(format!("coin support refusal {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    std::fs::create_dir(scratch.join("assets")).unwrap();
    std::fs::write(scratch.join("assets/panel.html"), b"fixture marker").unwrap();
    std::fs::write(scratch.join("pyproject.toml"), b"# fixture marker\n").unwrap();
    let executable = scratch.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let images = scratch.join("images");
    std::fs::create_dir(&images).unwrap();
    std::fs::write(
        images.join("aaa.png"),
        std::fs::read(root.join("tests/fixtures/native/static-codec/alpha-edge.png")).unwrap(),
    )
    .unwrap();
    let state = scratch.join("state.json");
    let map = scratch.join("map.json");
    let cache = scratch.join("cache.json");
    let candidates = scratch.join("candidates.json");
    _native::atomic::write_json(&state, &json!({"sets":[{"index":1,"name":"s1"}]})).unwrap();
    _native::atomic::write_json(&map, &json!({"aaa":"STALE","other":"preserved"})).unwrap();
    let state_before = std::fs::read(&state).unwrap();
    let map_before = std::fs::read(&map).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let lease = _native::locks::Lease::acquire(
        &_native::locks::pack_family_path(&scratch, "cryptoemoji"),
        "fixture owner",
    )
    .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for (iteration, command) in ["coin-remap-ids", "coin-verify-logos"]
        .into_iter()
        .enumerate()
    {
        let work = scratch.join(format!("owner{iteration}"));
        std::fs::create_dir(&work).unwrap();
        let out = scratch.join(format!("out{iteration}.txt"));
        let err = scratch.join(format!("err{iteration}.txt"));
        let mut cmd = Command::new(&executable);
        cmd.arg(command)
            .arg("--emoji-dir")
            .arg(&images)
            .arg("--state")
            .arg(&state);
        if iteration == 0 {
            cmd.arg("--out")
                .arg(&map)
                .arg("--cache")
                .arg(&cache)
                .arg("--candidates")
                .arg(&candidates)
                .args(["--apply", "--max-distance", "100"]);
        } else {
            cmd.arg("--map").arg(&map).args(["--fix", "--only", "aaa"]);
        }
        cmd.env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
            .env("NUMERA_TEST_API_BASE", &base)
            .env("NUMERA_TEST_PROVIDER_BASE", &base)
            .env("COIN_PACK_BASE", "cryptoemoji")
            .env("PACK_OWNER_USER_ID", "111111111")
            .stdin(Stdio::null())
            .stdout(Stdio::from(std::fs::File::create(&out).unwrap()))
            .stderr(Stdio::from(std::fs::File::create(&err).unwrap()));
        for (key, _) in std::env::vars() {
            if ["TOKEN", "SECRET", "PASSWORD", "API_KEY"]
                .iter()
                .any(|v| key.contains(v))
            {
                cmd.env_remove(key);
            }
        }
        cmd.env("TELEGRAM_BOT_TOKEN", "synthetic-support-refusal-token");
        let mut owner = OwnedProcess::spawn(&mut cmd, work.clone()).unwrap();
        let status = runtime
            .block_on(owner.wait(Duration::from_secs(10), Duration::from_secs(5)))
            .unwrap();
        owner.finish().unwrap();
        assert!(!work.exists());
        assert_eq!(
            status.code(),
            Some(4),
            "{}",
            std::fs::read_to_string(&err).unwrap()
        );
        assert_eq!(std::fs::read(&state).unwrap(), state_before);
        assert_eq!(std::fs::read(&map).unwrap(), map_before);
        assert!(
            !cache.exists()
                && !candidates.exists()
                && !map.with_extension("prebroken.json").exists()
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock,
            "lock refusal must precede every network operation"
        );
        assert!(
            !std::fs::read_to_string(&err)
                .unwrap()
                .contains("synthetic-support-refusal-token")
        );
    }
    drop(lease);
    let lease = _native::locks::Lease::acquire(
        &_native::locks::pack_family_path(&scratch, "cryptoemoji"),
        "fixture release",
    )
    .unwrap();
    drop(lease);
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-map-lock.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    let coins = scratch.join("coins");
    std::fs::create_dir(&coins).unwrap();
    let map = coins.join("ticker_to_id.json");
    let lease = _native::locks::Lease::acquire(
        &scratch.join(".locks/canonical_map.lock"),
        "fixture canonical owner",
    )
    .unwrap();
    for (index, case) in cases.iter().enumerate() {
        for (name, value) in [
            ("ticker_to_id.json", "mapping"),
            ("currency-emoji-inventory.md", "inventory"),
            ("keywords.csv", "keywords"),
        ] {
            std::fs::write(
                coins.join(name),
                case["input"][value].as_str().unwrap().as_bytes(),
            )
            .unwrap();
        }
        let before = std::fs::read(&map).unwrap();
        let work = scratch.join(format!("map-owner{index}"));
        std::fs::create_dir(&work).unwrap();
        let out = scratch.join(format!("map-out{index}.txt"));
        let err = scratch.join(format!("map-err{index}.txt"));
        let mut cmd = Command::new(&executable);
        cmd.arg(case["command"].as_str().unwrap())
            .arg("--coin-dir")
            .arg(&coins)
            .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::from(std::fs::File::create(&out).unwrap()))
            .stderr(Stdio::from(std::fs::File::create(&err).unwrap()));
        for (key, _) in std::env::vars_os() {
            if ["TOKEN", "SECRET", "PASSWORD", "API_KEY"]
                .iter()
                .any(|part| key.to_string_lossy().to_ascii_uppercase().contains(part))
            {
                cmd.env_remove(key);
            }
        }
        let mut owner = OwnedProcess::spawn(&mut cmd, work.clone()).unwrap();
        let status = runtime
            .block_on(owner.wait(Duration::from_secs(10), Duration::from_secs(5)))
            .unwrap();
        owner.finish().unwrap();
        assert!(!work.exists());
        assert_eq!(
            status.code().map(i64::from),
            case["expected"]["code"].as_i64(),
            "{}",
            std::fs::read_to_string(&err).unwrap()
        );
        assert_eq!(
            std::fs::read(&map).unwrap() == before,
            case["expected"]["mapping_unchanged"].as_bool().unwrap()
        );
        assert_eq!(
            coins.join("currency-emoji-inventory.filled.md").exists(),
            case["expected"]["output_exists"].as_bool().unwrap()
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        let errors = std::fs::read_to_string(&err).unwrap();
        assert!(
            errors.contains("canonical coin map is busy or unavailable"),
            "{errors}"
        );
    }
    drop(lease);
    let released = _native::locks::Lease::acquire(
        &scratch.join(".locks/canonical_map.lock"),
        "fixture canonical release",
    )
    .unwrap();
    drop(released);
    drop(listener);
    std::fs::remove_dir_all(scratch).unwrap();
}
