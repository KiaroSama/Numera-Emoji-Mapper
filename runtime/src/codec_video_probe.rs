//! Native FFprobe metadata with lossless integers, nonfinite floats and Unicode code points.
use crate::codec_error::CodecError;
use crate::codec_video_error::VideoError;
use serde_json::Value;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataInteger(String);
impl MetadataInteger {
    pub fn parse(text: &str) -> Result<Self, CodecError> {
        let original = text;
        let text = text.trim();
        let (negative, digits) = match text.strip_prefix('-') {
            Some(digits) => (true, digits),
            None => (false, text.strip_prefix('+').unwrap_or(text)),
        };
        let mut result = String::new();
        let mut previous_digit = false;
        for ch in digits.chars() {
            if ch == '_' && previous_digit {
                previous_digit = false;
                continue;
            }
            let Some(digit) = decimal_digit(ch) else {
                return Err(int_literal(original));
            };
            result.push(char::from(b'0' + digit));
            previous_digit = true;
        }
        if !previous_digit {
            return Err(int_literal(original));
        }
        let result = result.trim_start_matches('0');
        Ok(Self(if result.is_empty() {
            "0".into()
        } else if negative {
            format!("-{result}")
        } else {
            result.into()
        }))
    }
    pub fn equals(&self, number: i64) -> bool {
        self.0 == number.to_string()
    }
    pub fn greater_than_u64(&self, number: u64) -> bool {
        if self.0.starts_with('-') {
            return false;
        }
        let other = number.to_string();
        self.0.len() > other.len() || (self.0.len() == other.len() && self.0 > other)
    }
}
impl From<u64> for MetadataInteger {
    fn from(value: u64) -> Self {
        Self(value.to_string())
    }
}
impl From<i64> for MetadataInteger {
    fn from(value: i64) -> Self {
        Self(value.to_string())
    }
}
impl fmt::Display for MetadataInteger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Python strings may contain isolated surrogate code points; Rust UTF-8 strings cannot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataString(Vec<u32>);
impl From<&str> for MetadataString {
    fn from(value: &str) -> Self {
        Self(value.chars().map(u32::from).collect())
    }
}
impl MetadataString {
    pub fn as_utf8(&self) -> Option<String> {
        self.0.iter().map(|&point| char::from_u32(point)).collect()
    }
    pub fn equals(&self, value: &str) -> bool {
        self.0.iter().copied().eq(value.chars().map(u32::from))
    }
    pub fn contains_component(&self, value: &str) -> bool {
        self.0
            .split(|&point| point == 44)
            .any(|part| part.iter().copied().eq(value.chars().map(u32::from)))
    }
    pub fn repr(&self) -> String {
        let quote = if self.0.contains(&39) && !self.0.contains(&34) {
            '"'
        } else {
            '\''
        };
        let mut out = String::from(quote);
        for &point in &self.0 {
            match point {
                9 => out.push_str("\\t"),
                10 => out.push_str("\\n"),
                13 => out.push_str("\\r"),
                92 => out.push_str("\\\\"),
                point if point == quote as u32 => {
                    out.push('\\');
                    out.push(quote);
                }
                0..=31 | 127 => out.push_str(&format!("\\x{point:02x}")),
                0xd800..=0xdfff => out.push_str(&format!("\\u{point:04x}")),
                point => out.push(char::from_u32(point).unwrap_or('\u{fffd}')),
            }
        }
        out.push(quote);
        out
    }
}

#[derive(Debug)]
pub struct VideoInfo {
    pub width: MetadataInteger,
    pub height: MetadataInteger,
    pub duration: f64,
    pub codec: MetadataString,
    pub fps: f64,
    pub has_audio: bool,
    pub container: MetadataString,
}

fn error(kind: &str, message: impl Into<String>) -> CodecError {
    CodecError {
        kind: kind.into(),
        message: message.into(),
        handled: matches!(
            kind,
            "ValueError" | "JSONDecodeError" | "OSError" | "RuntimeError"
        ),
    }
}
fn int_literal(value: &str) -> CodecError {
    error(
        "ValueError",
        format!(
            "invalid literal for int() with base 10: {}",
            MetadataString::from(value).repr()
        ),
    )
}
pub(crate) fn known(error: CodecError) -> VideoError {
    VideoError {
        error,
        caught_by_exception: true,
    }
}

// Decimal digit blocks from Unicode Nd. FFprobe emits ASCII; these also preserve source coercion.
fn decimal_digit(ch: char) -> Option<u8> {
    let point = ch as u32;
    const STARTS: &[u32] = &[
        0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6, 0xc66, 0xce6, 0xd66,
        0xde6, 0xe50, 0xed0, 0xf20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946, 0x19d0, 0x1a80, 0x1a90,
        0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0, 0xa9f0, 0xaa50, 0xabf0,
        0xff10, 0x104a0, 0x10d30, 0x11066, 0x110f0, 0x11136, 0x111d0, 0x112f0, 0x11450, 0x114d0,
        0x11650, 0x116c0, 0x11730, 0x118e0, 0x11950, 0x11c50, 0x11d50, 0x11da0, 0x16a60, 0x16ac0,
        0x16b50, 0x1d7ce, 0x1d7d8, 0x1d7e2, 0x1d7ec, 0x1d7f6, 0x1e140, 0x1e2f0, 0x1e4f0, 0x1e950,
    ];
    STARTS
        .iter()
        .find_map(|&start| (point >= start && point < start + 10).then(|| (point - start) as u8))
}

pub(crate) fn float_text(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut normalized = String::new();
    let chars: Vec<_> = text.chars().collect();
    for (index, &ch) in chars.iter().enumerate() {
        if ch == '_' {
            if index == 0
                || index + 1 == chars.len()
                || decimal_digit(chars[index - 1]).is_none()
                || decimal_digit(chars[index + 1]).is_none()
            {
                return None;
            }
        } else if let Some(digit) = decimal_digit(ch) {
            normalized.push(char::from(b'0' + digit));
        } else {
            normalized.push(ch);
        }
    }
    match normalized.to_ascii_lowercase().as_str() {
        "nan" | "+nan" => Some(f64::NAN),
        "-nan" => Some(-f64::NAN),
        "inf" | "+inf" | "infinity" | "+infinity" => Some(f64::INFINITY),
        "-inf" | "-infinity" => Some(f64::NEG_INFINITY),
        _ => normalized.parse().ok(),
    }
}
// CPython-style shortest float repr, used only when malformed metadata asks for str(number).
fn float_repr(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value == f64::INFINITY {
        return "inf".into();
    }
    if value == f64::NEG_INFINITY {
        return "-inf".into();
    }
    let mut buffer = ryu::Buffer::new();
    let short = buffer.format_finite(value);
    if let Some((mantissa, exponent)) = short.split_once('e') {
        let exponent = exponent.parse::<i32>().unwrap_or(0);
        if (-4..16).contains(&exponent) {
            let sign = if mantissa.starts_with('-') { "-" } else { "" };
            let mantissa = mantissa.trim_start_matches('-');
            let decimal = mantissa.find('.').unwrap_or(mantissa.len()) as i32 + exponent;
            let digits = mantissa.replace('.', "");
            return if decimal <= 0 {
                format!("{sign}0.{}{digits}", "0".repeat((-decimal) as usize))
            } else if decimal as usize >= digits.len() {
                format!(
                    "{sign}{digits}{}.0",
                    "0".repeat(decimal as usize - digits.len())
                )
            } else {
                format!(
                    "{sign}{}.{}",
                    &digits[..decimal as usize],
                    &digits[decimal as usize..]
                )
            };
        }
        return format!(
            "{}e{}{:02}",
            mantissa.strip_suffix(".0").unwrap_or(mantissa),
            if exponent >= 0 { "+" } else { "-" },
            exponent.unsigned_abs()
        );
    }
    if value != 0.0 && value.abs() < 0.0001 {
        let exponential = format!("{value:e}");
        if let Some((mantissa, exponent)) = exponential.split_once('e') {
            let exponent = exponent.parse::<i32>().unwrap_or(0);
            return format!("{}e-{:02}", mantissa, exponent.unsigned_abs());
        }
    }
    if short.contains('.') {
        short.into()
    } else {
        format!("{short}.0")
    }
}

fn fps(text: &MetadataString) -> f64 {
    let Some(text) = text.as_utf8() else {
        return 0.0;
    };
    let (numerator, denominator) = text.split_once('/').unwrap_or((&text, "1"));
    let denominator = if denominator.is_empty() {
        "1"
    } else {
        denominator
    };
    match (float_text(numerator), float_text(denominator)) {
        (Some(_), Some(0.0)) => 0.0,
        (Some(numerator), Some(denominator)) => numerator / denominator,
        _ => 0.0,
    }
}

/// Object keys retain insertion order, including repeated-key overwrite behavior.
#[derive(Clone, Debug, Default)]
struct MetadataObject(Vec<(String, Datum)>);
impl MetadataObject {
    fn new() -> Self {
        Self::default()
    }
    fn get(&self, key: &str) -> Option<&Datum> {
        self.0
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }
    fn insert(&mut self, key: String, value: Datum) {
        if let Some((_, existing)) = self.0.iter_mut().find(|(name, _)| *name == key) {
            *existing = value;
        } else {
            self.0.push((key, value));
        }
    }
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.iter().map(|(key, _)| key)
    }
    fn iter(&self) -> impl Iterator<Item = &(String, Datum)> {
        self.0.iter()
    }
}

#[derive(Clone, Debug)]
enum Datum {
    Null,
    Bool(bool),
    Number(String),
    String(MetadataString),
    Array(Vec<Datum>),
    Object(MetadataObject),
}
impl Datum {
    fn kind(&self) -> &'static str {
        match self {
            Self::Null => "NoneType",
            Self::Bool(_) => "bool",
            Self::Number(n) if n.parse::<MetadataInteger>().is_ok() => "int",
            Self::Number(_) => "float",
            Self::String(_) => "str",
            Self::Array(_) => "list",
            Self::Object(_) => "dict",
        }
    }
    fn truth(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(b) => *b,
            Self::Number(n) => float_text(n).is_some_and(|n| n != 0.0),
            Self::String(s) => !s.0.is_empty(),
            Self::Array(v) => !v.is_empty(),
            Self::Object(v) => !v.is_empty(),
        }
    }
    fn get(&self, key: &str, default: Datum) -> Result<Datum, CodecError> {
        match self {
            Self::Object(map) => Ok(map.get(key).cloned().unwrap_or(default)),
            _ => Err(error(
                "AttributeError",
                format!("'{}' object has no attribute 'get'", self.kind()),
            )),
        }
    }
    fn repr(&self) -> MetadataString {
        match self {
            Self::String(s) => MetadataString::from(s.repr().as_str()),
            _ => self.string(),
        }
    }
    fn string(&self) -> MetadataString {
        match self {
            Self::Null => MetadataString::from("None"),
            Self::Bool(true) => MetadataString::from("True"),
            Self::Bool(false) => MetadataString::from("False"),
            Self::String(s) => s.clone(),
            Self::Number(n) => {
                let text = if let Ok(integer) = MetadataInteger::parse(n) {
                    integer.to_string()
                } else {
                    float_repr(float_text(n).unwrap_or(f64::NAN))
                };
                MetadataString::from(text.as_str())
            }
            Self::Array(values) => MetadataString::from(
                format!(
                    "[{}]",
                    values
                        .iter()
                        .map(|v| v.repr().as_utf8().unwrap_or_default())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
                .as_str(),
            ),
            Self::Object(values) => MetadataString::from(
                format!(
                    "{{{}}}",
                    values
                        .iter()
                        .map(|(k, v)| format!(
                            "{}: {}",
                            MetadataString::from(k.as_str()).repr(),
                            v.repr().as_utf8().unwrap_or_default()
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
                .as_str(),
            ),
        }
    }
    fn integer(&self) -> Result<MetadataInteger, CodecError> {
        match self {
            Self::Bool(value) => Ok(MetadataInteger::from(i64::from(*value))),
            Self::String(s) => {
                let text = s.as_utf8().ok_or_else(|| {
                    error(
                        "ValueError",
                        format!("invalid literal for int() with base 10: {}", s.repr()),
                    )
                })?;
                MetadataInteger::parse(&text)
            }
            Self::Number(s) => {
                if let Ok(value) = MetadataInteger::parse(s) {
                    return Ok(value);
                }
                let n = float_text(s).ok_or_else(|| int_literal(s))?;
                if n.is_nan() {
                    return Err(error("ValueError", "cannot convert float NaN to integer"));
                }
                if n.is_infinite() {
                    return Err(error(
                        "OverflowError",
                        "cannot convert float infinity to integer",
                    ));
                }
                MetadataInteger::parse(&format!("{:.0}", n.trunc()))
            }
            _ => Err(error(
                "TypeError",
                format!(
                    "int() argument must be a string, a bytes-like object or a real number, not '{}'",
                    self.kind()
                ),
            )),
        }
    }
    fn float(&self) -> Result<f64, CodecError> {
        match self {
            Self::Bool(b) => Ok(u8::from(*b) as f64),
            Self::Number(n) => {
                let value = float_text(n)
                    .ok_or_else(|| error("OverflowError", "int too large to convert to float"))?;
                if value.is_infinite() && MetadataInteger::parse(n).is_ok() {
                    return Err(error("OverflowError", "int too large to convert to float"));
                }
                Ok(value)
            }
            Self::String(s) => s.as_utf8().and_then(|s| float_text(&s)).ok_or_else(|| {
                error(
                    "ValueError",
                    format!("could not convert string to float: {}", s.repr()),
                )
            }),
            _ => Err(error(
                "TypeError",
                format!(
                    "float() argument must be a string or a real number, not '{}'",
                    self.kind()
                ),
            )),
        }
    }
}
impl std::str::FromStr for MetadataInteger {
    type Err = CodecError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

struct Parser {
    text: Vec<char>,
    position: usize,
    depth: usize,
}
impl Parser {
    fn fail(&self, message: &str) -> CodecError {
        let line = self.text[..self.position]
            .iter()
            .filter(|&&ch| ch == '\n')
            .count()
            + 1;
        let column = self.text[..self.position]
            .iter()
            .rposition(|&ch| ch == '\n')
            .map_or(self.position + 1, |index| self.position - index);
        error(
            "JSONDecodeError",
            format!(
                "{message}: line {line} column {column} (char {})",
                self.position
            ),
        )
    }
    fn peek(&self) -> Option<char> {
        self.text.get(self.position).copied()
    }
    fn whitespace(&mut self) {
        while self
            .peek()
            .is_some_and(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))
        {
            self.position += 1;
        }
    }
    fn literal(&mut self, text: &str) -> bool {
        let size = text.chars().count();
        if self
            .text
            .get(self.position..self.position + size)
            .is_some_and(|chars| chars.iter().copied().eq(text.chars()))
        {
            self.position += size;
            true
        } else {
            false
        }
    }
    fn string(&mut self) -> Result<MetadataString, CodecError> {
        self.position += 1;
        let mut points = Vec::new();
        loop {
            let c = self
                .peek()
                .ok_or_else(|| self.fail("Unterminated string starting at"))?;
            self.position += 1;
            match c {
                '"' => break,
                '\\' => {
                    let c = self
                        .peek()
                        .ok_or_else(|| self.fail("Unterminated string starting at"))?;
                    self.position += 1;
                    points.push(match c {
                        '"' => 34,
                        '\\' => 92,
                        '/' => 47,
                        'b' => 8,
                        'f' => 12,
                        'n' => 10,
                        'r' => 13,
                        't' => 9,
                        'u' => {
                            let mut point = 0;
                            for _ in 0..4 {
                                let c = self
                                    .peek()
                                    .and_then(|ch| ch.to_digit(16))
                                    .ok_or_else(|| self.fail("Invalid \\uXXXX escape"))?;
                                point = point * 16 + c;
                                self.position += 1;
                            }
                            point
                        }
                        _ => return Err(self.fail("Invalid \\escape")),
                    });
                }
                c if (c as u32) < 32 => return Err(self.fail("Invalid control character at")),
                c => points.push(c as u32),
            }
        }
        let mut combined = Vec::new();
        let mut iter = points.into_iter().peekable();
        while let Some(point) = iter.next() {
            if (0xd800..=0xdbff).contains(&point)
                && iter
                    .peek()
                    .is_some_and(|next| (0xdc00..=0xdfff).contains(next))
            {
                let next = iter.next().unwrap();
                combined.push(0x10000 + ((point - 0xd800) << 10) + (next - 0xdc00));
            } else {
                combined.push(point);
            }
        }
        Ok(MetadataString(combined))
    }
    fn value(&mut self) -> Result<Datum, CodecError> {
        self.whitespace();
        self.depth += 1;
        if self.depth > 128 {
            return Err(error(
                "RecursionError",
                "maximum recursion depth exceeded while decoding a JSON object from a unicode string",
            ));
        }
        let value = match self.peek() {
            Some('"') => Datum::String(self.string()?),
            Some('{') => {
                self.position += 1;
                self.whitespace();
                let mut map = MetadataObject::new();
                if self.peek() == Some('}') {
                    self.position += 1;
                } else {
                    loop {
                        self.whitespace();
                        if self.peek() != Some('"') {
                            return Err(
                                self.fail("Expecting property name enclosed in double quotes")
                            );
                        }
                        let key = self.string()?.as_utf8().unwrap_or_default();
                        self.whitespace();
                        if self.peek() != Some(':') {
                            return Err(self.fail("Expecting ':' delimiter"));
                        }
                        self.position += 1;
                        map.insert(key, self.value()?);
                        self.whitespace();
                        match self.peek() {
                            Some('}') => {
                                self.position += 1;
                                break;
                            }
                            Some(',') => self.position += 1,
                            _ => return Err(self.fail("Expecting ',' delimiter")),
                        }
                    }
                }
                Datum::Object(map)
            }
            Some('[') => {
                self.position += 1;
                self.whitespace();
                let mut values = Vec::new();
                if self.peek() == Some(']') {
                    self.position += 1;
                } else {
                    loop {
                        values.push(self.value()?);
                        self.whitespace();
                        match self.peek() {
                            Some(']') => {
                                self.position += 1;
                                break;
                            }
                            Some(',') => self.position += 1,
                            _ => return Err(self.fail("Expecting ',' delimiter")),
                        }
                    }
                }
                Datum::Array(values)
            }
            _ if self.literal("null") => Datum::Null,
            _ if self.literal("true") => Datum::Bool(true),
            _ if self.literal("false") => Datum::Bool(false),
            _ if self.literal("NaN") => Datum::Number("NaN".into()),
            _ if self.literal("Infinity") => Datum::Number("Infinity".into()),
            _ if self.literal("-Infinity") => Datum::Number("-Infinity".into()),
            Some('-' | '0'..='9') => {
                let start = self.position;
                if self.peek() == Some('-') {
                    self.position += 1;
                }
                match self.peek() {
                    Some('0') => self.position += 1,
                    Some('1'..='9') => {
                        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                            self.position += 1;
                        }
                    }
                    _ => return Err(self.fail("Expecting value")),
                }
                if self.peek() == Some('.')
                    && self
                        .text
                        .get(self.position + 1)
                        .is_some_and(char::is_ascii_digit)
                {
                    self.position += 1;
                    while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                        self.position += 1;
                    }
                }
                if matches!(self.peek(), Some('e' | 'E')) {
                    let exponent = self.position;
                    self.position += 1;
                    if matches!(self.peek(), Some('+' | '-')) {
                        self.position += 1;
                    }
                    if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
                        self.position = exponent;
                    } else {
                        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                            self.position += 1;
                        }
                    }
                }
                Datum::Number(self.text[start..self.position].iter().collect())
            }
            _ => return Err(self.fail("Expecting value")),
        };
        self.depth -= 1;
        Ok(value)
    }
}

pub(crate) fn parse_stdout(stdout: &[u8]) -> Result<VideoInfo, VideoError> {
    (||{
        let text=String::from_utf8_lossy(stdout);let mut parser=Parser{text:text.chars().collect(),position:0,depth:0};
        if parser.peek()==Some('\u{feff}'){return Err(parser.fail("Unexpected UTF-8 BOM (decode using utf-8-sig)"));}
        let data=parser.value()?;parser.whitespace();if parser.peek().is_some(){return Err(parser.fail("Extra data"));}
        let streams=data.get("streams",Datum::Null)?;
        let streams=if !streams.truth(){Vec::new()}else{match streams{Datum::Array(v)=>v,Datum::Object(map)=>map.keys().map(|s|Datum::String(MetadataString::from(s.as_str()))).collect(),Datum::String(s)=>s.0.into_iter().map(|c|Datum::String(MetadataString(vec![c]))).collect(),value=>return Err(error("TypeError",format!("'{}' object is not iterable",value.kind())))}};
        let mut video=Datum::Object(MetadataObject::new());
        for stream in &streams{if matches!(stream.get("codec_type",Datum::Null)?,Datum::String(ref s) if s.equals("video")){video=stream.clone();break;}}
        let mut has_audio=false;for stream in &streams{if matches!(stream.get("codec_type",Datum::Null)?,Datum::String(ref s) if s.equals("audio")){has_audio=true;break;}}
        let format=data.get("format",Datum::Object(MetadataObject::new()))?;
        let or_zero=|v:Datum|if v.truth(){v}else{Datum::Number("0".into())};
        let width=or_zero(video.get("width",Datum::Number("0".into()))?).integer()?;
        let height=or_zero(video.get("height",Datum::Number("0".into()))?).integer()?;
        let duration=or_zero(format.get("duration",Datum::Number("0".into()))?).float()?;
        let codec=video.get("codec_name",Datum::String(MetadataString::from("")))?.string();
        let rate=video.get("avg_frame_rate",Datum::String(MetadataString::from("0/1")))?.string();
        let container=format.get("format_name",Datum::String(MetadataString::from("")))?.string();
        Ok(VideoInfo{width,height,duration,codec,fps:fps(&rate),has_audio,container})
    })().map_err(known)
}
impl VideoInfo {
    pub fn from_stdout(stdout: &[u8]) -> Result<Self, CodecError> {
        parse_stdout(stdout).map_err(|error| error.error)
    }
    pub fn to_value(&self) -> Result<Value, CodecError> {
        let unrepresentable =
            || CodecError::unverified("video metadata cannot be represented losslessly as JSON");
        if !self.duration.is_finite() || !self.fps.is_finite() {
            return Err(unrepresentable());
        }
        Ok(
            serde_json::json!({"width":Value::Number(self.width.0.parse().map_err(|_|unrepresentable())?),"height":Value::Number(self.height.0.parse().map_err(|_|unrepresentable())?),"duration":self.duration,"codec":self.codec.as_utf8().ok_or_else(unrepresentable)?,"fps":self.fps,"has_audio":self.has_audio,"container":self.container.as_utf8().ok_or_else(unrepresentable)?}),
        )
    }
    pub(crate) fn alpha_decoder(&self) -> Result<Option<&'static str>, VideoError> {
        if self.codec.0.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            match self.codec.as_utf8().map(|s| s.to_lowercase()).as_deref() {
                Some("vp9") => "libvpx-vp9",
                Some("vp8") => "libvpx",
                _ => "",
            },
        ))
    }
}
