//! Integration-only binary location and independent diagnostic expectations.
use std::path::PathBuf;
#[path = "worker_fixture.rs"]
mod worker_fixture;
pub use worker_fixture::{test_environment, worker};
pub fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji"))
}
fn quoted(text: &str) -> String {
    // Independent historical list representation; no target formatter or interpreter.
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch == quote => {
                out.push('\\');
                out.push(ch);
            }
            ch => out.push(ch),
        }
    }
    out.push(quote);
    out
}
pub fn command_failure<'a>(tool: &str, args: impl Iterator<Item = &'a str>, code: i32) -> String {
    let tool = numera_runtime::codec_process::executable(tool).unwrap();
    let command = std::iter::once(quoted(&tool.to_string_lossy()))
        .chain(args.map(quoted))
        .collect::<Vec<_>>()
        .join(", ");
    format!("Command '[{command}]' returned non-zero exit status {code}.")
}

pub fn verify_frozen_json(name: &str, actual: &serde_json::Value) {
    let bytes: &[u8] = match name {
        "lottie-preview.json" => {
            include_bytes!("../../../tests/fixtures/native/lottie-preview.json")
        }
        _ => panic!("unlisted frozen corpus"),
    };
    let frozen: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(
        actual, &frozen,
        "external corpus differs from immutable source fixture"
    );
}
