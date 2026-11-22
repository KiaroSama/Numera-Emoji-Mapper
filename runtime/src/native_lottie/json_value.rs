use super::{LottieError, Result};

// Codepoints, rather than Rust String, also preserve JSON's escaped lone surrogates.
pub(super) type Text = Vec<u32>;
#[derive(Clone, Debug)]
pub(super) enum Number {
    Integer(String),
    Float(f64),
}
#[derive(Clone, Debug)]
pub(super) enum Node {
    Null,
    Bool(bool),
    Number(Number),
    String(Text),
    Array(Vec<Node>),
    Object(Vec<(Text, Node)>),
}
impl Node {
    pub(super) fn get(&self, name: &str) -> Option<&Node> {
        let Self::Object(entries) = self else {
            return None;
        };
        entries
            .iter()
            .find(|(key, _)| key.iter().copied().eq(name.chars().map(u32::from)))
            .map(|(_, v)| v)
    }
    pub(super) fn get_mut(&mut self, name: &str) -> Option<&mut Node> {
        let Self::Object(entries) = self else {
            return None;
        };
        entries
            .iter_mut()
            .find(|(key, _)| key.iter().copied().eq(name.chars().map(u32::from)))
            .map(|(_, v)| v)
    }
    pub(super) fn kind(&self) -> &'static str {
        match self {
            Self::Null => "NoneType",
            Self::Bool(_) => "bool",
            Self::Number(Number::Integer(_)) => "int",
            Self::Number(_) => "float",
            Self::String(_) => "str",
            Self::Array(_) => "list",
            Self::Object(_) => "dict",
        }
    }
    pub(super) fn truthy(&self) -> bool {
        match self {
            Self::Null | Self::Bool(false) => false,
            Self::Number(Number::Integer(v)) => v != "0",
            Self::Number(Number::Float(v)) => *v != 0.,
            Self::String(v) => !v.is_empty(),
            Self::Array(v) => !v.is_empty(),
            Self::Object(v) => !v.is_empty(),
            Self::Bool(true) => true,
        }
    }
    pub(super) fn text(&self) -> Option<String> {
        let Self::String(value) = self else {
            return None;
        };
        value.iter().map(|&v| char::from_u32(v)).collect()
    }
    pub(super) fn repr(&self) -> String {
        match self {
            Self::Null => "None".into(),
            Self::Bool(true) => "True".into(),
            Self::Bool(false) => "False".into(),
            Self::String(text) => quoted_text(text),
            Self::Number(Number::Integer(n)) => n.clone(),
            Self::Number(Number::Float(n)) => float_repr(*n),
            Self::Array(items) => format!(
                "[{}]",
                items.iter().map(Self::repr).collect::<Vec<_>>().join(", ")
            ),
            Self::Object(items) => format!(
                "{{{}}}",
                items
                    .iter()
                    .map(|(k, v)| format!("{}: {}", Node::String(k.clone()).repr(), v.repr()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
    pub(super) fn display(&self) -> String {
        self.text().unwrap_or_else(|| self.repr())
    }
    pub(super) fn numeric(&self) -> bool {
        matches!(self, Self::Number(_) | Self::Bool(_))
    }
    pub(super) fn float(&self) -> Result<f64> {
        match self {
            Self::Bool(value) => Ok(u8::from(*value) as f64),
            Self::Number(Number::Float(value)) => Ok(*value),
            Self::Number(Number::Integer(value)) => {
                let result: f64 = value.parse().map_err(|_| {
                    LottieError::new("OverflowError", "int too large to convert to float")
                })?;
                if result.is_finite() {
                    Ok(result)
                } else {
                    Err(LottieError::new(
                        "OverflowError",
                        "int too large to convert to float",
                    ))
                }
            }
            Self::String(_) => {
                let bad = || {
                    LottieError::new(
                        "ValueError",
                        format!("could not convert string to float: {}", self.repr()),
                    )
                };
                let original = self.text().ok_or_else(bad)?;
                let original = original.trim_matches(char::is_whitespace);
                let mut normalized = String::new();
                for c in original.chars() {
                    if let Some(n) = crate::emoji_ids::decimal_value(c) {
                        normalized.push((b'0' + n as u8) as char);
                    } else {
                        normalized.push(c);
                    }
                }
                let bytes = normalized.as_bytes();
                for (i, &c) in bytes.iter().enumerate() {
                    if c == b'_'
                        && (i == 0
                            || i + 1 == bytes.len()
                            || !bytes[i - 1].is_ascii_digit()
                            || !bytes[i + 1].is_ascii_digit())
                    {
                        return Err(bad());
                    }
                }
                normalized.retain(|c| c != '_');
                let lower = normalized.to_ascii_lowercase();
                let unsigned = lower.trim_start_matches(['+', '-']);
                if unsigned == "nan" {
                    return Ok(f64::NAN);
                }
                if matches!(unsigned, "inf" | "infinity") {
                    return Ok(if lower.starts_with('-') {
                        f64::NEG_INFINITY
                    } else {
                        f64::INFINITY
                    });
                }
                if !valid_decimal(&normalized) {
                    return Err(bad());
                }
                normalized.parse::<f64>().map_err(|_| bad())
            }
            _ => Err(LottieError::new(
                "TypeError",
                format!(
                    "float() argument must be a string or a real number, not '{}'",
                    self.kind()
                ),
            )),
        }
    }
    pub(super) fn is_512(&self) -> bool {
        self.numeric() && self.float().is_ok_and(|v| v == 512.)
    }
    pub(super) fn write_json(&self, out: &mut String, sorted: bool, ascii: bool) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
            Self::Number(Number::Integer(n)) => out.push_str(n),
            Self::Number(Number::Float(n)) => {
                let text = if n.is_nan() {
                    "NaN".into()
                } else if *n == f64::INFINITY {
                    "Infinity".into()
                } else if *n == f64::NEG_INFINITY {
                    "-Infinity".into()
                } else {
                    float_repr(*n)
                };
                out.push_str(&text);
            }
            Self::String(value) => write_string(value, out, ascii),
            Self::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i != 0 {
                        out.push(',');
                    }
                    item.write_json(out, sorted, ascii);
                }
                out.push(']');
            }
            Self::Object(items) => {
                out.push('{');
                let mut order = (0..items.len()).collect::<Vec<_>>();
                if sorted {
                    order.sort_by(|&a, &b| items[a].0.cmp(&items[b].0));
                }
                for (i, index) in order.into_iter().enumerate() {
                    if i != 0 {
                        out.push(',');
                    }
                    write_string(&items[index].0, out, ascii);
                    out.push(':');
                    items[index].1.write_json(out, sorted, ascii);
                }
                out.push('}');
            }
        }
    }
}

fn valid_decimal(text: &str) -> bool {
    let b = text.as_bytes();
    let mut i = usize::from(matches!(b.first(), Some(b'+') | Some(b'-')));
    let start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut digits = i - start;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        digits += i - start;
    }
    if digits == 0 {
        return false;
    }
    if matches!(b.get(i), Some(b'e') | Some(b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+') | Some(b'-')) {
            i += 1;
        }
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    i == b.len()
}

fn quoted_text(text: &[u32]) -> String {
    if let Some(text) = text
        .iter()
        .copied()
        .map(char::from_u32)
        .collect::<Option<String>>()
    {
        return crate::python_repr::quoted(&text);
    }
    let quote = if text.contains(&u32::from('\'')) && !text.contains(&u32::from('"')) {
        '"'
    } else {
        '\''
    };
    let mut output = String::from(quote);
    for &c in text {
        match char::from_u32(c) {
            None => output.push_str(&format!("\\u{c:04x}")),
            Some(c) if c == quote => {
                output.push('\\');
                output.push(c);
            }
            Some(c @ ('\'' | '"')) => output.push(c),
            Some(c) => {
                let single = crate::python_repr::quoted(&c.to_string());
                output.push_str(&single[1..single.len() - 1]);
            }
        }
    }
    output.push(quote);
    output
}

pub(super) fn integer_coercion(value: &Node) -> Result<String> {
    match value {
        Node::Number(Number::Integer(value)) => Ok(value.clone()),
        Node::Bool(value) => Ok(u8::from(*value).to_string()),
        Node::Number(Number::Float(value)) => {
            if value.is_nan() {
                return Err(LottieError::new(
                    "ValueError",
                    "cannot convert float NaN to integer",
                ));
            }
            if !value.is_finite() {
                return Err(LottieError::new(
                    "OverflowError",
                    "cannot convert float infinity to integer",
                ));
            }
            Ok(if *value == 0. {
                "0".into()
            } else {
                format!("{:.0}", value.trunc())
            })
        }
        Node::String(_) => {
            let invalid = || {
                LottieError::new(
                    "ValueError",
                    format!("invalid literal for int() with base 10: {}", value.repr()),
                )
            };
            let text = value.text().ok_or_else(invalid)?;
            let text = text.trim_matches(char::is_whitespace);
            let mut digits = String::new();
            let mut negative = false;
            let mut previous_digit = false;
            let mut chars = text.chars().peekable();
            if chars.peek() == Some(&'-') {
                negative = true;
                chars.next();
            } else if chars.peek() == Some(&'+') {
                chars.next();
            }
            while let Some(c) = chars.next() {
                if let Some(n) = crate::emoji_ids::decimal_value(c) {
                    digits.push((b'0' + n as u8) as char);
                    previous_digit = true;
                } else if c == '_'
                    && previous_digit
                    && chars
                        .peek()
                        .is_some_and(|&c| crate::emoji_ids::decimal_value(c).is_some())
                {
                    previous_digit = false;
                } else {
                    return Err(invalid());
                }
            }
            if digits.is_empty() || !previous_digit {
                return Err(invalid());
            }
            if digits.len() > 4300 {
                return Err(LottieError::new(
                    "ValueError",
                    "Exceeds the limit (4300 digits) for integer string conversion",
                ));
            }
            let digits = digits.trim_start_matches('0');
            Ok(if digits.is_empty() {
                "0".into()
            } else {
                format!("{}{digits}", if negative { "-" } else { "" })
            })
        }
        _ => Err(LottieError::new(
            "TypeError",
            format!(
                "int() argument must be a string, a bytes-like object or a real number, not '{}'",
                value.kind()
            ),
        )),
    }
}

/// The historical catalogue's Python 3.11 shortest-double spelling.
pub fn float_repr(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value == f64::INFINITY {
        return "inf".into();
    }
    if value == f64::NEG_INFINITY {
        return "-inf".into();
    }
    if value == 0. {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .into();
    }
    let mut buffer = ryu::Buffer::new();
    let rendered = buffer.format_finite(value.abs());
    let (mantissa, explicit) = rendered.split_once('e').map_or((rendered, 0), |(m, e)| {
        (m, e.parse::<i32>().expect("Ryu exponent"))
    });
    let dot = mantissa.find('.').unwrap_or(mantissa.len()) as i32;
    let mut digits: String = mantissa.chars().filter(|&c| c != '.').collect();
    let leading = digits.bytes().take_while(|&b| b == b'0').count();
    let exponent = dot - leading as i32 - 1 + explicit;
    digits.drain(..leading);
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
    }
    let mut result = if value.is_sign_negative() {
        "-".into()
    } else {
        String::new()
    };
    if !(-4..16).contains(&exponent) {
        result.push(digits.as_bytes()[0] as char);
        if digits.len() > 1 {
            result.push('.');
            result.push_str(&digits[1..]);
        }
        result.push('e');
        result.push(if exponent < 0 { '-' } else { '+' });
        result.push_str(&format!("{:02}", exponent.abs()));
    } else {
        let point = exponent + 1;
        if point <= 0 {
            result.push_str("0.");
            result.push_str(&"0".repeat((-point) as usize));
            result.push_str(&digits);
        } else if point as usize >= digits.len() {
            result.push_str(&digits);
            result.push_str(&"0".repeat(point as usize - digits.len()));
            result.push_str(".0");
        } else {
            let point = point as usize;
            result.push_str(&digits[..point]);
            result.push('.');
            result.push_str(&digits[point..]);
        }
    }
    result
}

fn write_string(text: &[u32], out: &mut String, ascii: bool) {
    out.push('"');
    for &c in text {
        match c {
            0x22 => out.push_str("\\\""),
            0x5c => out.push_str("\\\\"),
            8 => out.push_str("\\b"),
            12 => out.push_str("\\f"),
            10 => out.push_str("\\n"),
            13 => out.push_str("\\r"),
            9 => out.push_str("\\t"),
            c if c < 32 || (ascii && c >= 127) || (0xd800..=0xdfff).contains(&c) => {
                if c <= 0xffff {
                    out.push_str(&format!("\\u{c:04x}"));
                } else {
                    let v = c - 0x10000;
                    out.push_str(&format!(
                        "\\u{:04x}\\u{:04x}",
                        0xd800 + (v >> 10),
                        0xdc00 + (v & 1023)
                    ));
                }
            }
            c => out.push(char::from_u32(c).expect("parsed Unicode scalar")),
        }
    }
    out.push('"');
}
