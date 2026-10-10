#![deny(unsafe_code)]

use _native::{
    config::Config, logging::RunLog, media_adapter::MediaAdapter, panel_http,
    panel_preview::Previews, panel_session,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

fn root() -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    for directory in executable.ancestors().skip(1) {
        if directory.join("assets/panel.html").is_file()
            && directory.join("pyproject.toml").is_file()
        {
            return Ok(directory.into());
        }
    }
    Err("cannot locate project assets beside native executable".into())
}
fn value(args: &[String], i: &mut usize) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or("option requires a value".into())
}
fn panel_options(args: &[String]) -> Result<Value, String> {
    let mut out = json!({"data_dir":"collection","port":9450,"preview_fps":15,"show_published":false,"bot":"","packs":[],"no_open":false});
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
fn pack_names(data: &Path, wanted: &[u64]) -> Result<Value, String> {
    let mut paths = std::fs::read_dir(data)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with("publish_") && s.ends_with(".json"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    let mut out = serde_json::Map::new();
    for path in paths {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let Ok(doc) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        if let Some(sets) = doc.get("sets").and_then(Value::as_array) {
            for set in sets {
                if let (Some(n), Some(name)) = (
                    set.get("index").and_then(Value::as_u64),
                    set.get("name").and_then(Value::as_str),
                ) && wanted.contains(&n)
                    && !name.is_empty()
                {
                    out.insert(name.into(), json!(n));
                }
            }
        }
    }
    Ok(json!(out))
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
];
fn main() -> std::process::ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        println!(
            "Numera Emoji Mapper native backend\nUsage: numera-emoji COMMAND [OPTIONS]\nCommands: panel {}\nUse COMMAND --help for options (emoji-bot uses environment configuration).",
            COMMANDS.join(" ")
        );
        return std::process::ExitCode::SUCCESS;
    }
    if COMMANDS.contains(&args[0].as_str()) {
        let result = (|| {
            let root = root()?;
            let config = Config::load(root.clone())?;
            let log = RunLog::open(&root, &args[0], config.secret_values());
            let result = if args[0] == "add-media" {
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
        if existing != session {
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
    let keep = pack_names(&data, &packs)?;
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
    let python = std::env::var_os("PYO3_PYTHON")
        .filter(|_| config.value("NUMERA_EMOJI_MAPPER_NO_DOTENV") == "1")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            config.root.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        });
    if !python.is_file() {
        return Err("required exact-media Python environment is missing".into());
    }
    let previews = Previews::new(
        MediaAdapter {
            python,
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
    runtime
        .block_on(panel_http::serve(panel, port))
        .map_err(|e| format!("cannot listen: {e}"))
}
