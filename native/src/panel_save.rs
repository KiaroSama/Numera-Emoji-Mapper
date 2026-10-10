//! Scoped curation saves under one catalog lease. The panel never mutates Telegram.
use crate::{atomic, catalog::Catalog, intent, panel_view};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

type Result<T> = std::result::Result<T, String>;
fn list(payload: &Value, field: &str, required: bool) -> Result<BTreeSet<String>> {
    let Some(value) = payload.get(field) else {
        return if required {
            Err(format!("missing {field}"))
        } else {
            Ok(BTreeSet::new())
        };
    };
    value
        .as_array()
        .ok_or_else(|| format!("{field} must be a list of keys"))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{field} must be a list of keys"))
        })
        .collect()
}
fn response(code: u16, error: &str) -> Value {
    json!({"status":code,"body":{"error":error}})
}

pub fn save(
    cat: &mut Catalog,
    payload: &Value,
    view_options: &Value,
    plan_path: &Path,
    utc: &str,
) -> Result<Value> {
    let object = payload.as_object().ok_or("expected a JSON object")?;
    if object
        .keys()
        .any(|k| !["excluded", "known", "packs"].contains(&k.as_str()))
    {
        return Ok(response(400, "unknown keys"));
    }
    let excluded = match list(payload, "excluded", false) {
        Ok(v) => v,
        Err(e) => return Ok(response(400, &e)),
    };
    let scope = match list(payload, "known", true) {
        Ok(v) => v,
        Err(_) => {
            return Ok(response(
                409,
                "this page is from an older panel run and cannot say which emoji it was showing; reload the page and save again",
            ));
        }
    };
    let mut targets = BTreeMap::new();
    if let Some(pairs) = payload.get("packs") {
        let Some(pairs) = pairs.as_array() else {
            return Ok(response(400, "packs must be [key, pack] pairs"));
        };
        for pair in pairs {
            let Some(p) = pair.as_array().filter(|p| p.len() == 2) else {
                return Ok(response(400, "packs must be [key, pack] pairs"));
            };
            let Some(k) = p[0].as_str() else {
                return Ok(response(400, "packs must be [key, pack] pairs"));
            };
            let Some(n) = p[1].as_u64().filter(|n| *n > 0 && *n <= 9007199254740991) else {
                return Ok(response(400, "packs must be [key, pack] pairs"));
            };
            if !scope.contains(k) || targets.insert(k.to_owned(), n).is_some() {
                return Ok(response(
                    400,
                    "pack targets must be unique and inside known scope",
                ));
            }
        }
    }
    if !excluded.is_subset(&scope) {
        return Ok(response(400, "excluded keys must be inside known scope"));
    }
    let items = cat.all(None)?;
    let current = items
        .iter()
        .filter_map(|v| v.get("content_key").and_then(Value::as_str))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let mut options = view_options.clone();
    options["items"] = json!(items);
    options["published"] = cat.dispatch(json!({"operation":"published_keys"}))?;
    options["published_sets"] = cat.dispatch(json!({"operation":"published_sets"}))?;
    let mut fresh = panel_view::build(options)?;
    let view = fresh
        .get_mut("view")
        .and_then(Value::as_array_mut)
        .ok_or("invalid panel view")?;
    let visible = view
        .iter()
        .filter(|v| v.get("isLogo") != Some(&Value::Bool(true)))
        .filter_map(|v| v.get("key").and_then(Value::as_str))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if !scope.is_subset(&current) || !scope.is_subset(&visible) {
        return Ok(response(
            409,
            "catalog identities or visibility changed; export the draft and reload",
        ));
    }
    let previous = read_plan(plan_path)?;
    for card in view.iter_mut() {
        let key = card
            .get("key")
            .and_then(Value::as_str)
            .ok_or("card has no key")?;
        if scope.contains(key) {
            card["included"] = json!(!excluded.contains(key));
        }
    }
    if !object.contains_key("packs") {
        targets = intent::targets(&previous)?
            .into_iter()
            .filter(|(k, _)| scope.contains(k))
            .collect();
    }
    let plan = if object.contains_key("packs") || !previous.is_null() {
        Some(intent::execute(
            json!({"operation":"merge","previous":previous,"view":view,"targets":targets,
            "scope":scope,"per_set":200,"live":current,"written_utc":utc}),
        )?)
    } else {
        None
    };
    let mut all_excluded = excluded;
    for item in &items {
        let k = item
            .get("content_key")
            .and_then(Value::as_str)
            .ok_or("item has no key")?;
        if item.get("included") == Some(&Value::Bool(false)) && !scope.contains(k) {
            all_excluded.insert(k.to_owned());
        }
    }
    let counts = cat.inclusion(&all_excluded.into_iter().collect::<Vec<_>>())?;
    let mut body = json!({"ok":true,"included":counts[0],"excluded":counts[1]});
    if let Some(plan) = plan {
        atomic::write_json(plan_path, &plan).map_err(|e| e.to_string())?;
        body["moves"] = json!(plan["moves"].as_array().ok_or("invalid moves")?.len());
        body["held"] = json!(plan["held"].as_array().ok_or("invalid held")?.len());
    }
    Ok(json!({"status":200,"body":body,"view":fresh}))
}

pub fn read_plan(path: &Path) -> Result<Value> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Value::Null),
        Err(e) => return Err(e.to_string()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("unsupported plan path; preserve and repair it".into());
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let doc: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("cannot read plan; preserve and repair it: {e}"))?;
    if doc.get("version").and_then(Value::as_u64) != Some(1) {
        return Err("unsupported plan schema".into());
    }
    for field in ["moves", "held"] {
        for row in doc
            .get(field)
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{field} must be a list"))?
        {
            if row.get("key").and_then(Value::as_str).is_none() {
                return Err(format!("{field} contains an invalid key"));
            }
        }
    }
    for field in ["known", "excluded"] {
        list(&doc, field, false)?;
    }
    intent::targets(&doc)?;
    Ok(doc)
}
