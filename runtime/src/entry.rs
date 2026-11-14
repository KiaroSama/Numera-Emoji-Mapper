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
];
pub fn run() -> std::process::ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
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
                "{command} belongs to the optional numera-compat developer toolkit. The standalone application runs without it."
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
                _ => package_command(&root, command, &args[1..]),
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
            "Numera Emoji Mapper native backend\nUsage: numera-emoji COMMAND [OPTIONS]\nCommands: panel doctor menu check native-convert-owner package-native package-verify install-prebuilt {}\nUse COMMAND --help for options (emoji-bot uses environment configuration).",
            COMMANDS.join(" ")
        );
        return std::process::ExitCode::SUCCESS;
    }
    if COMMANDS.contains(&args[0].as_str()) {
        let result = (|| {
            let root = root()?;
            let config = Config::load(root.clone())?;
            let mut secrets = config.secret_values();
            // Register before argument validation: even an error may contain the selected value.
            for option in args[1..].windows(2).filter(|pair| pair[0] == "--token-env") {
                let token = config.value(&option[1]);
                if !token.is_empty() {
                    secrets.push(token.into());
                }
            }
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
    let data = config.path(
        options["data_dir"]
            .as_str()
            .ok_or("invalid data directory")?,
    );
    let _sandbox_lifetime = _native::sandbox_lease::inherit(&data)?;
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
    if let Some(existing) = panel_session::probe(port) {
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
        if let Err(error) = runtime.block_on(_native::browser_open::open(
            port,
            options["no_open"] == true,
        )) {
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
            scratch: data.join("tmp/native-codec"),
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

fn package_command(root: &std::path::Path, command: &str, args: &[String]) -> i32 {
    if args == ["--help"] || args == ["-h"] {
        match command {
            "package-native" => println!(
                "Usage: numera-emoji package-native [--executable FILE] [--libraries DIR] [--out DIR] [--revision SHA] [--target PLATFORM]"
            ),
            "install-prebuilt" => println!(
                "Usage: numera-emoji install-prebuilt [--package-root DIR] --destination DIR"
            ),
            _ => println!("Usage: numera-emoji package-verify [--package-root DIR]"),
        }
        return 0;
    }
    let result = (|| -> Result<(), String> {
        let mut values = std::collections::BTreeMap::new();
        let allowed: &[&str] = match command {
            "package-native" => &[
                "--executable",
                "--libraries",
                "--out",
                "--revision",
                "--target",
            ],
            "install-prebuilt" => &["--package-root", "--destination"],
            _ => &["--package-root"],
        };
        let mut index = 0;
        while index < args.len() {
            let (name, attached) = args[index]
                .split_once('=')
                .map_or((args[index].as_str(), None), |(name, value)| {
                    (name, Some(value))
                });
            if !allowed.contains(&name) {
                return Err(format!("unknown {command} option: {name}"));
            }
            index += 1;
            let value = match attached {
                Some(value) => value,
                None => {
                    let value = args
                        .get(index)
                        .filter(|value| !value.starts_with("--"))
                        .ok_or_else(|| format!("{name} requires a value"))?;
                    index += 1;
                    value
                }
            };
            if value.is_empty() || values.insert(name, value).is_some() {
                return Err(format!("empty or duplicate {command} option: {name}"));
            }
        }
        let resolve = |value: &str| {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        };
        let package_root = values
            .get("--package-root")
            .map(|value| resolve(value))
            .unwrap_or_else(|| root.to_path_buf());
        match command {
            "package-native" => {
                let executable = match values.get("--executable") {
                    Some(value) => resolve(value),
                    None => std::env::current_exe().map_err(|error| error.to_string())?,
                };
                let libraries = values
                    .get("--libraries")
                    .map(|value| resolve(value))
                    .or_else(|| root.join("lib").is_dir().then(|| root.join("lib")));
                let output = values
                    .get("--out")
                    .map(|value| resolve(value))
                    .unwrap_or_else(|| root.join("dist"));
                let system = match std::env::consts::OS {
                    "windows" => "windows",
                    "linux" => "linux",
                    "macos" => "macos",
                    _ => return Err("unsupported package operating system".into()),
                };
                let arch = match std::env::consts::ARCH {
                    "x86_64" => "x64",
                    "aarch64" => "arm64",
                    _ => return Err("unsupported package CPU architecture".into()),
                };
                let default_target = format!("{system}-{arch}");
                let target = values.get("--target").copied().unwrap_or(&default_target);
                let revision = values.get("--revision").copied().unwrap_or("unknown");
                let archive = crate::package::package(
                    root,
                    &executable,
                    libraries.as_deref(),
                    &output,
                    revision,
                    target,
                )?;
                println!("{}", archive.display());
            }
            "install-prebuilt" => {
                let destination = values
                    .get("--destination")
                    .ok_or("--destination is required")?;
                let installed = crate::package::install(&package_root, &resolve(destination))?;
                println!("Installed standalone application: {}", installed.display());
            }
            _ => {
                crate::package::verify(&package_root)?;
                println!("[OK] Package contents match their manifest.");
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("ERROR: {}", crate::logging::redact(&error, &[]));
            2
        }
    }
}
