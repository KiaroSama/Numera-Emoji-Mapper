//! Read-only Bot API manifest command with locked inputs and atomic report outputs.
use crate::{config::Config, logging::RunLog};
use serde_json::Value;
use std::collections::BTreeMap;

pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    let coins = config.root.join("coins");
    let mut directory = coins.clone();
    let mut state = coins.join("rebuild_dedup_state.json");
    let mut mapping = coins.join("ticker_to_id.json");
    let mut token_key = "TELEGRAM_BOT_TOKEN".to_owned();
    let mut help = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => help = true,
            "--out-dir" | "--state" | "--map" | "--token-env" => {
                let option = &args[i];
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| format!("{option} requires a value"))?;
                match option.as_str() {
                    "--out-dir" => directory = config.path(value),
                    "--state" => state = config.path(value),
                    "--map" => mapping = config.path(value),
                    _ => token_key.clone_from(value),
                }
            }
            value => return Err(format!("unknown coin-write-manifests option: {value}")),
        }
        i += 1;
    }
    if help {
        println!(
            "Usage: numera-emoji coin-write-manifests [--out-dir DIR] [--state PATH] [--map PATH] [--token-env KEY]"
        );
        return Ok(0);
    }
    if token_key.is_empty()
        || !token_key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err("coin manifest token environment key is invalid".into());
    }
    let token = config.require(&token_key)?;
    let base = config.require("COIN_PACK_BASE")?;
    crate::coin_manifest::output_path(&directory)?;
    for path in [&state, &mapping] {
        crate::coin_provider_stage::physical(path)?;
    }
    let family_path = crate::locks::pack_family_path(&config.root, base);
    let map_path = config.root.join(".locks/canonical_map.lock");
    crate::coin_manifest::output_path(&family_path)?;
    crate::coin_manifest::output_path(&map_path)?;
    let _family = crate::locks::Lease::acquire(&family_path, &crate::logging::utc())
        .map_err(|_| "coin manifest pack family is busy or unavailable")?;
    let _map = crate::locks::Lease::acquire(&map_path, &crate::logging::utc())
        .map_err(|_| "coin manifest canonical map is busy or unavailable")?;
    let state = Value::Object(crate::coin_provider_recover::object(&state, false)?);
    let mapping = Value::Object(crate::coin_provider_recover::object(&mapping, false)?);
    let api = if config.value("NUMERA_EMOJI_MAPPER_NO_DOTENV") == "1" {
        config.require("NUMERA_TEST_API_BASE")?
    } else {
        "https://api.telegram.org"
    };
    let tg =
        crate::telegram::Telegram::new(token.into(), api.into()).map_err(|e| e.command_error())?;
    let (packs, stickers) = crate::coin_manifest::write(&directory, &state, &mapping, |name| {
        tg.read_call(
            "getStickerSet",
            &BTreeMap::from([("name".into(), name.into())]),
        )
        .map_err(|e| e.command_error())
    })?;
    log.event(
        "INFO",
        "coins",
        &format!("manifest_packs={packs} manifest_stickers={stickers}"),
    );
    println!("wrote manifests/INDEX.md | {packs} packs, {stickers} stickers");
    Ok(0)
}
