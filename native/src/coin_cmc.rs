//! CMC rejected credentials stop the run; a missing symbol is not an unanswered lookup.
use crate::coin_http::{ProviderHttp, Retry};
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue};
use serde_json::Value;
use std::time::Duration;

pub enum Response {
    Payload(Value),
    Missing,
    Transient,
}

pub fn headers(key: &str) -> Result<HeaderMap, String> {
    let key = key.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c));
    if key.is_empty() {
        return Err("CMC_API_KEY missing in .env".into());
    }
    let mut value = HeaderValue::from_str(key).map_err(|_| "invalid CMC_API_KEY header")?;
    value.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert("x-cmc_pro_api_key", value);
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    Ok(headers)
}

pub fn http_json(
    client: &ProviderHttp,
    url: &str,
    headers: &HeaderMap,
) -> Result<Response, String> {
    let policy = Retry {
        stop_on: vec![400, 401, 403, 404],
        ..Retry::default()
    };
    let Some(reply) = client.get(url, headers, &policy)? else {
        return Ok(Response::Transient);
    };
    match reply.status {
        401 | 403 => Err(format!(
            "CMC_API_KEY rejected by CoinMarketCap (HTTP {})",
            reply.status
        )),
        400 | 404 => Ok(Response::Missing),
        _ => serde_json::from_slice(&reply.bytes)
            .map(Response::Payload)
            .map_err(|_| "invalid CoinMarketCap JSON".into()),
    }
}

pub fn symbols(ticker: &str) -> Vec<String> {
    let full = ticker.to_uppercase();
    let base = crate::coin_inventory::base_ticker(ticker).to_uppercase();
    if full == base {
        vec![full]
    } else {
        vec![full, base]
    }
}

pub fn candidates_with<F, W>(
    symbols: &[String],
    mut get: F,
    mut wait: W,
) -> Result<(Vec<Value>, bool), String>
where
    F: FnMut(&str) -> Result<Response, String>,
    W: FnMut(Duration) -> Result<(), String>,
{
    let mut seen = serde_json::Map::new();
    let mut transient = false;
    for symbol in symbols {
        let response = get(symbol)?;
        wait(Duration::from_secs(1))?;
        let value = match response {
            Response::Transient => {
                transient = true;
                continue;
            }
            Response::Missing => continue,
            Response::Payload(value) => value,
        };
        let Some(data) = value.get("data").filter(|data| !data.is_null()) else {
            continue;
        };
        for candidate in data.as_array().ok_or("CoinMarketCap data must be a list")? {
            let id = candidate
                .get("id")
                .ok_or("CoinMarketCap candidate ID missing")?;
            let id = match id {
                Value::String(id) => id.clone(),
                Value::Number(id) if id.is_i64() || id.is_u64() => id.to_string(),
                _ => return Err("CoinMarketCap candidate ID must be an integer or string".into()),
            };
            seen.insert(
                id.clone(),
                serde_json::json!({"id":id,
                "name":candidate.get("name").cloned().unwrap_or(Value::String(String::new())),
                "symbol":candidate.get("symbol").cloned().unwrap_or(Value::String(String::new()))}),
            );
        }
    }
    Ok((
        seen.into_iter().map(|(_, value)| value).collect(),
        transient,
    ))
}

pub fn candidates(
    client: &ProviderHttp,
    headers: &HeaderMap,
    ticker: &str,
) -> Result<(Vec<Value>, bool), String> {
    candidates_with(
        &symbols(ticker),
        |symbol| {
            let mut url =
                reqwest::Url::parse("https://pro-api.coinmarketcap.com/v1/cryptocurrency/map")
                    .map_err(|_| "invalid CoinMarketCap map URL")?;
            url.query_pairs_mut().append_pair("symbol", symbol);
            http_json(client, url.as_str(), headers)
        },
        |duration| {
            std::thread::sleep(duration);
            Ok(())
        },
    )
}

pub fn logo_with<F, W>(id: &str, mut get: F, mut wait: W) -> Result<Option<String>, String>
where
    F: FnMut(&str) -> Result<Response, String>,
    W: FnMut(Duration) -> Result<(), String>,
{
    let response = get(&format!(
        "https://pro-api.coinmarketcap.com/v2/cryptocurrency/info?id={id}"
    ))?;
    wait(Duration::from_secs(1))?;
    let Response::Payload(value) = response else {
        return Ok(None);
    };
    let Some(data) = value.get("data") else {
        return Ok(None);
    };
    let Some(logo) = data.get(id).and_then(|info| info.get("logo")) else {
        return Ok(None);
    };
    if logo.is_null() || logo.as_str() == Some("") || logo.as_bool() == Some(false) {
        return Ok(None);
    }
    logo.as_str()
        .map(|url| Some(url.replace("/64x64/", "/128x128/")))
        .ok_or_else(|| "CoinMarketCap logo URL must be a string".into())
}

pub fn logo(
    client: &ProviderHttp,
    headers: &HeaderMap,
    id: &str,
) -> Result<Option<String>, String> {
    if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("CoinMarketCap ID must contain decimal digits".into());
    }
    logo_with(
        id,
        |url| http_json(client, url, headers),
        |duration| {
            std::thread::sleep(duration);
            Ok(())
        },
    )
}
