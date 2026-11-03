//! Animated ingest preserves source codec catches, warning order and atomic replacement.
use crate::codec_error::CodecError;
use pyo3::prelude::*;
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
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
    {
        return Err(CodecError::value(
            "animated source must be an absolute regular non-symlink file",
        ));
    }
    Python::initialize();
    let encoded = Python::attach(|py| {
        // The source's initial reencode read is outside its codec-error catch.
        let before = if tint.is_none() {
            Some(
                crate::codec_lottie_load::source_bytes(py, path)
                    .map_err(|error| CodecError::from_python(py, error))?,
            )
        } else {
            None
        };
        let action = if tint.is_some() {
            "repaint"
        } else {
            "re-encode"
        };
        let document = match crate::codec_lottie_load::load_object(py, path) {
            Ok(document) => document,
            Err(error) if error.caught_by_exception => {
                return Ok(Err(skipped(path, action, &error.message)));
            }
            Err(error) => {
                return Err(CodecError {
                    kind: error.kind,
                    message: error.message,
                    handled: error.handled,
                });
            }
        };
        let encoded = (|| -> PyResult<Vec<u8>> {
            if let Some(rgb) = tint {
                crate::codec_tint::tint_lottie(
                    &document,
                    rgb.map(|channel| f64::from(channel) / 255.0),
                )?;
            }
            crate::codec_lottie_package::encode_object(py, &document, true)
        })();
        match encoded {
            Ok(bytes) => Ok(Ok((bytes, before))),
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyException>(py) => {
                let error = CodecError::from_python(py, error);
                Ok(Err(skipped(path, action, &error.message)))
            }
            Err(error) => Err(CodecError::from_python(py, error)),
        }
    })?;
    let (bytes, before) = match encoded {
        Ok(encoded) => encoded,
        Err(outcome) => return Ok(outcome),
    };
    if before.as_deref() == Some(bytes.as_slice()) {
        return Ok(AnimatedRewrite {
            rewritten: false,
            warning: None,
        });
    }
    if bytes.len() > 65536 {
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
    // Persistence is outside the source's codec-error catch: a failed write cannot report success.
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
        let outcome = rewrite_typed(source, None)?;
        warnings.extend(outcome.warning);
    }
    if let Some(tint) = request.get("tint") {
        let tint = tint.as_str().ok_or("tint must be #RRGGBB")?;
        let rgb = crate::codec_tint::parse_tint(tint).map_err(CodecError::media)?;
        let outcome = rewrite_typed(source, Some(rgb))?;
        warnings.extend(outcome.warning);
        if !outcome.rewritten {
            return Err(CodecError::value("requested tint could not be applied"));
        }
    }
    crate::codec_lottie_identity::fingerprint_typed(source)
}
