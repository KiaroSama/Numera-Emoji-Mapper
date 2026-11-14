//! Byte comparison precedes format-specific identity's ordinary-error catch.
use crate::codec_error::CodecError;
use serde_json::{Value, json};
use std::path::Path;

pub(crate) fn same_bytes(source: &Path, other: &Path) -> Result<bool, CodecError> {
    use std::io::{BufRead, BufReader};
    if std::fs::metadata(source)?.len() != std::fs::metadata(other)?.len() {
        return Ok(false);
    }
    let mut left = BufReader::with_capacity(65536, std::fs::File::open(source)?);
    let mut right = BufReader::with_capacity(65536, std::fs::File::open(other)?);
    loop {
        let a = left.fill_buf()?;
        let b = right.fill_buf()?;
        let size = a.len().min(b.len());
        if size == 0 {
            return Ok(a.is_empty() && b.is_empty());
        }
        if a[..size] != b[..size] {
            return Ok(false);
        }
        left.consume(size);
        right.consume(size);
    }
}

pub fn execute_at(
    root: Option<&Path>,
    source: &Path,
    other: &Path,
    format: &str,
    compare_bytes: bool,
) -> Result<Value, CodecError> {
    if compare_bytes && same_bytes(source, other)? {
        return Ok(json!(true));
    }
    let result = match format {
        "static" => crate::codec_image::same_image_typed(source, other)?,
        "animated" => crate::codec_lottie_identity::same_image_typed(source, other)?,
        "video" => {
            let root = root.ok_or_else(|| {
                CodecError::value("video worker requires an explicit project root")
            })?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| CodecError::unverified("cannot initialize video worker runtime"))?;
            let decoder =
                crate::codec_video::VideoDecoder::new(root).map_err(CodecError::unverified)?;
            runtime.block_on(decoder.same_image_typed(source, other))?
        }
        _ => return Err(CodecError::value("unsupported media format")),
    };
    Ok(json!(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn historical_animated_byte_shortcut_and_unknown_policy_are_preserved() {
        let root = crate::native_lottie::fixture_root();
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/native/lottie-file.json"))
                .unwrap();
        for case in fixture["comparisons"].as_array().unwrap() {
            let a = root.join(case["a"].as_str().unwrap());
            let b = root.join(case["b"].as_str().unwrap());
            assert_eq!(
                execute_at(None, &a, &b, "animated", false).unwrap(),
                case["same_image"]
            );
            assert_eq!(
                execute_at(None, &a, &b, "animated", true).unwrap(),
                case["compare"]
            );
        }
    }
}
