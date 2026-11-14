//! Animated ingest retains warning order, original bytes on refusal, and atomic replacement.
use crate::codec_error::CodecError;
use serde_json::Value;
use std::path::Path;

#[derive(Debug)]
pub struct AnimatedRewrite {
    pub rewritten: bool,
    pub warning: Option<String>,
}

fn skipped(path: &Path, action: &str, message: &str) -> AnimatedRewrite {
    AnimatedRewrite {
        rewritten: false,
        warning: Some(crate::logging::redact(
            &format!(
                "{action} skipped for {} (animated): {message}",
                path.file_name().unwrap_or_default().to_string_lossy(),
            ),
            &[],
        )),
    }
}

pub fn rewrite_typed(path: &Path, tint: Option<[u8; 3]>) -> Result<AnimatedRewrite, CodecError> {
    if !path.is_absolute()
        || !std::fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
    {
        return Err(CodecError::value(
            "animated source must be an absolute regular non-symlink file",
        ));
    }
    // Initial reencode reads and final persistence are outside the codec-error warning catch.
    let before = if tint.is_none() {
        Some(crate::native_lottie::source_bytes(path)?)
    } else {
        None
    };
    let action = if tint.is_some() {
        "repaint"
    } else {
        "re-encode"
    };
    let mut document = match crate::native_lottie::load(path) {
        Ok(document) => document,
        Err(error) if error.caught_by_exception => {
            return Ok(skipped(path, action, &error.message));
        }
        Err(error) => return Err(error.into()),
    };
    if let Some(rgb) = tint {
        document.tint(rgb);
    }
    let bytes = match crate::codec_lottie_package::encode_object(&document, true) {
        Ok(bytes) => bytes,
        Err(error) => return Ok(skipped(path, action, &error.message)),
    };
    if before.as_deref() == Some(bytes.as_slice()) {
        return Ok(AnimatedRewrite {
            rewritten: false,
            warning: None,
        });
    }
    if bytes.len() > crate::native_lottie::MAX_TGS_BYTES {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let message = if tint.is_some() {
            format!(
                "repaint of {name} would be {} bytes, over the 65536 cap; keeping the original",
                bytes.len()
            )
        } else {
            format!(
                "re-encode of {name} would be {} bytes, over the animated cap of 65536; keeping the original",
                bytes.len()
            )
        };
        return Ok(AnimatedRewrite {
            rewritten: false,
            warning: Some(crate::logging::redact(&message, &[])),
        });
    }
    crate::atomic::write_bytes(path, &bytes)
        .map_err(|_| CodecError::unverified("cannot replace prepared animated source"))?;
    Ok(AnimatedRewrite {
        rewritten: true,
        warning: None,
    })
}

pub fn rewrite(path: &Path, tint: Option<[u8; 3]>) -> Result<bool, String> {
    let outcome = rewrite_typed(path, tint).map_err(|error| error.message)?;
    if let Some(warning) = outcome.warning {
        eprintln!("WARNING: {}", crate::logging::redact(&warning, &[]));
    }
    Ok(outcome.rewritten)
}

pub fn prepare(
    source: &Path,
    request: &Value,
    warnings: &mut Vec<String>,
) -> Result<Value, CodecError> {
    let reencode = request
        .get("reencode")
        .unwrap_or(&Value::Bool(false))
        .as_bool()
        .ok_or("reencode must be boolean")?;
    if reencode {
        warnings.extend(rewrite_typed(source, None)?.warning);
    }
    if let Some(tint) = request.get("tint") {
        let rgb = crate::codec_tint::parse_tint(tint.as_str().ok_or("tint must be #RRGGBB")?)
            .map_err(CodecError::media)?;
        let outcome = rewrite_typed(source, Some(rgb))?;
        warnings.extend(outcome.warning);
        if !outcome.rewritten {
            return Err(CodecError::value("requested tint could not be applied"));
        }
    }
    crate::codec_lottie_identity::fingerprint_typed(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn retained_prepare_fixtures_keep_bytes_repeat_behavior_and_oversize_originals() {
        let root = crate::native_lottie::fixture_root();
        let scratch = Scratch(std::env::temp_dir().join(format!(
            "numera-lottie-prepare-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        )));
        std::fs::create_dir(&scratch.0).unwrap();
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/native/tgs-prepare.json"))
                .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let path = scratch
                .0
                .join(format!("{}.tgs", case["name"].as_str().unwrap()));
            std::fs::copy(root.join(case["input"].as_str().unwrap()), &path).unwrap();
            let tint = case["tint"]
                .as_str()
                .map(crate::codec_tint::parse_tint)
                .transpose()
                .unwrap();
            let result = rewrite_typed(&path, tint).unwrap();
            assert_eq!(
                result.rewritten, case["expected"]["changed"],
                "{}",
                case["name"]
            );
            assert_eq!(
                rewrite_typed(&path, tint).unwrap().rewritten,
                case["expected"]["repeat"],
                "{}",
                case["name"]
            );
            let bytes = std::fs::read(&path).unwrap();
            let hash = Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            assert_eq!(
                bytes.len(),
                case["expected"]["bytes"].as_u64().unwrap() as usize,
                "{}",
                case["name"]
            );
            assert_eq!(hash, case["expected"]["sha256"], "{}", case["name"]);
        }
    }
}
