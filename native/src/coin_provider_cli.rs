//! Provider CLI admission and shared durable publisher; dry search never selects live effects.
use crate::{
    coin_http::ProviderHttp, coin_provider_recover::Files, config::Config, logging::RunLog,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

pub fn refill(directory: &Path, mapping: &Map<String, Value>) -> Result<(), String> {
    let text = std::fs::read_to_string(directory.join("currency-emoji-inventory.md"))
        .map_err(|_| "cannot read coin inventory")?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let ids = mapping
        .iter()
        .map(|(k, v)| {
            Ok((
                k.clone(),
                v.as_str().ok_or("coin map IDs must be strings")?.into(),
            ))
        })
        .collect::<Result<BTreeMap<String, String>, String>>()?;
    let result = crate::coin_inventory::refill(&ids, &text);
    let text = if cfg!(windows) {
        result.text.replace('\n', "\r\n")
    } else {
        result.text
    };
    crate::atomic::write_bytes(
        &directory.join("currency-emoji-inventory.filled.md"),
        text.as_bytes(),
    )
    .map_err(|_| "cannot save filled coin inventory")?;
    println!("inventory filled: {}/{}", result.filled, result.total);
    Ok(())
}
fn code(succeeded: usize, failed: usize, unknown: usize) -> u8 {
    let result = if failed == 0 {
        0
    } else if succeeded > 0 {
        3
    } else {
        4
    };
    if unknown > 0 { result.max(3) } else { result }
}

pub fn run(config: &Config, provider: &str, args: &[String], log: &RunLog) -> Result<u8, String> {
    let mut directory = config.root.join("coins");
    let mut dry = false;
    let mut retry = false;
    let mut help = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => help = true,
            "--dry" => dry = true,
            "--retry-unmatched" if provider == "paprika" => retry = true,
            "--coin-dir" => {
                i += 1;
                directory = config.path(args.get(i).ok_or("--coin-dir requires a path")?);
            }
            value => return Err(format!("unknown coin-fetch-{provider} option: {value}")),
        }
        i += 1;
    }
    if help {
        println!(
            "Usage: numera-emoji coin-fetch-{provider} [--dry] [--coin-dir DIR]{}",
            if provider == "paprika" {
                " [--retry-unmatched]"
            } else {
                ""
            }
        );
        return Ok(0);
    }
    let base = config.require("COIN_PACK_BASE")?;
    let title = config.require("COIN_PACK_TITLE")?;
    let headers = if provider == "cmc" {
        Some(crate::coin_cmc::headers(config.value("CMC_API_KEY"))?)
    } else {
        None
    };
    let token = if dry {
        None
    } else {
        Some(config.value("TELEGRAM_BOT_TOKEN").to_owned())
    };
    if token.as_ref().is_some_and(|t| t.is_empty()) {
        return Err("TELEGRAM_BOT_TOKEN is not set (see .env.example)".into());
    }
    crate::coin_provider_stage::physical(&directory)?;
    let mapping =
        crate::coin_provider_recover::object(&directory.join("ticker_to_id.json"), false)?;
    let have = mapping.keys().cloned().collect::<BTreeSet<_>>();
    let inventory = std::fs::read_to_string(directory.join("currency-emoji-inventory.md"))
        .map_err(|_| "cannot read coin inventory")?;
    let missing = crate::coin_inventory_parse::missing(&inventory, &have);
    let client = ProviderHttp::new(Duration::from_secs(40))?;
    let (confident, unknown) = if provider == "paprika" {
        let (resolved, cache) = crate::coin_provider_resolve::resolve_file(
            &directory.join("paprika_matches.json"),
            &missing,
            retry,
            |name, ticker| crate::coin_paprika::search(&client, name, ticker),
        )?;
        let confident = cache
            .iter()
            .filter(|(ticker, _)| !have.contains(*ticker))
            .filter_map(|(ticker, row)| {
                row["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .map(|id| (ticker.clone(), id.to_owned()))
            })
            .collect::<Vec<_>>();
        println!(
            "confident(new)={} quota_hit={} unknown={}",
            confident.len(),
            resolved.quota,
            resolved.unknown
        );
        (confident, resolved.unknown)
    } else {
        let headers = headers.as_ref().ok_or("CMC headers missing")?;
        let mut confident = Vec::new();
        let mut unknown = 0;
        for (name, ticker) in &missing {
            let (candidates, transient) = crate::coin_cmc::candidates(&client, headers, ticker)?;
            let result = crate::coin_provider_policy::classify(&candidates, name, ticker)?;
            if let Some(id) = result[0].as_str().filter(|id| !id.is_empty()) {
                if matches!(result[1].as_str(), Some("name-exact" | "symbol")) {
                    confident.push((ticker.clone(), id.into()));
                }
            } else if transient {
                unknown += 1;
            }
        }
        (confident, unknown)
    };
    log.event(
        "INFO",
        "coins",
        &format!(
            "provider={provider} dry={dry} confident={} unknown={unknown}",
            confident.len()
        ),
    );
    if dry || confident.is_empty() {
        if dry {
            println!("dry run: no downloads/pack changes.");
        }
        return Ok(code(0, 0, unknown));
    }
    let mut stage = crate::coin_provider_stage::Stage::new(&directory.join("logos/emoji"))?;
    let mut handed_to_publisher = false;
    let result = (|| {
        let mut fetched = Vec::new();
        let mut failed = 0;
        for (ticker, id) in confident {
            let data = if provider == "paprika" {
                let url = format!("https://static.coinpaprika.com/coin/{id}/logo.png");
                let mut data = crate::coin_paprika::http_bytes(&client, &url)?;
                if data.as_ref().is_none_or(Vec::is_empty) {
                    let detail = crate::coin_paprika::http_json(
                        &client,
                        &format!("https://api.coinpaprika.com/v1/coins/{id}"),
                    )?;
                    std::thread::sleep(Duration::from_millis(2500));
                    if let crate::coin_paprika::Response::Payload(value) = detail
                        && let Some(other) = value["logo"].as_str().filter(|other| *other != url)
                    {
                        data = crate::coin_paprika::http_bytes(&client, other)?;
                    }
                }
                data
            } else {
                let url = crate::coin_cmc::logo(
                    &client,
                    headers.as_ref().ok_or("CMC headers missing")?,
                    &id,
                )?;
                if let Some(url) = url {
                    let mut data = crate::coin_paprika::http_bytes(&client, &url)?;
                    if data.as_ref().is_none_or(Vec::is_empty) {
                        data = crate::coin_paprika::http_bytes(
                            &client,
                            &url.replace("/128x128/", "/64x64/"),
                        )?;
                    }
                    data
                } else {
                    None
                }
            };
            if let Some(data) = data.filter(|data| !data.is_empty()) {
                if crate::codec_image::provider_png(&data, &stage.source(&ticker)?)? {
                    fetched.push(ticker);
                } else {
                    failed += 1;
                }
            } else {
                failed += 1;
            }
        }
        if fetched.is_empty() {
            return Ok(code(0, failed, unknown));
        }
        let token = token.ok_or("live provider run has no token")?;
        let api = config.value("TELEGRAM_API_BASE");
        let api = if api.is_empty() {
            "https://api.telegram.org"
        } else {
            api
        };
        let tg =
            crate::telegram::Telegram::new(token, api.into()).map_err(|e| e.command_error())?;
        let keywords = crate::ticker_keywords::load(&directory.join("keywords.csv"))?
            .into_iter()
            .map(|(k, v)| (k, json!(v)))
            .collect();
        let files = Files {
            root: config.root.clone(),
            state: directory.join("rebuild_dedup_state.json"),
            mapping: directory.join("ticker_to_id.json"),
            published: directory.join("logos/emoji"),
            base: base.into(),
        };
        handed_to_publisher = true;
        let (added, add_failed, mapping) = crate::coin_provider_effects::run(
            &files,
            &tg,
            &mut stage,
            &fetched,
            title,
            config.integer("PACK_OWNER_USER_ID", 0, 0, i64::MAX) as u64,
            &keywords,
        )?;
        refill(&directory, &mapping)?;
        Ok(code(added, failed + add_failed, unknown))
    })();
    // Only the publisher's locked state may decide cleanup once mutation can exist.
    if !handed_to_publisher {
        stage.clean(None)?;
    }
    result
}
