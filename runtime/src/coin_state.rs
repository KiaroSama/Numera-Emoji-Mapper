//! Frozen rebuild cursor/order/intent validation: corrupt bookkeeping never drives a mutation.
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn count(value: &Value) -> Option<u64> {
    value.as_u64()
}
fn quoted(value: &Value) -> String {
    match value {
        Value::String(text) => crate::python_repr::quoted(text),
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        _ => value.to_string(),
    }
}
fn names(values: &[Value]) -> String {
    format!(
        "[{}]",
        values.iter().map(quoted).collect::<Vec<_>>().join(", ")
    )
}

pub fn problem(state: &Value, plan: Option<&[Value]>) -> String {
    if !state.is_object() {
        return "state is not an object".into();
    }
    let empty = json!([]);
    for field in [
        "sets",
        "sent",
        "order",
        "deleted_old_packs",
        "provider_added",
    ] {
        if !state.get(field).unwrap_or(&empty).is_array() {
            return format!("{field:?} must be a list").replace('"', "'");
        }
    }
    for field in ["deleted_old", "final_sent"] {
        if state.get(field).is_some_and(|value| !value.is_boolean()) {
            return format!("'{field}' must be true or false");
        }
    }
    for field in ["sent", "order", "deleted_old_packs", "provider_added"] {
        if state
            .get(field)
            .unwrap_or(&empty)
            .as_array()
            .is_some_and(|values| {
                values
                    .iter()
                    .any(|value| value.as_str().is_none_or(str::is_empty))
            })
        {
            return format!("'{field}' must hold non-empty names");
        }
    }
    let duplicates = |field: &str| -> Vec<Value> {
        let mut seen = BTreeSet::new();
        let mut repeated = BTreeSet::new();
        for value in state
            .get(field)
            .unwrap_or(&empty)
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(text) = value.as_str()
                && !seen.insert(text)
            {
                repeated.insert(text);
            }
        }
        repeated.into_iter().map(|text| json!(text)).collect()
    };
    let repeated = duplicates("provider_added");
    if !repeated.is_empty() {
        return format!("'provider_added' repeats {}", names(&repeated));
    }
    let repeated = duplicates("order");
    if !repeated.is_empty() {
        return format!(
            "'order' records {} more than once",
            names(&repeated[..repeated.len().min(5)])
        );
    }
    let sets = state
        .get("sets")
        .unwrap_or(&empty)
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut seen_names = BTreeSet::new();
    let mut last = 0;
    for (index, entry) in sets.iter().enumerate() {
        if !entry.is_object() {
            return format!("sets[{index}] is not an object");
        }
        let Some(name) = entry["name"].as_str().filter(|name| !name.is_empty()) else {
            return format!("sets[{index}] has no name");
        };
        if !seen_names.insert(name) {
            return format!(
                "sets[{index}] repeats the set name {}",
                crate::python_repr::quoted(name)
            );
        }
        let Some(number) = count(&entry["index"]).filter(|number| *number >= 1) else {
            return format!("sets[{index}] has a bad index {}", quoted(&entry["index"]));
        };
        if number <= last {
            return format!("sets[{index}] index {number} does not ascend");
        }
        let live = entry.get("live").unwrap_or(&Value::Null);
        if entry.get("live").is_some() && count(live).is_none_or(|value| value > 200) {
            return format!("sets[{index}] live count {} outside 0..200", quoted(live));
        }
        last = number;
    }
    let default_cursor = json!(0);
    let cursor_value = state.get("cursor").unwrap_or(&default_cursor);
    let Some(cursor) = count(cursor_value) else {
        return format!("cursor {} is not a plan position", quoted(cursor_value));
    };
    let order = state
        .get("order")
        .unwrap_or(&empty)
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if let Some(plan) = plan {
        if cursor > plan.len() as u64 {
            return format!(
                "cursor {cursor} is past the end of the {}-entry plan",
                plan.len()
            );
        }
        let mut position = 0;
        for rep in order {
            let mut found = false;
            while position < cursor as usize {
                let current = &plan[position]["rep"];
                position += 1;
                if current == rep {
                    found = true;
                    break;
                }
            }
            if !found {
                return format!(
                    "'order' records {}, which is not among the first {cursor} plan entries the cursor claims were processed, in that order",
                    quoted(rep)
                );
            }
        }
    }
    if !order.is_empty() && sets.is_empty() {
        return format!(
            "{} upload(s) are recorded but no set holds them",
            order.len()
        );
    }
    let marker = &state["in_flight"];
    if marker.is_null() {
        return String::new();
    }
    if let Some(text) = marker.as_str() {
        let repr = crate::python_repr::quoted(text);
        return format!(
            "in_flight is the legacy bare key {repr}, which names no set to probe. Check by hand whether {text}.png is live in the newest pack, then repair the state: if it IS live, set 'in_flight' to null AND append {repr} to 'order'; if it is NOT live, set 'in_flight' to null AND step 'cursor' back by one so it is retried"
        );
    }
    if !marker.is_object() {
        return "'in_flight' must be an intent object or null".into();
    }
    for field in ["key", "operation", "set_name"] {
        if marker[field].as_str().is_none_or(str::is_empty) {
            return format!("in_flight is missing '{field}'");
        }
    }
    if !matches!(marker["operation"].as_str(), Some("add" | "create")) {
        return format!(
            "in_flight operation {} is unknown",
            quoted(&marker["operation"])
        );
    }
    if count(&marker["set_index"]).is_none() {
        return format!(
            "in_flight set_index {} is not a set number",
            quoted(&marker["set_index"])
        );
    }
    if !marker["expected_before"].is_null() && count(&marker["expected_before"]).is_none() {
        return format!(
            "in_flight expected_before {} is not a count",
            quoted(&marker["expected_before"])
        );
    }
    if let Some(plan) = plan
        && (cursor == 0 || plan[cursor as usize - 1]["rep"] != marker["key"])
    {
        return format!(
            "in_flight {} is not plan entry {cursor}, the one the cursor was moved past to run it",
            quoted(&marker["key"])
        );
    }
    String::new()
}
