//! Pure Lottie recolouring; original alpha and gradient opacity entries remain untouched.
use serde_json::{Value, json};

pub fn parse_tint(text: &str) -> Result<[u8; 3], String> {
    // CPython also strips these four ASCII record separators, unlike Rust's whitespace set.
    let value = text
        .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
        .trim_start_matches('#');
    if value.len() != 6 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "tint must be #RRGGBB, got {}",
            crate::python_repr::quoted(text)
        ));
    }
    let mut rgb = [0; 3];
    for (i, channel) in rgb.iter_mut().enumerate() {
        *channel = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)
            .expect("six validated ASCII hex digits");
    }
    Ok(rgb)
}

pub fn numeric(value: &Value) -> bool {
    value.as_array().is_some_and(|values| {
        values
            .iter()
            .all(|v| matches!(v, Value::Number(_) | Value::Bool(_)))
    })
}

fn recolour(values: &mut [Value], rgb: [f64; 3]) {
    for (slot, colour) in values.iter_mut().take(3).zip(rgb) {
        *slot = json!(colour);
    }
}

fn stop_count(value: Option<&Value>, length: usize) -> usize {
    let fallback = || {
        if length.is_multiple_of(4) {
            length / 4
        } else {
            0
        }
    };
    match value {
        Some(Value::Bool(v)) => usize::from(*v),
        Some(Value::Number(v)) => {
            let text = v.to_string();
            if text.contains(['.', 'e', 'E']) || (text.starts_with('-') && text != "-0") {
                fallback()
            } else {
                // The source's arbitrary integer is eventually clamped to the existing array.
                if text == "-0" {
                    0
                } else {
                    text.parse::<usize>().unwrap_or(length / 4)
                }
            }
        }
        _ => fallback(),
    }
}

pub fn tint_gradient(gradient: &mut Value, rgb: [f64; 3]) {
    let Some(gradient) = gradient.as_object_mut() else {
        return;
    };
    let declared = gradient.get("p").cloned();
    let Some(stops) = gradient
        .get_mut("k")
        .and_then(Value::as_object_mut)
        .and_then(|k| k.get_mut("k"))
    else {
        return;
    };
    if !numeric(stops) {
        return;
    }
    let stops = stops
        .as_array_mut()
        .expect("numeric checks the array shape");
    let count = stop_count(declared.as_ref(), stops.len()).min(stops.len() / 4);
    for stop in stops[..count * 4].as_chunks_mut::<4>().0 {
        recolour(&mut stop[1..4], rgb);
    }
}

pub fn tint_lottie(node: &mut Value, rgb: [f64; 3]) {
    match node {
        Value::Object(object) => {
            if let Some(values) = object
                .get_mut("c")
                .and_then(Value::as_object_mut)
                .and_then(|colour| colour.get_mut("k"))
            {
                if numeric(values) {
                    recolour(values.as_array_mut().expect("numeric array"), rgb);
                } else if let Some(frames) = values.as_array_mut() {
                    for frame in frames {
                        if let Some(frame) = frame.as_object_mut() {
                            for field in ["s", "e"] {
                                if let Some(value) = frame.get_mut(field).filter(|v| numeric(v)) {
                                    recolour(value.as_array_mut().expect("numeric keyframe"), rgb);
                                }
                            }
                        }
                    }
                }
            }
            if let Some(gradient) = object.get_mut("g") {
                tint_gradient(gradient, rgb);
            }
            for value in object.values_mut() {
                tint_lottie(value, rgb);
            }
        }
        Value::Array(array) => {
            for value in array {
                tint_lottie(value, rgb);
            }
        }
        _ => {}
    }
}
