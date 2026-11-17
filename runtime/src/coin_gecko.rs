//! Market-cap names win; long-tail list still runs after a partial market lookup.
use crate::coin_http::{ProviderHttp, Retry};
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde_json::{Map, Value};
use std::time::Duration;

pub fn http_json(client: &ProviderHttp, url: &str) -> Result<Value, String> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Mozilla/5.0 (logo-fetcher; local tool)"),
    );
    let policy = Retry {
        attempts: 6,
        backoff: 8.0,
        max_backoff: 40.0,
        stop_on: Vec::new(),
    };
    let reply = client
        .get(url, &headers, &policy)?
        .ok_or("CoinGecko GET failed after6attempts")?;
    serde_json::from_slice(&reply.bytes).map_err(|_| "invalid CoinGecko JSON".into())
}

fn add(names: &mut Map<String, Value>, coins: &[Value]) -> Result<(), String> {
    for coin in coins {
        let symbol = coin
            .get("symbol")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase();
        let symbol =
            symbol.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c));
        let name = coin
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c));
        if !coin.is_object() {
            return Err("CoinGecko coin must be an object".into());
        }
        if !symbol.is_empty() && !names.contains_key(symbol) {
            names.insert(symbol.into(), Value::String(name.into()));
        }
    }
    Ok(())
}

pub fn names_with<F, W>(
    delay: Duration,
    mut get: F,
    mut wait: W,
) -> Result<(Map<String, Value>, bool), String>
where
    F: FnMut(&str) -> Result<Value, String>,
    W: FnMut(Duration) -> Result<(), String>,
{
    let mut names = Map::new();
    let mut complete = true;
    for page in 1..=12 {
        let url = format!(
            "https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page=250&page={page}&sparkline=false"
        );
        let payload = match get(&url) {
            Ok(value) => value,
            Err(_) => {
                complete = false;
                break;
            }
        };
        let Some(coins) = payload.as_array().filter(|coins| !coins.is_empty()) else {
            break;
        };
        add(&mut names, coins)?;
        wait(delay)?;
    }
    match get("https://api.coingecko.com/api/v3/coins/list") {
        Ok(payload) => add(
            &mut names,
            payload
                .as_array()
                .ok_or("CoinGecko coins/list must be a list")?,
        )?,
        Err(_) => complete = false,
    }
    Ok((names, complete))
}

pub fn names(client: &ProviderHttp, delay: Duration) -> Result<(Map<String, Value>, bool), String> {
    names_with(
        delay,
        |url| http_json(client, url),
        |duration| {
            std::thread::sleep(duration);
            Ok(())
        },
    )
}
