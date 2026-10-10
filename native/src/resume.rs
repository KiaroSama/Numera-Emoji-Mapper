//! Collection resume validation preserves recorded membership and refuses corrupt state.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, String>;
fn nonempty(value: &Value) -> Option<&str> {
    value.as_str().filter(|s| !s.is_empty())
}
fn string_list(value: &Value, field: &str, non_empty: bool) -> Result<Vec<String>> {
    value
        .as_array()
        .ok_or_else(|| format!("{field} must be a list"))?
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| !non_empty || !s.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("{field} must hold string keys"))
        })
        .collect()
}

pub fn collection(mut state: Value, base: &str) -> Result<Value> {
    let doc = state
        .as_object_mut()
        .ok_or("state is not a publish-state object")?;
    if doc.entry("base").or_insert(json!(base)) != base {
        return Err("state belongs to a different base".into());
    }
    for field in ["sets", "sent", "sent_full", "skipped"] {
        if !doc.entry(field).or_insert(json!([])).is_array() {
            return Err(format!("{field} must be a list"));
        }
    }
    for field in ["sent", "skipped"] {
        string_list(&doc[field], field, true)?;
    }
    let mut names = BTreeSet::new();
    let mut all_keys = BTreeSet::new();
    let mut indices = BTreeMap::new();
    for (i, set) in doc
        .get_mut("sets")
        .and_then(Value::as_array_mut)
        .ok_or("sets must be a list")?
        .iter_mut()
        .enumerate()
    {
        let row = set
            .as_object_mut()
            .ok_or_else(|| format!("sets[{i}] is not an object"))?;
        let name = row
            .get("name")
            .and_then(nonempty)
            .ok_or_else(|| format!("sets[{i}] has no name"))?
            .to_owned();
        let fmt = row
            .get("fmt")
            .and_then(Value::as_str)
            .filter(|s| ["static", "video", "animated", "mixed"].contains(s))
            .ok_or_else(|| format!("sets[{i}] has unknown format"))?
            .to_owned();
        let index = row
            .get("index")
            .and_then(Value::as_u64)
            .filter(|n| *n > 0)
            .ok_or_else(|| format!("sets[{i}] has a bad index"))?;
        if !names.insert(name.clone()) {
            return Err(format!("sets[{i}] repeats the set name {name}"));
        }
        if index <= *indices.get(&fmt).unwrap_or(&0) {
            return Err(format!(
                "sets[{i}] index does not follow the recorded format index"
            ));
        }
        indices.insert(fmt, index);
        if nonempty(row.entry("title").or_insert(json!(name))).is_none() {
            return Err(format!("sets[{i}] has a bad title"));
        }
        let logo = row
            .entry("logo")
            .or_insert(json!(false))
            .as_bool()
            .ok_or_else(|| format!("sets[{i}] has a non-boolean logo"))?;
        let keys = string_list(row.entry("keys").or_insert(json!([])), "keys", true)?;
        let unique = keys.iter().collect::<BTreeSet<_>>();
        if unique.len() != keys.len() {
            return Err(format!("sets[{i}] records the same emoji twice"));
        }
        let recorded = unique.len() as u64 + u64::from(logo);
        for k in keys {
            if !all_keys.insert(k) {
                return Err(format!(
                    "sets[{i}] records a key an earlier set already claims"
                ));
            }
        }
        let live = row
            .entry("live")
            .or_insert(json!(0))
            .as_u64()
            .filter(|n| *n <= 200)
            .ok_or_else(|| format!("sets[{i}] live count is outside 0..200"))?;
        if live < recorded {
            return Err(format!(
                "sets[{i}] live count is smaller than recorded membership"
            ));
        }
    }
    Ok(state)
}

pub fn frozen(plan: &Value) -> Result<()> {
    let doc = plan.as_object().ok_or("frozen plan must be an object")?;
    for (fmt, keys) in doc {
        string_list(keys, fmt, false)?;
    }
    Ok(())
}

pub fn freeze(mut plan: Value, items: &[Value], formats: &[String]) -> Result<Value> {
    frozen(&plan)?;
    for fmt in formats {
        let mut existing = plan
            .get(fmt)
            .map(|v| string_list(v, fmt, false))
            .transpose()?
            .unwrap_or_default();
        let have = existing.iter().cloned().collect::<BTreeSet<_>>();
        for item in items {
            if fmt != "mixed" && item.get("fmt").and_then(Value::as_str) != Some(fmt) {
                continue;
            }
            let k = item
                .get("content_key")
                .and_then(Value::as_str)
                .ok_or("item has no content key")?;
            if !have.contains(k) {
                existing.push(k.to_owned());
            }
        }
        plan[fmt] = json!(existing);
    }
    Ok(plan)
}

pub fn execute(request: Value) -> Result<Value> {
    let op = request
        .get("operation")
        .and_then(Value::as_str)
        .ok_or("missing operation")?;
    match op {
        "collection" => collection(
            request.get("state").cloned().ok_or("missing state")?,
            request
                .get("base")
                .and_then(Value::as_str)
                .ok_or("missing base")?,
        ),
        "frozen" => {
            let p = request.get("plan").ok_or("missing plan")?;
            frozen(p)?;
            Ok(p.clone())
        }
        "freeze" => freeze(
            request.get("plan").cloned().ok_or("missing plan")?,
            request
                .get("items")
                .and_then(Value::as_array)
                .ok_or("missing items")?,
            &string_list(
                request.get("formats").ok_or("missing formats")?,
                "formats",
                false,
            )?,
        ),
        _ => Err("unknown resume operation".into()),
    }
}
