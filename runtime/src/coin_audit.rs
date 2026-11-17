//! Audit reports derive from current live IDs; failed analyses remain retryable.
use serde_json::{Map, Value, json};
use std::time::Duration;

pub trait AuditIo {
    fn list(&mut self, name: &str) -> Result<Vec<Value>, String>;
    fn analyze(&mut self, sticker: &Value) -> Result<(String, bool), String>;
    fn save(&mut self, cache: &Map<String, Value>) -> Result<(), String>;
    fn report(&mut self, text: &str) -> Result<(), String>;
    fn wait(&mut self, duration: Duration);
}
fn successful(value: &Value) -> bool {
    value.is_object()
        && value
            .get("error")
            .is_none_or(|error| error.is_null() || error.as_str() == Some(""))
}

pub fn report(
    cache: &Map<String, Value>,
    live: &Map<String, Value>,
) -> Result<(String, u8), String> {
    let ok = cache
        .iter()
        .filter(|(cid, value)| live.contains_key(*cid) && successful(value))
        .collect::<Vec<_>>();
    let errors = live
        .keys()
        .filter(|cid| !cache.get(*cid).is_some_and(successful))
        .collect::<Vec<_>>();
    let blanks = ok
        .iter()
        .filter(|(_, value)| value["blank"] == true)
        .collect::<Vec<_>>();
    let mut groups: Map<String, Value> = Map::new();
    for (cid, value) in &ok {
        if let Some(hash) = value["hash"].as_str().filter(|hash| !hash.is_empty()) {
            groups
                .entry(hash)
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or("invalid audit group")?
                .push(json!(cid));
        }
    }
    let groups = groups
        .iter()
        .filter(|(_, values)| values.as_array().is_some_and(|values| values.len() > 1))
        .collect::<Vec<_>>();
    let mut lines = vec![
        format!("live stickers: {}", live.len()),
        format!("analysed: {}", ok.len()),
        format!("FAILED (retried on next run): {}", errors.len()),
    ];
    for cid in &errors {
        let error = cache
            .get(*cid)
            .and_then(|row| row.get("error"))
            .and_then(Value::as_str)
            .unwrap_or("not analysed");
        lines.push(format!(
            "  failed cid={cid} set={} pos={} ({error})",
            live[*cid]["set"], live[*cid]["pos"]
        ));
    }
    lines.push(format!("BLANK stickers: {}", blanks.len()));
    for (cid, _) in blanks {
        lines.push(format!(
            "  blank cid={cid} set={} pos={}",
            live[*cid]["set"], live[*cid]["pos"]
        ));
    }
    let extra = groups
        .iter()
        .map(|(_, values)| values.as_array().map_or(0, |values| values.len() - 1))
        .sum::<usize>();
    lines.push(format!(
        "DUPLICATE image groups: {} (extra duplicate stickers: {extra})",
        groups.len()
    ));
    for (hash, values) in groups {
        let positions = values
            .as_array()
            .ok_or("invalid audit group")?
            .iter()
            .map(|cid| {
                let cid = cid.as_str().ok_or("invalid audit ID")?;
                Ok(format!(
                    "set{}/pos{}/cid{cid}",
                    live[cid]["set"], live[cid]["pos"]
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        lines.push(format!("  hash {hash}: {}", positions.join(", ")));
    }
    Ok((
        lines.join("\n"),
        if errors.is_empty() {
            0
        } else if ok.is_empty() {
            4
        } else {
            3
        },
    ))
}

pub fn run<I: AuditIo>(
    sets: &[Value],
    cache: &mut Map<String, Value>,
    io: &mut I,
) -> Result<u8, String> {
    let mut sets = sets
        .iter()
        .map(|set| {
            let index = set["index"]
                .as_i64()
                .ok_or("audit set index must be an integer")?;
            let name = set["name"]
                .as_str()
                .filter(|name| {
                    !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                })
                .ok_or("unsafe audit pack name")?;
            Ok((index, name))
        })
        .collect::<Result<Vec<_>, String>>()?;
    sets.sort_by_key(|(index, _)| *index);
    if cache.values().any(|value| !value.is_object()) {
        return Err("audit cache entries must be objects".into());
    }
    let mut live = Map::new();
    let mut processed = 0_usize;
    for (index, name) in sets {
        let mut stickers = None;
        for attempt in 1..=4 {
            match io.list(name) {
                Ok(value) => {
                    stickers = Some(value);
                    break;
                }
                Err(_) => io.wait(Duration::from_secs(2 * attempt)),
            }
        }
        let Some(stickers) = stickers else {
            io.save(cache)?;
            return Ok(4);
        };
        for (position, sticker) in stickers.iter().enumerate() {
            let cid = sticker["custom_emoji_id"]
                .as_str()
                .ok_or("audit live sticker ID must be text")?;
            live.insert(cid.into(), json!({"set":index,"pos":position}));
            if cache.get(cid).is_some_and(|value| {
                successful(value) && value.as_object().is_some_and(|value| !value.is_empty())
            }) {
                continue;
            }
            let result = io.analyze(sticker);
            cache.insert(
                cid.into(),
                match result {
                    Ok((hash, blank)) => json!({"hash":hash,"blank":blank}),
                    Err(error) => {
                        json!({"error":if error.is_empty() {"download failed".into()} else {error}})
                    }
                },
            );
            processed += 1;
            if processed.is_multiple_of(100) {
                io.save(cache)?;
            }
            io.wait(Duration::from_millis(30));
        }
        io.save(cache)?;
    }
    cache.retain(|cid, _| live.contains_key(cid));
    io.save(cache)?;
    let (text, code) = report(cache, &live)?;
    io.report(&text)?;
    Ok(code)
}
