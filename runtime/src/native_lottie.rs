//! Python-free Lottie/TGS loading, catalogue identity, packaging, validation and rendering.
//!
//! JSON numbers and strings are retained independently of serde_json: catalogue keys historically
//! use Python's integer/float distinction, ASCII escaping and nonfinite JSON extension. Rendering
//! calls the exact native rlottie revision used by the historical 1.3.8 wrapper (see vendor pin).
use base64::{
    Engine as _, alphabet,
    engine::{
        DecodePaddingMode,
        general_purpose::{GeneralPurpose, GeneralPurposeConfig},
    },
};
use flate2::{Compression, GzBuilder, read::MultiGzDecoder};
use image::RgbaImage;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    ffi::{CString, c_char, c_void},
    io::{Read, Write},
    path::Path,
    ptr::NonNull,
};

pub const MAX_UNPACKED: usize = 8 * 1024 * 1024;
pub const MAX_TGS_BYTES: usize = 65536;
pub const MAX_SOURCE_BYTES: usize = 128 * 1024 * 1024;
const MAX_IMAGE_ASSETS: usize = 64;
const MAX_ASSETS: usize = 4096;
const MAX_ASSET_PIXELS: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct LottieError {
    pub kind: String,
    pub message: String,
    pub raw_fallback: bool,
    pub handled: bool,
    pub caught_by_exception: bool,
}
impl LottieError {
    fn new(kind: &str, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            raw_fallback: false,
            handled: matches!(
                kind,
                "MediaError" | "ValueError" | "OSError" | "RuntimeError"
            ),
            caught_by_exception: true,
        }
    }
    fn media(message: impl Into<String>) -> Self {
        Self::new("MediaError", message)
    }
    fn io(error: std::io::Error) -> Self {
        let mut result = Self::new("OSError", error.to_string());
        result.raw_fallback = true;
        result
    }
}
impl std::fmt::Display for LottieError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for LottieError {}
pub type Result<T> = std::result::Result<T, LottieError>;

// Codepoints, rather than Rust String, also preserve JSON's escaped lone surrogates.
type Text = Vec<u32>;
#[derive(Clone, Debug)]
enum Number {
    Integer(String),
    Float(f64),
}
#[derive(Clone, Debug)]
enum Node {
    Null,
    Bool(bool),
    Number(Number),
    String(Text),
    Array(Vec<Node>),
    Object(Vec<(Text, Node)>),
}
impl Node {
    fn get(&self, name: &str) -> Option<&Node> {
        let Self::Object(entries) = self else {
            return None;
        };
        entries
            .iter()
            .find(|(key, _)| key.iter().copied().eq(name.chars().map(u32::from)))
            .map(|(_, v)| v)
    }
    fn get_mut(&mut self, name: &str) -> Option<&mut Node> {
        let Self::Object(entries) = self else {
            return None;
        };
        entries
            .iter_mut()
            .find(|(key, _)| key.iter().copied().eq(name.chars().map(u32::from)))
            .map(|(_, v)| v)
    }
    fn kind(&self) -> &'static str {
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
    fn truthy(&self) -> bool {
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
    fn text(&self) -> Option<String> {
        let Self::String(value) = self else {
            return None;
        };
        value.iter().map(|&v| char::from_u32(v)).collect()
    }
    fn repr(&self) -> String {
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
    fn display(&self) -> String {
        self.text().unwrap_or_else(|| self.repr())
    }
    fn numeric(&self) -> bool {
        matches!(self, Self::Number(_) | Self::Bool(_))
    }
    fn float(&self) -> Result<f64> {
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
    fn is_512(&self) -> bool {
        self.numeric() && self.float().is_ok_and(|v| v == 512.)
    }
    fn write_json(&self, out: &mut String, sorted: bool, ascii: bool) {
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

fn integer_coercion(value: &Node) -> Result<String> {
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

struct Parser<'a> {
    text: &'a str,
    at: usize,
}
impl Parser<'_> {
    fn error(&self, message: &str) -> LottieError {
        let prefix = &self.text[..self.at];
        let line = prefix.bytes().filter(|&b| b == b'\n').count() + 1;
        let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
        let mut result = LottieError::new(
            "JSONDecodeError",
            format!(
                "{message}: line {line} column {column} (char {})",
                prefix.chars().count()
            ),
        );
        result.raw_fallback = true;
        result.handled = true;
        result
    }
    fn ws(&mut self) {
        while matches!(
            self.text.as_bytes().get(self.at),
            Some(b' ' | b'\t' | b'\r' | b'\n')
        ) {
            self.at += 1;
        }
    }
    fn consume(&mut self, byte: u8) -> bool {
        if self.text.as_bytes().get(self.at) == Some(&byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn string(&mut self) -> Result<Text> {
        if !self.consume(b'"') {
            return Err(self.error("Expecting property name enclosed in double quotes"));
        }
        let start = self.at - 1;
        let mut output = Vec::new();
        loop {
            let Some(&byte) = self.text.as_bytes().get(self.at) else {
                self.at = start;
                return Err(self.error("Unterminated string starting at"));
            };
            if self.consume(b'"') {
                break;
            }
            if self.consume(b'\\') {
                let Some(&escape) = self.text.as_bytes().get(self.at) else {
                    self.at = start;
                    return Err(self.error("Unterminated string starting at"));
                };
                self.at += 1;
                let c = match escape {
                    b'"' => 0x22,
                    b'\\' => 0x5c,
                    b'/' => 0x2f,
                    b'b' => 8,
                    b'f' => 12,
                    b'n' => 10,
                    b'r' => 13,
                    b't' => 9,
                    b'u' => {
                        let begin = self.at;
                        let c = self.hex4()?;
                        if (0xd800..=0xdbff).contains(&c) && self.text[self.at..].starts_with("\\u")
                        {
                            self.at += 2;
                            let low = self.hex4()?;
                            if (0xdc00..=0xdfff).contains(&low) {
                                0x10000 + ((c - 0xd800) << 10) + low - 0xdc00
                            } else {
                                self.at = begin + 4;
                                c
                            }
                        } else {
                            c
                        }
                    }
                    _ => {
                        self.at -= 1;
                        return Err(self.error("Invalid \\escape"));
                    }
                };
                output.push(c);
            } else {
                if byte < 32 {
                    return Err(self.error("Invalid control character at"));
                }
                let c = self.text[self.at..].chars().next().expect("nonempty UTF-8");
                self.at += c.len_utf8();
                output.push(c as u32);
            }
        }
        Ok(output)
    }
    fn hex4(&mut self) -> Result<u32> {
        let Some(text) = self.text.get(self.at..self.at + 4) else {
            return Err(self.error("Invalid \\uXXXX escape"));
        };
        let value =
            u32::from_str_radix(text, 16).map_err(|_| self.error("Invalid \\uXXXX escape"))?;
        self.at += 4;
        Ok(value)
    }
    fn node(&mut self, depth: usize) -> Result<Node> {
        if depth > 900 {
            return Err(LottieError::new(
                "RecursionError",
                "maximum recursion depth exceeded while decoding a JSON object",
            ));
        }
        self.ws();
        let start = self.at;
        match self.text.as_bytes().get(self.at).copied() {
            Some(b'{') => {
                self.at += 1;
                self.ws();
                let mut entries: Vec<(Text, Node)> = Vec::new();
                if !self.consume(b'}') {
                    loop {
                        self.ws();
                        let key = self.string()?;
                        self.ws();
                        if !self.consume(b':') {
                            return Err(self.error("Expecting ':' delimiter"));
                        }
                        let value = self.node(depth + 1)?;
                        if let Some((_, old)) = entries.iter_mut().find(|(k, _)| *k == key) {
                            *old = value;
                        } else {
                            entries.push((key, value));
                        }
                        self.ws();
                        if self.consume(b'}') {
                            break;
                        }
                        if !self.consume(b',') {
                            return Err(self.error("Expecting ',' delimiter"));
                        }
                    }
                }
                Ok(Node::Object(entries))
            }
            Some(b'[') => {
                self.at += 1;
                self.ws();
                let mut items = Vec::new();
                if !self.consume(b']') {
                    loop {
                        items.push(self.node(depth + 1)?);
                        self.ws();
                        if self.consume(b']') {
                            break;
                        }
                        if !self.consume(b',') {
                            return Err(self.error("Expecting ',' delimiter"));
                        }
                    }
                }
                Ok(Node::Array(items))
            }
            Some(b'"') => self.string().map(Node::String),
            _ => {
                for (literal, node) in [
                    ("true", Node::Bool(true)),
                    ("false", Node::Bool(false)),
                    ("null", Node::Null),
                    ("NaN", Node::Number(Number::Float(f64::NAN))),
                    ("Infinity", Node::Number(Number::Float(f64::INFINITY))),
                    ("-Infinity", Node::Number(Number::Float(f64::NEG_INFINITY))),
                ] {
                    if self.text[self.at..].starts_with(literal) {
                        self.at += literal.len();
                        return Ok(node);
                    }
                }
                self.consume(b'-');
                if !self.consume(b'0') {
                    let begin = self.at;
                    while self
                        .text
                        .as_bytes()
                        .get(self.at)
                        .is_some_and(u8::is_ascii_digit)
                    {
                        self.at += 1;
                    }
                    if self.at == begin {
                        self.at = start;
                        return Err(self.error("Expecting value"));
                    }
                }
                let mut float = false;
                if self.text.as_bytes().get(self.at) == Some(&b'.')
                    && self
                        .text
                        .as_bytes()
                        .get(self.at + 1)
                        .is_some_and(u8::is_ascii_digit)
                {
                    float = true;
                    self.at += 1;
                    while self
                        .text
                        .as_bytes()
                        .get(self.at)
                        .is_some_and(u8::is_ascii_digit)
                    {
                        self.at += 1;
                    }
                }
                if matches!(self.text.as_bytes().get(self.at), Some(b'e') | Some(b'E')) {
                    let begin = self.at;
                    self.at += 1;
                    if matches!(self.text.as_bytes().get(self.at), Some(b'+') | Some(b'-')) {
                        self.at += 1;
                    }
                    let digits = self.at;
                    while self
                        .text
                        .as_bytes()
                        .get(self.at)
                        .is_some_and(u8::is_ascii_digit)
                    {
                        self.at += 1;
                    }
                    if self.at == digits {
                        self.at = begin;
                    } else {
                        float = true;
                    }
                }
                let token = &self.text[start..self.at];
                if float {
                    Ok(Node::Number(Number::Float(
                        token.parse().map_err(|_| self.error("Expecting value"))?,
                    )))
                } else if token.trim_start_matches('-').len() > 4300 {
                    let mut error = LottieError::new(
                        "ValueError",
                        "Exceeds the limit (4300 digits) for integer string conversion",
                    );
                    error.raw_fallback = true;
                    Err(error)
                } else {
                    Ok(Node::Number(Number::Integer(
                        if token == "-0" { "0" } else { token }.into(),
                    )))
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Document {
    root: Node,
}
impl Document {
    fn preflight_image_assets(&self) -> Result<()> {
        let Some(Node::Array(assets)) = self.root.get("assets") else {
            return Ok(());
        };
        if assets.len() > MAX_ASSETS {
            return Err(LottieError::media("Lottie exceeds asset count limit"));
        }
        let (mut count, mut encoded, mut pixels) = (0, 0_u64, 0_u64);
        // rlottie's parser loads only the flat root assets array. PNG/JPEG payloads
        // cannot contain nested image references, so this walk has fixed depth.
        for asset in assets {
            let Node::Object(fields) = asset else {
                continue;
            };
            // The last p/layers field selects Image/Precomp in the native parser.
            let mut image = false;
            for (key, _) in fields {
                if key.iter().copied().eq("p".chars().map(u32::from)) {
                    image = true;
                } else if key.iter().copied().eq("layers".chars().map(u32::from)) {
                    image = false;
                }
            }
            if !image {
                continue;
            }
            count += 1;
            if count > MAX_IMAGE_ASSETS {
                return Err(LottieError::media("Lottie exceeds image asset count limit"));
            }
            let filename = asset.get("p").and_then(Node::text).ok_or_else(|| {
                LottieError::media("Lottie image asset path must be a Unicode string")
            })?;
            let embedded = match asset.get("e") {
                None => false,
                Some(Node::Number(Number::Integer(value))) => {
                    value.parse::<i32>().map(|value| value != 0).map_err(|_| {
                        LottieError::media(
                            "Lottie image asset embedded flag is outside integer range",
                        )
                    })?
                }
                _ => {
                    return Err(LottieError::media(
                        "Lottie image asset embedded flag must be an integer",
                    ));
                }
            };
            if !embedded {
                // Our C API passes resourcePath="". The native resource-path safety
                // check cannot establish that base and leaves external assets optional;
                // do not introduce JSON-parent or working-directory resolution here.
                continue;
            }
            if !filename.starts_with("data:") {
                continue;
            }
            let Some((_, payload)) = filename.split_once(',') else {
                continue;
            };
            let remaining_bytes = (MAX_SOURCE_BYTES as u64).saturating_sub(encoded);
            if payload.len() > MAX_UNPACKED
                || (payload.len() as u64).div_ceil(4) * 3 > remaining_bytes
            {
                return Err(LottieError::media(
                    "Lottie image assets exceed encoded byte limit",
                ));
            }
            // The native decoder accepts unpadded input and these URL/alternate
            // alphabet spellings. Check the same decoded bytes without narrowing
            // valid existing embedded images to only canonical base64 spelling.
            let payload = if payload
                .bytes()
                .any(|byte| matches!(byte, b'-' | b'.' | b'_' | b','))
            {
                Cow::Owned(
                    payload
                        .bytes()
                        .map(|byte| match byte {
                            b'-' | b'.' => b'+',
                            b'_' | b',' => b'/',
                            _ => byte,
                        })
                        .collect::<Vec<_>>(),
                )
            } else {
                Cow::Borrowed(payload.as_bytes())
            };
            let decoder = GeneralPurpose::new(
                &alphabet::STANDARD,
                GeneralPurposeConfig::new()
                    .with_decode_padding_mode(DecodePaddingMode::Indifferent)
                    .with_decode_allow_trailing_bits(true),
            );
            let data = decoder.decode(payload).map_err(|_| {
                LottieError::media("Lottie image asset contains invalid base64 data")
            })?;
            if !matches!(
                image::guess_format(&data),
                Ok(image::ImageFormat::Png | image::ImageFormat::Jpeg)
            ) {
                return Err(LottieError::media(
                    "Lottie image asset must contain a PNG or JPEG image",
                ));
            }
            let remaining_pixels = MAX_ASSET_PIXELS.saturating_sub(pixels);
            let limits = crate::native_raster::DecodeLimits {
                max_encoded_bytes: remaining_bytes,
                max_pixels: remaining_pixels,
                max_decoded_bytes: remaining_pixels * 4,
                ..Default::default()
            };
            let decoded =
                crate::native_raster::decode_with_limits(&data, limits).map_err(|error| {
                    LottieError::media(format!(
                        "Lottie image asset exceeds decode bounds or is invalid: {error}"
                    ))
                })?;
            encoded += data.len() as u64;
            pixels += u64::from(decoded.rgba.width()) * u64::from(decoded.rgba.height());
        }
        Ok(())
    }

    pub fn from_bytes(raw: &[u8], filename: &str) -> Result<Self> {
        let decoded = if raw.starts_with(&[0x1f, 0x8b]) {
            let mut decoded = Vec::new();
            MultiGzDecoder::new(raw)
                .take(MAX_UNPACKED as u64 + 1)
                .read_to_end(&mut decoded)
                .map_err(|error| {
                    if error.kind() == std::io::ErrorKind::UnexpectedEof {
                        LottieError::new(
                            "EOFError",
                            "Compressed file ended before the end-of-stream marker was reached",
                        )
                    } else {
                        LottieError::io(error)
                    }
                })?;
            if decoded.len() > MAX_UNPACKED {
                return Err(LottieError::media(format!(
                    "{filename}: Lottie expands beyond {MAX_UNPACKED} bytes"
                )));
            }
            Cow::Owned(decoded)
        } else {
            if raw.len() > MAX_UNPACKED {
                return Err(LottieError::media(format!(
                    "{filename}: Lottie expands beyond {MAX_UNPACKED} bytes"
                )));
            }
            Cow::Borrowed(raw)
        };
        let text = std::str::from_utf8(&decoded).map_err(|error| {
            let mut result = LottieError::new("UnicodeDecodeError", error.to_string());
            result.raw_fallback = true;
            result.handled = true;
            result
        })?;
        let mut parser = Parser { text, at: 0 };
        if text.starts_with('\u{feff}') {
            return Err(parser.error("Unexpected UTF-8 BOM (decode using utf-8-sig)"));
        }
        let root = parser.node(0)?;
        parser.ws();
        if parser.at != text.len() {
            return Err(parser.error("Extra data"));
        }
        if !matches!(root, Node::Object(_)) {
            return Err(LottieError::media(format!(
                "{filename}: Lottie root is {}, expected an object",
                root.kind()
            )));
        }
        Ok(Self { root })
    }
    pub fn from_value(value: &Value) -> Result<Self> {
        Self::from_bytes(value.to_string().as_bytes(), "document")
    }
    pub fn to_value(&self) -> Result<Value> {
        // serde cannot represent nonfinite numbers or lone surrogates. Keep those in Document.
        serde_json::from_str(&self.json(false, false)).map_err(|error| {
            LottieError::new(
                "ValueError",
                format!("Lottie cannot be represented as standard JSON: {error}"),
            )
        })
    }
    pub fn json(&self, sorted: bool, ensure_ascii: bool) -> String {
        let mut out = String::new();
        self.root.write_json(&mut out, sorted, ensure_ascii);
        out
    }
    pub fn canonical_bytes(&self) -> Vec<u8> {
        self.json(true, true).into_bytes()
    }
    pub fn encode_gzip(&self, ensure_ascii: bool) -> Result<Vec<u8>> {
        let mut encoder = GzBuilder::new()
            .mtime(0)
            .operating_system(255)
            .write(Vec::new(), Compression::best());
        encoder
            .write_all(self.json(false, ensure_ascii).as_bytes())
            .map_err(LottieError::io)?;
        encoder.finish().map_err(LottieError::io)
    }
    pub fn validate(&self, filename: &str, compressed_size: u64) -> Result<()> {
        validate_size(filename, compressed_size)?;
        for key in ["v", "fr", "ip", "op", "layers"] {
            if self.root.get(key).is_none() {
                return Err(LottieError::media(format!(
                    "{filename}: Lottie missing required key '{key}'"
                )));
            }
        }
        let w = self.root.get("w").unwrap_or(&Node::Null);
        let h = self.root.get("h").unwrap_or(&Node::Null);
        if !w.is_512() || !h.is_512() {
            return Err(LottieError::media(format!(
                "{filename}: canvas {}x{}, expected 512x512",
                w.display(),
                h.display()
            )));
        }
        let originals = ["fr", "ip", "op"].map(|k| self.root.get(k).expect("required field"));
        let mut values = Vec::new();
        for value in originals {
            values.push(value.float().map_err(|error| {
                if matches!(error.kind.as_str(), "TypeError" | "ValueError") {
                    LottieError::media(format!("{filename}: non-numeric fr/ip/op ({error})"))
                } else {
                    error
                }
            })?);
        }
        for (name, value) in ["fr", "ip", "op"].into_iter().zip(&values) {
            if !value.is_finite() {
                return Err(LottieError::media(format!(
                    "{filename}: {name} is {}, not a finite number",
                    float_repr(*value)
                )));
            }
        }
        let (fr, ip, op) = (values[0], values[1], values[2]);
        if fr <= 0. {
            return Err(LottieError::media(format!(
                "{filename}: frame rate {} must be positive",
                float_repr(fr)
            )));
        }
        if fr > 60. {
            return Err(LottieError::media(format!(
                "{filename}: {} fps > 60 fps",
                float_repr(fr)
            )));
        }
        let duration = (op - ip) / fr;
        if duration <= 0. {
            return Err(LottieError::media(format!(
                "{filename}: empty timeline (ip={}, op={})",
                originals[1].display(),
                originals[2].display()
            )));
        }
        if duration > 3.01 {
            return Err(LottieError::media(format!(
                "{filename}: {duration:.2}s > 3.0s"
            )));
        }
        let mut layers = items(self.root.get("layers"))?;
        for asset in items(self.root.get("assets"))? {
            if matches!(asset, Node::Object(_)) {
                layers.extend(items(asset.get("layers"))?);
            }
        }
        for layer in layers {
            if !matches!(layer, Node::Object(_)) {
                continue;
            }
            for mask in items(layer.get("masksProperties"))? {
                if !matches!(mask, Node::Object(_)) {
                    return Err(LottieError::new(
                        "AttributeError",
                        format!("'{}' object has no attribute 'get'", mask.kind()),
                    ));
                }
                if mask.get("mode").and_then(Node::text).as_deref() == Some("s") {
                    let name = layer.get("nm").map_or_else(|| "'?'".into(), Node::repr);
                    return Err(LottieError::media(format!(
                        "{filename}: layer {name} uses a SUBTRACT mask, which Telegram refuses on upload (add masks are fine). Re-export without it."
                    )));
                }
            }
        }
        Ok(())
    }
    /// Apply tint to flat/keyframed colours and gradient colour stops, preserving alpha/opacity.
    pub fn tint(&mut self, rgb: [u8; 3]) {
        tint_node(&mut self.root, rgb.map(|c| c as f64 / 255.));
    }
    /// Package policy accepts int-coercible canvas fields, followed by strict TGS validation.
    pub fn package_bytes(&self, source_name: &str, output_name: &str) -> Result<Vec<u8>> {
        let integer = |key| -> Result<String> {
            self.root
                .get(key)
                .map_or_else(|| Ok("0".into()), integer_coercion)
        };
        let (w, h) = (integer("w")?, integer("h")?);
        if w != "512" || h != "512" {
            return Err(LottieError::media(format!(
                "{source_name}: animated emoji require a 512x512 Lottie canvas, got {w}x{h}. Re-export the animation at 512x512."
            )));
        }
        let bytes = self.encode_gzip(false)?;
        // Re-decode to enforce the same bounded expansion check even for plain JSON inputs.
        Self::from_bytes(&bytes, output_name)?.validate(output_name, bytes.len() as u64)?;
        Ok(bytes)
    }
}

fn items(value: Option<&Node>) -> Result<Vec<&Node>> {
    let Some(value) = value.filter(|v| v.truthy()) else {
        return Ok(Vec::new());
    };
    match value {
        Node::Array(items) => Ok(items.iter().collect()),
        // Iterating Python dictionaries/strings yields strings, ignored by the layer walker.
        Node::Object(items) => Ok(items.iter().map(|_| &STRING_NODE).collect()),
        Node::String(items) => Ok(items.iter().map(|_| &STRING_NODE).collect()),
        _ => Err(LottieError::new(
            "TypeError",
            format!("'{}' object is not iterable", value.kind()),
        )),
    }
}
static STRING_NODE: Node = Node::String(Vec::new());

fn tint_node(node: &mut Node, rgb: [f64; 3]) {
    fn numeric(values: &Node) -> bool {
        matches!(values,Node::Array(items) if items.iter().all(Node::numeric))
    }
    fn recolour(values: &mut Node, rgb: [f64; 3]) {
        if let Node::Array(items) = values {
            for (i, &c) in rgb.iter().take(items.len()).enumerate() {
                items[i] = Node::Number(Number::Float(c));
            }
        }
    }
    if let Some(colour) = node.get_mut("c").and_then(|v| v.get_mut("k")) {
        if numeric(colour) {
            recolour(colour, rgb);
        } else if let Node::Array(frames) = colour {
            for frame in frames {
                for key in ["s", "e"] {
                    if let Some(value) = frame.get_mut(key).filter(|v| numeric(v)) {
                        recolour(value, rgb);
                    }
                }
            }
        }
    }
    if let Some(gradient) = node.get_mut("g") {
        let count = gradient.get("p").and_then(|v| match v {
            Node::Number(Number::Integer(n)) if !n.starts_with('-') => {
                Some(n.parse::<usize>().unwrap_or(usize::MAX))
            }
            Node::Bool(b) => Some(usize::from(*b)),
            _ => None,
        });
        if let Some(Node::Array(items)) = gradient
            .get_mut("k")
            .and_then(|v| v.get_mut("k"))
            .filter(|v| numeric(v))
        {
            let count = count
                .unwrap_or_else(|| {
                    if items.len().is_multiple_of(4) {
                        items.len() / 4
                    } else {
                        0
                    }
                })
                .min(items.len() / 4);
            for base in (0..count * 4).step_by(4) {
                for (i, &c) in rgb.iter().enumerate() {
                    items[base + 1 + i] = Node::Number(Number::Float(c));
                }
            }
        }
    }
    match node {
        Node::Object(items) => {
            for (_, value) in items {
                tint_node(value, rgb);
            }
        }
        Node::Array(items) => {
            for value in items {
                tint_node(value, rgb);
            }
        }
        _ => {}
    }
}
fn validate_size(filename: &str, size: u64) -> Result<()> {
    if size > MAX_TGS_BYTES as u64 {
        Err(LottieError::media(format!(
            "{filename}: TGS {size} > {MAX_TGS_BYTES} bytes"
        )))
    } else {
        Ok(())
    }
}

fn source_limit_error(path: &Path) -> LottieError {
    LottieError::media(format!(
        "{}: Lottie source exceeds {MAX_SOURCE_BYTES} bytes",
        path.file_name().unwrap_or_default().to_string_lossy()
    ))
}

fn checked_file(
    path: &Path,
    limit: usize,
    oversized: &impl Fn(u64) -> LottieError,
) -> Result<std::fs::File> {
    let file = std::fs::File::open(path).map_err(LottieError::io)?;
    let size = file.metadata().map_err(LottieError::io)?.len();
    if size > limit as u64 {
        return Err(oversized(size));
    }
    Ok(file)
}

fn read_bounded(
    path: &Path,
    limit: usize,
    oversized: impl Fn(u64) -> LottieError,
) -> Result<Vec<u8>> {
    let file = checked_file(path, limit, &oversized)?;
    let mut bytes = Vec::new();
    // The read limit also applies if a file grows after its metadata was checked.
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(LottieError::io)?;
    if bytes.len() > limit {
        return Err(oversized(bytes.len() as u64));
    }
    Ok(bytes)
}

/// Bound encoded input before allocating, including the initial reencode read.
pub fn source_bytes(path: &Path) -> Result<Vec<u8>> {
    read_bounded(path, MAX_SOURCE_BYTES, |_| source_limit_error(path))
}

fn hash_source(path: &Path) -> Result<sha2::digest::Output<Sha256>> {
    let mut reader = checked_file(path, MAX_SOURCE_BYTES, &|_| source_limit_error(path))?
        .take(MAX_SOURCE_BYTES as u64 + 1);
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0;
    loop {
        let count = reader.read(&mut buffer).map_err(LottieError::io)?;
        if count == 0 {
            return Ok(digest.finalize());
        }
        total += count;
        if total > MAX_SOURCE_BYTES {
            return Err(source_limit_error(path));
        }
        digest.update(&buffer[..count]);
    }
}

pub fn load(path: &Path) -> Result<Document> {
    if !path.is_absolute()
        || !std::fs::symlink_metadata(path)
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
    {
        let mut error = LottieError::new(
            "UnverifiedCodecError",
            "Lottie source must be an absolute regular non-symlink file",
        );
        error.caught_by_exception = false;
        return Err(error);
    }
    let raw = source_bytes(path)?;
    Document::from_bytes(
        &raw,
        &path.file_name().unwrap_or_default().to_string_lossy(),
    )
}
pub fn canonical_bytes(path: &Path) -> Result<Vec<u8>> {
    load(path).map(|d| d.canonical_bytes())
}
pub fn fingerprint(path: &Path) -> Result<Value> {
    let digest = match canonical_bytes(path) {
        Ok(bytes) => Sha256::digest(&bytes),
        Err(error) if error.raw_fallback => hash_source(path)?,
        Err(error) => return Err(error),
    };
    let hash = digest
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect::<String>();
    Ok(
        json!({"content_key":format!("a:{}",&hash[..32]),"phash":null,"fmt":"animated","extension":".tgs"}),
    )
}
pub fn validate_file(path: &Path) -> Result<()> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let size = std::fs::metadata(path).map_err(LottieError::io)?.len();
    validate_size(&name, size)?;
    let raw = read_bounded(path, MAX_TGS_BYTES, |size| {
        LottieError::media(format!("{name}: TGS {size} > {MAX_TGS_BYTES} bytes"))
    })?;
    if !raw.starts_with(&[0x1f, 0x8b]) {
        return Err(LottieError::media(format!(
            "{name}: TGS must be gzip-compressed Lottie"
        )));
    }
    let document = Document::from_bytes(&raw, &name).map_err(|error| {
        if error.raw_fallback {
            LottieError::media(format!("{name}: not a valid TGS/Lottie ({error})"))
        } else {
            error
        }
    })?;
    document.validate(&name, size)
}

unsafe extern "C" {
    fn lottie_animation_from_data(
        data: *const c_char,
        key: *const c_char,
        resources: *const c_char,
    ) -> *mut c_void;
    fn lottie_animation_destroy(animation: *mut c_void);
    fn lottie_animation_get_size(animation: *const c_void, width: *mut usize, height: *mut usize);
    fn lottie_animation_get_duration(animation: *const c_void) -> f64;
    fn lottie_animation_get_totalframe(animation: *const c_void) -> usize;
    fn lottie_animation_get_framerate(animation: *const c_void) -> f64;
    fn lottie_animation_get_frame_at_pos(animation: *const c_void, position: f32) -> usize;
    fn lottie_animation_render(
        animation: *mut c_void,
        frame: usize,
        buffer: *mut u32,
        width: usize,
        height: usize,
        stride: usize,
    );
}

/// Owns one engine animation; calls are synchronous and cannot outlive the instance.
pub struct Animation {
    handle: NonNull<c_void>,
}
impl Drop for Animation {
    fn drop(&mut self) {
        unsafe {
            lottie_animation_destroy(self.handle.as_ptr());
        }
    }
}
impl Animation {
    pub fn from_document(document: &Document) -> Result<Self> {
        document.preflight_image_assets()?;
        let text = CString::new(document.json(false, true))
            .map_err(|_| LottieError::new("ValueError", "Lottie JSON contains a NUL byte"))?;
        // Hash the data as its cache key, never reuse the wrapper's empty key across documents.
        let key = CString::new(
            Sha256::digest(text.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
        )
        .expect("hex key");
        // Native loadFromData copies its strings; all returned pixel storage is separately owned.
        let pointer =
            unsafe { lottie_animation_from_data(text.as_ptr(), key.as_ptr(), c"".as_ptr()) };
        let handle = NonNull::new(pointer).ok_or_else(|| {
            LottieError::new("RuntimeError", "rlottie could not load the Lottie document")
        })?;
        let result = Self { handle };
        if !result.frame_rate().is_finite()
            || result.frame_rate() <= 0.
            || !result.duration().is_finite()
            || result.total_frames() == 0
        {
            return Err(LottieError::new(
                "RuntimeError",
                "rlottie animation has an invalid or empty timeline",
            ));
        }
        Ok(result)
    }
    pub fn from_path(path: &Path) -> Result<Self> {
        Self::from_document(&load(path)?)
    }
    pub fn frame_rate(&self) -> f64 {
        unsafe { lottie_animation_get_framerate(self.handle.as_ptr()) }
    }
    pub fn total_frames(&self) -> usize {
        unsafe { lottie_animation_get_totalframe(self.handle.as_ptr()) }
    }
    pub fn duration(&self) -> f64 {
        unsafe { lottie_animation_get_duration(self.handle.as_ptr()) }
    }
    pub fn size(&self) -> (usize, usize) {
        let (mut w, mut h) = (0, 0);
        unsafe {
            lottie_animation_get_size(self.handle.as_ptr(), &mut w, &mut h);
        }
        (w, h)
    }
    pub fn frame_at_pos(&self, pos: f64) -> Result<usize> {
        if !pos.is_finite() || !(0.0..=1.0).contains(&pos) {
            return Err(LottieError::new(
                "ValueError",
                "Lottie position must be finite and between 0 and 1",
            ));
        }
        Ok(unsafe { lottie_animation_get_frame_at_pos(self.handle.as_ptr(), pos as f32) })
    }
    /// Frame is relative to the animation in-point. The historical native C API uses integer frames.
    pub fn render_rgba(&mut self, frame: f64, width: u32, height: u32) -> Result<RgbaImage> {
        if !frame.is_finite()
            || frame < 0.
            || frame >= self.total_frames() as f64
            || width == 0
            || height == 0
        {
            return Err(LottieError::new(
                "ValueError",
                "invalid Lottie frame or output dimensions",
            ));
        }
        let pixels = (width as usize)
            .checked_mul(height as usize)
            .filter(|n| *n <= 64 * 1024 * 1024)
            .ok_or_else(|| {
                LottieError::new("ValueError", "Lottie output dimensions are too large")
            })?;
        let mut surface = vec![0_u32; pixels];
        // SAFETY: surface owns width*height aligned pixels; stride and all dimensions match.
        unsafe {
            lottie_animation_render(
                self.handle.as_ptr(),
                frame as usize,
                surface.as_mut_ptr(),
                width as usize,
                height as usize,
                width as usize * 4,
            );
        }
        let mut rgba = Vec::with_capacity(pixels * 4);
        // rlottie emits native-endian ARGB32. The historical wrapper interprets those channels
        // as BGRA without unpremultiplication; preserving that step keeps preview appearance.
        for p in surface {
            rgba.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8]);
        }
        RgbaImage::from_raw(width, height, rgba)
            .ok_or_else(|| LottieError::new("RuntimeError", "invalid rlottie raster buffer"))
    }
}

#[cfg(test)]
pub(crate) fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .filter(|root| root.join("tests/fixtures").is_dir())
        .or_else(|| {
            Path::new(file!())
                .ancestors()
                .nth(3)
                .filter(|root| root.join("tests/fixtures").is_dir())
        })
        .expect("repository fixture directory")
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    fn root() -> std::path::PathBuf {
        fixture_root()
    }
    fn digest(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[test]
    fn historical_catalogue_identities_are_unchanged() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/animated-key.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let source = root().join(case["input"].as_str().unwrap());
            let actual = fingerprint(&source);
            if let Some(expected) = case.get("expected") {
                assert_eq!(&actual.unwrap(), expected, "{}", source.display());
            } else {
                let error = actual.unwrap_err();
                assert_eq!(error.kind, case["error_kind"]);
                assert_eq!(error.message, case["error"]);
            }
        }
    }

    #[test]
    fn every_historical_validation_case_keeps_its_policy_and_error_class() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/lottie-validation.json"
        ))
        .unwrap();
        for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            let document = Document::from_value(&case["document"]).unwrap();
            let actual = document.validate(
                case["filename"].as_str().unwrap(),
                case["compressed_size"].as_u64().unwrap(),
            );
            if case["expected"].is_null() {
                assert!(actual.is_ok(), "case {index}: {actual:?}");
            } else {
                let error = actual.unwrap_err();
                assert_eq!(error.kind, case["source_error_kind"], "case {index}");
                assert_eq!(error.message, case["expected"], "case {index}");
            }
        }
    }

    #[test]
    fn gzip_packaging_matches_the_historical_fixture() {
        let source = load(&root().join("tests/fixtures/native/tgs-package/valid.json")).unwrap();
        let bytes = source.package_bytes("valid.json", "valid.tgs").unwrap();
        assert_eq!(bytes.len(), 100);
        assert_eq!(
            digest(&bytes),
            "8b83d1e5effe7f27100badeeba5fc39883fd2835986305ef6f7e8c736b0040eb"
        );
        assert_eq!(
            Document::from_bytes(&bytes, "valid.tgs")
                .unwrap()
                .canonical_bytes(),
            source.canonical_bytes()
        );
    }

    #[test]
    fn malformed_and_nonfinite_documents_are_not_silently_normalized() {
        let base = r#"{"v":"5.5","fr":30,"ip":0,"op":60,"w":512,"h":512,"layers":[]}"#;
        for key in ["fr", "ip", "op"] {
            let mut value: Value = serde_json::from_str(base).unwrap();
            value[key] = Value::String("nan".into());
            assert!(
                Document::from_value(&value)
                    .unwrap()
                    .validate("bad.tgs", 100)
                    .unwrap_err()
                    .message
                    .contains(key)
            );
        }
        let doc = Document::from_bytes(
            br#"{"n":NaN,"i":Infinity,"z":-0.0,"s":"\ud800","p":"\ud83d\ude00"}"#,
            "special",
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(doc.canonical_bytes()).unwrap(),
            r#"{"i":Infinity,"n":NaN,"p":"\ud83d\ude00","s":"\ud800","z":-0.0}"#
        );
        assert!(doc.to_value().is_err());
        let error = Document::from_bytes(b"{invalid", "bad.json").unwrap_err();
        assert_eq!(
            error.message,
            "Expecting property name enclosed in double quotes: line 1 column 2 (char 1)"
        );
        assert!(error.raw_fallback);
    }

    #[test]
    fn plain_json_obeys_the_same_decoded_byte_boundary_as_gzip() {
        let mut plain = vec![b' '; MAX_UNPACKED + 1];
        plain[..2].copy_from_slice(b"{}");
        assert!(Document::from_bytes(&plain[..MAX_UNPACKED], "plain.json").is_ok());
        let error = Document::from_bytes(&plain, "plain.json").unwrap_err();
        assert_eq!(error.kind, "MediaError");
        assert_eq!(
            error.message,
            format!("plain.json: Lottie expands beyond {MAX_UNPACKED} bytes")
        );
        assert!(!error.raw_fallback);
    }

    #[test]
    fn oversized_sparse_sources_are_refused_before_reading_or_fallback_hashing() {
        struct Scratch(std::path::PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let path = Scratch(std::env::temp_dir().join(format!(
            "numera-lottie-read-bound-{}-{}.json", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        )));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path.0)
            .unwrap();
        let size = MAX_SOURCE_BYTES as u64 + 1;
        file.set_len(size).unwrap();
        drop(file);
        for error in [
            source_bytes(&path.0).unwrap_err(),
            load(&path.0).unwrap_err(),
            hash_source(&path.0).unwrap_err(),
            fingerprint(&path.0).unwrap_err(),
        ] {
            assert_eq!(error.kind, "MediaError");
            assert!(
                error
                    .message
                    .ends_with(&format!("Lottie source exceeds {MAX_SOURCE_BYTES} bytes"))
            );
            assert!(!error.raw_fallback);
        }
        // Telegram's smaller encoded-size rejection still precedes magic/JSON inspection.
        let error = validate_file(&path.0).unwrap_err();
        assert_eq!(error.kind, "MediaError");
        assert_eq!(
            error.message,
            format!(
                "{}: TGS {size} > {MAX_TGS_BYTES} bytes",
                path.0.file_name().unwrap().to_string_lossy()
            )
        );
    }

    fn image_document(png: &[u8], count: usize) -> Document {
        let path = format!("data:image/png;base64,{}", STANDARD.encode(png));
        let assets = (0..count)
            .map(|index| {
                json!({
                    "id":format!("image{index}"),"w":2,"h":2,"p":path,"e":1
                })
            })
            .collect::<Vec<_>>();
        Document::from_value(&json!({
            "v":"5.7.4","w":2,"h":2,"fr":60,"ip":0,"op":1,"assets":assets,
            "layers":[{"ty":2,"ind":1,"refId":"image0","sr":1,"ip":0,"op":1,"st":0,"bm":0,
                "ks":{"o":{"a":0,"k":100},"r":{"a":0,"k":0},"p":{"a":0,"k":[0,0,0]},
                    "a":{"a":0,"k":[0,0,0]},"s":{"a":0,"k":[100,100,100]}}}]
        }))
        .unwrap()
    }

    #[test]
    fn embedded_image_headers_and_asset_counts_are_bounded_before_native_entry() {
        let original = crate::native_raster::encode_png(&RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([220, 30, 50, 255]),
        ))
        .unwrap();
        for (width, height) in [(16_385_u32, 1_u32), (8193, 8193)] {
            let mut png = original.clone();
            png[16..20].copy_from_slice(&width.to_be_bytes());
            png[20..24].copy_from_slice(&height.to_be_bytes());
            // Keep a valid IHDR checksum so refusal proves dimension/pixel policy,
            // rather than relying on corruption stopping a later image decoder.
            let mut crc = 0xffff_ffff_u32;
            for &byte in &png[12..29] {
                crc ^= u32::from(byte);
                for _ in 0..8 {
                    crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
                }
            }
            png[29..33].copy_from_slice(&(!crc).to_be_bytes());
            let error = Animation::from_document(&image_document(&png, 1))
                .err()
                .unwrap();
            assert_eq!(error.kind, "MediaError");
            assert!(error.message.contains("decode bounds"), "{error}");
        }
        let error = Animation::from_document(&image_document(&original, MAX_IMAGE_ASSETS + 1))
            .err()
            .unwrap();
        assert!(error.message.contains("image asset count limit"));
    }

    #[test]
    fn checked_embedded_raster_is_rendered_by_the_actual_native_engine() {
        let png = crate::native_raster::encode_png(&RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([220, 30, 50, 255]),
        ))
        .unwrap();
        let document = image_document(&png, 1);
        let mut animation = Animation::from_document(&document).unwrap();
        let rgba = animation.render_rgba(0., 2, 2).unwrap();
        // Independent rlottie-python1.3.8 observation, including the native
        // image compositor's opacity/premultiplication rounding.
        assert!(rgba.pixels().all(|pixel| pixel.0 == [218, 29, 49, 253]));

        let mut unpadded = document.to_value().unwrap();
        let path = unpadded["assets"][0]["p"]
            .as_str()
            .unwrap()
            .trim_end_matches('=')
            .replace('+', "-")
            .replace('/', "_");
        unpadded["assets"][0]["p"] = json!(path);
        let mut unpadded =
            Animation::from_document(&Document::from_value(&unpadded).unwrap()).unwrap();
        assert_eq!(unpadded.render_rgba(0., 2, 2).unwrap(), rgba);

        let mut optional = document.to_value().unwrap();
        optional["assets"][0]["e"] = json!(0);
        optional["assets"][0]["p"] = json!(format!(
            "numera-missing-external-{}.png",
            std::process::id()
        ));
        let optional = Document::from_value(&optional).unwrap();
        assert!(Animation::from_document(&optional).is_ok());
    }

    #[test]
    fn same_native_engine_renders_the_historical_frames() {
        // Independent rlottie-python1.3.8 raw RGBA observations, before lossy WebP encoding.
        let mut animation =
            Animation::from_path(&root().join("tests/fixtures/native/lottie-preview/moving.json"))
                .unwrap();
        assert_eq!(animation.size(), (512, 512));
        assert_eq!(animation.total_frames(), 61);
        assert_eq!(animation.frame_rate(), 60.);
        assert_eq!(animation.duration(), 1.);
        for (position, expected) in [
            (
                0.,
                "6bc5c1dfb887be7987713544cef01274e388f37fcb6accb44913772f4c7eb140",
            ),
            (
                0.25,
                "5c0156aa37de481620f02e4a2c2aaa82fecbc527188716cd1c580f2d1f949f3d",
            ),
            (
                0.5,
                "820e978ddc10d87713fc9036ba7f6058e82552ab1f60ba3eda793f2b8f9616d8",
            ),
            (
                0.75,
                "05aaa7422503984bc7bee40594c9051291ed489e7729962412ea42b749c82c36",
            ),
        ] {
            let frame = animation.frame_at_pos(position).unwrap();
            assert_eq!(
                digest(
                    animation
                        .render_rgba(frame as f64, 52, 52)
                        .unwrap()
                        .as_raw()
                ),
                expected,
                "position {position}"
            );
        }
        assert!(animation.render_rgba(f64::NAN, 52, 52).is_err());
        assert!(animation.render_rgba(0., 0, 52).is_err());
        assert!(animation.render_rgba(1000., 52, 52).is_err());
        assert!(animation.frame_at_pos(f64::NAN).is_err());
        assert!(animation.frame_at_pos(1.1).is_err());
        let invalid = Document::from_bytes(b"{}", "invalid.json").unwrap();
        assert!(Animation::from_document(&invalid).is_err());
    }

    #[test]
    fn tint_preserves_keyframe_alpha_and_gradient_opacity() {
        let value = json!({"c":{"k":[{"s":[1.,0.,0.,0.25],"e":[1.,0.,0.,0.5]}]},
            "g":{"p":2,"k":{"k":[0.,1.,0.,0.,1.,0.,1.,0.,0.,0.2,1.,0.8]}}});
        let mut document = Document::from_value(&value).unwrap();
        document.tint([0, 0, 255]);
        let result = document.to_value().unwrap();
        assert_eq!(result["c"]["k"][0]["s"], json!([0., 0., 1., 0.25]));
        assert_eq!(result["c"]["k"][0]["e"], json!([0., 0., 1., 0.5]));
        assert_eq!(
            result["g"]["k"]["k"],
            json!([0., 0., 0., 1., 1., 0., 0., 1., 0., 0.2, 1., 0.8])
        );
    }

    #[test]
    fn every_historical_tint_tree_and_gradient_is_preserved() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/native/lottie-tint.json"))
                .unwrap();
        for category in ["trees", "gradients"] {
            for (index, case) in fixture[category].as_array().unwrap().iter().enumerate() {
                let wrapper = if category == "gradients" {
                    json!({"g":case["input"]})
                } else {
                    json!({"tree":case["input"]})
                };
                let mut document = Document::from_value(&wrapper).unwrap();
                document.tint([18, 52, 86]);
                let output = document.to_value().unwrap();
                let field = if category == "gradients" { "g" } else { "tree" };
                assert_eq!(output[field], case["expected"], "{category} case {index}");
            }
        }
    }
}
