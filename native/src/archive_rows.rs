//! Archive names and metadata describe actual live slots, never positional identity joins.
use serde_json::{Value, json};
pub const LOGO_NAME: &str = "001_logo.png";
pub const META: [&str; 3] = ["_history.json", "_history.md", "_manifest.md"];
pub fn folder(title: &str) -> String {
    let text = title
        .chars()
        .map(|c| {
            if c <= '\u{1f}' || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect::<String>();
    let text = text.trim_end_matches(['.', ' ']);
    if text.is_empty() {
        "_".into()
    } else {
        text.into()
    }
}
pub fn name(slot: u64, fmt: &str, key: &str, suffix: &str) -> Result<String, String> {
    if !["static", "animated", "video"].contains(&fmt) || slot == 0 || suffix.contains(['/', '\\'])
    {
        return Err("invalid archive filename input".into());
    }
    let digest = key.split_once(':').ok_or("invalid archive content key")?.1;
    Ok(format!(
        "{slot:03}_{fmt}_{}{suffix}",
        digest.chars().take(12).collect::<String>()
    ))
}
pub fn rows(
    record: &Value,
    live: &[Value],
    items: &Value,
    keys: &Value,
) -> Result<Vec<Value>, String> {
    let mut out = Vec::with_capacity(live.len());
    for (position, sticker) in live.iter().enumerate() {
        let slot = position as u64 + 1;
        let id = sticker["custom_emoji_id"].as_str().unwrap_or("");
        let key = keys.get(id).and_then(Value::as_str);
        let row = if let Some(key) = key {
            let item = items
                .get(key)
                .ok_or("archive identity has no catalog row")?;
            let path =
                std::path::Path::new(item["path"].as_str().ok_or("archive item has no path")?);
            let suffix = path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| format!(".{s}"))
                .unwrap_or_default();
            json!({"position":slot,"file":name(slot,item["fmt"].as_str().ok_or("archive item has no format")?,key,&suffix)?,"format":item["fmt"],"premium_id":id,"content_key":key,"emoji":sticker["emoji"]})
        } else if slot == 1 {
            json!({"position":slot,"file":LOGO_NAME,"format":"static","premium_id":null,"content_key":null,"note":"brand logo, copied from the repo"})
        } else {
            let _ = record;
            json!({"position":slot,"file":null,"format":null,"premium_id":if id.is_empty(){Value::Null}else{json!(id)},"content_key":null,"note":"not in the catalog (unidentified)"})
        };
        out.push(row);
    }
    Ok(out)
}
pub fn history(record: &Value, rows: &[Value]) -> Result<String, String> {
    let name = record["name"]
        .as_str()
        .ok_or("archive record has no name")?;
    let mut text = format!(
        "# {name}\n\nPack: https://t.me/addemoji/{name}\nEmoji in the pack: **{}**\n\n`premium_id` is the id THIS pack's emoji has now -- the one to use in a\n`tg-emoji` tag. `content key` is the catalog's hash of the artwork, which\nsurvives a recolour and is how a file is matched back to its emoji.\n\n| # | file | format | premium_id | content key |\n|---|------|--------|------------|-------------|\n",
        rows.len()
    );
    for row in rows {
        let id = row["premium_id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| format!("`{s}`"))
            .unwrap_or("`-`".into());
        if let Some(file) = row["file"].as_str() {
            let key = row["content_key"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or("brand logo");
            text += &format!(
                "| {} | `{file}` | {} | {id} | `{key}` |\n",
                row["position"],
                row["format"].as_str().ok_or("archive row has no format")?
            );
        } else {
            text += &format!(
                "| {} | `-` | - | {id} | `unidentified` |\n",
                row["position"]
            );
        }
    }
    Ok(text)
}
pub fn manifest(record: &Value, rows: &[Value], items: &Value) -> Result<String, String> {
    let title = record["title"]
        .as_str()
        .ok_or("archive record has no title")?;
    let name = record["name"]
        .as_str()
        .ok_or("archive record has no name")?;
    let real = rows.iter().filter(|r|r["content_key"].as_str().is_some_and(|s|!s.is_empty())).enumerate().map(|(i,r)| {
        let key = r["content_key"].as_str().unwrap();
        let keywords = crate::pack_rows::keywords(&items[key]);
        json!({"n":i+1,"cid":r["premium_id"],"name":if keywords.is_empty(){key.to_owned()}else{keywords}})
    }).collect::<Vec<_>>();
    Ok(format!(
        "# {title}\n\nPack: https://t.me/addemoji/{name}  |  format: {}  |  {} emoji\n\n{}\n",
        record["fmt"].as_str().unwrap_or("mixed"),
        real.len(),
        crate::pack_rows::markdown(&real, &[("n", "#"), ("name", "Name"), ("cid", "Emoji ID")])
    ))
}
