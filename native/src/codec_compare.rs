//! Worker comparison keeps the byte shortcut outside the identity probe's unknown result.
use crate::codec_error::CodecError;
use pyo3::prelude::*;
use serde_json::{Value, json};
use std::path::Path;

fn same_bytes(source: &Path, other: &Path) -> Result<bool, CodecError> {
    Python::initialize();
    Python::attach(|py| {
        let result = (|| -> PyResult<bool> {
            let class = py.import("pathlib")?.getattr("Path")?;
            let source_path = class.call1((source.to_path_buf(),))?;
            let other_path = class.call1((other.to_path_buf(),))?;
            let left = source_path.call_method0("stat")?.getattr("st_size")?;
            let right = other_path.call_method0("stat")?.getattr("st_size")?;
            if !left.eq(&right)? {
                return Ok(false);
            }
            let left = crate::codec_lottie_load::source_bytes(py, source)?;
            let right = crate::codec_lottie_load::source_bytes(py, other)?;
            Ok(left == right)
        })();
        result.map_err(|error| CodecError::from_python(py, error))
    })
}

pub fn execute_at(
    root: Option<&Path>,
    source: &Path,
    other: &Path,
    format: &str,
    compare_bytes: bool,
) -> Result<Value, CodecError> {
    // Source stat/read failures escape before same_image's ordinary-error catch.
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
