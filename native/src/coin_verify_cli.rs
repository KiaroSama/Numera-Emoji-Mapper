//! Explicit logo review or operator-selected repairs; a review never selects fixes automatically.
use crate::{
    coin_http::{ProviderHttp, Retry},
    config::Config,
    logging::RunLog,
};
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde_json::Value;
use std::time::Duration;

fn safe_ticker(value: &str) -> bool {
    !value.is_empty()
        && !value.trim_matches(['.', '_', '-']).is_empty()
        && !value.contains("..")
        && value.bytes().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'.' | b'_' | b'-')
        })
}
fn markets(client: &ProviderHttp, url: &str) -> Result<Value, String> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Mozilla/5.0 (logo-verify; local tool)"),
    );
    let reply = client
        .get(url, &headers, &Retry::default())?
        .ok_or("logo review markets unavailable")?;
    serde_json::from_slice(&reply.bytes).map_err(|_| "invalid logo review market JSON".into())
}
pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    let coins = config.root.join("coins");
    let mut files = crate::coin_replace_disk::Files {
        root: config.root.clone(),
        map: coins.join("ticker_to_id.json"),
        state: coins.join("rebuild_dedup_state.json"),
        images: coins.join("logos/emoji"),
        intent: coins.join("verify_logos_intent.json"),
        base: String::new(),
    };
    let mut images = false;
    let mut help = false;
    let mut fix = false;
    let mut top = 100_i64;
    let mut threshold = 20_i64;
    let mut only = String::new();
    let mut token_key = "TELEGRAM_BOT_TOKEN".to_owned();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => help = true,
            "--fix" => fix = true,
            "--emoji-dir" | "--token-env" | "--map" | "--state" | "--top" | "--threshold"
            | "--only" => {
                let option = &args[i];
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| format!("{option} requires a value"))?;
                match option.as_str() {
                    "--emoji-dir" => {
                        files.images = config.path(value);
                        images = true;
                    }
                    "--token-env" => token_key.clone_from(value),
                    "--map" => files.map = config.path(value),
                    "--state" => files.state = config.path(value),
                    "--top" => top = value.parse().map_err(|_| "--top must be an integer")?,
                    "--threshold" => {
                        threshold = value
                            .parse()
                            .map_err(|_| "--threshold must be an integer")?
                    }
                    _ => only.clone_from(value),
                }
            }
            value => return Err(format!("unknown coin-verify-logos option: {value}")),
        }
        i += 1;
    }
    if help {
        println!(
            "Usage: numera-emoji coin-verify-logos --emoji-dir DIR [--top N] [--threshold N] [--fix --only a,b] [--map PATH] [--state PATH] [--token-env KEY]"
        );
        return Ok(0);
    }
    if !images {
        return Err("--emoji-dir is required".into());
    }
    if !(0..=10000).contains(&top) {
        return Err("--top must be0..10000".into());
    }
    crate::coin_provider_stage::physical(&files.images)?;
    if !files.images.is_dir() {
        return Err("logo review images must be a directory".into());
    }
    files.base = config.require("COIN_PACK_BASE")?.into();
    let client = ProviderHttp::new(Duration::from_secs(60))?;
    if !fix {
        let flagged = crate::coin_logo_review::review_with(
            top,
            threshold,
            |url| markets(&client, url),
            |symbol| safe_ticker(symbol) && files.images.join(format!("{symbol}.png")).is_file(),
            |url| crate::coin_paprika::http_bytes(&client, url),
            |symbol, bytes| {
                let source = files.images.join(format!("{symbol}.png"));
                crate::coin_provider_stage::physical(&source)?;
                crate::codec_image::logo_distance(
                    &std::fs::read(source).map_err(|_| "cannot read local review art")?,
                    bytes,
                )
            },
            std::thread::sleep,
        )?;
        for (distance, symbol, name, _) in &flagged {
            println!("{symbol} distance={distance} {name}");
        }
        log.event(
            "INFO",
            "coins",
            &format!(
                "review_flagged={} top={top} threshold={threshold}",
                flagged.len()
            ),
        );
        println!("Review only. Flags are not proof of wrong art; repair only with --fix --only.");
        return Ok(0);
    }
    let tickers = only
        .split(',')
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if tickers.is_empty() || tickers.iter().any(|ticker| !safe_ticker(ticker)) {
        return Err("--fix requires --only with explicit safe tickers".into());
    }
    if token_key.is_empty()
        || !token_key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err("invalid logo repair token environment key".into());
    }
    let token = config.require(&token_key)?;
    let user = config.integer("PACK_OWNER_USER_ID", 0, 0, i64::MAX) as u64;
    if user == 0 {
        return Err("PACK_OWNER_USER_ID must be set to a positive numeric ID".into());
    }
    for path in [&files.map, &files.state] {
        crate::coin_provider_stage::physical(path)?;
        crate::coin_provider_recover::object(path, false)?;
    }
    crate::coin_manifest::output_path(&files.intent)?;
    let api = if config.value("NUMERA_EMOJI_MAPPER_NO_DOTENV") == "1" {
        config.require("NUMERA_TEST_API_BASE")?
    } else {
        "https://api.telegram.org"
    };
    let tg =
        crate::telegram::Telegram::new(token.into(), api.into()).map_err(|e| e.command_error())?;
    let family = crate::locks::pack_family_path(&files.root, &files.base);
    crate::coin_manifest::output_path(&family)?;
    let _family = crate::locks::Lease::acquire(&family, &crate::logging::utc())
        .map_err(|_| "logo repair family is busy or unavailable")?;
    // Recovery runs before provider requests or any new intent, under the same family lease.
    if !crate::coin_replace_disk::recover_locked(&files, &tg)? {
        return Err("previous logo replacement remains unresolved".into());
    }
    let mut fixed = 0;
    for ticker in &tickers {
        let payload = markets(
            &client,
            "https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page=250&page=1&sparkline=false",
        )?;
        std::thread::sleep(Duration::from_secs(3));
        if let Some(coin) = payload
            .as_array()
            .ok_or("logo repair markets must be a list")?
            .iter()
            .find(|coin| {
                coin["symbol"]
                    .as_str()
                    .is_some_and(|symbol| symbol.to_lowercase() == *ticker)
            })
            && let Some(url) = coin["image"].as_str()
            && let Some(bytes) = crate::coin_paprika::http_bytes(&client, url)?
            && crate::coin_replace_effects::replace_locked(
                &files,
                &tg,
                ticker,
                coin["name"].as_str().unwrap_or(""),
                &bytes,
                user,
            )?
        {
            fixed += 1;
        }
        if files.intent.exists() {
            return Err(
                "unresolved logo replacement retains intent and source; no further fixes attempted"
                    .into(),
            );
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    let failed = tickers.len() - fixed;
    log.event(
        "INFO",
        "coins",
        &format!("requested={} fixed={fixed} failed={failed}", tickers.len()),
    );
    Ok(if failed == 0 {
        0
    } else if fixed > 0 {
        3
    } else {
        4
    })
}
