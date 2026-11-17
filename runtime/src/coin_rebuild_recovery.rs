//! Rebuild recovery owns cursor/order state, distinct from provider top-ups and ticker done keys.
use crate::telegram::SetState;
use serde_json::{Value, json};

pub trait RecoveryIo {
    fn source_exists(&mut self, key: &str) -> bool;
    fn probe(&mut self, name: &str) -> Result<(SetState, Vec<Value>), String>;
    fn compare(&mut self, sticker: &Value, key: &str) -> Result<Option<bool>, String>;
    fn source_hash(&mut self, key: &str) -> Result<u64, String>;
    fn live_hash(&mut self, sticker: &Value) -> Result<u64, String>;
    fn save(&mut self, state: &Value) -> Result<(), String>;
    fn report(&mut self, text: &str);
}
fn add_landed<I: RecoveryIo>(marker: &Value, io: &mut I) -> Result<bool, String> {
    let key = marker["key"].as_str().ok_or("rebuild intent has no key")?;
    let name = marker["set_name"]
        .as_str()
        .ok_or("rebuild intent has no set name")?;
    if name.is_empty() || !io.source_exists(key) {
        return Err(format!(
            "cannot check the in-flight {key}: its set name or source image is gone"
        ));
    }
    let (status, stickers) = io.probe(name)?;
    match status {
        SetState::Unknown => {
            return Err(format!(
                "live state of {name} is unknown; cannot tell whether the in-flight {key} landed"
            ));
        }
        SetState::Missing => return Ok(false),
        SetState::Exists => {}
    }
    let before = marker
        .get("expected_before")
        .filter(|v| !v.is_null())
        .map_or(Ok(0), |v| {
            v.as_u64()
                .ok_or("rebuild intent has invalid expected count")
        })?;
    let want = io.source_hash(key)?;
    let mut hits = 0;
    for sticker in stickers
        .iter()
        .skip(usize::try_from(before).map_err(|_| "rebuild expected count is too large")?)
    {
        let got = io.live_hash(sticker).map_err(|error| {
            format!(
                "sticker {} in {name} could not be read ({error})",
                sticker["custom_emoji_id"].as_str().unwrap_or("None")
            )
        })?;
        if (want ^ got).count_ones() <= 8 {
            hits += 1;
        }
    }
    if hits > 1 {
        return Err(format!(
            "{hits} live stickers carry {key}.png; the pack already contains a duplicate"
        ));
    }
    Ok(hits == 1)
}
pub fn recover<I: RecoveryIo>(
    state: &mut Value,
    mut total: u64,
    title: &str,
    io: &mut I,
) -> Result<u64, String> {
    let marker = state["in_flight"].clone();
    if marker.is_null() {
        return Ok(total);
    }
    let key = marker["key"].as_str().ok_or("rebuild intent has no key")?;
    let name = marker["set_name"]
        .as_str()
        .ok_or("rebuild intent has no set name")?;
    let operation = marker["operation"]
        .as_str()
        .ok_or("rebuild intent has no operation")?;
    let sets = state["sets"]
        .as_array()
        .ok_or("rebuild state has no sets")?;
    let known = sets.iter().any(|s| s["name"].as_str() == Some(name));
    let mut adoption = None;
    let landed = if operation == "create" {
        let (status, stickers) = io.probe(name)?;
        if status == SetState::Unknown {
            return Err(format!(
                "live state of {name} is unknown; cannot tell whether the in-flight create landed"
            ));
        }
        if status == SetState::Exists {
            if stickers.len() != 1 {
                return Err(format!(
                    "{name} exists but holds {} sticker(s); the create that made it leaves exactly one, so this is not the set this rebuild created. Inspect that pack: if its first sticker IS {key}.png, another tool added to a set this rebuild had just made -- record it by hand in 'sets' (index {}, name {name}), set 'in_flight' to null and append {} to 'order'. If it is NOT ours, the name collides with another pack: rename this family's base, or delete that set, then re-run",
                    stickers.len(),
                    marker["set_index"],
                    crate::python_repr::quoted(key)
                ));
            }
            let same = if io.source_exists(key) {
                io.compare(&stickers[0], key)?
            } else {
                None
            };
            match same {
                Some(true) => {}
                Some(false) => {
                    return Err(format!(
                        "{name} exists but its first sticker is not {key}.png; refusing to adopt a set this rebuild did not create"
                    ));
                }
                None => {
                    return Err(format!(
                        "{name} exists but its first sticker could not be compared with {key}.png; refusing to adopt a set on an unverified identity"
                    ));
                }
            }
            if !known {
                let index = marker["set_index"]
                    .as_u64()
                    .filter(|i| *i != 0)
                    .unwrap_or(sets.len() as u64 + 1);
                adoption = Some(
                    json!({"index":index,"name":name,"title":format!("{title} {index}"),"live":stickers.len()}),
                );
                total = total
                    .checked_add(stickers.len() as u64)
                    .ok_or("rebuild live total overflow")?;
            }
            true
        } else {
            false
        }
    } else if operation == "add" {
        add_landed(&marker, io)?
    } else {
        return Err("rebuild intent operation is unknown".into());
    };
    // Validate writable fields before adoption; malformed state cannot half-apply a proof.
    state["order"]
        .as_array()
        .ok_or("rebuild state has no order")?;
    let cursor = state["cursor"]
        .as_u64()
        .ok_or("rebuild cursor is invalid")?;
    let before = state.clone();
    if let Some(set) = adoption {
        state["sets"]
            .as_array_mut()
            .ok_or("rebuild state has no sets")?
            .push(set);
        io.report(&format!(
            "  resume: adopted {name}, created before the interruption"
        ));
    }
    if landed {
        state["order"]
            .as_array_mut()
            .ok_or("rebuild state has no order")?
            .push(json!(key));
        io.report(&format!("  resume: {key} did land before the interruption"));
    } else {
        io.report(&format!("  resume: {key} did not land; retrying"));
        state["cursor"] = json!(cursor.saturating_sub(1));
    }
    state["in_flight"] = Value::Null;
    if let Err(error) = io.save(state) {
        *state = before;
        return Err(error);
    }
    Ok(total)
}
