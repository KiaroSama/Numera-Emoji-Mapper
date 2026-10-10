//! Pure bot extraction/rendering; IDs validated before entering HTML or copy buttons.
use serde_json::{Value, json};
use std::collections::BTreeSet;
fn digits(value: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&value.chars().count()) && value.chars().all(|c| c.is_ascii_digit())
}
pub fn typed(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for part in text
        .trim()
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .filter(|s| !s.is_empty())
    {
        if !digits(part, 15, 25) {
            return vec![];
        }
        if seen.insert(part) {
            out.push(part.to_owned());
        }
    }
    out
}
pub fn extract(message: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for entities in [
        message.get("entities"),
        message.get("caption_entities"),
        message.pointer("/quote/entities"),
        message.pointer("/external_reply/quote/entities"),
    ] {
        for entity in entities.and_then(Value::as_array).into_iter().flatten() {
            if entity["type"] != "custom_emoji" {
                continue;
            }
            let value = match &entity["custom_emoji_id"] {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => continue,
            };
            if digits(&value, 1, 25) && seen.insert(value.clone()) {
                out.push(value);
            }
        }
    }
    out
}
pub fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}
fn copy_text(ids: &[String]) -> String {
    format!("{}\n", ids.join("\n"))
}
fn keyboard(ids: &[String]) -> Value {
    let mut chunks = Vec::new();
    let mut current = Vec::new();
    let mut length = 0;
    for id in ids {
        let cost = id.chars().count() + 1;
        if !current.is_empty() && length + cost > 256 {
            chunks.push(current);
            current = Vec::new();
            length = 0;
        }
        current.push(id.clone());
        length += cost;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    if chunks.len() <= 1 {
        return json!({"inline_keyboard":[[{"text":format!("📋 Copy all {} IDs",ids.len()),"copy_text":{"text":copy_text(ids)}}]]});
    }
    let mut low = 1;
    let mut rows = Vec::new();
    for chunk in chunks {
        let high = low + chunk.len() - 1;
        rows.push(json!([{ "text":format!("📋 Copy {low}-{high}"),"copy_text":{"text":copy_text(&chunk)}}]));
        low = high + 1;
    }
    json!({"inline_keyboard":rows})
}
pub fn payloads(ids: &[String], labels: &Value, rich: bool) -> Result<Value, String> {
    if ids.iter().any(|id| !digits(id, 1, 25)) {
        return Err("invalid custom emoji ID".into());
    }
    if ids.is_empty() {
        return Ok(
            json!([["No premium (custom) emoji found in that message. Send me one or more premium emoji in a row (spaces/newlines don't matter), or a post that contains premium emoji.",{"inline_keyboard":[]}]]),
        );
    }
    let batches = ids.chunks((3500 - 260) / 110).collect::<Vec<_>>();
    let mut out = Vec::new();
    for (i, batch) in batches.iter().enumerate() {
        let mut head = format!("Found <b>{}</b> premium emoji", ids.len());
        if batches.len() > 1 {
            head += &format!(" — part {}/{}", i + 1, batches.len());
        }
        let rows = batch
            .iter()
            .map(|id| {
                let fallback = escape(
                    labels[id]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("⭐"),
                );
                let emoji = if rich {
                    format!(
                        "<tg-emoji emoji-id=\"{}\">{fallback}</tg-emoji>",
                        escape(id)
                    )
                } else {
                    fallback
                };
                format!("{emoji} <code>{}</code>", escape(id))
            })
            .collect::<Vec<_>>()
            .join("\n");
        let text = format!(
            "{head} — tap to expand; tap an ID to copy it, or use the “Copy” button(s) below:\n<blockquote expandable>{rows}</blockquote>"
        );
        out.push(json!([text, keyboard(batch)]));
    }
    Ok(json!(out))
}
pub fn execute(request: &Value) -> Result<Value, String> {
    match request["operation"]
        .as_str()
        .ok_or("missing bot operation")?
    {
        "typed" => Ok(json!(typed(request["text"].as_str().unwrap_or("")))),
        "extract" => Ok(json!(extract(&request["message"]))),
        "payloads" => {
            let ids = request["ids"]
                .as_array()
                .ok_or("missing IDs")?
                .iter()
                .map(|v| v.as_str().map(str::to_owned).ok_or("invalid ID".into()))
                .collect::<Result<Vec<_>, String>>()?;
            payloads(
                &ids,
                &request["labels"],
                request["rich"].as_bool().unwrap_or(true),
            )
        }
        _ => Err("unknown bot operation".into()),
    }
}
