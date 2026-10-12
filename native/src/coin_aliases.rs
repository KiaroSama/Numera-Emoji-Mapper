//! Coin-name aliases: one normalized name may resolve only one distinct existing emoji ID.
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, PartialEq, Eq)]
pub struct AliasError {
    pub kind: &'static str,
    pub message: String,
}

fn error(kind: &'static str, message: impl Into<String>) -> AliasError {
    AliasError {
        kind,
        message: message.into(),
    }
}

pub fn normalize_name(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut stripped = String::with_capacity(lower.len());
    let mut rest = lower.as_str();
    // Python's non-DOTALL, non-greedy pattern ends at the first ')' on the same line.
    while let Some(start) = rest.find('(') {
        stripped.push_str(&rest[..start]);
        let suffix = &rest[start + 1..];
        let line = suffix.split('\n').next().unwrap_or("");
        if let Some(end) = line.find(')') {
            stripped.push(' ');
            rest = &suffix[end + 1..];
        } else {
            stripped.push('(');
            rest = suffix;
        }
    }
    stripped.push_str(rest);
    stripped
        .split(|c: char| !c.is_ascii_lowercase() && !c.is_ascii_digit())
        .filter(|word| {
            !matches!(
                *word,
                "network"
                    | "token"
                    | "protocol"
                    | "finance"
                    | "labs"
                    | "the"
                    | "bridged"
                    | "new"
                    | "coin"
                    | "inc"
                    | "io"
                    | "governance"
                    | "chain"
                    | "erc20"
                    | "bep20"
                    | "trc20"
                    | "usd"
            )
        })
        .collect()
}

fn python_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.to_string().contains(['.', 'e', 'E']) => "float",
        Value::Number(_) => "int",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}

fn string_lower(value: &Value) -> Result<String, AliasError> {
    value.as_str().map(str::to_lowercase).ok_or_else(|| {
        error(
            "AttributeError",
            format!("'{}' object has no attribute 'lower'", python_type(value)),
        )
    })
}

fn hashable(value: &Value) -> Result<(), AliasError> {
    if matches!(value, Value::Array(_) | Value::Object(_)) {
        Err(error(
            "TypeError",
            format!("unhashable type: '{}'", python_type(value)),
        ))
    } else {
        Ok(())
    }
}

fn numeric_id(value: &Value) -> Option<String> {
    match value {
        Value::Bool(value) => Some(if *value { "1" } else { "0" }.into()),
        Value::Number(number) => {
            let text = number.to_string();
            if !text.contains(['.', 'e', 'E']) {
                return Some(if text == "-0" { "0".into() } else { text });
            }
            let float = text.parse::<f64>().ok()?;
            if float == 0.0 {
                Some("0".into())
            } else if float.is_finite() && float.fract() == 0.0 {
                // Compare the float's actual integer, not a rounded conversion of the int.
                Some(format!("{float:.0}"))
            } else {
                Some(format!("float:{:016x}", float.to_bits()))
            }
        }
        _ => None,
    }
}

fn same_id(a: &Value, b: &Value) -> bool {
    // Python sets equate bool/int/float numerically without rounding large integers.
    match (numeric_id(a), numeric_id(b)) {
        (Some(a), Some(b)) => a == b,
        (None, None) => a == b,
        _ => false,
    }
}

pub fn apply_aliases(
    mapping: &Map<String, Value>,
    rows: &[Value],
    missing: &[(String, String)],
) -> Result<Value, AliasError> {
    let mut groups: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in rows {
        let row = row
            .as_object()
            .ok_or_else(|| error("ValueError", "keyword row must be an object"))?;
        let ticker = string_lower(
            row.get("ticker")
                .ok_or_else(|| error("KeyError", "'ticker'"))?,
        )?;
        if mapping.contains_key(&ticker) {
            let name = row.get("name").filter(|v| truthy(v));
            let key = match name {
                Some(value) => normalize_name(&string_lower(value)?),
                None => String::new(),
            };
            if !key.is_empty() {
                groups.entry(key).or_default().insert(ticker);
            }
        }
    }
    let mut out = mapping.clone();
    let mut messages = Vec::new();
    let mut added = 0usize;
    let mut ambiguous = 0usize;
    // Groups use the initial map, but repeated inventory entries observe earlier alias writes.
    for (name, ticker) in missing {
        let key = normalize_name(name);
        let Some(logos) = groups.get(&key) else {
            continue;
        };
        let mut ids = Vec::new();
        for logo in logos {
            let id = &out[logo];
            hashable(id)?;
            if !ids.iter().any(|old| same_id(old, id)) {
                ids.push(id.clone());
            }
        }
        if ids.len() == 1 {
            let logo = logos
                .first()
                .expect("groups never contain an empty logo set");
            let id = out[logo].clone();
            out.insert(ticker.clone(), id);
            added += 1;
            messages.push(format!("  alias {ticker} -> {logo} ({name})"));
        } else if !ids.is_empty() {
            ambiguous += 1;
            messages.push(format!("  AMBIGUOUS {ticker} ({name}): '{key}' matches {} -> left unmapped, resolve by hand", logos.iter().cloned().collect::<Vec<_>>().join(", ")));
        }
    }
    Ok(json!({"mapping":out,"added":added,"ambiguous":ambiguous,"messages":messages}))
}

/// JSON boundary used by the command adapter and pinned source fixtures; no filesystem mutation.
pub fn apply_json(request: &Value) -> Result<Value, AliasError> {
    let mapping = request
        .get("mapping")
        .and_then(Value::as_object)
        .ok_or_else(|| error("ValueError", "mapping must be an object"))?;
    let rows = request
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| error("ValueError", "rows must be an array"))?;
    let entries = request
        .get("missing")
        .and_then(Value::as_array)
        .ok_or_else(|| error("ValueError", "missing must be an array"))?;
    let mut missing = Vec::with_capacity(entries.len());
    for entry in entries {
        let pair = entry
            .as_array()
            .filter(|v| v.len() == 2)
            .ok_or_else(|| error("ValueError", "missing entry must contain name and ticker"))?;
        let name = pair[0]
            .as_str()
            .ok_or_else(|| error("ValueError", "missing name must be text"))?;
        let ticker = pair[1]
            .as_str()
            .ok_or_else(|| error("ValueError", "missing ticker must be text"))?;
        missing.push((name.to_owned(), ticker.to_owned()));
    }
    apply_aliases(mapping, rows, &missing)
}
