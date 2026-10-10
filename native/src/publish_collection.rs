//! Collection publication state machine: progress is committed by content identity.
use crate::{atomic, logging, publication::Publisher, reconcile, resume, telegram::ApiError};
use serde_json::{Value, json};
use std::path::PathBuf;

pub struct Options {
    pub base: String,
    pub title: String,
    pub bot: String,
    pub user_id: u64,
    pub fmt: String,
    pub per_set: u64,
    pub new_set: bool,
    pub into_pack: Option<u64>,
    pub repaint: bool,
    pub default_emoji: String,
    pub logo: Option<Value>,
}
fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str, String> {
    v.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing {k}"))
}
fn save(path: &std::path::Path, state: &Value) -> Result<(), String> {
    atomic::write_json(path, state).map_err(|e| e.to_string())
}
fn keys(set: &Value) -> Result<Vec<String>, String> {
    set["keys"]
        .as_array()
        .ok_or("set has no keys")?
        .iter()
        .map(|v| v.as_str().map(str::to_owned).ok_or("invalid key".into()))
        .collect()
}

pub fn publish(
    publisher: &mut Publisher<'_>,
    options: &Options,
    plan: &[String],
    state: &mut Value,
) -> Result<Value, String> {
    if !(1..=200).contains(&options.per_set)
        || options.user_id == 0
        || options.base.is_empty()
        || options.bot.is_empty()
    {
        return Err("invalid publication configuration".into());
    }
    let tag = match options.fmt.as_str() {
        "static" => "s",
        "video" => "v",
        "animated" => "a",
        "mixed" => "",
        _ => return Err("invalid family format".into()),
    };
    crate::collection_names::name_length(&options.base, &options.bot, tag)?;
    if options.new_set && options.into_pack.is_some() {
        return Err("--new-set and --into-pack are mutually exclusive".into());
    }
    if plan.is_empty() {
        return Ok(json!({"uploaded":0,"failed":0}));
    }
    let _family = crate::locks::Lease::acquire(
        &publisher
            .data
            .join(format!("publish_{}.lock", options.base)),
        &logging::utc(),
    )
    .map_err(|e| e.to_string())?;
    publish_locked(publisher, options, plan, state)
}

pub fn publish_locked(
    publisher: &mut Publisher<'_>,
    options: &Options,
    plan: &[String],
    state: &mut Value,
) -> Result<Value, String> {
    let path = publisher
        .data
        .join(format!("publish_{}.json", options.base));
    if path.exists() {
        if path.is_symlink() || !path.is_file() {
            return Err("publish state is not a regular file".into());
        }
        *state = serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("cannot read publish state: {e}"))?;
    }
    *state = resume::collection(state.clone(), &options.base)?;
    crate::publication_recovery::recover(publisher, options, state, &path)?;
    let indices = state["sets"]
        .as_array()
        .ok_or("invalid sets")?
        .iter()
        .enumerate()
        .filter(|(_, s)| s["fmt"] == options.fmt)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    let mut active = if let Some(n) = options.into_pack {
        Some(
            *indices
                .iter()
                .find(|&&i| state["sets"][i]["index"].as_u64() == Some(n))
                .ok_or("requested pack is not recorded in this family")?,
        )
    } else {
        indices.last().copied()
    };
    if let Some(i) = active {
        publisher.reconcile(&mut state["sets"][i], &options.base)?;
        save(&path, state)?;
        if !reconcile::open(&state["sets"][i]) {
            if options.into_pack.is_some() {
                return Err("requested pack contains unattributed stickers".into());
            }
            active = None;
        }
        if options.new_set {
            active = None;
        }
        if options.into_pack.is_some()
            && state["sets"][i]["live"].as_u64().unwrap_or(0) >= options.per_set
        {
            return Err("requested pack is full".into());
        }
    }
    let mut index = indices
        .iter()
        .filter_map(|&i| state["sets"][i]["index"].as_u64())
        .max()
        .unwrap_or(0);
    let mut skipped = state["skipped"]
        .as_array()
        .ok_or("invalid skipped")?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<std::collections::BTreeSet<_>>();
    let skipped_at_start = skipped.len();
    let published = publisher
        .cat
        .dispatch(json!({"operation":"published_keys"}))?;
    // Per-family filtering is authoritative, even when another base published the item.
    let mut already = std::collections::BTreeSet::new();
    for key in published
        .as_array()
        .ok_or("invalid published keys")?
        .iter()
        .filter_map(Value::as_str)
    {
        if publisher
            .cat
            .dispatch(json!({"operation":"published","base":options.base,"content_key":key}))?
            == true
        {
            already.insert(key.to_owned());
        }
    }
    let pending = reconcile::pending(&publisher.cat.all(None)?, plan, &already, &skipped)?;
    let (mut count, failed) = (0u64, 0u64);
    for key in pending {
        if publisher
            .cat
            .dispatch(json!({"operation":"published","base":options.base,"content_key":key}))?
            == true
        {
            continue;
        }
        let Some(mut item) = publisher.cat.get(&key)? else {
            continue;
        };
        let source = PathBuf::from(text(&item, "file_path")?);
        if !source.is_file() || source.metadata().map_err(|e| e.to_string())?.len() == 0 {
            skipped.insert(key);
            state["skipped"] = json!(skipped);
            save(&path, state)?;
            continue;
        }
        let usable = publisher.runtime.block_on(
            publisher
                .media
                .call(json!({"operation":"usable","source":source,"format":item["fmt"]})),
        )?;
        if usable != true {
            skipped.insert(key);
            state["skipped"] = json!(skipped);
            save(&path, state)?;
            continue;
        }
        if item["emojis"].as_array().is_none_or(|v| v.is_empty()) {
            item["emojis"] = json!([options.default_emoji]);
        }
        if let Some(i) = active
            && (!reconcile::open(&state["sets"][i])
                || state["sets"][i]["live"].as_u64().unwrap_or(0) >= options.per_set)
        {
            if options.into_pack.is_some() {
                return Err("requested pack cannot accept another sticker".into());
            }
            active = None;
        }
        let creating = active.is_none();
        let name = if let Some(i) = active {
            text(&state["sets"][i], "name")?.to_owned()
        } else {
            index += 1;
            let tag = match options.fmt.as_str() {
                "static" => "s",
                "video" => "v",
                "animated" => "a",
                "mixed" => "",
                _ => return Err("invalid family format".into()),
            };
            format!("{}{tag}{index}_by_{}", options.base, options.bot)
        };
        let title = format!(
            "{} {}",
            options.title,
            state["sets"].as_array().ok_or("invalid sets")?.len() + 1
        );
        let target_index = active
            .and_then(|i| state["sets"][i]["index"].as_u64())
            .unwrap_or(index);
        let intent = json!({"key":key,"operation":if creating{"create"}else{"add"},"set_name":name,"set_index":target_index,
            "expected_before":active.map(|i|state["sets"][i]["live"].clone()),"title":title,"format":item["fmt"],"started_utc":logging::utc()});
        state["in_flight"] = intent;
        save(&path, state)?;
        let first = if creating {
            options.logo.as_ref().unwrap_or(&item)
        } else {
            &item
        };
        let result = publisher.mutate(
            first,
            &name,
            creating.then_some(title.as_str()),
            options.user_id,
            options.repaint,
        );
        let result = match result {
            Ok(out) => out,
            Err(ApiError::FloodWait { method, seconds }) => {
                save(&path, state)?;
                return Err(format!("FLOOD_WAIT {method} {seconds}"));
            }
            Err(ApiError::Rejected(error))
                if creating && error.to_lowercase().contains("occupied") =>
            {
                crate::publication_recovery::recover(publisher, options, state, &path)?;
                let i = state["sets"]
                    .as_array()
                    .ok_or("invalid sets")?
                    .iter()
                    .position(|set| set["name"] == name)
                    .ok_or("occupied set could not be adopted")?;
                active = Some(i);
                if publisher.cat.dispatch(
                    json!({"operation":"published","base":options.base,"content_key":key}),
                )? == true
                {
                    continue;
                }
                if !reconcile::open(&state["sets"][i]) {
                    return Err("adopted set contains unattributed stickers".into());
                }
                state["in_flight"] = json!({"key":key,"operation":"add","set_name":name,"set_index":index,
                    "expected_before":state["sets"][i]["live"],"format":item["fmt"],"started_utc":logging::utc()});
                save(&path, state)?;
                let out = publisher
                    .mutate(&item, &name, None, options.user_id, options.repaint)
                    .map_err(crate::telegram::ApiError::command_error)?;
                record(publisher, state, i, &item, &out, &options.base)?;
                state["in_flight"] = Value::Null;
                count += 1;
                save(&path, state)?;
                continue;
            }
            Err(ApiError::Ambiguous(_)) => {
                if let Some(i) = active {
                    publisher.reconcile(&mut state["sets"][i], &options.base)?;
                    save(&path, state)?;
                }
                return Err(
                    "ambiguous mutation retained in state; reconcile before another mutation"
                        .into(),
                );
            }
            Err(ApiError::Rejected(e)) if e.to_lowercase().contains("wrong file type") => {
                skipped.insert(key);
                state["skipped"] = json!(skipped);
                state["in_flight"] = Value::Null;
                save(&path, state)?;
                continue;
            }
            Err(error) => {
                save(&path, state)?;
                return Err(format!("publication failed with retained intent: {error}"));
            }
        };
        if creating {
            let logo = options.logo.is_some();
            state["sets"].as_array_mut().ok_or("invalid sets")?.push(json!({"fmt":options.fmt,"index":index,"name":name,"title":title,"live":if logo{1}else{0},"logo":logo,"keys":[]}));
            active = Some(state["sets"].as_array().ok_or("invalid sets")?.len() - 1);
            save(&path, state)?;
            if logo {
                state["in_flight"] = json!({"key":key,"operation":"add","set_name":name,
                    "set_index":index,"expected_before":1,"format":item["fmt"],"started_utc":logging::utc()});
                save(&path, state)?;
                let out = publisher
                    .mutate(&item, &name, None, options.user_id, options.repaint)
                    .map_err(crate::telegram::ApiError::command_error)?;
                record(
                    publisher,
                    state,
                    active.ok_or("missing active set")?,
                    &item,
                    &out,
                    &options.base,
                )?;
            } else {
                record(
                    publisher,
                    state,
                    active.ok_or("missing active set")?,
                    &item,
                    &result,
                    &options.base,
                )?;
            }
        } else {
            record(
                publisher,
                state,
                active.ok_or("missing active set")?,
                &item,
                &result,
                &options.base,
            )?;
        }
        state["in_flight"] = Value::Null;
        count += 1;
        save(&path, state)?;
        if let Some(i) = active
            && state["sets"][i]["live"]
                .as_u64()
                .is_some_and(|n| n >= options.per_set)
        {
            let set = state["sets"][i].clone();
            crate::collection_notify::notify(publisher, options.user_id, state, &path, &set, true)?;
        }
    }
    for set in state["sets"]
        .as_array_mut()
        .ok_or("invalid sets")?
        .iter_mut()
        .filter(|set| set["fmt"] == options.fmt)
    {
        if set["keys"].as_array().is_some_and(|keys| !keys.is_empty()) {
            publisher.reconcile(set, &options.base)?;
            crate::collection_manifest::write(&publisher.data, publisher.cat, set, &options.base)?;
        }
    }
    save(&path, state)?;
    if skipped.len() == skipped_at_start
        && failed == 0
        && let Some(i) = active
    {
        let set = state["sets"][i].clone();
        crate::collection_notify::notify(publisher, options.user_id, state, &path, &set, false)?;
    }
    Ok(json!({"uploaded":count,"failed":failed}))
}
fn record(
    publisher: &mut Publisher<'_>,
    state: &mut Value,
    i: usize,
    item: &Value,
    out: &Value,
    base: &str,
) -> Result<(), String> {
    let key = text(item, "content_key")?;
    let sticker = out.get("sticker").ok_or("missing confirmed sticker")?;
    let fuid = sticker["file_unique_id"].as_str().unwrap_or("");
    let owner = publisher
        .cat
        .dispatch(json!({"operation":"seen","file_unique_id":fuid}))?;
    if owner.as_str().is_some_and(|v| v != key) {
        return Err("confirmed live sticker belongs to another catalog key".into());
    }
    let mut recorded = keys(&state["sets"][i])?;
    if recorded.iter().any(|k| k == key) {
        return Err("recorded upload already appears in this set".into());
    }
    publisher.cat.record_seen(fuid, key)?;
    publisher.cat.uploaded(&json!({"content_key":key,"custom_emoji_id":sticker.get("custom_emoji_id"),"base":base,"set_name":state["sets"][i]["name"]}),&logging::utc())?;
    recorded.push(key.into());
    state["sets"][i]["keys"] = json!(recorded);
    state["sets"][i]["live"] = out["live"].clone();
    Ok(())
}
