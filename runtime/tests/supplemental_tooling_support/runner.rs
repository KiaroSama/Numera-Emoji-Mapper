//! Exercise the actual runner binary, then load its observed environment with the real Config API.
use crate::process_liveness;
use crate::tooling_fixture;
use numera_runtime::{logging::RunLog, tooling_environment, tooling_process};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};
use tooling_fixture::Root;

fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .into()
}
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
fn observer() -> &'static Path {
    static OBSERVER: OnceLock<PathBuf> = OnceLock::new();
    OBSERVER.get_or_init(|| {
        let directory = project()
            .join("logs/tmp")
            .join(format!("runner-observer-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let output = directory.join(format!("observer{}", std::env::consts::EXE_SUFFIX));
        let compiler = env!("NUMERA_BASELINE_RUSTC");
        let args = vec![
            OsString::from(compiler),
            "--edition=2024".into(),
            "--crate-name=supplemental_runner_observer".into(),
            project()
                .join("runtime/tests/supplemental_tooling_support/runner_observer.rs")
                .into_os_string(),
            "-o".into(),
            output.clone().into_os_string(),
        ];
        let log = RunLog::open(&directory, "runner-observer-compiler", vec![]);
        let result = runtime()
            .block_on(tooling_process::capture(
                &directory,
                &args,
                &std::env::vars().collect(),
                Duration::from_secs(60),
                Duration::from_secs(30),
                &log,
            ))
            .unwrap();
        assert_eq!(result.code, 0, "{}", result.stderr);
        output
    })
}
fn run(root: &Path, mode: &str) -> tooling_process::Completed {
    let mut environment = BTreeMap::new();
    for name in [
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "PATHEXT",
        "LANG",
        "LC_ALL",
        "LD_LIBRARY_PATH",
        "NUMERA_RUNTIME_TEST_ADMISSION",
    ] {
        if let Ok(value) = std::env::var(name) {
            environment.insert(name.into(), value);
        }
    }
    environment.insert("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into());
    for name in [
        "GENERAL_BOT_TOKEN",
        "OTHER_API_KEY_SUFFIX",
        "mixed_Secret",
        "PASSWORD",
        "PACK_OWNER_USER_ID",
        "BOT_ALLOWED_USER_IDS",
        "PACK_LINKS_CHAT_ID",
        "BRAND_LOGO_PATH",
        "HTTP_PROXY",
        "PYTHONPATH",
        "GIT_CONFIG_COUNT",
        "NUMERA_UNRECOGNIZED_SETTING",
        "NUMERA_TEST_WORKSPACE",
        "PLAYWRIGHT_BROWSERS_PATH",
        "PYO3_PYTHON",
        "NUMERA_STATIC_CONVERT_ORACLE",
    ] {
        environment.insert(name.into(), "synthetic-never-admitted".into());
    }
    let environment = tooling_environment::environment(root, &environment, false).unwrap();
    let args = [
        OsString::from(env!("CARGO_BIN_EXE_numera-runtime-tests")),
        observer().as_os_str().to_owned(),
        mode.into(),
        root.as_os_str().to_owned(),
    ];
    let log = RunLog::open(root, "actual-runner-supplement", vec![]);
    runtime()
        .block_on(tooling_process::capture(
            root,
            &args,
            &environment,
            Duration::from_secs(30),
            Duration::from_secs(10),
            &log,
        ))
        .unwrap()
}

#[test]
fn actual_runner_scrubs_before_config_preserves_paths_and_parent_environment() {
    let root = Root::new("actual runner environment café");
    let before = std::env::vars_os().collect::<BTreeMap<_, _>>();
    for bytes in [b"GENERAL_BOT_TOKEN=synthetic-dotenv-token\nPACK_OWNER_USER_ID=222222222\nUNLISTED_VALUE=synthetic-dotenv-only\n".as_slice(), b"GENERAL_BOT_TOKEN=\xff".as_slice()] {
        std::fs::write(root.0.join(".env"),bytes).unwrap();
        let actual = run(&root.0,"environment");
        assert_eq!(actual.code,0,"{}",actual.stderr);
        assert!(!actual.stdout.contains("synthetic-never-admitted"));
        let environment = actual.stdout.lines().filter_map(|line| line.strip_prefix("ENV|").and_then(|line|line.split_once('|')))
            .map(|(key,value)|(key.into(),value.into())).collect::<BTreeMap<String,String>>();
        assert_eq!(environment["NUMERA_EMOJI_MAPPER_NO_DOTENV"],"1");
        assert_eq!(environment["NUMERA_RUNTIME_TEST_ADMISSION"],"isolated-v1");
        assert_eq!(environment["TELEGRAM_API_BASE"],"http://127.0.0.1:9");
        let allowed = ["PATH", "SystemRoot", "SYSTEMROOT", "WINDIR", "PATHEXT", "LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH", "DYLD_FALLBACK_LIBRARY_PATH", "LANG", "LC_ALL", "NUMERA_EMOJI_MAPPER_NO_DOTENV", "NUMERA_RUNTIME_TEST_ADMISSION", "NUMERA_CODEC_OWNED_GROUP", "TELEGRAM_API_BASE", "BRAND_LOGO_BOTS", "COLLECTION_PACK_BASE", "COIN_PACK_BASE", "COIN_PACK_TITLE", "RUST_BACKTRACE", "TEMP", "TMP", "TMPDIR", "XDG_CACHE_HOME"];
        // The Linux process owner injects this exact marker after env_clear;
        // Windows uses Job handles and injects no equivalent environment key.
        #[cfg(target_os = "linux")]
        assert_eq!(environment["NUMERA_CODEC_OWNED_GROUP"], "1");
        #[cfg(windows)]
        assert!(!environment.contains_key("NUMERA_CODEC_OWNED_GROUP"));
        for key in environment.keys() { assert!(allowed.contains(&key.as_str()), "unexpected admitted variable: {key}"); }
        assert_eq!(environment["BRAND_LOGO_BOTS"], "");
        assert_eq!(environment["COLLECTION_PACK_BASE"], "YourBrand_Emoji_Packs");
        assert_eq!(environment["COIN_PACK_BASE"], "cryptoemoji");
        assert_eq!(environment["COIN_PACK_TITLE"], "@YourBrand Crypto Emoji");
        assert!(environment.keys().any(|key|key.eq_ignore_ascii_case("PATH")));
        #[cfg(windows)] assert!(environment.keys().any(|key|key.eq_ignore_ascii_case("SystemRoot")));
        let config = numera_runtime::config::Config::load_with_environment(root.0.clone(),&environment).unwrap();
        for name in ["GENERAL_BOT_TOKEN", "OTHER_API_KEY_SUFFIX", "PACK_OWNER_USER_ID", "UNLISTED_VALUE"] { assert_eq!(config.value(name),""); }
        for name in ["state_emoji_bot.json", "logs", "collection/catalog.db"] { assert_eq!(config.path(name),root.0.join(name)); }
        assert_eq!(std::fs::read(root.0.join(".env")).unwrap(),bytes);
        assert!(!root.0.join("collection").exists());
    }
    assert_eq!(std::env::vars_os().collect::<BTreeMap<_, _>>(), before);
    root.clean().unwrap();
}

#[test]
fn actual_runner_preserves_failure_evidence_and_cleans_same_environment_scratch() {
    let root = Root::new("actual runner failure");
    let prefix = "runtime-tests-";
    let storage = project().join("logs/tmp");
    let names = || {
        std::fs::read_dir(&storage)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name.to_string_lossy().starts_with(prefix))
            .collect::<std::collections::BTreeSet<_>>()
    };
    let before = names();
    for (mode, code) in [("failure", 7), ("early", 9)] {
        let actual = run(&root.0, mode);
        assert_eq!(actual.code, code, "{}", actual.stderr);
        if mode == "failure" {
            assert!(actual.stdout.contains("stdout café 😀"));
            assert!(actual.stderr.contains("stderr failure evidence"));
        }
        assert_eq!(names(), before);
    }
    assert!(!numera_runtime::process_owner::cleanup_pending(&root.0));
    root.clean().unwrap();
}

#[test]
fn actual_runner_allows_private_loopback_and_reaps_acknowledged_closed_stdio_descendant() {
    let root = Root::new("actual runner network");
    let actual = run(&root.0, "network");
    assert_eq!(actual.code, 0, "{}", actual.stderr);
    assert!(
        actual
            .stdout
            .contains("private loopback works; outbound refused")
    );
    let pid = std::fs::read_to_string(root.0.join("runner-descendant.pid"))
        .unwrap()
        .parse()
        .unwrap();
    process_liveness::assert_stopped(pid);
    root.clean().unwrap();
}
