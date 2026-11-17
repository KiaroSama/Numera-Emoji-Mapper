//! New Worker administration behavior contracts, separate from the1516 original identities.
use numera_runtime::{logging::RunLog, tooling_process};
use std::{collections::BTreeMap, ffi::OsString, path::PathBuf, time::Duration};
const GENERAL: &str = "1111111111:synthetic_general_never_issued_aaaaaaaa";
const COIN: &str = "2222222222:synthetic_coin_never_issued_bbbbbbbbb";
const PUBLISH: &str = "synthetic_publish_never_issued_333333333333333";
const GENERAL_HOOK: &str = "synthetic_general_hook_never_issued_444444444";
const COIN_HOOK: &str = "synthetic_coin_hook_never_issued_5555555555555";
fn binary() -> PathBuf {
    option_env!("CARGO_BIN_EXE_numera-worker-admin")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/debug")
                .join(if cfg!(windows) {
                    "numera-worker-admin.exe"
                } else {
                    "numera-worker-admin"
                })
        })
}
fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("assets")).unwrap();
    std::fs::write(
        root.path().join("assets/panel.html"),
        "owned project marker",
    )
    .unwrap();
    std::fs::create_dir(root.path().join("worker")).unwrap();
    std::fs::write(
        root.path().join("worker/wrangler.toml"),
        "name = \"owned-never-deployed\"\n",
    )
    .unwrap();
    root
}
fn run(root: &tempfile::TempDir, args: &[&str]) -> tooling_process::Completed {
    let name = if cfg!(windows) {
        "numera-worker-admin.exe"
    } else {
        "numera-worker-admin"
    };
    let executable = root.path().join(name);
    if !executable.exists() && std::fs::hard_link(binary(), &executable).is_err() {
        std::fs::copy(binary(), &executable).expect("dedicated Worker admin binary admission");
    }
    let mut env = BTreeMap::new();
    for key in [
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "LD_LIBRARY_PATH",
        "NUMERA_RUNTIME_TEST_ADMISSION",
    ] {
        if let Ok(value) = std::env::var(key) {
            env.insert(key.into(), value);
        }
    }
    env.insert("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into());
    let mut argv = vec![executable.into_os_string()];
    argv.extend(args.iter().map(OsString::from));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime
        .block_on(tooling_process::capture(
            root.path(),
            &argv,
            &env,
            Duration::from_secs(10),
            Duration::from_secs(5),
            &RunLog::open(
                root.path(),
                "worker-admin-contract",
                vec![
                    GENERAL.into(),
                    COIN.into(),
                    PUBLISH.into(),
                    GENERAL_HOOK.into(),
                    COIN_HOOK.into(),
                ],
            ),
        ))
        .unwrap()
}
// Break caught: missing native command, wrong admin composition, dry-run side effects, or credential disclosure.
#[test]
fn dry_run_composes_worker_admins_without_writing_or_starting_children() {
    let root = fixture();
    let env = root.path().join("operator-private.env");
    let original = format!(
        "GENERAL_BOT_TOKEN={GENERAL}\nTELEGRAM_BOT_TOKEN={COIN}\nPACK_LINKS_CHAT_ID=@owned_synthetic_channel\nLOG_CHAT_ID=-1001234567\nPACK_OWNER_USER_ID=42\nBOT_ALLOWED_USER_IDS=42,7;42 invalid 00 ١٢ 9007199254740992\nWORKER_PUBLISH_SECRET={PUBLISH}\nGENERAL_WEBHOOK_SECRET={GENERAL_HOOK}\nCOIN_WEBHOOK_SECRET={COIN_HOOK}\n"
    );
    std::fs::write(&env, &original).unwrap();
    let result = run(
        &root,
        &["secrets", "--env-file", env.to_str().unwrap(), "--dry-run"],
    );
    assert_eq!(
        result.code, 0,
        "native Worker secrets dry-run must succeed without a local Wrangler installation"
    );
    let output = format!("{}{}", result.stdout, result.stderr);
    for key in [
        "GENERAL_BOT_TOKEN",
        "COIN_BOT_TOKEN",
        "PACK_LINKS_CHAT_ID",
        "LOG_CHAT_ID",
        "ADMIN_USER_IDS",
        "PUBLISH_SECRET",
        "GENERAL_WEBHOOK_SECRET",
        "COIN_WEBHOOK_SECRET",
    ] {
        assert!(output.contains(key), "planned key must be reported");
    }
    assert!(output.contains("2 id(s)"));
    assert!(output.contains("4 admin id(s)"));
    assert!(output.to_lowercase().contains("dry run"));
    for secret in [GENERAL, COIN, PUBLISH, GENERAL_HOOK, COIN_HOOK] {
        assert!(
            !output.contains(secret),
            "configured values must never reach stdout/stderr"
        );
    }
    assert_eq!(std::fs::read_to_string(env).unwrap(), original);
    assert!(!root.path().join("worker/.dev.vars").exists());
    assert!(!root.path().join("worker/node_modules").exists());
    assert!(!numera_runtime::process_owner::cleanup_pending(root.path()));
}

fn supplied_environment() -> String {
    format!(
        "GENERAL_BOT_TOKEN={GENERAL}\nTELEGRAM_BOT_TOKEN={COIN}\nPACK_LINKS_CHAT_ID=@owned_synthetic_channel\nLOG_CHAT_ID=-1001234567\nPACK_OWNER_USER_ID=42\nBOT_ALLOWED_USER_IDS=7\nWORKER_PUBLISH_SECRET={PUBLISH}\nGENERAL_WEBHOOK_SECRET={GENERAL_HOOK}\nCOIN_WEBHOOK_SECRET={COIN_HOOK}\n"
    )
}
// Break caught: falling back to owner/admin extras despite an explicit list, or lexical dedup of the same numeric id.
#[test]
fn explicit_admin_list_wins_and_deduplicates_numeric_identity() {
    let root = fixture();
    let env = root.path().join("operator-private.env");
    std::fs::write(
        &env,
        format!("{}ADMIN_USER_IDS=9;0009,9\n", supplied_environment()),
    )
    .unwrap();
    let result = run(
        &root,
        &["secrets", "--env-file", env.to_str().unwrap(), "--dry-run"],
    );
    assert_eq!(result.code, 0);
    let output = format!("{}{}", result.stdout, result.stderr);
    assert!(output.contains("ADMIN_USER_IDS (explicit)"));
    assert!(output.contains("1 id(s)"));
    assert!(!output.contains("2 id(s)"));
}
// Break caught: an explicit all-invalid list must never silently admit the otherwise valid owner.
#[test]
fn explicit_invalid_admin_list_fails_closed_without_effects() {
    let root = fixture();
    let env = root.path().join("operator-private.env");
    let original = format!(
        "{}ADMIN_USER_IDS=00,١٢,9007199254740992\n",
        supplied_environment()
    );
    std::fs::write(&env, &original).unwrap();
    let result = run(
        &root,
        &["secrets", "--env-file", env.to_str().unwrap(), "--dry-run"],
    );
    assert_ne!(result.code, 0);
    let output = format!("{}{}", result.stdout, result.stderr);
    assert!(output.contains("ADMIN_USER_IDS"));
    assert!(!output.contains("١٢"));
    assert!(!output.contains("9007199254740992"));
    assert_eq!(std::fs::read_to_string(env).unwrap(), original);
    assert!(!root.path().join("worker/.dev.vars").exists());
}
// Break caught: generating persistent keys during dry-run or accidentally printing generated opaque values.
#[test]
fn dry_run_previews_three_generated_keys_without_persisting_them() {
    let root = fixture();
    let env = root.path().join("operator-private.env");
    let original = supplied_environment()
        .lines()
        .filter(|line| {
            !line.starts_with("WORKER_PUBLISH_SECRET=")
                && !line.starts_with("GENERAL_WEBHOOK_SECRET=")
                && !line.starts_with("COIN_WEBHOOK_SECRET=")
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    std::fs::write(&env, &original).unwrap();
    let result = run(
        &root,
        &["secrets", "--env-file", env.to_str().unwrap(), "--dry-run"],
    );
    assert_eq!(result.code, 0);
    let output = format!("{}{}", result.stdout, result.stderr);
    assert_eq!(output.matches("generated (32 random bytes)").count(), 3);
    assert!(output.contains("43 chars"));
    assert!(
        !output.split_whitespace().any(|word| word.len() == 43
            && word
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')),
        "opaque generated values must never be printed"
    );
    assert_eq!(std::fs::read_to_string(env).unwrap(), original);
    assert!(!root.path().join("worker/.dev.vars").exists());
}
// Break caught: inventing a missing token/channel or calling tools before required configuration is complete.
#[test]
fn missing_required_channel_names_the_key_and_writes_nothing() {
    let root = fixture();
    let env = root.path().join("operator-private.env");
    let original = supplied_environment()
        .lines()
        .filter(|line| !line.starts_with("LOG_CHAT_ID="))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    std::fs::write(&env, &original).unwrap();
    let result = run(
        &root,
        &["secrets", "--env-file", env.to_str().unwrap(), "--dry-run"],
    );
    assert_ne!(result.code, 0);
    assert!(format!("{}{}", result.stdout, result.stderr).contains("LOG_CHAT_ID"));
    assert_eq!(std::fs::read_to_string(env).unwrap(), original);
    assert!(!root.path().join("worker/node_modules").exists());
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
