//! Native coin rebuild entry point; parsing refuses typos before any destructive operation.
use crate::{
    coin_rebuild_map_disk::Files, coin_rebuild_walk::Options, config::Config, logging::RunLog,
};
use serde_json::Value;
use std::collections::BTreeMap;

pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    let mut directory = config.root.join("coins");
    let mut images = None;
    let mut token_key = "TELEGRAM_BOT_TOKEN".to_owned();
    let mut command = None;
    let mut help = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => help = true,
            "--coin-dir" | "--emoji-dir" | "--token-env" => {
                let option = &args[index];
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("{option} requires a value"))?;
                match option.as_str() {
                    "--coin-dir" => directory = config.path(value),
                    "--emoji-dir" => images = Some(config.path(value)),
                    _ => token_key.clone_from(value),
                }
            }
            value if ["all", "build", "map", "links"].contains(&value) && command.is_none() => {
                command = Some(value)
            }
            value => return Err(format!("unknown coin-rebuild argument: {value}")),
        }
        index += 1;
    }
    if help {
        println!(
            "Usage: numera-emoji coin-rebuild [all|build|map|links] [--coin-dir DIR] [--emoji-dir DIR] [--token-env KEY]\nDefault all and build may delete old packs and upload; links sends messages. map reads Telegram only. Use only explicitly authorized operator targets."
        );
        return Ok(0);
    }
    if token_key.is_empty()
        || !token_key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err("coin rebuild token environment key is invalid".into());
    }
    let base = config.require("COIN_PACK_BASE")?;
    let title = config.require("COIN_PACK_TITLE")?;
    let token = config.require(&token_key)?;
    let owner = config.integer("PACK_OWNER_USER_ID", 0, 0, i64::MAX) as u64;
    let command = command.unwrap_or("all");
    if command != "map" && owner == 0 {
        return Err("coin rebuild requires PACK_OWNER_USER_ID".into());
    }
    let api = if config.value("NUMERA_EMOJI_MAPPER_NO_DOTENV") == "1" {
        config.require("NUMERA_TEST_API_BASE")?
    } else {
        "https://api.telegram.org"
    };
    let tg =
        crate::telegram::Telegram::new(token.into(), api.into()).map_err(|e| e.command_error())?;
    let me = tg
        .read_call("getMe", &BTreeMap::new())
        .map_err(|e| e.command_error())?;
    let bot = me["username"]
        .as_str()
        .ok_or("coin rebuild bot username is unavailable")?;
    let files = Files {
        root: config.root.clone(),
        images: images.unwrap_or_else(|| directory.join("logos/emoji")),
        directory,
        base: base.into(),
    };
    if command == "map" {
        crate::coin_rebuild_map_disk::run(&files, &tg)?;
    } else if command == "links" {
        crate::coin_rebuild_links::send(config, &files.directory, base, title, owner, &tg)?;
    } else {
        let result = crate::coin_rebuild_effects::run(
            config,
            &files,
            &Options {
                owner,
                base: base.into(),
                title: title.into(),
                bot: bot.into(),
            },
            &tg,
        );
        if let Err(error) = result {
            let error = crate::logging::redact(&error, &config.secret_values());
            log.event("ERROR", "coins", &format!("rebuild=partial reason={error}"));
            eprintln!("STOP: {error}");
            return Ok(3);
        }
        if command == "build" {
            let family = crate::locks::pack_family_path(&config.root, base);
            crate::coin_manifest::output_path(&family)?;
            let _lease = crate::locks::Lease::acquire(&family, &crate::logging::utc())
                .map_err(|_| "coin rebuild checkpoint family is busy or unavailable")?;
            let state = Value::Object(crate::coin_provider_recover::object(
                &files.directory.join("rebuild_dedup_state.json"),
                false,
            )?);
            let plan = crate::coin_plan_disk::load(&files.directory)?;
            let size = crate::coin_plan_disk::validate(&plan)?.len();
            let cursor = state["cursor"]
                .as_u64()
                .ok_or("coin rebuild checkpoint cursor is invalid")?;
            println!("buildonly checkpoint: plan position {cursor}/{size}");
            if cursor < size as u64 || !state["in_flight"].is_null() {
                return Ok(3);
            }
        } else {
            crate::coin_rebuild_map_disk::run(&files, &tg)?;
            crate::coin_rebuild_links::send(config, &files.directory, base, title, owner, &tg)?;
        }
    }
    log.event(
        "INFO",
        "coins",
        &format!("rebuild_command={command} result=complete"),
    );
    Ok(0)
}
