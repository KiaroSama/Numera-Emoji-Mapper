//! Source request validation preserves short-circuit ordering and JSON hashability errors.
use crate::codec_error::CodecError;
use serde_json::{Value, json};

pub(crate) fn unhashable(value: &Value) -> Option<String> {
    let kind = match value {
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
        _ => return None,
    };
    Some(format!("unhashable type: '{kind}'"))
}

pub(crate) fn preview(request: &Value) -> Result<(u32, u32, bool), CodecError> {
    request
        .get("key")
        .and_then(Value::as_str)
        .filter(|key| !key.contains(['/', '\\']))
        .ok_or_else(|| CodecError::value("invalid preview parameters"))?;
    let size = request.get("size").cloned().unwrap_or(json!(104));
    let fps = request.get("fps").cloned().unwrap_or(json!(15));
    let still = request.get("still").cloned().unwrap_or(json!(false));
    if let Some(message) = unhashable(&size) {
        return Err(CodecError::type_error(message));
    }
    // Python accepts a numerically equal float size, but bool and float fps are invalid.
    let size = size
        .as_f64()
        .filter(|value| [52.0, 72.0, 104.0].contains(value))
        .map(|value| value as u32)
        .ok_or_else(|| CodecError::value("invalid preview parameters"))?;
    let fps = fps
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| (1..=30).contains(value))
        .ok_or_else(|| CodecError::value("invalid preview parameters"))?;
    let still = still
        .as_bool()
        .ok_or_else(|| CodecError::value("invalid preview parameters"))?;
    Ok((size, fps, still))
}
