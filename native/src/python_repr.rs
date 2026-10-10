//! CPython3.11/UCD14 row repr ordering for original version2 signatures, not a Python backend.
use rusqlite::types::Value;
use std::sync::OnceLock;
fn printable(c: char) -> bool {
    static RANGES: OnceLock<Vec<[u32; 2]>> = OnceLock::new();
    let ranges = RANGES.get_or_init(|| {
        let doc: serde_json::Value =
            serde_json::from_str(include_str!("../data/python311-nonprintable.json"))
                .expect("checked builtin Unicode table");
        serde_json::from_value(doc["nonprintable"].clone()).expect("checked Unicode ranges")
    });
    let code = c as u32;
    let index = ranges.partition_point(|range| range[1] < code);
    ranges.get(index).is_none_or(|range| range[0] > code)
}
fn quoted(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if !printable(c) => {
                let n = c as u32;
                out += &if n <= 255 {
                    format!("\\x{n:02x}")
                } else if n <= 65535 {
                    format!("\\u{n:04x}")
                } else {
                    format!("\\U{n:08x}")
                };
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}
fn bytes_repr(bytes: &[u8]) -> String {
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    let mut out = format!("b{}", quote as char);
    for &b in bytes {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'\t' => out.push_str("\\t"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b if b == quote => {
                out.push('\\');
                out.push(b as char);
            }
            32..=126 => out.push(b as char),
            b => out += &format!("\\x{b:02x}"),
        }
    }
    out.push(quote as char);
    out
}
pub fn row(row: &[Value]) -> Result<String, String> {
    let values = row
        .iter()
        .map(|value| match value {
            Value::Null => Ok("None".into()),
            Value::Integer(n) => Ok(n.to_string()),
            Value::Text(text) => Ok(quoted(text)),
            Value::Blob(bytes) => Ok(bytes_repr(bytes)),
            Value::Real(_) => Err(
                "version2 signature REAL formatting is not verified; refusing before writes".into(),
            ),
        })
        .collect::<Result<Vec<String>, String>>()?;
    Ok(format!("[{}]", values.join(", ")))
}
