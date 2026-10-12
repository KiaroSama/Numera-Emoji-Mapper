//! Canonical vector identity; only source-compatible decoding errors permit raw-byte fallback.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

pub fn same_image(a: &Path, b: &Path) -> Option<bool> {
    let a = fingerprint(a).ok()?;
    let b = fingerprint(b).ok()?;
    (a["content_key"] == b["content_key"]).then_some(true)
}

pub fn fingerprint(path: &Path) -> Result<Value, String> {
    let bytes = match crate::codec_lottie_load::canonical_bytes(path) {
        Ok(bytes) => bytes,
        Err(error) if error.raw_fallback => {
            std::fs::read(path).map_err(|_| "cannot read animated identity source")?
        }
        Err(error) => return Err(error.message),
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
