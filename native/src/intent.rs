//! Pack intent is separate from live membership. No I/O or Telegram effects here.
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, String>;

fn object(value: &Value) -> Result<&Map<String, Value>> {
    value.as_object().ok_or_else(|| "expected an object".into())
}
fn array(value: &Value) -> Result<&Vec<Value>> {
    value.as_array().ok_or_else(|| "expected a list".into())
}
fn key(value: &Value) -> Result<&str> {
    value.as_str().ok_or_else(|| "expected a string key".into())
}
fn pack(value: &Value) -> Result<u64> {
    value
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| "pack numbers must be positive integers".into())
}
fn strings(value: &Value) -> Result<BTreeSet<String>> {
    array(value)?
        .iter()
        .map(|v| key(v).map(str::to_owned))
        .collect()
}
fn optional_strings(value: Option<&Value>) -> Result<BTreeSet<String>> {
    match value {
        None => Ok(BTreeSet::new()),
        Some(v) => strings(v),
    }
}
fn logo(card: &Value) -> bool {
    card.get("isLogo").and_then(Value::as_bool).unwrap_or(false)
}
fn live_pack(card: &Value) -> Result<Option<u64>> {
    match card.get("pack") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => pack(v).map(Some),
    }
}
fn card_key(card: &Value) -> Result<&str> {
    key(card.get("key").ok_or("card has no key")?)
}
fn included(card: &Value) -> Result<bool> {
    match card.get("included") {
        None => Ok(false),
        Some(v) => v.as_bool().ok_or_else(|| "included must be boolean".into()),
    }
}

pub fn targets(previous: &Value) -> Result<BTreeMap<String, u64>> {
    if previous.is_null() {
        return Ok(BTreeMap::new());
    }
    let doc = object(previous)?;
    let mut out = BTreeMap::new();
    if let Some(pairs) = doc.get("targets").filter(|v| !v.is_null()) {
        for pair in array(pairs)? {
            let p = array(pair)?;
            if p.len() != 2 {
                return Err("invalid target pair".into());
            }
            if out.insert(key(&p[0])?.into(), pack(&p[1])?).is_some() {
                return Err("duplicate pack target keys".into());
            }
        }
    } else {
        for row in array(doc.get("moves").ok_or("plan has no moves")?)? {
            if out
                .insert(
                    card_key(row)?.into(),
                    pack(row.get("to_pack").ok_or("move has no target")?)?,
                )
                .is_some()
            {
                return Err("duplicate pack target keys".into());
            }
        }
    }
    Ok(out)
}

fn request_targets(request: &Value) -> Result<BTreeMap<String, u64>> {
    object(request.get("targets").ok_or("missing targets")?)?
        .iter()
        .map(|(k, v)| Ok((k.clone(), pack(v)?)))
        .collect()
}

fn counts_json(counts: &BTreeMap<u64, u64>) -> Value {
    Value::Object(
        counts
            .iter()
            .map(|(n, c)| (n.to_string(), json!(c)))
            .collect(),
    )
}

fn build(view: &[Value], wants: &BTreeMap<String, u64>, capacity: u64, utc: &str) -> Result<Value> {
    let (mut moves, mut held, mut counts) = (Vec::new(), Vec::new(), BTreeMap::new());
    for card in view {
        object(card)?;
        let k = card_key(card)?;
        if k.is_empty() || logo(card) {
            continue;
        }
        let live = live_pack(card)?;
        let label = card
            .get("label")
            .filter(|v| v.as_str().is_some_and(|s| !s.is_empty()))
            .cloned()
            .unwrap_or_else(|| json!(k));
        if !included(card)? {
            if let Some(n) = live {
                held.push(json!({"key": k, "label": label, "from_pack": n}));
            }
            continue;
        }
        if let Some(want) = wants.get(k) {
            *counts.entry(*want).or_insert(0) += 1;
            if live.is_some_and(|n| n != *want) {
                moves.push(json!({"key": k, "label": label, "from_pack": live, "to_pack": want}));
            }
        }
    }
    let over = counts
        .iter()
        .filter(|(_, c)| **c > capacity)
        .map(|(n, c)| (*n, *c))
        .collect();
    Ok(
        json!({"version":1,"written_utc":utc,"per_set":capacity,"counts":counts_json(&counts),
        "over_capacity":counts_json(&over),"moves":moves,"held":held}),
    )
}

fn old_rows(previous: &Value, field: &str, scope: &BTreeSet<String>) -> Result<Vec<Value>> {
    let Some(rows) = previous.get(field) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for row in array(rows)? {
        if !scope.contains(card_key(row)?) {
            out.push(row.clone());
        }
    }
    Ok(out)
}

fn merge(
    request: &Value,
    view: &[Value],
    wants: &BTreeMap<String, u64>,
    capacity: u64,
    utc: &str,
) -> Result<Value> {
    let previous = request.get("previous").unwrap_or(&Value::Null);
    if !previous.is_null() {
        object(previous)?;
    }
    let scope = strings(request.get("scope").ok_or("missing scope")?)?;
    let live = request
        .get("live")
        .filter(|v| !v.is_null())
        .map(strings)
        .transpose()?;
    let mut scoped = Vec::new();
    for c in view {
        if logo(c) || scope.contains(card_key(c)?) {
            scoped.push(c.clone());
        }
    }
    let mut plan = build(&scoped, wants, capacity, utc)?;
    let mut combined = targets(previous)?;
    combined.retain(|k, _| !scope.contains(k));
    combined.extend(wants.clone());
    for field in ["moves", "held"] {
        let mut rows = old_rows(previous, field, &scope)?;
        rows.extend(array(&plan[field])?.clone());
        plan[field] = json!(rows);
    }
    let mut old_excluded = optional_strings(previous.get("excluded"))?;
    if let Some(rows) = previous.get("held") {
        for row in array(rows)? {
            old_excluded.insert(card_key(row)?.into());
        }
    }
    let mut excluded = old_excluded
        .difference(&scope)
        .cloned()
        .collect::<BTreeSet<_>>();
    for card in &scoped {
        if !logo(card) && !included(card)? {
            excluded.insert(card_key(card)?.into());
        }
    }
    let mut known = optional_strings(previous.get("known"))?;
    known.extend(combined.keys().cloned());
    known.extend(old_excluded);
    known.extend(scope);
    if let Some(live) = &live {
        combined.retain(|k, _| live.contains(k));
        known.retain(|k| live.contains(k));
        excluded.retain(|k| live.contains(k));
        for field in ["moves", "held"] {
            let mut kept = Vec::new();
            for row in array(&plan[field])? {
                if live.contains(card_key(row)?) {
                    kept.push(row.clone());
                }
            }
            plan[field] = json!(kept);
        }
    }
    plan["targets"] = Value::Array(combined.iter().map(|(k, n)| json!([k, n])).collect());
    plan["known"] = json!(known);
    plan["excluded"] = json!(excluded);
    let mut counts: BTreeMap<u64, u64> = BTreeMap::new();
    for (k, n) in &combined {
        if !excluded.contains(k) {
            *counts.entry(*n).or_insert(0) += 1;
        }
    }
    let mut logos = match previous.get("logo_slots") {
        None => Map::new(),
        Some(v) => object(v)?.clone(),
    };
    for card in view {
        if logo(card) {
            match live_pack(card)? {
                Some(n) => {
                    logos.insert(n.to_string(), json!(1));
                }
                None => {
                    for n in counts.keys() {
                        logos.insert(n.to_string(), json!(1));
                    }
                }
            }
        }
    }
    if live.is_some() {
        let mut alive = counts.keys().map(u64::to_string).collect::<BTreeSet<_>>();
        for card in view {
            if logo(card)
                && let Some(n) = live_pack(card)?
            {
                alive.insert(n.to_string());
            }
        }
        logos.retain(|k, _| alive.contains(k));
    }
    let mut over = BTreeMap::new();
    for (n, c) in &counts {
        let slots = logos
            .get(&n.to_string())
            .map(|v| v.as_u64().ok_or("invalid logo slot count"))
            .transpose()?
            .unwrap_or(0);
        let total = c.checked_add(slots).ok_or("pack count overflow")?;
        if total > capacity {
            over.insert(*n, total);
        }
    }
    plan["logo_slots"] = json!(logos);
    plan["counts"] = counts_json(&counts);
    plan["over_capacity"] = counts_json(&over);
    Ok(plan)
}

pub fn execute(request: Value) -> Result<Value> {
    object(&request)?;
    let operation = request
        .get("operation")
        .and_then(Value::as_str)
        .ok_or("missing operation")?;
    let previous = request.get("previous").unwrap_or(&Value::Null);
    if operation == "targets" {
        return Ok(json!(targets(previous)?));
    }
    let view = array(request.get("view").ok_or("missing view")?)?;
    if operation == "overlay" {
        let wants = targets(previous)?;
        let mut cards = Vec::with_capacity(view.len());
        for c in view {
            let mut card = c.clone();
            object(&card)?;
            if !logo(c)
                && let Some(n) = wants.get(card_key(c)?)
            {
                card["pack"] = json!(n);
            }
            cards.push(card);
        }
        return Ok(json!(cards));
    }
    let wants = request_targets(&request)?;
    let capacity = request
        .get("per_set")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
        .ok_or("invalid per_set")?;
    let utc = request
        .get("written_utc")
        .and_then(Value::as_str)
        .ok_or("missing written_utc")?;
    match operation {
        "build" => build(view, &wants, capacity, utc),
        "merge" => merge(&request, view, &wants, capacity, utc),
        _ => Err("unknown intent operation".into()),
    }
}
