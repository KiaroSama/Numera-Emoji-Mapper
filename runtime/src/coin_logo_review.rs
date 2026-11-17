//! A logo review flags differences for a human; it never authorizes a repair.
use serde_json::Value;
use std::time::Duration;
pub fn review_with<G, E, F, D, W>(
    top: i64,
    threshold: i64,
    mut get: G,
    mut exists: E,
    mut image: F,
    mut distance: D,
    mut wait: W,
) -> Result<Vec<(u32, String, String, String)>, String>
where
    G: FnMut(&str) -> Result<Value, String>,
    E: FnMut(&str) -> bool,
    F: FnMut(&str) -> Result<Option<Vec<u8>>, String>,
    D: FnMut(&str, &[u8]) -> Result<u32, String>,
    W: FnMut(Duration),
{
    if top > 10000 {
        return Err("logo review exceeds10000-coin bounded limit".into());
    }
    let mut coins = Vec::new();
    for page in 1..=(top + 249) / 250 {
        let url = format!(
            "https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page=250&page={page}&sparkline=false"
        );
        let payload = get(&url)?;
        coins.extend(
            payload
                .as_array()
                .ok_or("logo review markets must be a list")?
                .iter()
                .cloned(),
        );
        wait(Duration::from_secs(3));
    }
    coins.truncate(usize::try_from(top.max(0)).map_err(|_| "invalid review count")?);
    let mut flagged = Vec::new();
    for coin in coins {
        if !coin.is_object() {
            return Err("logo review coin must be an object".into());
        }
        let symbol = coin["symbol"].as_str().unwrap_or("").to_lowercase();
        if symbol.is_empty() || !exists(&symbol) {
            continue;
        }
        let url = coin["image"].as_str().unwrap_or("");
        let Some(bytes) = image(url)?.filter(|bytes| !bytes.is_empty()) else {
            continue;
        };
        let difference = match distance(&symbol, &bytes) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if i64::from(difference) > threshold {
            flagged.push((
                difference,
                symbol,
                coin["name"].as_str().unwrap_or("").into(),
                url.into(),
            ));
        }
        wait(Duration::from_millis(50));
    }
    flagged.sort_by(|left, right| right.cmp(left));
    Ok(flagged)
}
