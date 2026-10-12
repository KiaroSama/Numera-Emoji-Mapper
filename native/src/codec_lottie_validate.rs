//! Validation of an already decoded Lottie document; gzip/JSON/file ownership lives elsewhere.
use serde_json::Value;
use std::sync::OnceLock;

fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
        Value::Number(n) if n.to_string().contains(['.', 'e', 'E']) => "float",
        Value::Number(_) => "int",
    }
}
fn float_text(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-inf"
        } else {
            "inf"
        }
        .into();
    }
    let abs = value.abs();
    if abs != 0.0 && !(1e-4..1e16).contains(&abs) {
        let raw = format!("{value:e}");
        let (mantissa, exponent) = raw.split_once('e').expect("scientific format");
        let exponent: i32 = exponent.parse().expect("scientific exponent");
        format!("{mantissa}e{exponent:+03}")
    } else {
        let mut text = value.to_string();
        if !text.contains('.') {
            text.push_str(".0");
        }
        text
    }
}
fn repr(value: &Value, nested: bool) -> String {
    match value {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::String(s) => {
            if nested {
                crate::python_repr::quoted(s)
            } else {
                s.clone()
            }
        }
        Value::Number(n) if kind(value) == "float" => {
            float_text(n.as_f64().unwrap_or(f64::INFINITY))
        }
        Value::Number(n) => n.to_string(),
        Value::Array(a) => format!(
            "[{}]",
            a.iter()
                .map(|v| repr(v, true))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Object(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, v)| format!("{}: {}", crate::python_repr::quoted(k), repr(v, true)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}
// Unlike str.strip(), CPython float() rejects ASCII record separators.
fn whitespace(c: char) -> bool {
    c.is_whitespace()
}
fn decimal(c: char) -> Option<char> {
    static RANGES: OnceLock<Vec<[u32; 2]>> = OnceLock::new();
    let ranges = RANGES.get_or_init(|| {
        let doc: Value = serde_json::from_str(include_str!("../data/python311-decimal.json"))
            .expect("checked decimal table");
        serde_json::from_value(doc["decimal"].clone()).expect("checked decimal ranges")
    });
    let code = c as u32;
    let index = ranges.partition_point(|r| r[1] < code);
    ranges
        .get(index)
        .filter(|r| r[0] <= code)
        .map(|r| char::from(b'0' + ((code - r[0]) % 10) as u8))
}
fn source_float(value: &Value) -> Result<f64, String> {
    match value {
        Value::Bool(v) => Ok(if *v { 1.0 } else { 0.0 }),
        Value::Number(n) => {
            let text = n.to_string();
            let float = text
                .parse::<f64>()
                .map_err(|_| "int too large to convert to float".to_owned())?;
            if !text.contains(['.', 'e', 'E']) && !float.is_finite() {
                Err("int too large to convert to float".into())
            } else {
                Ok(float)
            }
        }
        Value::String(s) => {
            let text: String = s
                .trim_matches(whitespace)
                .chars()
                .map(|c| decimal(c).unwrap_or(c))
                .collect();
            let chars: Vec<char> = text.chars().collect();
            let underscores_valid = chars.iter().enumerate().all(|(i, c)| {
                *c != '_'
                    || (i > 0
                        && i + 1 < chars.len()
                        && chars[i - 1].is_ascii_digit()
                        && chars[i + 1].is_ascii_digit())
            });
            if underscores_valid {
                let cleaned = text.replace('_', "");
                let special = cleaned.to_ascii_lowercase();
                match special.as_str() {
                    "inf" | "+inf" | "infinity" | "+infinity" => return Ok(f64::INFINITY),
                    "-inf" | "-infinity" => return Ok(f64::NEG_INFINITY),
                    "nan" | "+nan" | "-nan" => return Ok(f64::NAN),
                    _ => {}
                }
                if let Ok(v) = cleaned.parse::<f64>() {
                    return Ok(v);
                }
            }
            Err(format!(
                "could not convert string to float: {}",
                crate::python_repr::quoted(s)
            ))
        }
        _ => Err(format!(
            "float() argument must be a string or a real number, not '{}'",
            kind(value)
        )),
    }
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}
fn iterable(value: Option<&Value>) -> Result<Vec<Value>, String> {
    match value.filter(|v| truthy(v)) {
        None => Ok(Vec::new()),
        Some(Value::Array(v)) => Ok(v.clone()),
        Some(Value::Object(v)) => Ok(v.keys().cloned().map(Value::String).collect()),
        Some(Value::String(v)) => Ok(v.chars().map(|c| Value::String(c.to_string())).collect()),
        Some(v) => Err(format!("'{}' object is not iterable", kind(v))),
    }
}

pub fn validate_document(data: &Value, filename: &str, compressed_size: u64) -> Result<(), String> {
    if compressed_size > 65536 {
        return Err(format!("{filename}: TGS {compressed_size} > 65536 bytes"));
    }
    let object = data.as_object().ok_or_else(|| {
        format!(
            "{filename}: Lottie root is {}, expected an object",
            kind(data)
        )
    })?;
    for key in ["v", "fr", "ip", "op", "layers"] {
        if !object.contains_key(key) {
            return Err(format!("{filename}: Lottie missing required key '{key}'"));
        }
    }
    let null = Value::Null;
    let w = object.get("w").unwrap_or(&null);
    let h = object.get("h").unwrap_or(&null);
    let canvas = |v: &Value| match v {
        Value::Number(number) => {
            let text = number.to_string();
            if text.contains(['.', 'e', 'E']) {
                source_float(v) == Ok(512.0)
            } else {
                text == "512"
            }
        }
        _ => false,
    };
    if !canvas(w) || !canvas(h) {
        return Err(format!(
            "{filename}: canvas {}x{}, expected 512x512",
            repr(w, false),
            repr(h, false)
        ));
    }
    let number = |key: &str| {
        source_float(&object[key]).map_err(|e| {
            if e == "int too large to convert to float" {
                e
            } else {
                format!("{filename}: non-numeric fr/ip/op ({e})")
            }
        })
    };
    let fr = number("fr")?;
    let ip = number("ip")?;
    let op = number("op")?;
    let frames = op - ip;
    for (name, value) in [("fr", fr), ("ip", ip), ("op", op)] {
        if !value.is_finite() {
            return Err(format!(
                "{filename}: {name} is {}, not a finite number",
                float_text(value)
            ));
        }
    }
    if fr <= 0.0 {
        return Err(format!(
            "{filename}: frame rate {} must be positive",
            float_text(fr)
        ));
    }
    if fr > 60.0 {
        return Err(format!("{filename}: {} fps > 60 fps", float_text(fr)));
    }
    let duration = frames / fr;
    if duration <= 0.0 {
        return Err(format!(
            "{filename}: empty timeline (ip={}, op={})",
            repr(&object["ip"], false),
            repr(&object["op"], false)
        ));
    }
    if duration > 3.01 {
        return Err(format!("{filename}: {duration:.2}s > 3.0s"));
    }
    let mut layers = iterable(object.get("layers"))?;
    for asset in iterable(object.get("assets"))? {
        if let Some(asset) = asset.as_object() {
            layers.extend(iterable(asset.get("layers"))?);
        }
    }
    for layer in layers {
        let Some(layer) = layer.as_object() else {
            continue;
        };
        for mask in iterable(layer.get("masksProperties"))? {
            let mask = mask
                .as_object()
                .ok_or_else(|| format!("'{}' object has no attribute 'get'", kind(&mask)))?;
            if mask.get("mode") == Some(&Value::String("s".into())) {
                let name = layer
                    .get("nm")
                    .cloned()
                    .unwrap_or_else(|| Value::String("?".into()));
                return Err(format!(
                    "{filename}: layer {} uses a SUBTRACT mask, which Telegram refuses on upload (add masks are fine). Re-export without it.",
                    repr(&name, true)
                ));
            }
        }
    }
    Ok(())
}
