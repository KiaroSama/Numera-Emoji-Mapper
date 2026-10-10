//! Live-roster entry: source IDs, current ID and ordered replacement history are separate.
use serde_json::{Value, json};

pub fn pack(
    record: &Value,
    live: &Value,
    family: &str,
    provenance: &Value,
    prior: &Value,
    captured: &str,
) -> Result<Value, String> {
    let name = record["name"].as_str().ok_or("recorded pack has no name")?;
    if !["general", "coins"].contains(&family) {
        return Err("unknown roster family".into());
    }
    let stickers = live["stickers"]
        .as_array()
        .ok_or("live pack has no stickers")?;
    let mut entries = Vec::with_capacity(stickers.len());
    for (pos, sticker) in stickers.iter().enumerate() {
        let id = sticker["custom_emoji_id"].as_str().unwrap_or("");
        let info = provenance.get(id);
        let (logo, label, key, sources) = if family == "coins" {
            let tickers = info.and_then(Value::as_array).cloned().unwrap_or_default();
            let tickers = tickers
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(",");
            let label = if tickers.is_empty() {
                Value::Null
            } else {
                json!(tickers.replace(',', ", "))
            };
            let key = if tickers.is_empty() {
                format!("id:{id}")
            } else {
                format!("coin:{tickers}")
            };
            (false, label, key, json!([]))
        } else {
            let logo = pos == 0
                && record["logo"] != false
                && info.is_none_or(|v| v.as_object().is_some_and(|v| v.is_empty()));
            let content = info
                .and_then(|v| v["content_key"].as_str())
                .filter(|s| !s.is_empty());
            let key = if logo {
                format!("logo:{name}")
            } else if let Some(key) = content {
                format!("ck:{key}")
            } else {
                format!("id:{id}")
            };
            let label = if logo {
                json!("brand logo")
            } else {
                info.map(|v| v["name"].clone()).unwrap_or(Value::Null)
            };
            let sources = info
                .and_then(|v| v["source_emoji_ids"].as_array())
                .cloned()
                .unwrap_or_default();
            (logo, label, key, json!(sources))
        };
        entries.push(row(&json!({"pos":pos,"st":sticker,"is_logo":logo,"label":label,"sources":sources,"key":key,"prior":prior.get(&key)}))?);
    }
    // Field insertion order is also the original self-contained HTML JSON contract.
    Ok(
        json!({"set_name":name,"title":record["title"],"family":family,
        "pack_index":record["index"],"link":format!("https://t.me/addemoji/{name}"),
        "count":entries.len(),"captured_utc":captured,"emoji":entries}),
    )
}

pub fn identity(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        _ => String::new(),
    }
}

pub fn row(request: &Value) -> Result<Value, String> {
    let pos = request["pos"].as_u64().ok_or("invalid roster position")?;
    let sticker = request
        .get("st")
        .and_then(Value::as_object)
        .ok_or("invalid live sticker")?;
    let current = identity(sticker.get("custom_emoji_id").unwrap_or(&Value::Null));
    let mut history = request
        .pointer("/prior/history")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let old = identity(request.pointer("/prior/id").unwrap_or(&Value::Null));
    if !old.is_empty() && old != current && !history.iter().any(|v| v == &old) {
        history.push(json!(old));
    }
    let format = if sticker.get("is_animated") == Some(&Value::Bool(true)) {
        "animated"
    } else if sticker.get("is_video") == Some(&Value::Bool(true)) {
        "video"
    } else {
        "static"
    };
    Ok(
        json!({"index":pos,"slot":pos+1,"name":request["label"],"history_key":request["key"],
        "role":if request["is_logo"]==true {"brand-logo"} else {"emoji"},
        "glyph":sticker.get("emoji").cloned().unwrap_or(Value::Null),
        "custom_emoji_id":current,"format":format,"source_emoji_ids":request["sources"],
        "previous_custom_emoji_ids":history}),
    )
}
