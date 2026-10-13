//! Coin reports retain live ordering and use the existing shared Markdown renderer.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn id(value: &Value) -> Result<String, String> {
    match value {
        Value::Null => Ok("None".into()),
        Value::Bool(true) => Ok("True".into()),
        Value::Bool(false) => Ok("False".into()),
        Value::String(text) => Ok(text.clone()),
        Value::Number(number) if !number.to_string().contains(['.', 'e', 'E']) => {
            Ok(number.to_string())
        }
        _ => Err("coin manifest primitive ID compatibility is unverified".into()),
    }
}
fn name(set: &Value) -> Result<&str, String> {
    set["name"]
        .as_str()
        .filter(|name| {
            !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        })
        .ok_or_else(|| "coin manifest has an unsafe pack name".into())
}
pub fn render(
    set: &Value,
    stickers: &[Value],
    aliases: &BTreeMap<String, Vec<String>>,
) -> Result<String, String> {
    let name = name(set)?;
    let title = match set.get("title") {
        None => name,
        Some(Value::String(text)) => text,
        _ => return Err("coin manifest title must be text".into()),
    };
    let rows = stickers
        .iter()
        .enumerate()
        .map(|(index, sticker)| {
            if !sticker.is_object() {
                return Err("coin manifest sticker must be an object".into());
            }
            let cid = id(&sticker["custom_emoji_id"])?;
            let mut tickers = aliases.get(&cid).cloned().unwrap_or_default();
            tickers.sort();
            let joined = tickers.join(", ");
            let label = if joined.is_empty() {
                "(unmapped)".into()
            } else {
                joined
            };
            Ok(json!({"n":index+1,"cid":cid,"tickers":label}))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(format!(
        "# {title}\n\nPack: https://t.me/addemoji/{name}  |  {} stickers\n\n{}\n",
        stickers.len(),
        crate::pack_rows::markdown(
            &rows,
            &[("n", "#"), ("tickers", "Ticker(s)"), ("cid", "Emoji ID")]
        )
    ))
}

pub fn write<F>(
    directory: &Path,
    state: &Value,
    mapping: &Value,
    mut live: F,
) -> Result<(usize, usize), String>
where
    F: FnMut(&str) -> Result<Value, String>,
{
    crate::coin_provider_stage::physical(directory)?;
    let mut sets = state["sets"]
        .as_array()
        .ok_or("coin manifest state has no sets")?
        .iter()
        .map(|set| {
            Ok((
                set["index"]
                    .as_i64()
                    .ok_or("coin manifest set index must be an integer")?,
                set,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    sets.sort_by_key(|(index, _)| *index);
    let mut seen = BTreeSet::new();
    for (_, set) in &sets {
        if !seen.insert(name(set)?) {
            return Err("coin manifest state repeats a pack name".into());
        }
    }
    let mut aliases: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (ticker, value) in mapping
        .as_object()
        .ok_or("coin manifest map must be an object")?
    {
        aliases.entry(id(value)?).or_default().push(ticker.clone());
    }
    let mut reports = Vec::new();
    let mut total = 0;
    let mut index = String::from(
        "# Coin Emoji Packs - Manifest Index\n\n| Pack | Stickers | Link |\n|------|----------|------|\n",
    );
    // A failed read is unknown, not an empty pack or permission to overwrite a prior report.
    for (_, set) in &sets {
        let name = name(set)?;
        let pack = live(name)?;
        let stickers = pack["stickers"]
            .as_array()
            .ok_or("live coin manifest pack has no stickers")?;
        reports.push((name.to_owned(), render(set, stickers, &aliases)?));
        total += stickers.len();
        index += &format!(
            "| {name} | {} | https://t.me/addemoji/{name} |\n",
            stickers.len()
        );
    }
    index += &format!("\nTotal: {} packs, {total} stickers.\n", sets.len());
    let out = directory.join("manifests");
    crate::coin_provider_stage::physical(&out)?;
    for (name, _) in &reports {
        crate::coin_provider_stage::physical(&out.join(format!("{name}.md")))?;
    }
    crate::coin_provider_stage::physical(&out.join("INDEX.md"))?;
    std::fs::create_dir_all(&out).map_err(|_| "cannot create coin manifest directory")?;
    for (name, body) in reports {
        crate::atomic::write_bytes(&out.join(format!("{name}.md")), body.as_bytes())
            .map_err(|_| "cannot save coin manifest")?;
    }
    crate::atomic::write_bytes(&out.join("INDEX.md"), index.as_bytes())
        .map_err(|_| "cannot save coin manifest index")?;
    Ok((sets.len(), total))
}
