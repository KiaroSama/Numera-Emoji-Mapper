//! Deterministic bounded TGS packaging followed by exclusive publication.
use crate::{codec_error::CodecError, native_lottie::Document};
use std::path::Path;

pub(crate) fn encode_object(
    document: &Document,
    ensure_ascii: bool,
) -> Result<Vec<u8>, CodecError> {
    document.encode_gzip(ensure_ascii).map_err(Into::into)
}

pub fn package_typed(source: &Path, output: &Path) -> Result<(), CodecError> {
    if !output.is_absolute() || std::fs::symlink_metadata(output).is_ok() {
        return Err(CodecError::value(
            "TGS output must be absolute and must not already exist",
        ));
    }
    let source_name = source
        .file_name()
        .ok_or_else(|| CodecError::value("source has no filename"))?
        .to_string_lossy();
    let output_name = output
        .file_name()
        .ok_or_else(|| CodecError::value("TGS output has no filename"))?
        .to_string_lossy();
    let document = crate::native_lottie::load(source)?;
    let bytes = document.package_bytes(&source_name, &output_name)?;
    crate::codec_image::publish_bytes(output, "tgs", &bytes)?;
    Ok(())
}

pub fn package(source: &Path, output: &Path) -> Result<(), String> {
    package_typed(source, output).map_err(|error| error.message)
}
