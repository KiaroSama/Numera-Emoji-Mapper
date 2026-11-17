//! Collection publication state machine: progress is committed by content identity.
use crate::{
    atomic, logging,
    publication::{Failure, Publisher},
    reconcile, resume,
    telegram::ApiError,
};
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

pub fn publish<M: crate::media_adapter::MediaCodec>(
    publisher: &mut Publisher<'_, M>,
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

pub fn publish_locked<M: crate::media_adapter::MediaCodec>(
    publisher: &mut Publisher<'_, M>,
    options: &Options,
    plan: &[String],
    state: &mut Value,
) -> Result<Value, String> {
    publish_checked(publisher, options, plan, state).map_err(|failure| failure.message)
}

pub(crate) fn publish_checked<M: crate::media_adapter::MediaCodec>(
    publisher: &mut Publisher<'_, M>,
    options: &Options,
    plan: &[String],
    state: &mut Value,
) -> Result<Value, Failure> {
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
    crate::publication_recovery::recover_checked(publisher, options, state, &path)?;
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
        publisher.reconcile_checked(&mut state["sets"][i], &options.base)?;
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
    let pending_count = pending.len();
    let (mut count, mut failed) = (0u64, 0u64);
    'items: for key in pending {
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
                .publication_usable(&source, text(&item, "fmt")?),
        )?;
        if !usable {
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
        // A definite full-set refusal permits one transition to a fresh create,
        // never a resend to the refused set or a retry of an uncertain mutation.
        loop {
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
            let mut result = publisher.mutate(
                first,
                &name,
                creating.then_some(title.as_str()),
                options.user_id,
                options.repaint,
            );
            let occupied = creating
                && matches!(&result,
            Err(ApiError::Rejected(error)) if error.to_lowercase().contains("occupied"));
            let mut registered = false;
            if occupied || (creating && options.logo.is_some() && result.is_ok()) {
                if occupied {
                    match publisher.tg.probe(&name) {
                        (crate::telegram::SetState::Exists, Some(_)) => {}
                        (crate::telegram::SetState::Missing, _) => {
                            return Err(result.err().ok_or("missing occupied refusal")?.into());
                        }
                        _ => {
                            return Err(Failure::unknown(
                                "occupied set cannot be read; intent retained",
                            ));
                        }
                    }
                }
                // Existence records a container, not content. A confirmed logo create
                // also survives a later artwork refusal without reusing its index.
                state["sets"].as_array_mut().ok_or("invalid sets")?.push(json!({
                "fmt":options.fmt,"index":index,"name":name,"title":title,
                "live":u64::from(options.logo.is_some()),"logo":options.logo.is_some(),"keys":[]
            }));
                let i = state["sets"].as_array().ok_or("invalid sets")?.len() - 1;
                active = Some(i);
                registered = true;
                save(&path, state)?;
                if occupied {
                    publisher.reconcile_checked(&mut state["sets"][i], &options.base)?;
                    state["in_flight"] = Value::Null;
                    save(&path, state)?;
                    if publisher.cat.dispatch(
                        json!({"operation":"published","base":options.base,"content_key":key}),
                    )? == true
                    {
                        continue 'items;
                    }
                    if !reconcile::open(&state["sets"][i]) {
                        return Err("adopted set contains unattributed stickers".into());
                    }
                }
                state["in_flight"] = json!({"key":key,"operation":"add","set_name":name,"set_index":index,
                "expected_before":state["sets"][i]["live"],"format":item["fmt"],"started_utc":logging::utc()});
                save(&path, state)?;
                result = publisher.mutate(&item, &name, None, options.user_id, options.repaint);
            }
            let result = match result {
                Ok(out) => out,
                Err(ApiError::FloodWait { method, seconds }) => {
                    save(&path, state)?;
                    return Err(Failure::unknown(format!("FLOOD_WAIT {method} {seconds}")));
                }
                Err(ApiError::Rejected(error))
                    if !creating && error.contains("STICKERS_TOO_MUCH") =>
                {
                    state["in_flight"] = Value::Null;
                    save(&path, state)?;
                    if options.into_pack.is_some() {
                        return Err(
                            "requested pack is full; refusing to select another pack".into()
                        );
                    }
                    active = None;
                    continue;
                }
                Err(ApiError::Ambiguous(_)) => {
                    if let Some(i) = active {
                        publisher.reconcile_checked(&mut state["sets"][i], &options.base)?;
                        if publisher.cat.dispatch(
                            json!({"operation":"published","base":options.base,"content_key":key}),
                        )? != true
                        {
                            failed += 1;
                        }
                        state["in_flight"] = Value::Null;
                        save(&path, state)?;
                        if !reconcile::open(&state["sets"][i]) {
                            active = None;
                        }
                        continue 'items;
                    }
                    return Err(Failure::unknown(
                        "ambiguous mutation retained in state; reconcile before another mutation",
                    ));
                }
                Err(ApiError::Rejected(error)) => {
                    // Only an answered refusal proves that this attempt did not land.
                    // Unknown transport/confirmation keeps its intent and stops below.
                    if creating && !registered {
                        index -= 1;
                    }
                    if error.to_lowercase().contains("wrong file type") {
                        skipped.insert(key.clone());
                        state["skipped"] = json!(skipped);
                    } else {
                        failed += 1;
                    }
                    state["in_flight"] = Value::Null;
                    save(&path, state)?;
                    if let Some(log) = publisher.tg.log() {
                        log.event(
                            "WARNING",
                            "publisher",
                            &format!("[{}] upload refused for {key}: {error}", options.fmt),
                        );
                    }
                    continue 'items;
                }
                Err(error) => {
                    save(&path, state)?;
                    let mut failure = Failure::from(error);
                    failure.message = format!(
                        "publication failed with retained intent: {}",
                        failure.message
                    );
                    return Err(failure);
                }
            };
            if creating && !registered {
                state["sets"]
                    .as_array_mut()
                    .ok_or("invalid sets")?
                    .push(json!({
                        "fmt":options.fmt,"index":index,"name":name,"title":title,
                        "live":0,"logo":false,"keys":[]
                    }));
                active = Some(state["sets"].as_array().ok_or("invalid sets")?.len() - 1);
                save(&path, state)?;
            }
            record(
                publisher,
                state,
                active.ok_or("missing active set")?,
                &item,
                &result,
                &options.base,
            )?;
            state["in_flight"] = Value::Null;
            count += 1;
            save(&path, state)?;
            if let Some(i) = active {
                progress(
                    publisher,
                    options,
                    &item,
                    &state["sets"][i],
                    count,
                    pending_count,
                );
            }
            if let Some(i) = active
                && state["sets"][i]["live"]
                    .as_u64()
                    .is_some_and(|n| n >= options.per_set)
            {
                let set = state["sets"][i].clone();
                crate::collection_notify::notify(
                    publisher,
                    options.user_id,
                    state,
                    &path,
                    &set,
                    true,
                )?;
            }
            break;
        }
    }
    for set in state["sets"]
        .as_array_mut()
        .ok_or("invalid sets")?
        .iter_mut()
        .filter(|set| set["fmt"] == options.fmt)
    {
        if set["keys"].as_array().is_some_and(|keys| !keys.is_empty()) {
            publisher.reconcile_checked(set, &options.base)?;
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
fn progress<M: crate::media_adapter::MediaCodec>(
    publisher: &Publisher<'_, M>,
    options: &Options,
    item: &Value,
    set: &Value,
    count: u64,
    total: usize,
) {
    if let Some(log) = publisher.tg.log() {
        log.event(
            "INFO",
            "publisher",
            &format!(
                "[{}] uploaded {} -> {} #{} ({count}/{total} this run)",
                options.fmt,
                item["content_key"].as_str().unwrap_or("?"),
                set["name"].as_str().unwrap_or("?"),
                set["live"].as_u64().unwrap_or(0),
            ),
        );
    }
}

fn record<M: crate::media_adapter::MediaCodec>(
    publisher: &mut Publisher<'_, M>,
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
