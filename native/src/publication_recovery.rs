//! Recover durable native publication intent using fresh live identities, never positional guesses.
use crate::{atomic, publication::Publisher, publish_collection::Options, telegram::SetState};
use serde_json::{Value, json};
use std::path::Path;

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("publication intent is missing {key}"))
}

pub fn recover(
    publisher: &mut Publisher<'_>,
    options: &Options,
    state: &mut Value,
    path: &Path,
) -> Result<(), String> {
    let Some(intent) = state.get("in_flight").filter(|v| !v.is_null()).cloned() else {
        return Ok(());
    };
    let name = text(&intent, "set_name")?;
    let key = text(&intent, "key")?;
    let operation = text(&intent, "operation")?;
    let index = intent["set_index"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or("publication intent has bad set index")?;
    if !["create", "add"].contains(&operation) {
        return Err("unknown publication intent operation".into());
    }
    let tag = match options.fmt.as_str() {
        "static" => "s",
        "video" => "v",
        "animated" => "a",
        "mixed" => "",
        _ => return Err("invalid family format".into()),
    };
    if name != format!("{}{tag}{index}_by_{}", options.base, options.bot) {
        return Err("publication intent names another pack family".into());
    }
    let sets = state["sets"].as_array().ok_or("invalid sets")?;
    let recorded = sets.iter().position(|set| set["name"] == name);
    if recorded
        .is_some_and(|i| sets[i]["index"].as_u64() != Some(index) || sets[i]["fmt"] != options.fmt)
    {
        return Err("publication intent disagrees with its recorded family".into());
    }
    match publisher.tg.probe(name) {
        (SetState::Unknown, _) => {
            return Err("publication recovery cannot determine live state; intent retained".into());
        }
        (SetState::Missing, _) => {
            if operation != "create" || recorded.is_some() {
                return Err("recorded publication set is missing; intent retained".into());
            }
            // A fresh proven negative permits the next run to try this name again.
            state["in_flight"] = Value::Null;
            return atomic::write_json(path, state).map_err(|e| e.to_string());
        }
        (SetState::Exists, Some(live)) => {
            if recorded.is_none() {
                if operation != "create" {
                    return Err("add intent names an unrecorded set".into());
                }
                let stickers = live["stickers"]
                    .as_array()
                    .ok_or("live recovery set has no stickers")?;
                if stickers.len() != 1 {
                    return Err("unrecorded create recovery requires exactly one live sticker; intent retained".into());
                }
                if let Some(logo) = &options.logo {
                    let first = stickers.first().ok_or("created set has no first sticker")?;
                    if publisher.same(first, Path::new(text(logo, "file_path")?), "static")?
                        != Some(true)
                    {
                        return Err("created set's first sticker cannot be proven to be the configured logo".into());
                    }
                } else {
                    let intended = publisher
                        .cat
                        .get(key)?
                        .ok_or("interrupted catalog item is unavailable")?;
                    let source = Path::new(text(&intended, "file_path")?);
                    let fmt = text(&intended, "fmt")?;
                    if publisher.same(&stickers[0], source, fmt)? != Some(true) {
                        return Err("created set's first sticker is not proven to be the interrupted item; intent retained".into());
                    }
                }
                let title = text(&intent, "title")?;
                state["sets"]
                    .as_array_mut()
                    .ok_or("invalid sets")?
                    .push(json!({
                        "name":name,"fmt":options.fmt,"index":index,"title":title,
                        "logo":options.logo.is_some(),"keys":[],"live":stickers.len()
                    }));
                atomic::write_json(path, state).map_err(|e| e.to_string())?;
            }
        }
        _ => return Err("publication recovery returned incomplete live evidence".into()),
    }
    let i = state["sets"]
        .as_array()
        .ok_or("invalid sets")?
        .iter()
        .position(|set| set["name"] == name)
        .ok_or("recovered set vanished")?;
    publisher.reconcile(&mut state["sets"][i], &options.base)?;
    // Save reconciled membership before clearing intent; a crash here is idempotent.
    atomic::write_json(path, state).map_err(|e| e.to_string())?;
    let published = publisher
        .cat
        .dispatch(json!({"operation":"published","base":options.base,"content_key":key}))?
        == true;
    if !published && !crate::reconcile::open(&state["sets"][i]) {
        return Err("recovery found unattributed live stickers; intent retained".into());
    }
    state["in_flight"] = Value::Null;
    atomic::write_json(path, state).map_err(|e| e.to_string())
}
