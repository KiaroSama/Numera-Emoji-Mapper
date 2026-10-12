//! Animated ingest rewrites preserve the original on codec failure or Telegram size refusal.
use std::path::Path;

pub fn rewrite(path: &Path, tint: Option<[u8; 3]>) -> Result<bool, String> {
    if !path.is_absolute()
        || !std::fs::symlink_metadata(path)
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
    {
        return Err("animated source must be an absolute regular non-symlink file".into());
    }
    let before = std::fs::read(path).map_err(|_| "cannot read animated preparation source")?;
    let Ok(mut document) = crate::codec_lottie_load::load(path) else {
        return Ok(false);
    };
    if let Some(rgb) = tint {
        crate::codec_tint::tint_lottie(
            &mut document,
            rgb.map(|channel| f64::from(channel) / 255.0),
        );
    }
    let Ok(bytes) = crate::codec_lottie_package::encode(&document, true) else {
        return Ok(false);
    };
    if bytes.len() > 65536 || (tint.is_none() && bytes == before) {
        return Ok(false);
    }
    // Persistence is outside the source's codec-error catch: a failed write cannot report success.
    crate::atomic::write_bytes(path, &bytes)
        .map_err(|_| "cannot replace prepared animated source")?;
    Ok(true)
}
