//! Native preview cache policy; the adapter only returns exact rendered codec bytes.
use crate::media_adapter::MediaAdapter;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};

pub struct Previews {
    adapter: MediaAdapter,
    cache: PathBuf,
    renders: Semaphore,
    locks: Mutex<std::collections::BTreeMap<PathBuf, Arc<Mutex<()>>>>,
}
impl Previews {
    pub fn new(adapter: MediaAdapter, data: &Path, workers: usize) -> Self {
        Self {
            adapter,
            cache: data.join("preview"),
            renders: Semaphore::new(workers.clamp(1, 6)),
            locks: Mutex::new(std::collections::BTreeMap::new()),
        }
    }
    fn destination(
        &self,
        key: &str,
        source: &Path,
        fps: u64,
        still: bool,
        size: u64,
    ) -> Result<PathBuf, String> {
        if key.contains('/')
            || key.contains('\\')
            || ![52, 72, 104].contains(&size)
            || !(1..=30).contains(&fps)
        {
            return Err("invalid preview parameters".into());
        }
        let ext = source
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext == "tgs" && size == 104 {
            let legacy = self.cache.join(format!(
                "{}@{}.webp",
                key.replace(':', "_"),
                if still {
                    "still".into()
                } else {
                    fps.to_string()
                }
            ));
            if legacy.is_file() {
                return Ok(legacy);
            }
        }
        let digest = Sha256::digest(key.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let tier = if still {
            "still".into()
        } else if ext == "webm" {
            format!("v{fps}")
        } else {
            fps.to_string()
        };
        Ok(self.cache.join(format!("{digest}@{tier}-{size}.webp")))
    }
    pub async fn warm(&self, view: &serde_json::Value, maximum_fps: u64) -> serde_json::Value {
        let (mut rendered, mut cached, mut failed) = (0, 0, 0);
        if let Some(cards) = view["view"].as_array() {
            for (size, fps) in [(104, maximum_fps.min(15)), (72, maximum_fps.min(10))] {
                for card in cards {
                    if card["isLogo"] == true {
                        continue;
                    }
                    let Some(key) = card["key"].as_str().filter(|key| !key.is_empty()) else {
                        continue;
                    };
                    let Some(source) = view["by_key"][key].as_str().map(Path::new) else {
                        continue;
                    };
                    let moving = matches!(card["fmt"].as_str(), Some("animated" | "video"));
                    let modes: &[bool] = if moving {
                        &[true, false]
                    } else if size == 72 && card["included"] == false {
                        &[true]
                    } else {
                        &[]
                    };
                    for &still in modes {
                        if self
                            .destination(key, source, fps, still, size)
                            .is_ok_and(|p| p.is_file())
                        {
                            cached += 1;
                        } else if self.bytes(key, source, fps, still, size).await.is_ok() {
                            rendered += 1;
                        } else {
                            failed += 1;
                        }
                    }
                }
            }
        }
        serde_json::json!({"rendered":rendered,"cached":cached,"failed":failed})
    }
    pub async fn bytes(
        &self,
        key: &str,
        source: &Path,
        fps: u64,
        still: bool,
        size: u64,
    ) -> Result<Vec<u8>, String> {
        let dest = self.destination(key, source, fps, still, size)?;
        let ext = source
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if dest.is_file() {
            return std::fs::read(dest).map_err(|e| e.to_string());
        }
        let lock = {
            self.locks
                .lock()
                .await
                .entry(dest.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _guard = lock.lock().await;
        if dest.is_file() {
            return std::fs::read(dest).map_err(|e| e.to_string());
        }
        if !source.is_file() || source.is_symlink() {
            return Err("preview source must be a regular non-symlink file".into());
        }
        let _render = self
            .renders
            .acquire()
            .await
            .map_err(|_| "preview renderer stopped")?;
        std::fs::create_dir_all(&self.cache).map_err(|e| e.to_string())?;
        let tmp = dest.with_extension(format!("{}.tmp.webp", std::process::id()));
        if tmp.exists() {
            return Err("preview staging path is already occupied".into());
        }
        let staged = Staged(tmp);
        let fmt = match ext.as_str() {
            "tgs" => "animated",
            "webm" => "video",
            _ => "static",
        };
        self.adapter
            .call(
                serde_json::json!({"operation":"preview","source":source,"format":fmt,
            "key":key,"fps":fps,"size":size,"still":still,"output":staged.0}),
            )
            .await?;
        let bytes = std::fs::read(&staged.0).map_err(|e| e.to_string())?;
        if bytes.is_empty() {
            return Err("empty preview renderer output".into());
        }
        std::fs::rename(&staged.0, &dest).map_err(|e| e.to_string())?;
        Ok(bytes)
    }
}
struct Staged(PathBuf);
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
