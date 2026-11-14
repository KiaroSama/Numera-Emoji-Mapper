//! Canonical vector identity and the historical unknown-result comparison policy.
use crate::codec_error::CodecError;
use serde_json::Value;
use std::path::Path;

pub fn fingerprint_typed(path: &Path) -> Result<Value, CodecError> {
    crate::native_lottie::fingerprint(path).map_err(Into::into)
}

pub fn fingerprint(path: &Path) -> Result<Value, String> {
    fingerprint_typed(path).map_err(|error| error.message)
}

pub fn same_image_typed(a: &Path, b: &Path) -> Result<Option<bool>, CodecError> {
    let fingerprint = |path: &Path| {
        crate::native_lottie::fingerprint(path).map_err(|error| {
            let caught = error.caught_by_exception;
            (CodecError::from(error), caught)
        })
    };
    let compared = (|| {
        let a = fingerprint(a)?;
        let b = fingerprint(b)?;
        Ok((a["content_key"] == b["content_key"]).then_some(true))
    })();
    match compared {
        Ok(value) => Ok(value),
        Err((_, true)) => Ok(None),
        Err((error, false)) => Err(error),
    }
}

pub fn same_image(a: &Path, b: &Path) -> Option<bool> {
    same_image_typed(a, b).ok().flatten()
}
