//! Reorder existing stickers only; content IDs survive and the logo stays pinned.
use crate::{
    atomic,
    catalog::Catalog,
    config::Config,
    logging::RunLog,
    telegram::{Result as ApiResult, Telegram},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub fn moves(live: &[Value], desired: &[String], pinned: usize) -> Result<Vec<Value>, String> {
    if live.len() != desired.len() + pinned {
        return Err("live stickers and desired order have different lengths".into());
    }
    let mut order = live.to_vec();
    let mut out = Vec::new();
    for (offset, cid) in desired.iter().enumerate() {
        let target = pinned + offset;
        if order[target]["custom_emoji_id"] == *cid {
            continue;
        }
        let index = order
            .iter()
            .position(|s| s["custom_emoji_id"] == *cid)
            .ok_or("desired custom emoji is not live")?;
        let file = order[index]["file_id"]
            .as_str()
            .ok_or("live sticker has no file ID")?;
        out.push(json!([target, cid, file]));
        let sticker = order.remove(index);
        order.insert(target, sticker);
    }
    Ok(out)
}
pub fn sync(
    tg: &Telegram,
    cat: &Catalog,
    record: &mut Value,
    apply: bool,
) -> Result<usize, String> {
    let name = record["name"].as_str().ok_or("recorded set has no name")?;
    let pack = tg
        .read_call(
            "getStickerSet",
            &BTreeMap::from([("name".into(), name.into())]),
        )
        .map_err(|e| e.to_string())?;
    let live = pack["stickers"]
        .as_array()
        .ok_or("live set has no stickers")?;
    let pinned = usize::from(record["logo"].as_bool().unwrap_or(false));
    if live.len() < pinned {
        return Err("live set no longer holds its pinned logo".into());
    }
    let items = cat.all(None)?;
    let known = items
        .iter()
        .filter_map(|item| item["custom_emoji_id"].as_str())
        .collect::<BTreeSet<_>>();
    let live_ids = live[pinned..]
        .iter()
        .map(|s| {
            s["custom_emoji_id"]
                .as_str()
                .ok_or("live sticker has no custom emoji ID")
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if live_ids.iter().any(|id| !known.contains(id)) {
        return Err(
            "live set holds stickers this catalog does not know; refusing to reorder around them"
                .into(),
        );
    }
    let desired = items
        .iter()
        .filter_map(|item| item["custom_emoji_id"].as_str())
        .filter(|id| live_ids.contains(id))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let planned = moves(live, &desired, pinned)?;
    if apply {
        for row in &planned {
            let data = BTreeMap::from([
                (
                    "sticker".into(),
                    row[2].as_str().ok_or("invalid reorder file ID")?.into(),
                ),
                ("position".into(), row[0].to_string()),
            ]);
            tg.call(
                "setStickerPositionInSet",
                &data,
                &[],
                5,
                None::<fn() -> ApiResult<Option<bool>>>,
            )
            .map_err(|e| e.to_string())?;
            std::thread::sleep(std::time::Duration::from_millis(120));
        }
        let by_id = items
            .iter()
            .filter_map(|item| {
                Some((
                    item["custom_emoji_id"].as_str()?,
                    item["content_key"].as_str()?,
                ))
            })
            .collect::<BTreeMap<_, _>>();
        record["keys"] = json!(
            desired
                .iter()
                .map(|id| by_id
                    .get(id.as_str())
                    .copied()
                    .ok_or("reordered key disappeared"))
                .collect::<Result<Vec<_>, _>>()?
        );
    }
    Ok(planned.len())
}
pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!(
            "Usage: numera-emoji sync-order --base NAME [--data-dir DIR] [--token-env KEY] [--pack N] [--apply]"
        );
        return Ok(0);
    }
    let (mut base, mut data, mut token_key, mut packs, mut apply) = (
        String::new(),
        "collection".to_owned(),
        "GENERAL_BOT_TOKEN".to_owned(),
        BTreeSet::new(),
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
            "--base" => base = take(&mut i)?,
            "--data-dir" => data = take(&mut i)?,
            "--token-env" => token_key = take(&mut i)?,
            "--pack" => {
                packs.insert(
                    take(&mut i)?
                        .parse::<u64>()
                        .ok()
                        .filter(|n| *n > 0)
                        .ok_or("invalid --pack")?,
                );
            }
            "--apply" => apply = true,
            value => return Err(format!("unknown sync-order option: {value}")),
        }
        i += 1;
    }
    crate::collection_names::valid_base(&base)?;
    let data = config.path(&data);
    let path = data.join(format!("publish_{base}.json"));
    if !data.join("catalog.db").is_file() || !path.is_file() {
        return Err("no catalog or recorded pack family".into());
    }
    let cat = Catalog::open(
        &data.join("catalog.db"),
        &crate::logging::utc(),
        &config.root,
    )?;
    let _family = crate::locks::Lease::acquire(
        &data.join(format!("publish_{base}.lock")),
        &crate::logging::utc(),
    )
    .map_err(|e| e.to_string())?;
    let mut state = crate::resume::collection(
        serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?,
        &base,
    )?;
    let sets = state["sets"].as_array_mut().ok_or("invalid sets")?;
    if sets.is_empty() {
        return Err("state records no sets".into());
    }
    if packs
        .iter()
        .any(|index| !sets.iter().any(|set| set["index"].as_u64() == Some(*index)))
    {
        return Err("requested pack is not recorded in this family".into());
    }
    let api = if std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1") {
        std::env::var("NUMERA_TEST_API_BASE").unwrap_or_else(|_| "https://api.telegram.org".into())
    } else {
        "https://api.telegram.org".into()
    };
    let tg = Telegram::new(config.require(&token_key)?.into(), api).map_err(|e| e.to_string())?;
    let (mut total, mut failed) = (0, 0);
    let mut indices = (0..sets.len())
        .filter(|i| {
            packs.is_empty()
                || sets[*i]["index"]
                    .as_u64()
                    .is_some_and(|n| packs.contains(&n))
        })
        .collect::<Vec<_>>();
    indices.sort_by_key(|i| sets[*i]["index"].as_u64());
    let set_count = indices.len();
    for i in indices {
        match sync(&tg, &cat, &mut sets[i], apply) {
            Ok(n) => total += n,
            Err(error) => {
                failed += 1;
                log.event("ERROR", "order", &error)
            }
        }
    }
    if apply {
        atomic::write_json(&path, &state).map_err(|e| e.to_string())?;
    }
    log.event(
        "INFO",
        "order",
        &format!(
            "{} {total} sticker(s) across {} set(s); {failed} set(s) skipped.",
            if apply { "moved" } else { "would move" },
            set_count
        ),
    );
    if !apply && total > 0 {
        println!("\n{total} sticker(s) are out of order. Re-run with --apply to fix them.\n");
    }
    Ok(if failed > 0 { 4 } else { 0 })
}
