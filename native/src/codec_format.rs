//! Source media sniffing and collision identities, distinct from local target-format selection.
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

pub fn detect_bytes(head: &[u8]) -> &'static str {
    if head.starts_with(b"\x1a\x45\xdf\xa3") {
        "video"
    } else if head.starts_with(b"\x1f\x8b") {
        "animated"
    } else if head.starts_with(b"\x89PNG\r\n\x1a\n")
        || head.starts_with(b"GIF87a")
        || head.starts_with(b"GIF89a")
        || (head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP"))
    {
        "static"
    } else {
        "unknown"
    }
}

fn header(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(16).read_to_end(&mut bytes))
        .map_err(|_| "cannot read media header")?;
    Ok(bytes)
}

pub fn detect(path: &Path) -> Result<&'static str, String> {
    let head = header(path)?;
    let format = detect_bytes(&head);
    if format != "unknown" {
        return Ok(format);
    }
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_lowercase();
    Ok(match extension.as_str() {
        "tgs" => "animated",
        "webm" | "mp4" | "mov" | "mkv" | "m4v" | "avi" => "video",
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "apng" | "svg" => "static",
        _ => "unknown",
    })
}

pub fn extension(path: &Path, format: &str) -> Result<&'static str, String> {
    match format {
        "video" => return Ok(".webm"),
        "animated" => return Ok(".tgs"),
        _ => {}
    }
    let head = header(path)?;
    Ok(
        if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP") {
            ".webp"
        } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
            ".gif"
        } else {
            ".png"
        },
    )
}

pub fn collision_key(path: &Path, sampled_key: &str) -> Result<String, String> {
    let (prefix, remainder) = sampled_key.split_once(':').unwrap_or((sampled_key, ""));
    let lookup = remainder.rsplit(':').next().unwrap_or("");
    let bytes = std::fs::read(path).map_err(|_| "cannot read collision identity source")?;
    let digest = Sha256::digest(bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect::<String>();
    Ok(format!("{prefix}:{}:{lookup}", &digest[..32]))
}
