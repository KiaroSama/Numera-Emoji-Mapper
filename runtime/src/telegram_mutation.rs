//! Shared legacy upload proof: transport ownership is native for both bindings and commands.
use crate::telegram::{ApiError, SetState, Telegram, input_sticker, usable_fuids};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn text<'a>(request: &'a Value, key: &str) -> crate::telegram::Result<&'a str> {
    request[key]
        .as_str()
        .ok_or_else(|| ApiError::Invalid(format!("missing {key}")))
}
fn live(tg: &Telegram, name: &str) -> Option<Vec<Value>> {
    match tg.probe(name) {
        (SetState::Exists, Some(set)) => set["stickers"].as_array().cloned(),
        _ => None,
    }
}
pub fn mutate<F>(tg: &Telegram, request: &Value, compare: F) -> crate::telegram::Result<Value>
where
    F: FnMut(&Telegram, &Value, &str, &str) -> crate::telegram::Result<Option<bool>>,
{
    mutate_with_wait(tg, request, compare, std::thread::sleep)
}

/// Preserve the public mutation and wire proof while injecting only its retry clock.
pub fn mutate_with_wait<F, S>(
    tg: &Telegram,
    request: &Value,
    mut compare: F,
    sleep: S,
) -> crate::telegram::Result<Value>
where
    F: FnMut(&Telegram, &Value, &str, &str) -> crate::telegram::Result<Option<bool>>,
    S: FnMut(std::time::Duration),
{
    let name = text(request, "name")?;
    let source = text(request, "source")?;
    let fmt = text(request, "format")?;
    let creating = request["create"]
        .as_bool()
        .ok_or_else(|| ApiError::Invalid("missing create flag".into()))?;
    let user = request["user_id"]
        .as_u64()
        .ok_or_else(|| ApiError::Invalid("invalid user ID".into()))?;
    let emojis: Vec<String> = serde_json::from_value(request["emojis"].clone())
        .map_err(|e| ApiError::Invalid(e.to_string()))?;
    let keywords: Vec<String> = serde_json::from_value(request["keywords"].clone())
        .map_err(|e| ApiError::Invalid(e.to_string()))?;
    let input = input_sticker(fmt, &emojis, &keywords)?;
    let file = crate::telegram::upload(std::path::Path::new(source), "file0")?;
    let before = if !creating && !request["expected_before"].is_null() {
        live(tg, name).and_then(|stickers| usable_fuids(&stickers))
    } else {
        None
    };
    let mut data = BTreeMap::from([
        ("user_id".into(), user.to_string()),
        ("name".into(), name.into()),
    ]);
    let method = if creating {
        data.insert("title".into(), text(request, "title")?.into());
        data.insert("sticker_type".into(), "custom_emoji".into());
        data.insert("stickers".into(), json!([input]).to_string());
        if request["needs_repainting"] == true {
            data.insert("needs_repainting".into(), "true".into());
        }
        "createNewStickerSet"
    } else {
        data.insert("sticker".into(), input.to_string());
        "addStickerToSet"
    };
    let mut fired = false;
    {
        let mut check = || -> crate::telegram::Result<Option<bool>> {
            fired = true;
            let (state, set) = tg.probe(name);
            if state == SetState::Unknown {
                return Ok(None);
            }
            if state == SetState::Missing {
                return Ok(if creating { Some(false) } else { None });
            }
            let Some(stickers) = set.and_then(|set| set["stickers"].as_array().cloned()) else {
                return Ok(None);
            };
            if creating {
                return if stickers.len() == 1 {
                    compare(tg, &stickers[0], source, fmt)
                } else {
                    Ok(None)
                };
            }
            let (Some(old), Some(now)) = (&before, usable_fuids(&stickers)) else {
                return Ok(None);
            };
            let new = now.difference(old).collect::<Vec<_>>();
            if new.is_empty() {
                return Ok(Some(false));
            }
            if new.len() != 1 {
                return Ok(None);
            }
            let sticker = stickers.iter().find(|st| {
                st["file_unique_id"]
                    .as_str()
                    .is_some_and(|id| id == new[0].as_str())
            });
            match sticker {
                Some(sticker) => compare(tg, sticker, source, fmt),
                None => Ok(None),
            }
        };
        tg.call_with_wait(method, &data, &[file], 5, Some(&mut check), sleep)?;
    }
    if creating || !fired {
        return Ok(Value::Null);
    }
    let stickers = live(tg, name).ok_or_else(|| {
        ApiError::Ambiguous(
            "addStickerToSet applied after retry but live state is unknown or missing".into(),
        )
    })?;
    if let (Some(old), Some(now)) = (&before, usable_fuids(&stickers)) {
        let mut found = false;
        for sticker in &stickers {
            if sticker["file_unique_id"]
                .as_str()
                .is_some_and(|id| now.contains(id) && !old.contains(id))
                && compare(tg, sticker, source, fmt)? == Some(true)
            {
                found = true;
                break;
            }
        }
        if !found {
            return Err(ApiError::Ambiguous(
                "addStickerToSet readback does not prove our source; count cannot be trusted"
                    .into(),
            ));
        }
    }
    Ok(json!(stickers.len()))
}
