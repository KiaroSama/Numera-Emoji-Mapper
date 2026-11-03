//! Canonical vector identity; only source-compatible decoding errors permit raw-byte fallback.
use crate::codec_error::CodecError;
use pyo3::prelude::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

pub fn same_image(a: &Path, b: &Path) -> Option<bool> {
    same_image_typed(a, b).ok().flatten()
}

pub fn fingerprint(path: &Path) -> Result<Value, String> {
    fingerprint_typed(path).map_err(|error| error.message)
}

pub fn same_image_typed(a: &Path, b: &Path) -> Result<Option<bool>, CodecError> {
    let compared = (|| {
        let a = fingerprint_classified(a)?;
        let b = fingerprint_classified(b)?;
        Ok((a["content_key"] == b["content_key"]).then_some(true))
    })();
    match compared {
        Ok(value) => Ok(value),
        Err((_, true)) => Ok(None),
        Err((error, false)) => Err(error),
    }
}

pub fn fingerprint_typed(path: &Path) -> Result<Value, CodecError> {
    fingerprint_classified(path).map_err(|(error, _)| error)
}

fn fingerprint_classified(path: &Path) -> Result<Value, (CodecError, bool)> {
    let bytes = match crate::codec_lottie_load::canonical_bytes(path) {
        Ok(bytes) => bytes,
        Err(error) if error.raw_fallback => Python::attach(|py| {
            crate::codec_lottie_load::source_bytes(py, path).map_err(|error| {
                let caught = error.is_instance_of::<pyo3::exceptions::PyException>(py);
                (CodecError::from_python(py, error), caught)
            })
        })?,
        Err(error) => {
            return Err((
                CodecError {
                    kind: error.kind,
                    message: error.message,
                    handled: error.handled,
                },
                error.caught_by_exception,
            ));
        }
    };
    let digest = Sha256::digest(&bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect::<String>();
    Ok(json!({
        "content_key":format!("a:{}", &digest[..32]),
        "phash":null,"fmt":"animated","extension":".tgs"
    }))
}
