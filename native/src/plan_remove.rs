//! Journal deletion intent before wire mutation; restart settles by ID without resending.
use crate::{
    atomic,
    catalog::Catalog,
    telegram::{ApiError, SetState, Telegram},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
fn live(tg: &Telegram, name: &str) -> Result<Vec<Value>, String> {
    match tg.probe(name) {
        (SetState::Exists, Some(pack)) => pack["stickers"]
            .as_array()
            .cloned()
            .ok_or("live set has no stickers".into()),
        (SetState::Missing, _) => Err("recorded set no longer exists on Telegram".into()),
        _ => Err("live state unknown; removal intent retained".into()),
    }
}
fn finish(
    cat: &mut Catalog,
    state: &mut Value,
    journal: &mut Value,
    data: &Path,
    base: &str,
    intent: &Value,
) -> Result<(), String> {
    let name = intent["set"].as_str().ok_or("intent has no set")?;
    let key = intent["key"].as_str().ok_or("intent has no key")?;
    let cid = intent["cid"]
        .as_str()
        .ok_or("intent has no custom emoji ID")?;
    let record = state["sets"]
        .as_array_mut()
        .ok_or("invalid state")?
        .iter_mut()
        .find(|r| r["name"] == name)
        .ok_or("journal names another family")?;
    let keys = record["keys"]
        .as_array_mut()
        .ok_or("invalid recorded keys")?;
    if let Some(index) = keys.iter().position(|v| v == key) {
        keys.remove(index);
        record["live"] = json!(record["live"].as_u64().unwrap_or(0).saturating_sub(1));
    }
    atomic::write_json(&data.join(format!("publish_{base}.json")), state)
        .map_err(|e| e.to_string())?;
    cat.dispatch(json!({"operation":"unpublish","base":base,"content_key":key}))?;
    let retired = journal["retired"]
        .as_array_mut()
        .ok_or("invalid retired journal")?;
    if !retired.iter().any(|v| v["cid"] == cid) {
        retired.push(json!({"key":key,"cid":cid,"from_set":name,"at":crate::logging::iso_utc()}));
    }
    journal["intent"] = Value::Null;
    atomic::write_json(&data.join(format!("plan_apply_{base}.json")), journal)
        .map_err(|e| e.to_string())
}
pub fn settle(
    tg: &Telegram,
    cat: &mut Catalog,
    state: &mut Value,
    journal: &mut Value,
    data: &Path,
    base: &str,
) -> Result<(), String> {
    let Some(intent) = journal.get("intent").filter(|v| !v.is_null()).cloned() else {
        return Ok(());
    };
    let name = intent["set"].as_str().ok_or("invalid journal set")?;
    if !state["sets"]
        .as_array()
        .ok_or("invalid state")?
        .iter()
        .any(|r| r["name"] == name)
    {
        return Err("journal names another family".into());
    }
    if live(tg, name)?
        .iter()
        .any(|s| s["custom_emoji_id"] == intent["cid"])
    {
        journal["intent"] = Value::Null;
        return atomic::write_json(&data.join(format!("plan_apply_{base}.json")), journal)
            .map_err(|e| e.to_string());
    }
    finish(cat, state, journal, data, base, &intent)
}
pub fn remove(
    tg: &Telegram,
    cat: &mut Catalog,
    state: &mut Value,
    journal: &mut Value,
    data: &Path,
    base: &str,
    target: (&str, u64),
) -> Result<(), String> {
    let (key, index) = target;
    let record = state["sets"]
        .as_array()
        .ok_or("invalid state")?
        .iter()
        .find(|r| r["index"].as_u64() == Some(index))
        .ok_or("requested pack is not recorded")?;
    let name = record["name"].as_str().ok_or("set has no name")?.to_owned();
    let cid = cat.dispatch(json!({"operation":"custom_id","base":base,"content_key":key}))?;
    let cid = cid
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("live custom emoji ID is not recorded")?
        .to_owned();
    let stickers = live(tg, &name)?;
    let at = stickers
        .iter()
        .position(|s| s["custom_emoji_id"] == cid)
        .ok_or("recorded custom emoji ID is not live")?;
    let keys = record["keys"].as_array().ok_or("invalid set keys")?;
    let recorded = keys
        .iter()
        .position(|k| k == key)
        .ok_or("set does not record this key")?
        + usize::from(record["logo"].as_bool().unwrap_or(false));
    if at != recorded {
        return Err("live identity and recorded position disagree; refusing deletion".into());
    }
    let file = stickers[at]["file_id"]
        .as_str()
        .ok_or("live sticker has no file ID")?;
    let intent =
        json!({"op":"remove","key":key,"set":name,"cid":cid,"at":crate::logging::iso_utc()});
    journal["intent"] = intent.clone();
    let path = data.join(format!("plan_apply_{base}.json"));
    atomic::write_json(&path, journal).map_err(|e| e.to_string())?;
    let gone = || -> crate::telegram::Result<Option<bool>> {
        match tg.probe(&name) {
            (SetState::Exists, Some(pack)) => Ok(pack["stickers"]
                .as_array()
                .map(|s| s.iter().all(|s| s["custom_emoji_id"] != cid))),
            _ => Ok(None),
        }
    };
    match tg.call(
        "deleteStickerFromSet",
        &BTreeMap::from([("sticker".into(), file.into())]),
        &[],
        5,
        Some(gone),
    ) {
        Ok(_) => {}
        Err(error @ ApiError::Rejected(_)) | Err(error @ ApiError::FloodWait { .. }) => {
            journal["intent"] = Value::Null;
            atomic::write_json(&path, journal).map_err(|e| e.to_string())?;
            return Err(error.command_error());
        }
        Err(error) => return Err(error.command_error()),
    }
    if gone().map_err(|e| e.to_string())? != Some(true) {
        return Err("removal outcome unconfirmed; journal intent retained".into());
    }
    finish(cat, state, journal, data, base, &intent)
}
