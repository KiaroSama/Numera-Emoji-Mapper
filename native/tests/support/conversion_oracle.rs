//! PNG compression bytes come from the admitted historical encoder on this platform.
//! Pixel identity and the input recipe remain pinned independently of the native target.
use serde_json::Value;
use std::path::PathBuf;

pub fn expected(input: &str) -> Value {
    let (original, provider) = _native::source_static_oracle::fixtures().unwrap();
    let fixture = std::env::var_os("NUMERA_STATIC_CONVERT_ORACLE")
        .map(PathBuf::from)
        .map(|path| serde_json::from_slice::<Value>(&std::fs::read(path).unwrap()).unwrap())
        .unwrap_or(original);
    _native::source_static_oracle::fixture_identity(&fixture, &provider).unwrap();
    fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["input"] == input)
        .unwrap_or_else(|| panic!("historical conversion recipe has no {input}"))["expected"]
        .clone()
}
