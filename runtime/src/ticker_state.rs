//! Legacy ticker builder state is a separate contract from collection content-key state.
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub fn validate(state: &Value, base: &str, cap: u64) -> Result<(), String> {
    let object = state.as_object().ok_or("state is not an object")?;
    if state["base"] != base {
        return Err("state belongs to another base".into());
    }
    for field in ["done", "sent"] {
        if let Some(value) = object.get(field)
            && !value
                .as_array()
                .is_some_and(|v| v.iter().all(|s| s.is_string()))
        {
            return Err(format!("'{field}' must be a list of item keys"));
        }
    }
    let mut indexes = BTreeSet::new();
    let mut last = 0;
    let sets = object
        .get("sets")
        .and_then(Value::as_array)
        .ok_or("'sets' must be a list")?;
    for set in sets {
        let name = set["name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("recorded set has no name")?;
        let n = set["index"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or("recorded set has bad index")?;
        if !indexes.insert(n) || n < last {
            return Err("recorded set indexes repeat or go backwards".into());
        }
        last = n;
        if !set["count"].as_u64().is_some_and(|n| n <= cap) {
            return Err(format!("{name} count outside0..{cap}"));
        }
    }
    if let Some(intent) = object.get("in_flight").filter(|v| !v.is_null()) {
        if !intent.is_object() && !intent.is_string() {
            return Err("'in_flight' must be an intent object".into());
        }
        if intent.is_object() {
            for field in ["key", "operation", "set_name"] {
                if !intent[field].as_str().is_some_and(|s| !s.is_empty()) {
                    return Err(format!("in_flight has no {field}"));
                }
            }
            if !intent["operation"]
                .as_str()
                .is_some_and(|s| ["add", "create"].contains(&s))
            {
                return Err("in_flight operation is unknown".into());
            }
            if !intent["set_index"].as_u64().is_some_and(|n| n > 0)
                || !intent["expected_before"].as_u64().is_some_and(|n| n <= cap)
            {
                return Err("in_flight has bad index or expected count".into());
            }
        }
    }
    Ok(())
}
pub fn intent(key: &str, name: &str, index: u64, before: u64) -> Value {
    json!({"key":key,"operation":if before==0{"create"}else{"add"},"set_name":name,"set_index":index,"expected_before":before,"title":"","format":"static","started_utc":crate::logging::iso_utc()})
}
pub fn initial(base: &str, cap: u64) -> Value {
    json!({"base":base,"per_set":cap,"done":[],"sets":[],"in_flight":null,"sent":[]})
}
