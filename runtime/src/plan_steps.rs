//! Plan execution preview. Held-but-live items retain their slots until placed.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, String>;
fn keys(v: Option<&Value>) -> Result<Vec<String>> {
    match v {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(v) => v
            .as_array()
            .ok_or("expected key list")?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "invalid key".into())
            })
            .collect(),
    }
}
fn index(v: &Value) -> Result<u64> {
    v.as_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| "invalid pack index".into())
}

pub fn render(steps: &Value, ids: &Value, max_changes: u64) -> Result<String> {
    if max_changes == 0 {
        return Err("max changes must be positive".into());
    }
    let removals = steps["removals"].as_array().ok_or("invalid removals")?;
    let held = steps["held_live"].as_array().ok_or("invalid held rows")?;
    let adds = steps["adds"].as_object().ok_or("invalid adds")?;
    let reorder = steps["reorder"].as_array().ok_or("invalid reorder")?;
    let moved = keys(steps.get("moved_in"))?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut packs = BTreeSet::new();
    for row in removals.iter().chain(held) {
        packs.insert(index(&row[1])?);
    }
    for n in adds.keys() {
        packs.insert(n.parse::<u64>().map_err(|_| "invalid add pack")?);
    }
    for n in reorder {
        packs.insert(index(n)?);
    }
    let mut lines = Vec::new();
    for n in packs {
        let outs = removals
            .iter()
            .filter(|row| row[1] == n)
            .map(|row| {
                row[0]
                    .as_str()
                    .ok_or_else(|| "invalid removal key".to_owned())
            })
            .collect::<Result<Vec<_>>>()?;
        let ins = keys(adds.get(&n.to_string()))?;
        let moving = ins.iter().filter(|key| moved.contains(*key)).count();
        lines.push(format!(
            "pack {n}{}",
            if steps["create"] == n {
                "  (NEW: created by this plan)"
            } else {
                ""
            }
        ));
        if !outs.is_empty() {
            lines.push(format!("  remove {}: {}", outs.len(), outs.join(", ")));
        }
        if !ins.is_empty() {
            lines.push(format!(
                "  add {} ({moving} moved in, {} new): {}",
                ins.len(),
                ins.len() - moving,
                ins.join(", ")
            ));
        }
        if reorder.iter().any(|pack| *pack == n) {
            lines.push("  reorder to the panel's order (ids kept)".into());
        }
        let kept = held
            .iter()
            .filter(|row| row[1] == n)
            .map(|row| row[0].as_str().ok_or_else(|| "invalid held key".to_owned()))
            .collect::<Result<Vec<_>>>()?;
        if !kept.is_empty() {
            lines.push(format!(
                "  held, stays live until placed: {}",
                kept.join(", ")
            ));
        }
    }
    if !removals.is_empty() {
        lines.push(String::new());
        lines.push("custom_emoji_ids these moves RETIRE (the re-add mints a new id):".into());
        for row in removals {
            let key = row[0].as_str().ok_or("invalid removal key")?;
            lines.push(format!("  {}  {key}", ids[key].as_str().unwrap_or("?")));
        }
    }
    let skipped = keys(steps.get("unpublishable"))?;
    if !skipped.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "the publisher refuses these files (state 'skipped'); not counted: {}",
            skipped.join(", ")
        ));
    }
    lines.push(String::new());
    let counted = steps["counted"].as_u64().ok_or("invalid counted changes")?;
    lines.push(if steps["pending"] == true {
        format!(
            "{counted} counted change(s): {} run(s) at {max_changes} per run{}",
            counted.div_ceil(max_changes),
            if reorder.is_empty() {
                ""
            } else {
                ", then a reorder"
            }
        )
    } else {
        "nothing pending: the live packs match the plan".into()
    });
    Ok(lines.join("\n"))
}

pub fn compute(request: &Value) -> Result<Value> {
    let plan = request.get("plan").ok_or("missing plan")?;
    let state = request.get("state").ok_or("missing state")?;
    let items = request
        .get("items")
        .and_then(Value::as_object)
        .ok_or("missing items")?;
    let ids = request
        .get("ids")
        .and_then(Value::as_object)
        .ok_or("missing ids")?;
    let sets = state
        .get("sets")
        .and_then(Value::as_array)
        .ok_or("missing sets")?;
    let mut by_index = BTreeMap::new();
    let mut live = BTreeMap::new();
    for set in sets {
        if set.get("fmt").and_then(Value::as_str) != Some("mixed") {
            return Err("applier only handles --mixed families".into());
        }
        let n = index(set.get("index").ok_or("missing index")?)?;
        if by_index.insert(n, set).is_some() {
            return Err("duplicate set index".into());
        }
        for k in keys(set.get("keys"))? {
            live.insert(k, n);
        }
    }
    if plan
        .get("over_capacity")
        .and_then(Value::as_object)
        .is_some_and(|v| !v.is_empty())
    {
        return Err("plan overfills packs; fix it in the panel and save again".into());
    }
    let capacity = plan
        .get("per_set")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
        .unwrap_or(200)
        .min(200);
    let mut excluded = keys(plan.get("excluded"))?
        .into_iter()
        .collect::<BTreeSet<_>>();
    if let Some(held) = plan.get("held").and_then(Value::as_array) {
        for row in held {
            excluded.insert(
                row.get("key")
                    .and_then(Value::as_str)
                    .ok_or("invalid held key")?
                    .to_owned(),
            );
        }
    }
    let skipped = keys(state.get("skipped"))?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let pos = |k: &String| {
        items
            .get(k)
            .and_then(|v| v.get("pos"))
            .and_then(Value::as_u64)
            .unwrap_or(items.len() as u64)
    };
    let mut removals = Vec::new();
    let mut adds: BTreeMap<u64, Vec<String>> = BTreeMap::new();
    let mut moved = BTreeSet::new();
    let mut unpublishable = Vec::new();
    for (k, n) in crate::intent::targets(plan)? {
        if excluded.contains(&k) {
            continue;
        }
        let item = items
            .get(&k)
            .ok_or_else(|| format!("plan names missing catalog key {k}"))?;
        if item.get("included").and_then(Value::as_bool) != Some(true) {
            return Err(format!("plan puts excluded catalog key {k} in pack {n}"));
        }
        let src = live.get(&k).copied();
        if src == Some(n) {
            continue;
        }
        if src.is_none() && skipped.contains(&k) {
            unpublishable.push(k);
            continue;
        }
        if let Some(src) = src {
            if !ids
                .get(&k)
                .and_then(Value::as_str)
                .is_some_and(|v| !v.is_empty())
            {
                return Err(format!(
                    "{k} is live in pack {src} but its custom_emoji_id is not recorded, so its sticker cannot be found by id"
                ));
            }
            removals.push((k.clone(), src));
            moved.insert(k.clone());
        }
        adds.entry(n).or_default().push(k);
    }
    let top = by_index.keys().next_back().copied().unwrap_or(0);
    let new = adds
        .keys()
        .filter(|n| !by_index.contains_key(n))
        .copied()
        .collect::<Vec<_>>();
    let create = if new.is_empty() {
        None
    } else {
        if new != vec![top + 1] {
            return Err(format!(
                "the plan targets pack(s) {new:?}, but only pack {} can be created next (packs are numbered without gaps)",
                top + 1
            ));
        }
        if sets.is_empty() {
            return Err("family has no pack; start it with build_collection --mixed".into());
        }
        Some(top + 1)
    };
    let create_logo = if let Some(n) = create {
        plan.get("logo_slots")
            .and_then(|v| v.get(n.to_string()))
            .and_then(Value::as_u64)
            .map(|v| v != 0)
            .unwrap_or_else(|| {
                by_index
                    .values()
                    .next_back()
                    .and_then(|v| v.get("logo"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
    } else {
        false
    };
    for list in adds.values_mut() {
        list.sort_by_key(pos)
    }
    let out = removals
        .iter()
        .map(|(k, _)| k.clone())
        .collect::<BTreeSet<_>>();
    let mut reorder = Vec::new();
    for n in by_index
        .keys()
        .chain(adds.keys())
        .copied()
        .collect::<BTreeSet<_>>()
    {
        let rec = by_index.get(&n);
        let mut final_keys = keys(rec.and_then(|r| r.get("keys")))?
            .into_iter()
            .filter(|k| !out.contains(k))
            .collect::<Vec<_>>();
        if let Some(added) = adds.get(&n) {
            final_keys.extend(added.clone())
        }
        let logo = rec
            .and_then(|r| r.get("logo"))
            .and_then(Value::as_bool)
            .unwrap_or(create_logo);
        if final_keys.len() as u64 + u64::from(logo) > capacity {
            return Err(format!(
                "pack {n} would hold {} (logo included), over the cap of {capacity}",
                final_keys.len() as u64 + u64::from(logo)
            ));
        }
        let mut ordered = final_keys.clone();
        ordered.sort_by_key(pos);
        if rec.is_some() && ordered != final_keys {
            reorder.push(n)
        }
    }
    removals.sort_by_key(|(k, n)| (*n, pos(k)));
    let mut held = excluded
        .iter()
        .filter_map(|k| live.get(k).map(|n| (k.clone(), *n)))
        .collect::<Vec<_>>();
    held.sort_by_key(|(k, n)| (*n, pos(k)));
    let counted = removals.len()
        + adds.values().map(Vec::len).sum::<usize>()
        + usize::from(create.is_some() && create_logo);
    Ok(
        json!({"removals":removals,"adds":adds,"moved_in":moved,"create":create,"create_logo":create_logo,
        "reorder":reorder,"held_live":held,"unpublishable":unpublishable,"counted":counted,"pending":counted!=0||!reorder.is_empty()}),
    )
}
