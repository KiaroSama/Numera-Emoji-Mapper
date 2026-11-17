//! Human-readable roster reports share the same rows and zero-based semantics as JSON.
use serde_json::{Value, json};
fn joined(value: &Value, separator: &str) -> String {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(separator)
}
pub fn pack(doc: &Value) -> Result<String, String> {
    let name = doc["set_name"].as_str().ok_or("roster has no set name")?;
    let title = doc["title"]
        .as_str()
        .filter(|v| !v.is_empty())
        .unwrap_or(name);
    let family = doc["family"].as_str().ok_or("roster has no family")?;
    let index = doc
        .get("pack_index")
        .filter(|v| !v.is_null())
        .map(ToString::to_string)
        .unwrap_or("None".into());
    let captured = doc["captured_utc"]
        .as_str()
        .ok_or("roster has no capture time")?;
    let link = doc["link"].as_str().ok_or("roster has no link")?;
    let note = if family == "general" {
        "`#` counts from 0 and emoji 0 is the brand logo; `slot` is the position Telegram shows."
    } else {
        "`#` counts from 0; `slot` is the position Telegram shows. This family carries no brand logo, so emoji 0 is a real coin."
    };
    let rows = doc["emoji"].as_array().ok_or("roster has no entries")?.iter().map(|entry| {
        let prior = joined(&entry["previous_custom_emoji_ids"], " -> ");
        let source = joined(&entry["source_emoji_ids"], ", ");
        json!({"index":entry["index"],"slot":entry["slot"],"cid":format!("`{}`",entry["custom_emoji_id"].as_str().unwrap_or("")),
            "mine":if prior.is_empty(){String::new()}else{format!("`{prior}`")},"glyph":entry["glyph"],"format":entry["format"],"name":entry["name"],
            "was":if source.is_empty(){String::new()}else{format!("`{source}`")}})
    }).collect::<Vec<_>>();
    let table = crate::pack_rows::aligned(
        &rows,
        &[
            ("index", "#", true),
            ("slot", "slot", true),
            ("cid", "custom_emoji_id", false),
            ("mine", "ours before", false),
            ("glyph", "glyph", false),
            ("format", "format", false),
            ("name", "name", false),
            ("was", "was", false),
        ],
    );
    Ok(format!(
        "# {title}\n\n- **Set** `{name}`\n- **Link** {link}\n- **Family** {family}  |  **Pack** {index}  |  **Emoji** {}\n- **Captured** {captured} (live from Telegram)\n\n{note} *Was* is the id this emoji had in the pack it was taken from, empty when we made it ourselves. *Ours before* lists the ids it held in THIS estate earlier, oldest first: a replace mints a new id, and the dead one is what stale inventories still point at.\n\n{table}\n",
        doc["count"]
    ))
}

pub fn index(doc: &Value) -> Result<String, String> {
    let captured = doc["captured_utc"]
        .as_str()
        .ok_or("index has no capture time")?;
    let mut text = format!(
        "# Published emoji packs\n\n{} packs, {} emoji. Captured {captured} live from Telegram.\n\nEach pack has three files named after the set: `.md` to read, `.json` to parse, and `.html` to LOOK at -- one self-contained page with every thumbnail inline (animation included) and the same roster repeated in a `<script type=\"application/json\">` block. `index.json` also carries three flat lookups: `by_current_id` (id -> where it lives now), `by_source_id` (the id an emoji had in its original pack -> ours) and `by_previous_id` (an id of OURS that a replace retired -> the id that took its place), so a stale reference resolves either way.\n\n| Pack | Set | Emoji | Link |\n|------|-----|------:|------|\n",
        doc["pack_count"], doc["emoji_count"]
    );
    for pack in doc["packs"].as_array().ok_or("index has no packs")? {
        text += &format!(
            "| {} {} | `{}` | {} | {} |\n",
            pack["family"].as_str().ok_or("pack has no family")?,
            pack["pack_index"],
            pack["set_name"].as_str().ok_or("pack has no name")?,
            pack["count"],
            pack["link"].as_str().ok_or("pack has no link")?
        );
    }
    Ok(text)
}
