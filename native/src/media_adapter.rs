//! Exact codec/identity bridge with owned child-tree lifetime, never backend decisions.
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);

pub struct MediaAdapter {
    pub python: PathBuf,
    pub root: PathBuf,
    pub scratch: PathBuf,
}

impl MediaAdapter {
    pub async fn call(&self, request: Value) -> Result<Value, String> {
        #[cfg(target_os = "linux")]
        verify_inherited_group_bridge(&self.root)?;
        let codec_config = crate::config::Config::load(self.root.clone())?;
        let ffmpeg_timeout = codec_config.integer("EMOJI_FFMPEG_TIMEOUT", 300, 1, i64::MAX);
        std::fs::create_dir_all(&self.scratch).map_err(|e| e.to_string())?;
        let directory = loop {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = self
                .scratch
                .join(format!("codec-{}-{id}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => break path,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.to_string()),
            }
        };
        let request_path = directory.join("request.json");
        let response_path = directory.join("response.json");
        if let Err(error) = crate::atomic::write_json(&request_path, &request) {
            let _ = std::fs::remove_dir_all(&directory);
            return Err(error.to_string());
        }
        let mut command = Command::new(&self.python);
        command
            .arg("-m")
            .arg("emojikit.media_bridge")
            .arg(&request_path)
            .arg(&response_path)
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env("PYTHONUTF8", "1")
            .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
            .env("EMOJI_FFMPEG_TIMEOUT", ffmpeg_timeout.to_string())
            .env("TEMP", &directory)
            .env("TMP", &directory)
            .env("TMPDIR", &directory);
        for (name, _) in std::env::vars() {
            let upper = name.to_uppercase();
            if ["TOKEN", "SECRET", "PASSWORD", "API_KEY"]
                .iter()
                .any(|key| upper.contains(key))
            {
                command.env_remove(name);
            }
        }
        // The owner is the only workspace holder once a child can exist. Its Drop
        // kills containment and synchronously reaps before any scratch deletion.
        let mut owner = crate::process_owner::OwnedProcess::spawn(&mut command, directory)?;
        let result = async {
            let status = owner
                .wait(Duration::from_secs(320), Duration::from_secs(60))
                .await?;
            let file = std::fs::File::open(&response_path)
                .map_err(|_| "media adapter returned no response".to_owned())?;
            let metadata = file
                .metadata()
                .map_err(|_| "cannot inspect codec response".to_owned())?;
            if !metadata.is_file() || metadata.len() > 65536 {
                return Err("media adapter response must be a file under64KiB".into());
            }
            let mut bytes = Vec::new();
            file.take(65537)
                .read_to_end(&mut bytes)
                .map_err(|_| "cannot read codec response".to_owned())?;
            if bytes.len() > 65536 {
                return Err("media adapter response exceeds64KiB".into());
            }
            let response: Value = serde_json::from_slice(&bytes)
                .map_err(|_| "invalid media adapter response".to_owned())?;
            if !status.success() || response.get("ok") != Some(&Value::Bool(true)) {
                return Err(response
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("media adapter failed")
                    .to_owned());
            }
            response
                .get("result")
                .cloned()
                .ok_or_else(|| "media adapter response has no result".into())
        }
        .await;
        // Cleanup failure wins over success: a codec result cannot hide a survivor.
        owner.finish()?;
        result
    }

    pub async fn compare(&self, a: &Path, b: &Path, fmt: &str) -> Result<Option<bool>, String> {
        let value = self
            .call(serde_json::json!({"operation":"compare","source":a,"other":b,"format":fmt}))
            .await?;
        if value.is_null() {
            Ok(None)
        } else {
            value
                .as_bool()
                .map(Some)
                .ok_or_else(|| "invalid media comparison result".into())
        }
    }
}

#[cfg(target_os = "linux")]
fn verify_inherited_group_bridge(root: &Path) -> Result<(), String> {
    // This explicit protocol marker is added only with the Python _run flag
    // implementation. Legacy codecs start FFmpeg in a new session and escape PGID.
    let source = std::fs::read_to_string(root.join("emojikit/media.py"))
        .map_err(|_| "cannot verify Linux codec containment capability".to_owned())?;
    if !source
        .lines()
        .any(|line| line.trim() == "NATIVE_CODEC_GROUP_VERSION = 1")
    {
        return Err("Linux codec bridge needs inherited-group protocol1 in media._run; refusing unowned FFmpeg".into());
    }
    Ok(())
}
