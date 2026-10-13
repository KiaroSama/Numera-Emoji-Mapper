//! Remap CLI refuses uncalibrated writes before live work; no Telegram mutations.
use crate::{config::Config, logging::RunLog};
pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    let coins = config.root.join("coins");
    let mut files = crate::coin_remap::Files {
        root: config.root.clone(),
        state: coins.join("rebuild_dedup_state.json"),
        cache: coins.join("remap_live_cache.json"),
        images: coins.join("logos/emoji"),
        output: coins.join("ticker_to_id.json"),
        candidates: coins.join("ticker_to_id.candidate.json"),
        base: String::new(),
    };
    let mut supplied_images = false;
    let mut token_key = "TELEGRAM_BOT_TOKEN".to_owned();
    let mut help = false;
    let mut options = crate::coin_remap_policy::Options {
        apply: false,
        max_distance: None,
        min_margin: None,
        incomplete_sets: 0,
        failed_images: 0,
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => help = true,
            "--apply" => options.apply = true,
            "--emoji-dir" | "--token-env" | "--state" | "--cache" | "--out" | "--candidates"
            | "--max-distance" | "--min-margin" => {
                let option = &args[i];
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| format!("{option} requires a value"))?;
                match option.as_str() {
                    "--emoji-dir" => {
                        files.images = config.path(value);
                        supplied_images = true;
                    }
                    "--state" => files.state = config.path(value),
                    "--cache" => files.cache = config.path(value),
                    "--out" => files.output = config.path(value),
                    "--candidates" => files.candidates = config.path(value),
                    "--token-env" => token_key.clone_from(value),
                    "--max-distance" => {
                        options.max_distance = Some(
                            value
                                .parse()
                                .map_err(|_| "--max-distance must be numeric")?,
                        )
                    }
                    _ => {
                        options.min_margin =
                            Some(value.parse().map_err(|_| "--min-margin must be numeric")?)
                    }
                }
            }
            value => return Err(format!("unknown coin-remap-ids option: {value}")),
        }
        i += 1;
    }
    if help {
        println!(
            "Usage: numera-emoji coin-remap-ids --emoji-dir DIR [--token-env KEY] [--state PATH] [--cache PATH] [--out PATH] [--candidates PATH] [--max-distance N] [--min-margin N] [--apply]"
        );
        return Ok(0);
    }
    if !supplied_images {
        return Err("--emoji-dir is required".into());
    }
    if token_key.is_empty()
        || !token_key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err("remap token environment key is invalid".into());
    }
    crate::coin_remap_policy::decide(&[], options)?;
    files.base = config.require("COIN_PACK_BASE")?.into();
    let token = config.require(&token_key)?;
    let api = if config.value("NUMERA_EMOJI_MAPPER_NO_DOTENV") == "1" {
        config.require("NUMERA_TEST_API_BASE")?
    } else {
        "https://api.telegram.org"
    };
    let tg =
        crate::telegram::Telegram::new(token.into(), api.into()).map_err(|e| e.command_error())?;
    crate::coin_remap::run(&files, &tg, options, log)
}
