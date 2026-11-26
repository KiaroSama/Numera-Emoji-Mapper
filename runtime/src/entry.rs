use crate as _native;
use crate::{
    config::Config, logging::RunLog, media_native_adapter::NativeMediaAdapter, panel_http,
    panel_preview::Previews, panel_session,
};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn root() -> Result<PathBuf, String> {
    _native::tooling_environment::project_root()
}
fn command_secrets(config: &Config, args: &[String]) -> Vec<String> {
    let mut secrets = config.secret_values();
    for option in args.windows(2).filter(|pair| pair[0] == "--token-env") {
        let token = config.value(&option[1]);
        if !token.is_empty() {
            secrets.push(token.into());
        }
    }
    secrets
}

#[cfg(test)]
mod baseline_logging_tests {
    use super::*;
    // tests.test_logsetup.TestRedaction.test_a_token_env_of_any_name_is_masked
    #[test]
    fn selected_token_environment_is_registered_before_validation() {
        let scratch = crate::baseline_catalog_support::Scratch::new();
        let value = "an-unlisted-token-value-for-this-test";
        for name in ["SOME_OTHER_TOKEN", "OPERATOR_CREDENTIAL"] {
            let environment = std::collections::BTreeMap::from([
                ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
                (name.into(), value.into()),
            ]);
            let config = Config::load_with_environment(scratch.0.clone(), &environment).unwrap();
            let args = [
                "--base",
                "t",
                "--title",
                "T",
                "--user-id",
                "1",
                "--source-dir",
                "no-such-dir",
                "--token-env",
                name,
                "--dry-run",
            ]
            .map(str::to_owned);
            if name == "OPERATOR_CREDENTIAL" {
                assert!(config.secret_values().is_empty());
            }
            let secrets = command_secrets(&config, &args);
            assert!(secrets.iter().any(|secret| secret == value));
            let text = crate::logging::redact(&format!("token {value}"), &secrets);
            assert!(!text.contains(value));
            assert!(text.contains("[REDACTED]"));
        }
        // An actual entry/logging positive control detects a disconnected helper:
        // OPERATOR_CREDENTIAL is intentionally absent from secret_values().
        let assets = scratch.0.join("assets");
        std::fs::create_dir(&assets).unwrap();
        std::fs::write(assets.join("panel.html"), "isolated native root").unwrap();
        let current = std::env::current_exe().unwrap();
        let profile = current
            .ancestors()
            .find(|path| {
                path.file_name()
                    .is_some_and(|name| name == "debug" || name == "release")
            })
            .unwrap();
        let name = if cfg!(windows) {
            "numera-emoji.exe"
        } else {
            "numera-emoji"
        };
        let executable = scratch.0.join(name);
        std::fs::copy(profile.join(name), &executable).unwrap();
        let mut environment = std::collections::BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("OPERATOR_CREDENTIAL".into(), value.into()),
        ]);
        for name in ["PATH", "SystemRoot", "WINDIR", "SYSTEMROOT"] {
            if let Ok(value) = std::env::var(name) {
                environment.insert(name.into(), value);
            }
        }
        let mut argv = vec![executable.into_os_string()];
        argv.extend(
            [
                "build-pack",
                "--base",
                "t",
                "--title",
                "T",
                "--user-id",
                "1",
                "--source-dir",
                "no-such-dir",
                "--token-env",
                "OPERATOR_CREDENTIAL",
                "--dry-run",
                value,
            ]
            .map(std::ffi::OsString::from),
        );
        let log = RunLog::open(&scratch.0, "selected-token-control", vec![value.into()]);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let output = runtime
            .block_on(crate::tooling_process::capture(
                &scratch.0,
                &argv,
                &environment,
                std::time::Duration::from_secs(15),
                std::time::Duration::from_secs(10),
                &log,
            ))
            .unwrap();
        assert_eq!(output.code, 2);
        assert!(
            output
                .stderr
                .contains("unknown build-pack option: [REDACTED]"),
            "actual entry did not exercise selected-secret validation redaction"
        );
        assert!(!output.stdout.contains(value));
        assert!(!output.stderr.contains(value));
        let logs = std::fs::read_dir(scratch.0.join("logs"))
            .unwrap()
            .filter_map(|entry| {
                let path = entry.unwrap().path();
                path.is_file()
                    .then(|| std::fs::read_to_string(path).unwrap())
            })
            .collect::<String>();
        assert!(logs.contains("unknown build-pack option: [REDACTED]"));
        assert!(!logs.contains(value));
    }
}
fn value(args: &[String], i: &mut usize) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or("option requires a value".into())
}
fn panel_options(args: &[String]) -> Result<Value, String> {
    let mut out = json!({"data_dir":"collection","port":panel_session::DEFAULT_PORT,"preview_fps":15,"show_published":false,"bot":"","packs":[],"no_open":false});
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--data-dir" => out["data_dir"] = json!(value(args, &mut i)?),
            "--port" => {
                out["port"] = json!(
                    value(args, &mut i)?
                        .parse::<u16>()
                        .ok()
                        .filter(|n| *n > 0)
                        .ok_or("--port must be1..65535")?
                )
            }
            "--preview-fps" => {
                out["preview_fps"] = json!(
                    value(args, &mut i)?
                        .parse::<u64>()
                        .ok()
                        .filter(|n| (1..=30).contains(n))
                        .ok_or("--preview-fps must be1..30")?
                )
            }
            "--with-pack" => out["packs"]
                .as_array_mut()
                .ok_or("invalid options")?
                .push(json!(
                    value(args, &mut i)?
                        .parse::<u64>()
                        .ok()
                        .filter(|n| *n > 0)
                        .ok_or("--with-pack must bepositive")?
                )),
            "--bot-username" => out["bot"] = json!(value(args, &mut i)?),
            "--all" => out["show_published"] = json!(true),
            "--no-open" => out["no_open"] = json!(true),
            other => return Err(format!("unknown panel option: {other}")),
        }
        i += 1;
    }
    Ok(out)
}
const COMMANDS: &[&str] = &[
    "add-media",
    "fetch-pack",
    "fetch-emoji-ids",
    "build-collection",
    "build-pack",
    "emoji-bot",
    "sync-order",
    "plan-apply",
    "make-emoji-pngs",
    "plan-status",
    "pack-manifest",
    "status",
    "pack-archive",
    "identity-repair",
    "coin-alias-map",
    "coin-enhance-map",
    "coin-build-keywords",
    "coin-fetch-logos",
    "coin-fetch-paprika",
    "coin-fetch-cmc",
    "coin-write-manifests",
    "coin-check-packs",
    "coin-remap-ids",
    "coin-verify-logos",
    "coin-rebuild-map",
    "coin-rebuild",
    "coin-convert",
    "coin-rebuild-loop",
];
pub fn run() -> std::process::ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|arg| arg == "audit-publication") {
        let code = crate::publication_audit::run(&args[1..], root);
        return code.into();
    }
    if let Some(command) = args.first().map(String::as_str) {
        if matches!(
            command,
            "source-inventory"
                | "source-lottie-render"
                | "source-static-images"
                | "source-oracle-worker"
                | "benchmark-similarity"
                | "test-runner"
                | "test-net-bootstrap"
                | "test-plan"
                | "test-cargo"
                | "runtime-check"
                | "runtime-probe"
        ) {
            eprintln!(
                "{command} is a retired compatibility command. Use numera-dev --help for current Rust contributor commands and numera-runtime-tests for guarded tests."
            );
            return 2.into();
        }
        if matches!(
            command,
            "codec-worker"
                | "doctor"
                | "check"
                | "menu"
                | "native-convert-owner"
                | "package-native"
                | "package-verify"
                | "verify-package"
                | "verify-binary"
                | "verify-binary-version"
                | "install-prebuilt"
        ) {
            let result: Result<i32, String> = root().map(|root| match command {
                "codec-worker" => crate::codec_worker::run(&root, &args[1..]),
                "doctor" | "check" => crate::tooling_doctor::run(&root, &args[1..]),
                "menu" => crate::tooling_menu_cli::run(&root, &args[1..]),
                "native-convert-owner" => {
                    let log = Arc::new(RunLog::open(&root, "native_convert_owner", vec![]));
                    log.install_panic_hook();
                    let code = crate::tooling_convert_cli::run(&root, &args[1..], &log);
                    log.finish(code);
                    code
                }
                _ => crate::package::cli::run(&root, command, &args[1..]),
            });
            let code = match result {
                Ok(code) => code,
                Err(error) => {
                    eprintln!("ERROR: {}", crate::logging::redact(&error, &[]));
                    2
                }
            };
            #[cfg(windows)]
            std::process::exit(code);
            #[cfg(not(windows))]
            return (code as u8).into();
        }
    }
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        println!(
            "Numera Emoji Mapper native backend\nUsage: numera-emoji COMMAND [OPTIONS]\nCommands: panel doctor menu check native-convert-owner package-native package-verify verify-binary verify-binary-version install-prebuilt audit-publication {}\nUse COMMAND --help for options (emoji-bot uses environment configuration).",
            COMMANDS.join(" ")
        );
        return std::process::ExitCode::SUCCESS;
    }
    if COMMANDS.contains(&args[0].as_str()) {
        let result = (|| {
            let root = root()?;
            let config = Config::load(root.clone())?;
            // Register before argument validation: even an error may contain the selected value.
            let secrets = command_secrets(&config, &args[1..]);
            let log = Arc::new(RunLog::open(&root, &args[0], secrets));
            log.install_panic_hook();
            log.apply_retention(&root, config.value("EMOJI_LOG_RETENTION_DAYS"));
            let result = if matches!(args[0].as_str(), "coin-alias-map" | "coin-enhance-map") {
                _native::coin_map_cli::run(&config, &args[0], &args[1..], &log)
            } else if matches!(args[0].as_str(), "coin-fetch-paprika" | "coin-fetch-cmc") {
                _native::coin_provider_cli::run(
                    &config,
                    args[0].trim_start_matches("coin-fetch-"),
                    &args[1..],
                    &log,
                )
            } else if matches!(args[0].as_str(), "coin-convert" | "coin-rebuild-loop") {
                match _native::coin_workflows::run(&config, &args[0], &args[1..], &log) {
                    Ok(code) => {
                        #[cfg(windows)]
                        {
                            log.finish(code);
                            std::process::exit(code);
                        }
                        #[cfg(not(windows))]
                        {
                            Ok(code as u8)
                        }
                    }
                    Err(error) => Err(error),
                }
            } else if args[0] == "coin-rebuild" {
                _native::coin_rebuild_cli::run(&config, &args[1..], &log)
            } else if args[0] == "coin-rebuild-map" {
                _native::coin_rebuild_map_cli::run(&config, &args[1..], &log)
            } else if args[0] == "coin-verify-logos" {
                _native::coin_verify_cli::run(&config, &args[1..], &log)
            } else if args[0] == "coin-remap-ids" {
                _native::coin_remap_cli::run(&config, &args[1..], &log)
            } else if args[0] == "coin-check-packs" {
                _native::coin_audit_cli::run(&config, &args[1..], &log)
            } else if args[0] == "coin-write-manifests" {
                _native::coin_manifest_cli::run(&config, &args[1..], &log)
            } else if args[0] == "coin-fetch-logos" {
                _native::coin_gecko_cli::run(&config, &args[1..], &log)
            } else if args[0] == "coin-build-keywords" {
                _native::coin_keywords_cli::run(&config, &args[1..], &log)
            } else if args[0] == "add-media" {
                _native::local_cli::run(&config, &args[1..], &log)
            } else if args[0] == "fetch-pack" {
                _native::fetch_cli::run(&config, &args[1..], &log)
            } else if args[0] == "fetch-emoji-ids" {
                _native::fetch_ids_cli::run(&config, &args[1..], &log)
            } else if args[0] == "build-collection" {
                _native::publish_cli::run(&config, &args[1..], &log)
            } else if args[0] == "build-pack" {
                _native::ticker_build::run(&config, &args[1..], &log)
            } else if args[0] == "sync-order" {
                _native::sync_order::run(&config, &args[1..], &log)
            } else if args[0] == "plan-apply" {
                _native::plan_apply::run(&config, &args[1..], &log)
            } else if args[0] == "make-emoji-pngs" {
                _native::convert_cli::run(&config, &args[1..], &log)
            } else if args[0] == "plan-status" {
                _native::plan_status::run(&config, &args[1..], &log)
            } else if args[0] == "pack-manifest" {
                _native::roster_cli::run(&config, &args[1..], &log)
            } else if args[0] == "status" {
                _native::status::run(&config, &args[1..], &log)
            } else if args[0] == "pack-archive" {
                _native::archive_cli::run(&config, &args[1..], &log)
            } else if args[0] == "identity-repair" {
                _native::identity_cli::run(&config, &args[1..], &log)
            } else {
                _native::emoji_bot::run(&config, &args[1..], &log)
            };
            let code = match result {
                Ok(code) => code,
                Err(error) => {
                    log.event("ERROR", &args[0], &error);
                    if error.starts_with("FLOOD_WAIT") {
                        3
                    } else {
                        2
                    }
                }
            };
            log.finish(i32::from(code));
            Ok::<u8, String>(code)
        })();
        return match result {
            Ok(code) => code.into(),
            Err(error) => {
                eprintln!("ERROR: {error}");
                2.into()
            }
        };
    }
    if args[0] != "panel" {
        eprintln!("ERROR: unsupported native command: {}", args[0]);
        return 2.into();
    }
    if args[1..].iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "Usage: numera-emoji panel [--data-dir DIR] [--port N] [--preview-fps N] [--bot-username NAME] [--all] [--with-pack N] [--no-open]"
        );
        return std::process::ExitCode::SUCCESS;
    }
    let result = (|| {
        let root = root()?;
        let config = Config::load(root.clone())?;
        let options = panel_options(&args[1..])?;
        let log = Arc::new(RunLog::open(&root, "numera-emoji", config.secret_values()));
        log.install_panic_hook();
        log.apply_retention(&root, config.value("EMOJI_LOG_RETENTION_DAYS"));
        let code = match start_panel(&config, options, log.clone()) {
            Ok(()) => 0,
            Err(e) => {
                log.event("ERROR", "panel", &e);
                2
            }
        };
        log.finish(code);
        Ok::<u8, String>(code as u8)
    })();
    match result {
        Ok(code) => code.into(),
        Err(e) => {
            eprintln!("ERROR: {e}");
            2.into()
        }
    }
}
fn start_panel(config: &Config, options: Value, log: Arc<RunLog>) -> Result<(), String> {
    start_panel_with_browser(config, options, log, |runtime, port, no_open| {
        runtime.block_on(_native::browser_open::open(port, no_open))
    })
}
fn start_panel_with_browser(
    config: &Config,
    options: Value,
    log: Arc<RunLog>,
    mut open: impl FnMut(&tokio::runtime::Runtime, u16, bool) -> Result<(), String>,
) -> Result<(), String> {
    let data = config.path(
        options["data_dir"]
            .as_str()
            .ok_or("invalid data directory")?,
    );
    let sandbox_lifetime = _native::sandbox_lease::inherit(&data)?;
    let sandbox_owned = config.value("NUMERA_SANDBOX_OWNED") == "1";
    if sandbox_owned {
        _native::sandbox_lease::validate_marker(&data)?;
    }
    let db = data.join("catalog.db");
    if !db.is_file() {
        return Err("no catalog; run a collector first".into());
    }
    let packs = options["packs"]
        .as_array()
        .ok_or("invalid packs")?
        .iter()
        .map(|v| v.as_u64().ok_or("invalid pack"))
        .collect::<Result<Vec<_>, _>>()?;
    let show = options["show_published"]
        .as_bool()
        .ok_or("invalid show_published")?;
    let port = options["port"].as_u64().ok_or("invalid port")? as u16;
    let session = panel_session::identity(&db, show, &packs)?;
    if sandbox_lifetime.is_none()
        && !sandbox_owned
        && let Some(existing) = panel_session::probe(port)
    {
        if existing.get("legacy") == Some(&Value::Bool(true)) {
            log.event(
                "WARNING",
                "panel",
                "Reusing an older panel; its existing collection and view remain active.",
            );
        } else if existing != session {
            return Err(
                "a panel for another collection or view is already running; choose another --port"
                    .into(),
            );
        }
        println!("Panel already running: http://127.0.0.1:{port}/");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        if let Err(error) = open(&runtime, port, options["no_open"] == true) {
            log.event("WARNING", "panel", &error);
        }
        return Ok(());
    }
    let keep = _native::panel_view::packs_named(&data, &packs)?;
    if !packs.is_empty() && keep.as_object().is_none_or(|v| v.is_empty()) {
        return Err("no published pack matches requested indices".into());
    }
    let bot = options["bot"].as_str().unwrap_or("").to_lowercase();
    let logo = config.logo_path(false)?;
    let branded =
        config.logo_bots(false)?.contains(&bot) && logo.as_ref().is_some_and(|p| p.is_file());
    let view_options = json!({"keep_sets":keep,"show_published":show,"branded":branded,"logo_path":logo,
        "preview_fps":options["preview_fps"],"no_open":options["no_open"]});
    let mut random = [0u8; 24];
    getrandom::fill(&mut random).map_err(|_| "cannot create secure panel token")?;
    let token = random
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let previews = Previews::new(
        NativeMediaAdapter {
            executable: std::env::current_exe().map_err(|e| e.to_string())?,
            root: config.root.clone(),
            scratch: config.codec_directory(&data, "tmp/native-codec", false)?,
        },
        &data,
        2,
    );
    let branding = if bot.is_empty() && !config.value("GENERAL_BOT_TOKEN").is_empty() {
        let base = if config.value("NUMERA_EMOJI_MAPPER_NO_DOTENV") == "1" {
            config.value("NUMERA_TEST_API_BASE").to_owned()
        } else {
            "https://api.telegram.org".into()
        };
        Some((
            config.value("GENERAL_BOT_TOKEN").to_owned(),
            base,
            config.logo_bots(false)?,
        ))
    } else {
        None
    };
    let panel = panel_http::Panel {
        branding,
        detected_bot: Mutex::new(bot),
        root: config.root.clone(),
        db,
        token,
        options: view_options,
        view: Mutex::new(json!({"view":[],"by_key":{},"hidden":0})),
        session,
        last_refreshed: std::sync::Mutex::new(None),
        utc: _native::logging::iso_utc,
        log: log.clone(),
        events: Mutex::new(Default::default()),
        previews,
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    log.event(
        "INFO",
        "panel",
        &format!("listening on numeric loopback port={port}"),
    );
    println!("Emoji curate panel: http://127.0.0.1:{port}/");
    match runtime.block_on(panel_http::serve(panel, port)) {
        Ok(()) => Ok(()),
        Err(error) => {
            let holder = match runtime.block_on(panel_session::port_holder(&config.root, port)) {
                Ok(Some(pid)) => format!(" (pid {pid})"),
                Ok(None) => String::new(),
                Err(diagnostic) => {
                    log.event("WARNING", "panel", &diagnostic);
                    String::new()
                }
            };
            Err(format!(
                "Port {port}{holder} is unavailable; no reusable panel was identified. Choose another port with --port. [{error}]"
            ))
        }
    }
}

#[cfg(test)]
#[path = "baseline_cli_panel_tests.rs"]
mod baseline_cli_panel_tests;
