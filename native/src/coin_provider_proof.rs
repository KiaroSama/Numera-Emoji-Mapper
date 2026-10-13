//! Provider upload evidence uses its recorded dHash; unreadable/ambiguous never means missing.
use crate::telegram::SetState;
use serde_json::{Value, json};

pub fn landed<F>(
    state: SetState,
    stickers: &[Value],
    before: usize,
    want: u64,
    mut hash: F,
) -> Result<Option<String>, String>
where
    F: FnMut(&Value) -> Result<u64, String>,
{
    match state {
        SetState::Unknown => return Err("provider live state is unknown; intent retained".into()),
        SetState::Missing => return Ok(None),
        SetState::Exists => {}
    }
    let mut hits = Vec::new();
    for sticker in stickers.iter().skip(before) {
        let got = hash(sticker)
            .map_err(|_| "provider live sticker could not be read; intent retained")?;
        if (want ^ got).count_ones() <= 8 {
            hits.push(
                sticker
                    .get("custom_emoji_id")
                    .and_then(Value::as_str)
                    .filter(|id| !id.is_empty())
                    .ok_or("matched provider sticker has no custom emoji ID")?
                    .to_owned(),
            );
        }
    }
    if hits.len() > 1 {
        return Err("several live stickers carry provider source; intent retained".into());
    }
    Ok(hits.pop())
}

pub trait RecoveryIo {
    fn fallback_hash(&mut self, intent: &Value) -> Result<u64, String>;
    fn landed(&mut self, intent: &Value, want: u64) -> Result<Option<String>, String>;
    fn save(&mut self, kind: &str, value: &Value) -> Result<(), String>;
    fn promote(&mut self, intent: &Value, want: u64) -> Result<bool, String>;
}

pub fn recover<I: RecoveryIo>(
    state: &mut Value,
    mapping: &mut serde_json::Map<String, Value>,
    io: &mut I,
) -> Result<Option<String>, String> {
    let Some(intent) = state
        .get("provider_in_flight")
        .filter(|value| !value.is_null())
        .cloned()
    else {
        return Ok(None);
    };
    let ticker = intent
        .get("key")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("provider intent has no ticker")?;
    let name = intent
        .get("set_name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("provider intent has no set name")?;
    let want = match intent.get("source_dhash").filter(|value| !value.is_null()) {
        Some(value) => value
            .as_u64()
            .ok_or("provider recorded dHash must be an unsigned integer")?,
        None => io.fallback_hash(&intent)?,
    };
    let cid = io.landed(&intent, want)?;
    if let Some(cid) = &cid {
        let sets = state
            .get_mut("sets")
            .and_then(Value::as_array_mut)
            .ok_or("provider state has no sets list")?;
        if intent["operation"] == "create" && !sets.iter().any(|set| set["name"] == name) {
            sets.push(json!({"index":intent["set_index"],"name":name,"title":intent.get("title").cloned().unwrap_or(json!("")),"live":1}));
        }
        record(state, ticker)?;
        mapping.insert(ticker.into(), json!(cid));
        io.save("map", &Value::Object(mapping.clone()))?;
        // Missing/mismatched fallback art must never replace the original upload's oracle.
        io.promote(&intent, want)?;
    }
    state["provider_in_flight"] = Value::Null;
    io.save("state", state)?;
    Ok(cid.map(|_| ticker.to_owned()))
}

pub fn record(state: &mut Value, ticker: &str) -> Result<(), String> {
    let state = state
        .as_object_mut()
        .ok_or("provider publication state must be an object")?;
    let added = state
        .entry("provider_added")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or("provider_added must be a list")?;
    if !added.iter().any(|value| value.as_str() == Some(ticker)) {
        added.push(json!(ticker));
    }
    Ok(())
}
