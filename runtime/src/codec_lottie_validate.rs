use crate::codec_error::CodecError;
use serde_json::Value;
use std::path::Path;

pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}
pub fn validate_file_typed(path: &Path) -> Result<(), CodecError> {
    crate::native_lottie::validate_file(path).map_err(Into::into)
}
pub fn validate_file(path: &Path) -> Result<(), String> {
    validate_file_typed(path).map_err(|e| e.message)
}
pub fn validate_document_typed(
    value: &Value,
    filename: &str,
    bytes: u64,
) -> Result<(), CodecError> {
    crate::native_lottie::Document::from_value(value)?
        .validate(filename, bytes)
        .map_err(Into::into)
}
pub fn validate_document(value: &Value, filename: &str, bytes: u64) -> Result<(), String> {
    validate_document_typed(value, filename, bytes).map_err(|e| e.message)
}
