//! A saved ticker intent is reconciled by comparing the actual source image, never counts alone.
use crate::{atomic, telegram::SetState, upload_effect::UploadEffect};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
pub fn recover(
    publisher: &UploadEffect<'_>,
    state: &mut Value,
    sources: &[PathBuf],
    path: &Path,
    title: &str,
    base: &str,
    logo: Option<&Value>,
) -> Result<(), String> {
    let Some(mut intent) = state.get("in_flight").filter(|v| !v.is_null()).cloned() else {
        return Ok(());
    };
    if let Some(key) = intent.as_str() {
        let set = state["sets"]
            .as_array()
            .and_then(|s| s.last())
            .ok_or("legacy unresolved intent has no recorded target")?;
        intent = json!({"key":key,"operation":"add","set_name":set["name"],"set_index":set["index"],"expected_before":set["count"]});
    }
    if !intent.is_object() {
        return Err("invalid unresolved ticker intent".into());
    }
    let key = intent["key"].as_str().ok_or("intent has no key")?;
    let name = intent["set_name"]
        .as_str()
        .ok_or("intent has no set name")?;
    let index = intent["set_index"]
        .as_u64()
        .ok_or("intent has no set index")?;
    let before = intent["expected_before"]
        .as_u64()
        .ok_or("intent has no expected count")?;
    let suffix = name.strip_prefix(&format!("{base}{index}_by_"));
    if suffix
        .is_none_or(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
    {
        return Err("intent names another family".into());
    }
    let logo_path = logo
        .and_then(|v| v["file_path"].as_str())
        .map(PathBuf::from);
    let source = if key == "__brand_logo__" {
        logo_path
            .as_ref()
            .ok_or("interrupted brand logo is unavailable")?
    } else {
        sources
            .iter()
            .find(|p| {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.to_lowercase() == key)
            })
            .ok_or("interrupted source is unavailable")?
    };
    let operation = intent["operation"]
        .as_str()
        .ok_or("intent has no operation")?;
    let recorded = state["sets"]
        .as_array()
        .ok_or("invalid sets")?
        .iter()
        .position(|s| s["name"] == name);
    let (known, pack) = publisher.tg.probe(name);
    if known == SetState::Unknown {
        return Err("live state unknown; interrupted upload remains unresolved".into());
    }
    let applied = if operation == "create" {
        if known == SetState::Missing {
            false
        } else {
            let pack = pack.ok_or("missing live evidence")?;
            let stickers = pack["stickers"].as_array().ok_or("missing live stickers")?;
            if stickers.len() != 1 {
                return Err(
                    "interrupted create cannot own a set with more than one sticker".into(),
                );
            }
            if publisher.same(&stickers[0], source, "static")? != Some(true) {
                return Err("created sticker is not proven to be the interrupted source".into());
            }
            if recorded.is_none() {
                state["sets"].as_array_mut().ok_or("invalid sets")?.push(
                    json!({"name":name,"index":index,"count":1,"title":intent["title"].as_str().unwrap_or(&format!("{title} {index}"))}),
                );
            }
            true
        }
    } else if operation == "add" {
        if known == SetState::Missing {
            return Err("interrupted add targets a missing recorded set".into());
        }
        let pack = pack.ok_or("missing live evidence")?;
        let stickers = pack["stickers"].as_array().ok_or("missing live stickers")?;
        if (stickers.len() as u64) < before {
            return Err("live set shrank during interrupted upload".into());
        }
        let mut found = Some(false);
        let split = usize::try_from(before)
            .map_err(|_| "invalid expected count")?
            .min(stickers.len());
        for sticker in stickers[split..].iter().chain(&stickers[..split]) {
            match publisher.same(sticker, source, "static")? {
                Some(true) => {
                    found = Some(true);
                    break;
                }
                None => found = None,
                Some(false) => {}
            }
        }
        match found {
            Some(true) => {
                let at = recorded.ok_or("add intent has no recorded set")?;
                state["sets"][at]["count"] = json!(stickers.len());
                true
            }
            Some(false) if stickers.len() as u64 == before => false,
            Some(false) => return Err("set grew with foreign content; refusing attribution".into()),
            None => return Err("interrupted source comparison remains unknown".into()),
        }
    } else {
        return Err("unknown interrupted operation".into());
    };
    if applied && key != "__brand_logo__" {
        let done = state["done"].as_array_mut().ok_or("invalid done keys")?;
        if !done.iter().any(|v| v == key) {
            done.push(json!(key));
        }
    }
    state["in_flight"] = Value::Null;
    atomic::write_json(path, state).map_err(|e| e.to_string())
}
pub fn verify_counts(publisher: &UploadEffect<'_>, state: &Value) -> Result<(), String> {
    for set in state["sets"].as_array().ok_or("invalid sets")? {
        let name = set["name"].as_str().ok_or("set has no name")?;
        match publisher.tg.probe(name) {
            (SetState::Exists, Some(pack)) => {
                if pack["stickers"].as_array().map(Vec::len).map(|n| n as u64)
                    != set["count"].as_u64()
                {
                    return Err(
                        "live set and recorded count disagree with no unresolved intent".into(),
                    );
                }
            }
            (SetState::Missing, _) => {
                return Err("recorded set was deleted; refusing recreation".into());
            }
            _ => return Err("recorded live state unknown".into()),
        }
    }
    Ok(())
}
