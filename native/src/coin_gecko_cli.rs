//! CoinGecko fetch command keeps source page count and pooled retry behavior.
use crate::{
    coin_gecko_fetch::{Payload, fetch_with},
    coin_http::{ProviderHttp, Retry},
    config::Config,
    logging::RunLog,
};
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use std::time::Duration;

pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    let mut directory = config.root.join("coins");
    let mut pages = None;
    let mut help = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => help = true,
            "--coin-dir" => {
                index += 1;
                directory = config.path(args.get(index).ok_or("--coin-dir requires a path")?);
            }
            value if !value.starts_with('-') && pages.is_none() => {
                pages = Some(
                    value
                        .parse::<u32>()
                        .ok()
                        .filter(|p| *p > 0)
                        .ok_or("pages must be at least 1")?,
                );
            }
            value => return Err(format!("unknown coin-fetch-logos argument: {value}")),
        }
        index += 1;
    }
    if help {
        println!(
            "Usage: numera-emoji coin-fetch-logos [pages] [--coin-dir DIR]\nDefault: 40 market pages of 250 coins."
        );
        return Ok(0);
    }
    crate::coin_provider_stage::physical(&directory)?;
    let delay = config
        .value("COIN_PAGE_DELAY")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map(|v| v.max(0.0))
        .unwrap_or(2.0);
    let delay = Duration::try_from_secs_f64(delay)
        .map_err(|_| "COIN_PAGE_DELAY exceeds native duration range")?;
    let client = ProviderHttp::new(Duration::from_secs(30))?;
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Mozilla/5.0 (logo-fetcher; local tool)"),
    );
    let code = fetch_with(
        &directory,
        pages.unwrap_or(40),
        delay,
        |url, binary, retries| {
            let reply = client
                .get(
                    url,
                    &headers,
                    &Retry {
                        attempts: retries,
                        backoff: 8.0,
                        max_backoff: 60.0,
                        stop_on: Vec::new(),
                    },
                )?
                .ok_or_else(|| format!("GET failed after {retries} attempts"))?;
            if binary {
                Ok(Payload::Bytes(reply.bytes))
            } else {
                serde_json::from_slice(&reply.bytes)
                    .map(Payload::Json)
                    .map_err(|_| "invalid CoinGecko JSON".into())
            }
        },
        |duration| {
            std::thread::sleep(duration);
            Ok(())
        },
    )?;
    log.event(
        "INFO",
        "coins",
        &format!("logo_fetch_pages={} exit={code}", pages.unwrap_or(40)),
    );
    println!("Coin logo fetch finished; keywords.csv written.");
    if code == 3 {
        println!("INCOMPLETE: market paging stopped early; re-run to finish.");
    }
    Ok(code)
}
