//! Offline intended-layout report; held-live is reported but never silently removed.
use crate::{config::Config, logging::RunLog};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
fn list(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
pub fn summarize(plan: &Value, state: &Value) -> Result<Value, String> {
    let cap = plan["per_set"].as_u64().filter(|n| *n > 0).unwrap_or(200);
    let mut live = BTreeMap::new();
    for set in state["sets"].as_array().into_iter().flatten() {
        let index = set["index"].as_u64().ok_or("invalid recorded pack index")?;
        for key in list(&set["keys"]) {
            live.insert(key, index);
        }
    }
    let mut packs = BTreeMap::<u64, Value>::new();
    let pack = |n: u64| {
        json!({"target":plan["counts"].get(n.to_string()).and_then(Value::as_u64).unwrap_or(0),
        "logo":plan["logo_slots"].get(n.to_string()).and_then(Value::as_u64).unwrap_or(0),"cap":cap,"move_in":[],"move_out":[],"held_live":[],"candidates":[]})
    };
    for row in plan["moves"].as_array().into_iter().flatten() {
        let key = row["key"].as_str().ok_or("invalid move key")?;
        let to = row["to_pack"].as_u64().ok_or("invalid destination pack")?;
        let from = row["from_pack"].as_u64().ok_or("invalid source pack")?;
        packs.entry(to).or_insert_with(|| pack(to))["move_in"]
            .as_array_mut()
            .ok_or("invalid report")?
            .push(json!(key));
        packs.entry(from).or_insert_with(|| pack(from))["move_out"]
            .as_array_mut()
            .ok_or("invalid report")?
            .push(json!(key));
    }
    for row in plan["held"].as_array().into_iter().flatten() {
        let key = row["key"].as_str().ok_or("invalid held key")?;
        if let Some(n) = live.get(key) {
            packs.entry(*n).or_insert_with(|| pack(*n))["held_live"]
                .as_array_mut()
                .ok_or("invalid report")?
                .push(json!(key));
        }
    }
    let excluded = list(&plan["excluded"]).into_iter().collect::<BTreeSet<_>>();
    for (key, n) in crate::intent::targets(plan)? {
        if !live.contains_key(&key) && !excluded.contains(&key) {
            packs.entry(n).or_insert_with(|| pack(n))["candidates"]
                .as_array_mut()
                .ok_or("invalid report")?
                .push(json!(key));
        }
    }
    for n in plan["counts"]
        .as_object()
        .into_iter()
        .flat_map(|v| v.keys())
    {
        let n = n.parse().map_err(|_| "invalid report count pack")?;
        packs.entry(n).or_insert_with(|| pack(n));
    }
    let pending = packs.values().any(|p| {
        ["move_in", "move_out", "held_live", "candidates"]
            .iter()
            .any(|k| p[*k].as_array().is_some_and(|v| !v.is_empty()))
            || p["target"].as_u64().unwrap_or(0) + p["logo"].as_u64().unwrap_or(0) > cap
    });
    Ok(json!({"packs":packs,"pending":pending}))
}
pub fn render(
    summary: &Value,
    ids: &BTreeMap<String, String>,
    live_ids: Option<&BTreeSet<String>>,
) -> Result<String, String> {
    let mut lines = vec!["pack  target+logo/cap  in  out  held-live  candidates".to_owned()];
    let mut retire = Vec::new();
    let packs = summary["packs"].as_object().ok_or("invalid report packs")?;
    let mut ordered = packs
        .iter()
        .map(|(n, p)| {
            n.parse::<u64>()
                .map(|n| (n, p))
                .map_err(|_| "invalid report pack number")
        })
        .collect::<Result<Vec<_>, _>>()?;
    ordered.sort_by_key(|(n, _)| *n);
    for (n, p) in ordered {
        let target = p["target"].as_u64().ok_or("invalid target count")?;
        let logo = p["logo"].as_u64().ok_or("invalid logo count")?;
        let cap = p["cap"].as_u64().ok_or("invalid report cap")?;
        let over = if target.saturating_add(logo) > cap {
            "  OVER CAPACITY"
        } else {
            ""
        };
        let incoming = list(&p["move_in"]).len();
        let outgoing = list(&p["move_out"]);
        let held = list(&p["held_live"]);
        let candidates = list(&p["candidates"]).len();
        lines.push(format!(
            "{n:>4}  {target:>6}+{logo}/{cap:<5} {incoming:>3} {:>4} {:>10} {candidates:>11}{over}",
            outgoing.len(),
            held.len()
        ));
        retire.extend(outgoing);
        retire.extend(held);
    }
    if !retire.is_empty() {
        lines.push(String::new());
        lines.push("custom_emoji_ids a move or removal would RETIRE (a re-add mints a new id; the old one stops working):".into());
        for key in retire {
            let cid = ids.get(&key).map(String::as_str).unwrap_or("?");
            let note = if live_ids.is_some_and(|live| !live.contains(cid)) {
                "  (not live now)"
            } else {
                ""
            };
            lines.push(format!("  {cid}  {key}{note}"));
        }
    }
    lines.push(String::new());
    lines.push(
        if summary["pending"] == true {
            "PENDING: the plan asks for work (see docs/design/plan-applier.md)"
        } else {
            "nothing pending: the live packs match the plan"
        }
        .into(),
    );
    Ok(lines.join("\n"))
}
fn catalog_ids(data: &std::path::Path, base: &str) -> BTreeMap<String, String> {
    let read = || -> Result<BTreeMap<String, String>, String> {
        let db = rusqlite::Connection::open_with_flags(
            data.join("catalog.db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(|e| e.to_string())?;
        let mut query = db.prepare("SELECT content_key,custom_emoji_id FROM publications WHERE base=? AND custom_emoji_id IS NOT NULL").map_err(|e| e.to_string())?;
        query
            .query_map([base], |row| {
                let id: rusqlite::types::Value = row.get(1)?;
                let id = match id {
                    rusqlite::types::Value::Text(s) => s,
                    rusqlite::types::Value::Integer(n) => n.to_string(),
                    _ => {
                        return Err(rusqlite::Error::InvalidColumnType(
                            1,
                            "custom_emoji_id".into(),
                            id.data_type(),
                        ));
                    }
                };
                Ok((row.get::<_, String>(0)?, id))
            })
            .map_err(|e| e.to_string())?
            .map(|row| row.map_err(|e| e.to_string()))
            .collect()
    };
    read().unwrap_or_default()
}
pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("Usage: numera-emoji plan-status [--data-dir DIR] [--base NAME] [--live]");
        return Ok(0);
    }
    let (mut data, mut base, mut live) = (
        "collection".to_owned(),
        config.value("COLLECTION_PACK_BASE").to_owned(),
        false,
    );
    let mut i = 0;
    while i < args.len() {
        let take = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            args.get(*i)
                .cloned()
                .ok_or("option requires a value".into())
        };
        match args[i].as_str() {
            "--data-dir" => data = take(&mut i)?,
            "--base" => base = take(&mut i)?,
            "--live" => live = true,
            value => return Err(format!("unknown plan-status option: {value}")),
        }
        i += 1;
    }
    crate::collection_names::valid_base(&base)?;
    let data = config.path(&data);
    let plan = crate::panel_save::read_plan(&data.join("pack_plan.json"))?;
    if plan.is_null() {
        println!("the panel has saved no plan; nothing pending");
        return Ok(0);
    }
    let state = match std::fs::read(data.join(format!("publish_{base}.json"))) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(e) => return Err(e.to_string()),
    };
    let summary = summarize(&plan, &state)?;
    let mut live_ids = live.then(BTreeSet::new);
    if live {
        let api = if std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1") {
            std::env::var("NUMERA_TEST_API_BASE")
                .unwrap_or_else(|_| "https://api.telegram.org".into())
        } else {
            "https://api.telegram.org".into()
        };
        let tg = crate::telegram::Telegram::new(config.require("GENERAL_BOT_TOKEN")?.into(), api)
            .map_err(|e| e.to_string())?;
        for set in state["sets"].as_array().into_iter().flatten() {
            if let Some(name) = set["name"].as_str() {
                match tg.read_call(
                    "getStickerSet",
                    &BTreeMap::from([("name".into(), name.into())]),
                ) {
                    Ok(value) => {
                        for sticker in value["stickers"].as_array().into_iter().flatten() {
                            let id = match &sticker["custom_emoji_id"] {
                                Value::String(text) => text.clone(),
                                Value::Number(n) => n.to_string(),
                                _ => "None".into(),
                            };
                            if let Some(ids) = live_ids.as_mut() {
                                ids.insert(id);
                            }
                        }
                    }
                    Err(error) => log.event("WARNING", "plan-status", &error.to_string()),
                }
            }
        }
    }
    println!(
        "{}",
        render(&summary, &catalog_ids(&data, &base), live_ids.as_ref())?
    );
    Ok(if summary["pending"] == true { 3 } else { 0 })
}
