//! Replacement recovery is target-bound and content-proven; unknown leaves durable intent intact.
use serde_json::Value;
pub trait RecoveryIo {
    fn live(&mut self, name: &str) -> Result<Vec<Value>, String>;
    fn proof(&mut self, sticker: &Value, hash: u64) -> Result<bool, String>;
    fn promote(&mut self, intent: &Value) -> Result<(), String>;
    fn repoint(&mut self, old: &str, new: &str) -> Result<(), String>;
    fn clear(&mut self) -> Result<(), String>;
    fn clean(&mut self, intent: &Value) -> Result<(), String>;
}
pub fn recover<I: RecoveryIo>(
    intent: Option<&Value>,
    map_target: &str,
    state_target: &str,
    io: &mut I,
) -> Result<bool, String> {
    let Some(intent) = intent else {
        return Ok(true);
    };
    if intent["map_target"].as_str() != Some(map_target)
        || intent["state_target"].as_str() != Some(state_target)
    {
        return Ok(false);
    }
    let Some(name) = intent["set_name"].as_str() else {
        return Ok(false);
    };
    let Some(position) = intent["set_index"].as_i64() else {
        return Ok(false);
    };
    let Some(old) = intent["old_cid"].as_str() else {
        return Ok(false);
    };
    let Some(hash) = intent["source_dhash"].as_u64() else {
        return Ok(false);
    };
    let Some(before) = intent["before"].as_array().and_then(|values| {
        values
            .iter()
            .map(|value| value.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()
    }) else {
        return Ok(false);
    };
    let stickers = match io.live(name) {
        Ok(stickers) => stickers,
        Err(_) => return Ok(false),
    };
    let Some(after) = stickers
        .iter()
        .map(|sticker| sticker["custom_emoji_id"].as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
    else {
        return Ok(false);
    };
    if after.iter().any(|cid| cid == old) {
        io.clear()?;
        io.clean(intent)?;
        return Ok(true);
    }
    let Some(new) = crate::coin_replace_proof::new_id(&before, &after, position) else {
        return Ok(false);
    };
    let Some(sticker) = usize::try_from(position)
        .ok()
        .and_then(|position| stickers.get(position))
    else {
        return Ok(false);
    };
    if !io.proof(sticker, hash).unwrap_or(false) {
        return Ok(false);
    }
    io.promote(intent)?;
    io.repoint(old, &new)?;
    io.clear()?;
    io.clean(intent)?;
    Ok(true)
}
