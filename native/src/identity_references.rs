//! Reference verification keeps obsolete frozen/skipped history apart from live required keys.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
fn collect(value: &Value, keys: &mut BTreeSet<String>) {
    match value {
        Value::String(s)
            if s.len() > 2 && s.as_bytes()[1] == b':' && b"sva".contains(&s.as_bytes()[0]) =>
        {
            keys.insert(s.clone());
        }
        Value::Array(values) => {
            for value in values {
                collect(value, keys);
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                collect(value, keys);
            }
        }
        _ => {}
    }
}
pub fn stale(data: &Path, required: bool) -> Result<BTreeMap<String, Vec<String>>, String> {
    let known = crate::identity_survey::all_keys(&crate::identity_survey::read(data)?)?;
    let mut out = BTreeMap::new();
    for path in crate::state_artifacts::files(data)? {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("state filename not UTF-8")?;
        if required && name.starts_with("publish_plan_") {
            continue;
        }
        let doc = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
        let Some(mut doc) = doc else {
            out.insert(name.into(), vec!["<unreadable>".into()]);
            continue;
        };
        if !doc.is_object() {
            out.insert(name.into(), vec!["<invalid state>".into()]);
            continue;
        }
        let mut keys = BTreeSet::new();
        if name == "pack_plan.json" {
            let empty = BTreeMap::new();
            let plan = crate::state_artifacts::rewrite(name, &doc, &empty)?;
            for field in ["moves", "held"] {
                for row in plan[field].as_array().into_iter().flatten() {
                    if let Some(key) = row["key"].as_str() {
                        keys.insert(key.into());
                    }
                }
            }
            for pair in plan["targets"].as_array().into_iter().flatten() {
                if let Some(key) = pair[0].as_str() {
                    keys.insert(key.into());
                }
            }
            for field in ["known", "excluded"] {
                for key in plan[field].as_array().into_iter().flatten() {
                    if let Some(key) = key.as_str() {
                        keys.insert(key.into());
                    }
                }
            }
        } else {
            if required {
                doc.as_object_mut()
                    .ok_or("invalid state")?
                    .remove("skipped");
            }
            collect(&doc, &mut keys);
        }
        let missing = keys.difference(&known).cloned().collect::<Vec<_>>();
        if !missing.is_empty() {
            out.insert(name.into(), missing);
        }
    }
    Ok(out)
}
