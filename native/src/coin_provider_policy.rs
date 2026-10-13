//! Provider no-match, quota, rejected credentials and unanswered transport are different outcomes.
use serde_json::{Value, json};

pub fn classify(candidates: &[Value], name: &str, ticker: &str) -> Result<Value, String> {
    if candidates.is_empty() {
        return Ok(json!([null, "no-results"]));
    }
    let normalized = crate::coin_inventory::norm(name);
    let base = crate::coin_inventory::base_ticker(ticker);
    for candidate in candidates {
        let label = candidate.get("name").and_then(Value::as_str).unwrap_or("");
        if crate::coin_inventory::norm(label) == normalized {
            return Ok(json!([
                candidate.get("id").ok_or("provider candidate ID missing")?,
                "name-exact"
            ]));
        }
    }
    for candidate in candidates {
        let symbol = candidate
            .get("symbol")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase();
        if symbol == ticker || symbol == base {
            return Ok(json!([
                candidate.get("id").ok_or("provider candidate ID missing")?,
                "symbol"
            ]));
        }
    }
    for candidate in candidates {
        let label = crate::coin_inventory::norm(
            candidate.get("name").and_then(Value::as_str).unwrap_or(""),
        );
        if !label.is_empty() && (label.contains(&normalized) || normalized.contains(&label)) {
            let id = candidate["id"]
                .as_str()
                .ok_or("provider partial candidate ID must be a string")?;
            return Ok(json!([null, format!("partial:{id}")]));
        }
    }
    let id = candidates[0]["id"]
        .as_str()
        .ok_or("provider candidate ID must be a string")?;
    Ok(json!([null, format!("no-confident-match(top={id})")]))
}

pub fn response(provider: &str, status: Option<u16>, payload: Value) -> Result<Value, String> {
    if !matches!(provider, "paprika" | "cmc") {
        return Err("unsupported coin provider".into());
    }
    let Some(status) = status else {
        return Ok(json!({"kind":"transient"}));
    };
    if provider == "paprika" && status == 402 {
        return Ok(json!({"kind":"quota"}));
    }
    if provider == "cmc" {
        if matches!(status, 401 | 403) {
            return Ok(
                json!({"kind":"stop","message":format!("CMC_API_KEY rejected by CoinMarketCap (HTTP {status})")}),
            );
        }
        if matches!(status, 400 | 404) {
            return Ok(json!({"kind":"missing"}));
        }
    }
    Ok(json!({"kind":"payload","value":payload}))
}
