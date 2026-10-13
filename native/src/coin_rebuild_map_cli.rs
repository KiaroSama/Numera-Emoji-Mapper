//! Content-proven rebuild mapping; no Telegram mutation and no positional fallback.
use crate::{coin_rebuild_map_disk::Files, config::Config, logging::RunLog};

pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    let mut directory = config.root.join("coins");
    let mut images = None;
    let mut token_key = "TELEGRAM_BOT_TOKEN".to_owned();
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
            value => return Err(format!("unknown coin-rebuild-map option: {value}")),
        }
        index += 1;
    }
    if help {
        println!(
            "Usage: numera-emoji coin-rebuild-map [--coin-dir DIR] [--emoji-dir DIR] [--token-env KEY]"
        );
        return Ok(0);
    }
    if token_key.is_empty()
        || !token_key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err("rebuild map token environment key is invalid".into());
    }
    let token = config.require(&token_key)?;
    let base = config.require("COIN_PACK_BASE")?;
    let images = images.unwrap_or_else(|| directory.join("logos/emoji"));
    let api = if config.value("NUMERA_EMOJI_MAPPER_NO_DOTENV") == "1" {
        config.require("NUMERA_TEST_API_BASE")?
    } else {
        "https://api.telegram.org"
    };
    let tg =
        crate::telegram::Telegram::new(token.into(), api.into()).map_err(|e| e.command_error())?;
    crate::coin_rebuild_map_disk::run(
        &Files {
            root: config.root.clone(),
            directory,
            images,
            base: base.into(),
        },
        &tg,
    )?;
    log.event(
        "INFO",
        "coins",
        "rebuild_map=content_proven inventory=filled",
    );
    Ok(0)
}
