//! Genuine owned external CLI and anonymous stdin, using a standalone Rust observer.
use super::*;
use std::{hash::Hasher, time::Duration};
fn fixture() -> (tempfile::TempDir, Settings) {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("worker/node_modules/wrangler/bin")).unwrap();
    std::fs::create_dir(root.path().join("bin")).unwrap();
    std::fs::write(
        root.path()
            .join("worker/node_modules/wrangler/bin/wrangler.js"),
        "explicit external CLI stand-in\n",
    )
    .unwrap();
    std::fs::write(root.path().join("owned.env"), "GENERAL_BOT_TOKEN=synthetic_general_never_issued\nTELEGRAM_BOT_TOKEN=synthetic_coin_never_issued\nPACK_LINKS_CHAT_ID=@owned_channel\nLOG_CHAT_ID=-1001234567\nPACK_OWNER_USER_ID=42\nBOT_ALLOWED_USER_IDS=7\n").unwrap();
    let observer = root
        .path()
        .join("bin")
        .join(if cfg!(windows) { "node.exe" } else { "node" });
    let mut compiler =
        std::process::Command::new(option_env!("NUMERA_BASELINE_RUSTC").unwrap_or("rustc"));
    compiler
        .args(["--edition=2024", "--crate-name", "worker_node_observer"])
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/worker_admin_support/node_observer.rs"),
        )
        .arg("-o")
        .arg(&observer)
        .stdin(std::process::Stdio::null());
    assert!(
        owned_status(root.path(), compiler, &Settings::native()).unwrap(),
        "real Rust compiler admits external observer"
    );
    let mut settings = Settings::native();
    settings.environment.insert(
        "PATH".into(),
        root.path().join("bin").to_string_lossy().into_owned(),
    );
    settings.environment.insert(
        "NUMERA_TEST_WORKER_OBSERVER_ROOT".into(),
        root.path().join("worker").to_string_lossy().into_owned(),
    );
    (root, settings)
}
fn invoke(root: &Path, settings: &Settings, flags: &[&str]) -> (Result<u8, String>, String) {
    let mut args = vec![
        "secrets".into(),
        "--env-file".into(),
        root.join("owned.env").to_string_lossy().into_owned(),
    ];
    args.extend(flags.iter().map(|v| (*v).into()));
    let mut output = vec![];
    let result = execute_with(root, parse(&args).unwrap(), settings, &mut |line| {
        output.push(line.to_owned())
    });
    (result, output.join("\n"))
}
fn records(root: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(root.join("worker/observer-metadata.jsonl"))
        .unwrap()
        .lines()
        .map(|v| serde_json::from_str(v).unwrap())
        .collect()
}
#[test]
fn eight_ordered_puts_use_anonymous_seekable_stdin_and_suppress_noisy_cli() {
    let (root, settings) = fixture();
    let (result, output) = invoke(root.path(), &settings, &[]);
    assert_eq!(result.unwrap(), 0);
    let env = Environment::load(root.path(), Some(&root.path().join("owned.env"))).unwrap();
    let plan = [
        ("GENERAL_BOT_TOKEN", "synthetic_general_never_issued"),
        ("COIN_BOT_TOKEN", "synthetic_coin_never_issued"),
        ("PACK_LINKS_CHAT_ID", "@owned_channel"),
        ("LOG_CHAT_ID", "-1001234567"),
        ("ADMIN_USER_IDS", "42,7"),
        ("PUBLISH_SECRET", env.get("WORKER_PUBLISH_SECRET")),
        ("GENERAL_WEBHOOK_SECRET", env.get("GENERAL_WEBHOOK_SECRET")),
        ("COIN_WEBHOOK_SECRET", env.get("COIN_WEBHOOK_SECRET")),
    ];
    let generated = &plan[5..];
    assert_eq!(
        generated
            .iter()
            .map(|(_, value)| *value)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        3
    );
    for (_, value) in generated {
        assert_eq!(URL_SAFE_NO_PAD.decode(value).unwrap().len(), 32);
    }
    let calls = records(root.path());
    assert_eq!(calls.len(), 8);
    for (call, (key, value)) in calls.iter().zip(plan) {
        assert_eq!(call["key"], key);
        assert_eq!(call["regular"], true);
        assert_eq!(call["seekable"], true);
        #[cfg(target_os = "linux")]
        assert_eq!(call["unlinked"], true);
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        hash.write(format!("{value}\n").as_bytes());
        assert_eq!(call["checksum"], format!("{:016x}", hash.finish()));
        assert_eq!(call["bytes"], value.len() + 1);
        assert!(!output.contains(value));
        assert!(!call["argv"].to_string().contains(value));
    }
    assert!(
        !root
            .path()
            .join("worker/unexpected-log-enabled.txt")
            .exists()
    );
    let first = std::fs::read(root.path().join("owned.env")).unwrap();
    assert_eq!(invoke(root.path(), &settings, &[]).0.unwrap(), 0);
    assert_eq!(std::fs::read(root.path().join("owned.env")).unwrap(), first);
    assert_eq!(records(root.path()).len(), 16);
    assert_eq!(
        std::fs::read_dir(root.path().join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
}
#[test]
fn failed_put_stops_in_order_and_never_persists_generated_values() {
    let (root, mut settings) = fixture();
    let before = std::fs::read(root.path().join("owned.env")).unwrap();
    settings.environment.insert(
        "NUMERA_TEST_WORKER_FAIL_KEY".into(),
        "PUBLISH_SECRET".into(),
    );
    let error = invoke(root.path(), &settings, &[]).0.unwrap_err();
    assert!(error.contains("PUBLISH_SECRET"));
    assert_eq!(records(root.path()).len(), 6);
    assert_eq!(
        std::fs::read(root.path().join("owned.env")).unwrap(),
        before
    );
    assert!(!root.path().join("worker/.dev.vars").exists());
}
#[test]
fn bounded_put_terminates_its_real_owned_descendant_and_keeps_environment() {
    let (root, mut settings) = fixture();
    let before = std::fs::read(root.path().join("owned.env")).unwrap();
    settings.environment.insert(
        "NUMERA_TEST_WORKER_BLOCK_KEY".into(),
        "GENERAL_BOT_TOKEN".into(),
    );
    settings.wall = Duration::from_secs(2);
    settings.idle = Duration::from_secs(1);
    assert!(
        invoke(root.path(), &settings, &[])
            .0
            .unwrap_err()
            .contains("bounds")
    );
    assert_eq!(records(root.path()).len(), 1);
    let pid: u32 = std::fs::read_to_string(root.path().join("worker/descendant.pid"))
        .unwrap()
        .parse()
        .unwrap();
    assert!(pid > 0);
    #[cfg(target_os = "linux")]
    {
        let birth = std::fs::read_to_string(root.path().join("worker/descendant.birth")).unwrap();
        let path = Path::new("/proc").join(pid.to_string()).join("stat");
        match std::fs::read_to_string(path) {
            Ok(stat) => {
                let fields = stat
                    .rsplit_once(") ")
                    .expect("acknowledged proc identity")
                    .1
                    .split_whitespace()
                    .collect::<Vec<_>>();
                assert_eq!(
                    fields.get(19).copied(),
                    Some(birth.as_str()),
                    "must not treat a reused PID as acknowledged custody"
                );
                assert_eq!(
                    fields.first().copied(),
                    Some("Z"),
                    "the acknowledged descendant must be terminal; enclosing guard owns adopted reaping"
                );
            }
            Err(error) => assert_eq!(
                error.kind(),
                std::io::ErrorKind::NotFound,
                "unknown owned process state is not settled custody"
            ),
        }
    }
    assert_eq!(
        std::fs::read(root.path().join("owned.env")).unwrap(),
        before
    );
    assert_eq!(
        std::fs::read_dir(root.path().join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
}
#[test]
fn dev_vars_requires_actual_git_ignore_and_does_not_persist_generated_env() {
    let (root, mut settings) = fixture();
    let before = std::fs::read(root.path().join("owned.env")).unwrap();
    // Use the actual installed Git with isolated configuration; no operator repository/config reads.
    settings.environment = Settings::native().environment;
    settings
        .environment
        .insert("GIT_CONFIG_NOSYSTEM".into(), "1".into());
    let global = root.path().join("owned.gitconfig");
    std::fs::write(&global, "").unwrap();
    settings.environment.insert(
        "GIT_CONFIG_GLOBAL".into(),
        global.to_string_lossy().into_owned(),
    );
    let mut git = std::process::Command::new("git");
    git.current_dir(root.path())
        .args(["init", "--template="])
        .stdin(std::process::Stdio::null());
    assert!(owned_status(root.path(), git, &settings).unwrap());
    assert!(
        invoke(root.path(), &settings, &["--dev-vars"])
            .0
            .unwrap_err()
            .contains("check-ignore")
    );
    assert!(!root.path().join("worker/.dev.vars").exists());
    std::fs::write(root.path().join(".gitignore"), "/worker/.dev.vars\n").unwrap();
    let (result, output) = invoke(root.path(), &settings, &["--dev-vars"]);
    assert_eq!(result.unwrap(), 0);
    assert!(output.contains("3 secret(s)"));
    assert!(output.contains(".env is unchanged"));
    let dev = Environment::load(root.path(), Some(&root.path().join("worker/.dev.vars"))).unwrap();
    assert_eq!(dev.get("ADMIN_USER_IDS"), "42,7");
    for key in [
        "PUBLISH_SECRET",
        "GENERAL_WEBHOOK_SECRET",
        "COIN_WEBHOOK_SECRET",
    ] {
        assert_eq!(dev.get(key).len(), 43);
        assert!(
            dev.get(key)
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        );
    }
    assert_eq!(dev.values.len(), 8);
    assert_eq!(
        std::fs::read(root.path().join("owned.env")).unwrap(),
        before
    );
    assert!(!root.path().join("worker/observer-metadata.jsonl").exists());
}

#[test]
fn generated_key_commit_preserves_a_real_concurrent_unrelated_env_edit() {
    let (root, mut settings) = fixture();
    settings.environment.insert(
        "NUMERA_TEST_WORKER_EDIT_ENV".into(),
        root.path().join("owned.env").to_string_lossy().into_owned(),
    );
    assert_eq!(invoke(root.path(), &settings, &[]).0.unwrap(), 0);
    assert_eq!(records(root.path()).len(), 8);
    let actual = std::fs::read_to_string(root.path().join("owned.env")).unwrap();
    assert!(
        actual
            .lines()
            .any(|line| line == "UNRELATED_CONCURRENT_EDIT=preserve-me"),
        "the actual after-put concurrent writer must survive generated-key commit"
    );
    for key in [
        "WORKER_PUBLISH_SECRET",
        "GENERAL_WEBHOOK_SECRET",
        "COIN_WEBHOOK_SECRET",
    ] {
        assert!(
            actual.lines().any(
                |line| line.starts_with(&format!("{key}=")) && line.len() == key.len() + 1 + 43
            )
        );
    }
}
