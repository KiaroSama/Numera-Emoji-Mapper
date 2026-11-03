//! Recorded replacement effect requires structural and image proof before promotion/repoint.
use crate::coin_replace_recovery::RecoveryIo;
use serde_json::Value;
pub enum Mutation {
    Answered,
    Rejected,
    Unknown,
}
pub trait PublishIo: RecoveryIo {
    fn save_intent(&mut self, intent: &Value) -> Result<(), String>;
    fn mutate(&mut self, intent: &Value) -> Mutation;
}
pub fn replace<I: PublishIo>(intent: Option<&Value>, io: &mut I) -> Result<bool, String> {
    let Some(intent) = intent else {
        return Ok(false);
    };
    let name = intent["set_name"]
        .as_str()
        .ok_or("replacement intent has no set")?;
    let old = intent["old_cid"]
        .as_str()
        .ok_or("replacement intent has no old ID")?;
    let position = intent["set_index"]
        .as_i64()
        .ok_or("replacement intent has no position")?;
    let hash = intent["source_dhash"]
        .as_u64()
        .ok_or("replacement intent has no source identity")?;
    let before = intent["before"]
        .as_array()
        .ok_or("replacement intent has no before manifest")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or("replacement manifest ID must be text")
        })
        .collect::<Result<Vec<_>, _>>()?;
    io.save_intent(intent)?;
    if matches!(io.mutate(intent), Mutation::Rejected) {
        io.clear()?;
        return Ok(false);
    }
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
        return Ok(false);
    }
    let Some(new) = crate::coin_replace_proof::new_id(&before, &after, position) else {
        return Ok(false);
    };
    let sticker = usize::try_from(position)
        .ok()
        .and_then(|position| stickers.get(position))
        .ok_or("replacement position is outside proven set")?;
    if !io.proof(sticker, hash).unwrap_or(false) {
        return Ok(false);
    }
    io.promote(intent)?;
    io.repoint(old, &new)?;
    io.clear()?;
    io.clean(intent)?;
    Ok(true)
}
