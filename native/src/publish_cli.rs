//! Native collection command owns plan/state/catalog workflows; codecs remain exact adapters.
use crate::{
    atomic,
    catalog::Catalog,
    collection_names,
    config::Config,
    logging::RunLog,
    media_adapter::MediaAdapter,
    publication::Publisher,
    publish_collection::{self, Options},
    reconcile, resume,
    telegram::{ApiError, Telegram, upload},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn take(args: &[String], i: &mut usize) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or("option requires a value".into())
}
fn read(path: &Path, default: Value) -> Result<Value, String> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(default),
        Err(e) => Err(e.to_string()),
        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
            Err("state path is not a regular file".into())
        }
        Ok(_) => serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string()),
    }
}
pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "Usage: numera-emoji build-collection --base NAME --title TITLE [--mixed] [--formats LIST] [--per-set N] [--new-set|--into-pack N] [--emoji EMOJI] [--data-dir DIR] [--token-env KEY] [--user-id N] [--brand-logo FILE|--no-brand-logo] [--repaint] [--dry-run|--preflight]"
        );
        return Ok(0);
    }
    let mut options = json!({"base":"","title":"","token_env":"GENERAL_BOT_TOKEN","data":"collection",
        "formats":"static,video,animated","per_set":200,"new_set":false,"mixed":false,"into_pack":null,
        "emoji":"😀","repaint":false,"dry_run":false,"preflight":false,"no_logo":false,"logo":null,
        "user_id":config.integer("PACK_OWNER_USER_ID",0,0,i64::MAX)});
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--base" => options["base"] = json!(take(args, &mut i)?),
            "--title" => options["title"] = json!(take(args, &mut i)?),
            "--token-env" => options["token_env"] = json!(take(args, &mut i)?),
            "--data-dir" => options["data"] = json!(take(args, &mut i)?),
            "--formats" => options["formats"] = json!(take(args, &mut i)?),
            "--emoji" => options["emoji"] = json!(take(args, &mut i)?),
            "--brand-logo" => options["logo"] = json!(take(args, &mut i)?),
            "--per-set" => {
                options["per_set"] = json!(
                    take(args, &mut i)?
                        .parse::<u64>()
                        .map_err(|_| "invalid --per-set")?
                )
            }
            "--user-id" => {
                options["user_id"] = json!(
                    take(args, &mut i)?
                        .parse::<u64>()
                        .map_err(|_| "invalid --user-id")?
                )
            }
            "--into-pack" => {
                options["into_pack"] = json!(
                    take(args, &mut i)?
                        .parse::<u64>()
                        .ok()
                        .filter(|n| *n > 0)
                        .ok_or("invalid --into-pack")?
                )
            }
            "--new-set" => options["new_set"] = json!(true),
            "--mixed" => options["mixed"] = json!(true),
            "--repaint" => options["repaint"] = json!(true),
            "--dry-run" => options["dry_run"] = json!(true),
            "--preflight" => options["preflight"] = json!(true),
            "--no-brand-logo" => options["no_logo"] = json!(true),
            other => return Err(format!("unknown build-collection option: {other}")),
        }
        i += 1;
    }
    let base = options["base"].as_str().ok_or("missing --base")?;
    collection_names::valid_base(base)?;
    let title = options["title"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("--title is required")?;
    let per_set = options["per_set"]
        .as_u64()
        .filter(|n| (1..=200).contains(n))
        .ok_or("--per-set must be 1..200")?;
    if options["new_set"] == true && !options["into_pack"].is_null() {
        return Err("--new-set and --into-pack are mutually exclusive".into());
    }
    let formats = if options["mixed"] == true {
        vec!["mixed".into()]
    } else {
        collection_names::formats(options["formats"].as_str().ok_or("invalid formats")?)?
    };
    let data = config.path(options["data"].as_str().ok_or("invalid data directory")?);
    if !data.join("catalog.db").is_file() {
        return Err("no catalog; run a collector first".into());
    }
    let data = data.canonicalize().map_err(|e| e.to_string())?;
    let state_path = data.join(format!("publish_{base}.json"));
    let plan_path = data.join(format!("publish_plan_{base}.json"));
    let logo_bots = if options["no_logo"] == true {
        vec![]
    } else {
        config.logo_bots(true)?
    };
    let logo_path = if let Some(path) = options["logo"].as_str() {
        Some(config.path(path))
    } else if logo_bots.is_empty() {
        None
    } else {
        config.logo_path(true)?
    };
    if !logo_bots.is_empty() && logo_path.as_ref().is_none_or(|p| !p.is_file()) {
        return Err("required brand logo is missing; nothing was changed".into());
    }
    let capacity = per_set
        .checked_sub(u64::from(!logo_bots.is_empty()))
        .filter(|n| *n > 0)
        .ok_or("--per-set leaves no room for the logo")?;
    let ownership = crate::ownership::Ownership::acquire(
        &data,
        crate::ownership::Mode::Writer,
        &crate::logging::utc(),
    );
    let _ownership = match ownership {
        Ok(lease) => lease,
        Err(error) => {
            log.event("ERROR", "publisher", &error);
            return Ok(4);
        }
    };
    let _family = match crate::locks::Lease::acquire(
        &data.join(format!("publish_{base}.lock")),
        &crate::logging::utc(),
    ) {
        Ok(lease) => lease,
        Err(error) => {
            log.event("ERROR", "publisher", &error.to_string());
            return Ok(4);
        }
    };
    let mut state =
        match read(&state_path, json!({})).and_then(|state| resume::collection(state, base)) {
            Ok(state) => state,
            Err(error) => {
                log.event("ERROR", "publisher", &error);
                return Ok(4);
            }
        };
    let started = state["sets"]
        .as_array()
        .ok_or("invalid sets")?
        .iter()
        .filter_map(|s| s["fmt"].as_str())
        .collect::<BTreeSet<_>>();
    if started
        .iter()
        .any(|fmt| !formats.iter().any(|want| want == fmt))
    {
        return Err("cannot change this family's mixed/per-format publication mode".into());
    }
    let mut cat = Catalog::open(
        &data.join("catalog.db"),
        &crate::logging::utc(),
        &config.root,
    )?;
    cat.dispatch(json!({"operation":"adopt","base":base}))?;
    let plan = resume::freeze(read(&plan_path, json!({}))?, &cat.all(None)?, &formats)?;
    let skipped = state["skipped"]
        .as_array()
        .ok_or("invalid skipped")?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if options["dry_run"] == true {
        println!("DRY RUN: nothing uploaded.");
        for fmt in &formats {
            let keys = pending(&mut cat, &plan, fmt, base, &skipped)?;
            println!(
                "  {fmt}: {} emoji -> {} set(s) of up to {capacity}{}, named {base}{}1_by_<bot> ...",
                keys.len(),
                (keys.len() as u64).div_ceil(capacity),
                if logo_bots.is_empty() {
                    ""
                } else {
                    " (+1 brand logo each)"
                },
                tag(fmt)
            );
        }
        return Ok(0);
    }
    let token = config.require(options["token_env"].as_str().ok_or("invalid token key")?)?;
    let user_id = options["user_id"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or("provide --user-id or PACK_OWNER_USER_ID")?;
    let api = if std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1") {
        std::env::var("NUMERA_TEST_API_BASE").unwrap_or_else(|_| "https://api.telegram.org".into())
    } else {
        "https://api.telegram.org".into()
    };
    let tg = Telegram::new(token.into(), api).map_err(|e| e.to_string())?;
    let bot = tg
        .read_call("getMe", &BTreeMap::new())
        .map_err(|e| e.to_string())?["username"]
        .as_str()
        .ok_or("getMe has no username")?
        .to_owned();
    for fmt in &formats {
        collection_names::name_length(base, &bot, tag(fmt))?;
    }
    if options["preflight"] == true {
        return preflight(
            &tg,
            &mut cat,
            &plan,
            &formats,
            base,
            &skipped,
            (user_id, log),
        );
    }
    let python = std::env::var_os("PYO3_PYTHON")
        .filter(|_| std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            config.root.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        });
    let media = MediaAdapter {
        python,
        root: config.root.clone(),
        scratch: data.join("tmp/native-codec"),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let logo = if logo_bots.iter().any(|b| b == &bot.to_lowercase()) && options["no_logo"] != true {
        let path = logo_path.ok_or("required brand logo is missing")?;
        Some(runtime.block_on(crate::brand_logo::prepare(&media, &path, &data, config))?)
    } else {
        None
    };
    atomic::write_json(&plan_path, &plan).map_err(|e| e.to_string())?;
    let mut publisher = Publisher {
        tg: &tg,
        cat: &mut cat,
        media: &media,
        data: data.clone(),
        runtime: &runtime,
        config: Some(config),
    };
    let (mut done, mut failed) = (0u64, 0u64);
    let mut formats = formats;
    if let Some(name) = state.pointer("/in_flight/set_name").and_then(Value::as_str) {
        let recorded = state["sets"]
            .as_array()
            .and_then(|sets| sets.iter().find(|set| set["name"] == name))
            .and_then(|set| set["fmt"].as_str());
        let interrupted = recorded
            .or_else(|| {
                formats
                    .iter()
                    .find(|fmt| {
                        let prefix = format!("{base}{}", tag(fmt));
                        name.strip_prefix(&prefix)
                            .and_then(|suffix| suffix.split_once("_by_"))
                            .is_some_and(|(index, suffix)| {
                                !index.is_empty()
                                    && index.bytes().all(|b| b.is_ascii_digit())
                                    && suffix == bot
                            })
                    })
                    .map(String::as_str)
            })
            .ok_or("interrupted publication names an unknown family format")?;
        if let Some(at) = formats.iter().position(|fmt| fmt == interrupted) {
            let first = formats.remove(at);
            formats.insert(0, first);
        }
    }
    for fmt in formats {
        let keys = plan[&fmt]
            .as_array()
            .ok_or("invalid frozen plan")?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let opts = Options {
            base: base.into(),
            title: title.into(),
            bot: bot.clone(),
            user_id,
            fmt,
            per_set,
            new_set: options["new_set"] == true,
            into_pack: options["into_pack"].as_u64(),
            repaint: options["repaint"] == true,
            default_emoji: options["emoji"]
                .as_str()
                .ok_or("invalid fallback emoji")?
                .into(),
            logo: logo.clone(),
        };
        let result = publish_collection::publish_locked(&mut publisher, &opts, &keys, &mut state)?;
        done += result["uploaded"].as_u64().unwrap_or(0);
        failed += result["failed"].as_u64().unwrap_or(0);
    }
    if failed > 0 {
        println!("\nDONE: {done} uploaded, {failed} failed.");
    } else {
        println!("\nDONE.");
    }
    for set in state["sets"].as_array().ok_or("invalid sets")? {
        println!(
            "  https://t.me/addemoji/{}  [{}]",
            set["name"].as_str().ok_or("set has no name")?,
            set["fmt"].as_str().ok_or("set has no format")?
        );
    }
    Ok(if failed == 0 {
        0
    } else if done > 0 {
        3
    } else {
        4
    })
}
fn tag(fmt: &str) -> &str {
    match fmt {
        "static" => "s",
        "video" => "v",
        "animated" => "a",
        _ => "",
    }
}
fn pending(
    cat: &mut Catalog,
    plan: &Value,
    fmt: &str,
    base: &str,
    skipped: &BTreeSet<String>,
) -> Result<Vec<String>, String> {
    let keys = plan[fmt]
        .as_array()
        .ok_or("invalid frozen plan")?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let items = cat.all(if fmt == "mixed" { None } else { Some(fmt) })?;
    let mut already = BTreeSet::new();
    for key in &keys {
        if cat.dispatch(json!({"operation":"published","base":base,"content_key":key}))? == true {
            already.insert(key.clone());
        }
    }
    reconcile::pending(&items, &keys, &already, skipped)
}
fn preflight(
    tg: &Telegram,
    cat: &mut Catalog,
    plan: &Value,
    formats: &[String],
    base: &str,
    skipped: &BTreeSet<String>,
    context: (u64, &RunLog),
) -> Result<u8, String> {
    let (user, log) = context;
    let mut accepted = 0;
    let (mut refused, mut missing, mut unknown) = (Vec::new(), Vec::new(), Vec::new());
    for fmt in formats {
        for key in pending(cat, plan, fmt, base, skipped)? {
            let item = cat.get(&key)?.ok_or("queued catalog item vanished")?;
            let path = Path::new(item["file_path"].as_str().ok_or("missing media path")?);
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if !path.is_file() {
                log.event(
                    "ERROR",
                    "preflight",
                    &format!("{key} ({name}) is missing on disk"),
                );
                missing.push((key, name));
                continue;
            }
            let file = upload(path, "sticker").map_err(|e| e.to_string())?;
            let data = BTreeMap::from([
                ("user_id".into(), user.to_string()),
                (
                    "sticker_format".into(),
                    item["fmt"].as_str().ok_or("invalid format")?.into(),
                ),
            ]);
            match tg.call(
                "uploadStickerFile",
                &data,
                &[file],
                5,
                None::<fn() -> crate::telegram::Result<Option<bool>>>,
            ) {
                Ok(_) => accepted += 1,
                Err(ApiError::Rejected(error)) => {
                    log.event("ERROR", "preflight", &error);
                    refused.push((key, name, error));
                }
                Err(error) => {
                    log.event("WARNING", "preflight", &error.to_string());
                    unknown.push((key, name, error.to_string()));
                }
            }
        }
    }
    println!(
        "\nPREFLIGHT: {} file(s) queued -- {accepted} accepted, {} refused, {} missing, {} not checked.",
        accepted + refused.len() + missing.len() + unknown.len(),
        refused.len(),
        missing.len(),
        unknown.len()
    );
    for (key, name, why) in &refused {
        println!("  REFUSED {key}  ({name})\n          {why}");
    }
    for (key, name) in &missing {
        println!("  MISSING {key}  ({name}) is not on disk");
    }
    for (key, name, why) in &unknown {
        println!("  UNCHECKED {key}  ({name})\n            {why}");
    }
    if !refused.is_empty() || !missing.is_empty() {
        println!("\nNothing was published. Fix or deselect these, then publish.\n");
        Ok(4)
    } else if !unknown.is_empty() {
        println!(
            "\nNothing was published. Telegram could not be reached for {} file(s), so this run proves nothing about them. Re-run the preflight once the connection is back.\n",
            unknown.len()
        );
        Ok(3)
    } else if accepted == 0 {
        println!("Nothing was queued, so nothing was validated.\n");
        Ok(0)
    } else {
        println!("All {accepted} queued file(s) are acceptable to Telegram.\n");
        Ok(0)
    }
}
