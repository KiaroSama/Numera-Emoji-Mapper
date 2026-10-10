//! Digest-keyed brand cache; an unreadable/wrong-sized cache cannot lead a new pack.
use crate::{config::Config, media_adapter::MediaAdapter};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Staged(std::path::PathBuf);
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub async fn prepare(
    media: &MediaAdapter,
    source: &Path,
    data: &Path,
    config: &Config,
) -> Result<Value, String> {
    if !source.is_file() || source.is_symlink() {
        return Err("required brand logo is missing".into());
    }
    let digest = Sha256::digest(std::fs::read(source).map_err(|e| e.to_string())?);
    let digest = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let dir = data.join("brand");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let output = dir.join(format!("logo_{}.png", &digest[..12]));
    let usable = output.is_file()
        && !output.is_symlink()
        && media
            .call(json!({"operation":"dimensions","source":output,"format":"static"}))
            .await
            == Ok(json!([100, 100]));
    if !usable {
        let staging = loop {
            let path = dir.join(format!(
                ".logo-{}-{}.png",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => break Staged(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.to_string()),
            }
        };
        let rendered = staging.0.join("logo.png");
        media.call(json!({"operation":"convert","source":source,"output":rendered,"target_format":"static"})).await?;
        if media
            .call(json!({"operation":"dimensions","source":rendered,"format":"static"}))
            .await?
            != json!([100, 100])
        {
            return Err("brand codec returned wrong dimensions".into());
        }
        std::fs::rename(&rendered, &output).map_err(|e| e.to_string())?;
    }
    let mut item = media
        .call(json!({"operation":"fingerprint","source":output,"format":"static"}))
        .await?;
    item["file_path"] = json!(output);
    item["emojis"] = json!(["✅"]);
    let keywords = config
        .value("BRAND_LOGO_KEYWORDS")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    item["keywords"] = if keywords.is_empty() {
        json!(["logo"])
    } else {
        json!(keywords)
    };
    Ok(item)
}
