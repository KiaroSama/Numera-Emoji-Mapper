//! Native media sniffing and stable collision identities.
use crate::codec_error::CodecError;
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};

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
fn header(path: &Path) -> Result<Vec<u8>, CodecError> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(16)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}
pub fn detect_typed(path: &Path) -> Result<&'static str, CodecError> {
    let head = header(path)?;
    let detected = detect_bytes(&head);
    if detected != "unknown" {
        return Ok(detected);
    }
    Ok(
        match path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase()
            .as_str()
        {
            "tgs" => "animated",
            "webm" | "mp4" | "mov" | "mkv" | "m4v" | "avi" => "video",
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "apng" | "svg" => "static",
            _ => "unknown",
        },
    )
}
pub fn detect(path: &Path) -> Result<&'static str, String> {
    detect_typed(path).map_err(|e| e.message)
}
pub fn extension_typed(path: &Path, format: &str) -> Result<&'static str, CodecError> {
    if format == "video" {
        return Ok(".webm");
    }
    if format == "animated" {
        return Ok(".tgs");
    }
    let head = header(path)?;
    Ok(if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        ".png"
    } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        ".gif"
    } else if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP") {
        ".webp"
    } else {
        ".png"
    })
}
pub fn extension(path: &Path, format: &str) -> Result<&'static str, String> {
    extension_typed(path, format).map_err(|e| e.message)
}
/// Hash media with constant memory, including the fallback identity of large sources.
pub fn file_sha256(path: &Path) -> Result<String, CodecError> {
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

pub fn collision_key_typed(path: &Path, sampled_key: &str) -> Result<String, CodecError> {
    let (prefix, remainder) = sampled_key.split_once(':').unwrap_or((sampled_key, ""));
    let lookup = remainder.rsplit(':').next().unwrap_or("");
    let digest = file_sha256(path)?;
    Ok(format!("{prefix}:{}:{lookup}", &digest[..32]))
}
pub fn collision_key(path: &Path, key: &str) -> Result<String, String> {
    collision_key_typed(path, key).map_err(|e| e.message)
}
pub fn collision_request(
    path: &Path,
    key: &serde_json::Value,
) -> Result<serde_json::Value, CodecError> {
    let key = key
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| CodecError::value("collision requires a content key"))?;
    collision_key_typed(path, key).map(serde_json::Value::String)
}
