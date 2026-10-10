//! Panel plans remap structured keys; publisher documents retain the original recursive remap.
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
pub fn state_name(name: &str) -> bool {
    !name.contains(['/', '\\'])
        && (name == "pack_plan.json" || name.starts_with("publish_") && name.ends_with(".json"))
}
pub fn files(data: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(data).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(state_name)
        {
            if path.is_symlink() || !path.is_file() {
                return Err("unsupported application state path".into());
            }
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}
fn key(value: &mut Value, map: &BTreeMap<String, String>) -> Result<(), String> {
    let old = value.as_str().ok_or("state key must be a string")?;
    if let Some(new) = map.get(old) {
        *value = Value::String(new.clone());
    }
    Ok(())
}
fn keys(value: &mut Value, map: &BTreeMap<String, String>) -> Result<(), String> {
    for key_value in value.as_array_mut().ok_or("state keys must be a list")? {
        key(key_value, map)?;
    }
    Ok(())
}
fn remap(value: &mut Value, map: &BTreeMap<String, String>) {
    match value {
        Value::String(text) => {
            if let Some(new) = map.get(text) {
                *text = new.clone();
            }
        }
        Value::Array(values) => {
            for value in values {
                remap(value, map);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                remap(value, map);
            }
        }
        _ => {}
    }
}

pub fn rewrite(
    name: &str,
    document: &Value,
    map: &BTreeMap<String, String>,
) -> Result<Value, String> {
    if !state_name(name) || !document.is_object() {
        return Err("unsupported state filename/schema".into());
    }
    let mut out = document.clone();
    if name == "pack_plan.json" {
        if out["version"] != 1 {
            return Err("pack_plan.json has an unsupported schema".into());
        }
        for field in ["moves", "held"] {
            for row in out[field]
                .as_array_mut()
                .ok_or("pack plan records must be a list")?
            {
                key(&mut row["key"], map)?;
            }
        }
        if let Some(targets) = out.get_mut("targets") {
            for row in targets
                .as_array_mut()
                .ok_or("pack plan targets must be a list")?
            {
                let pair = row
                    .as_array_mut()
                    .filter(|p| p.len() == 2)
                    .ok_or("pack plan target must be a key/pack pair")?;
                key(&mut pair[0], map)?;
            }
        }
        for field in ["known", "excluded"] {
            if let Some(value) = out.get_mut(field) {
                keys(value, map)?;
            }
        }
    } else {
        remap(&mut out, map);
    }
    Ok(out)
}
