//! Bounded parser for actual reqwest fixture form uploads; no interpreter decoder.
use std::collections::BTreeMap;
type Parts = (BTreeMap<String, String>, BTreeMap<String, Vec<u8>>);
fn find(bytes: &[u8], needle: &[u8]) -> Option<usize> {
    bytes.windows(needle.len()).position(|part| part == needle)
}
pub fn parse(content_type: &str, body: &[u8]) -> Result<Parts, String> {
    let refuse = || "malformed fixture MIME tree".to_owned();
    if body.len() > 65536 || content_type.len() > 256 || content_type.contains(['\r', '\n']) {
        return Err("excessive or invalid fixture multipart request".into());
    }
    let boundary = content_type
        .split(';')
        .skip(1)
        .find_map(|part| part.trim().strip_prefix("boundary="))
        .ok_or_else(refuse)?
        .trim_matches('"');
    if boundary.is_empty() || !boundary.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(refuse());
    }
    let delimiter = format!("--{boundary}");
    let mut remaining = body.strip_prefix(delimiter.as_bytes()).ok_or_else(refuse)?;
    let (mut fields, mut files) = (BTreeMap::new(), BTreeMap::new());
    loop {
        if remaining == b"--\r\n" || remaining == b"--" {
            break;
        }
        if fields.len() + files.len() >= 8 {
            return Err(refuse());
        }
        remaining = remaining.strip_prefix(b"\r\n").ok_or_else(refuse)?;
        let end = find(remaining, b"\r\n\r\n").ok_or_else(refuse)?;
        let headers = std::str::from_utf8(&remaining[..end]).map_err(|_| refuse())?;
        let disposition = headers
            .split("\r\n")
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(key, _)| key.eq_ignore_ascii_case("content-disposition"))
                    .map(|(_, value)| value.trim())
            })
            .ok_or_else(refuse)?;
        if !disposition.starts_with("form-data;") {
            return Err(refuse());
        }
        let name = disposition
            .split(';')
            .skip(1)
            .find_map(|part| part.trim().strip_prefix("name=\""))
            .and_then(|name| name.strip_suffix('"'))
            .ok_or_else(refuse)?;
        if name.is_empty() || fields.contains_key(name) || files.contains_key(name) {
            return Err(refuse());
        }
        let data = &remaining[end + 4..];
        let separator = format!("\r\n{delimiter}");
        let next = find(data, separator.as_bytes()).ok_or_else(refuse)?;
        let bytes = &data[..next];
        if disposition
            .split(';')
            .any(|part| part.trim().starts_with("filename="))
        {
            files.insert(name.to_owned(), bytes.to_vec());
        } else {
            fields.insert(
                name.to_owned(),
                std::str::from_utf8(bytes).map_err(|_| refuse())?.to_owned(),
            );
        }
        remaining = &data[next + separator.len()..];
    }
    Ok((fields, files))
}
