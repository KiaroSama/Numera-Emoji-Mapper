//! Alpha-faithful video probe and frozen canonical frame stream; no timeline shortcut.
use crate::codec_video_cache::{DecodeKey, FileIdentity, WorkingSet};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct VideoDecoder {
    pub(crate) root: PathBuf,
    pub(crate) ffmpeg: PathBuf,
    ffprobe: PathBuf,
    decoders: Mutex<WorkingSet<DecodeKey, Vec<String>>>,
    capabilities: Mutex<WorkingSet<(String, FileIdentity), bool>>,
    frames: Mutex<WorkingSet<DecodeKey, Arc<Vec<u8>>>>,
    timelines: Mutex<WorkingSet<DecodeKey, crate::codec_video_timeline::Timeline>>,
}

impl VideoDecoder {
    pub fn new(root: &Path) -> Result<Self, String> {
        Ok(Self {
            root: root.into(),
            ffmpeg: crate::codec_process::executable("ffmpeg")?,
            ffprobe: crate::codec_process::executable("ffprobe")?,
            decoders: Mutex::new(WorkingSet::new(512)),
            capabilities: Mutex::new(WorkingSet::new(512)),
            frames: Mutex::new(WorkingSet::new(8)),
            timelines: Mutex::new(WorkingSet::new(8)),
        })
    }

    pub(crate) fn validate_source(path: &Path) -> Result<(), String> {
        let metadata =
            std::fs::symlink_metadata(path).map_err(|_| "cannot inspect video source")?;
        if !path.is_absolute() || !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("video source must be an absolute regular non-symlink file".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("video source must not be a reparse point".into());
            }
        }
        Ok(())
    }

    pub async fn probe(&self, path: &Path) -> Result<Value, String> {
        Self::validate_source(path)?;
        let args = vec!["-v".into(),"error".into(),"-show_entries".into(),
            "stream=index,codec_type,codec_name,width,height,avg_frame_rate:format=duration,format_name".into(),
            "-of".into(),"json".into(),path.to_string_lossy().into_owned()];
        let captured =
            crate::codec_process::capture(&self.root, &self.ffprobe, &args, 1024 * 1024).await?;
        let value: Value =
            serde_json::from_slice(&captured.stdout).map_err(|_| "invalid FFprobe JSON")?;
        let empty = Vec::new();
        let streams = value
            .get("streams")
            .and_then(Value::as_array)
            .unwrap_or(&empty);
        let video = streams
            .iter()
            .find(|stream| stream["codec_type"] == "video");
        let number = |key: &str| {
            video
                .and_then(|v| v.get(key))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        };
        let codec = video
            .and_then(|v| v.get("codec_name"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let rate = video
            .and_then(|v| v.get("avg_frame_rate"))
            .and_then(Value::as_str)
            .unwrap_or("0/1");
        let (numerator, denominator) = rate.split_once('/').unwrap_or((rate, "1"));
        let fps = numerator
            .parse::<f64>()
            .ok()
            .zip(denominator.parse::<f64>().ok())
            .filter(|(_, d)| *d != 0.0)
            .map(|(n, d)| n / d)
            .unwrap_or(0.0);
        let format = value.get("format");
        let duration = format
            .and_then(|v| v.get("duration"))
            .and_then(Value::as_str)
            .unwrap_or("0")
            .parse::<f64>()
            .map_err(|_| "invalid FFprobe duration")?;
        Ok(
            json!({"width":number("width"),"height":number("height"),"codec":codec,
            "duration":duration,"fps":fps,"has_audio":streams.iter().any(|s| s["codec_type"] == "audio"),
            "container":format.and_then(|v| v.get("format_name")).and_then(Value::as_str).unwrap_or("")}),
        )
    }

    pub async fn decoder_args(&self, path: &Path) -> Result<Vec<String>, String> {
        Self::validate_source(path)?;
        if crate::codec_format::detect(path)? != "video" {
            return Ok(Vec::new());
        }
        let key = DecodeKey::read(path, &self.ffmpeg, None);
        if let Some(key) = &key
            && let Some(args) = self
                .decoders
                .lock()
                .map_err(|_| "video decoder cache poisoned")?
                .get(key)
        {
            return Ok(args);
        }
        let info = self.probe(path).await?;
        let codec = info["codec"]
            .as_str()
            .ok_or("missing video codec")?
            .to_lowercase();
        if codec.is_empty() {
            return Err(
                "video container named no codec; alpha fidelity cannot be established".into(),
            );
        }
        let decoder = match codec.as_str() {
            "vp9" => "libvpx-vp9",
            "vp8" => "libvpx",
            _ => {
                if let Some(key) = key {
                    self.decoders
                        .lock()
                        .map_err(|_| "video decoder cache poisoned")?
                        .insert(key, Vec::new());
                }
                return Ok(Vec::new());
            }
        };
        let capability = FileIdentity::read(&self.ffmpeg).map(|tool| (decoder.to_owned(), tool));
        let known = capability
            .as_ref()
            .and_then(|key| self.capabilities.lock().ok()?.get(key));
        let available = if let Some(available) = known {
            available
        } else {
            let args = vec!["-v".into(), "error".into(), "-decoders".into()];
            let captured =
                crate::codec_process::capture(&self.root, &self.ffmpeg, &args, 1024 * 1024).await?;
            let available = captured
                .stdout
                .windows(decoder.len())
                .any(|window| window == decoder.as_bytes());
            if let Some(key) = capability {
                self.capabilities
                    .lock()
                    .map_err(|_| "video capability cache poisoned")?
                    .insert(key, available);
            }
            available
        };
        if !available {
            return Err(format!(
                "ffmpeg has no {decoder} decoder; alpha fidelity cannot be established"
            ));
        }
        let args = vec!["-c:v".into(), decoder.into()];
        if let Some(key) = key {
            self.decoders
                .lock()
                .map_err(|_| "video decoder cache poisoned")?
                .insert(key, args.clone());
        }
        Ok(args)
    }

    pub async fn frames_rgba(&self, path: &Path, fps: u32) -> Result<Vec<u8>, String> {
        if fps == 0 || fps > 30 {
            return Err("video sample rate must be1..30".into());
        }
        Self::validate_source(path)?;
        let key = DecodeKey::read(path, &self.ffmpeg, Some(fps));
        if let Some(key) = &key
            && let Some(raw) = self
                .frames
                .lock()
                .map_err(|_| "video frame cache poisoned")?
                .get(key)
        {
            return Ok((*raw).clone());
        }
        let decoder = self.decoder_args(path).await?;
        let sample = |rate: bool| {
            let mut args = vec![
                "-nostdin".into(),
                "-v".into(),
                "error".into(),
                "-t".into(),
                "3.0".into(),
            ];
            args.extend(decoder.clone());
            args.extend([
                "-i".into(),
                path.to_string_lossy().into_owned(),
                "-an".into(),
                "-threads".into(),
                "1".into(),
                "-filter_threads".into(),
                "1".into(),
                "-vf".into(),
                format!(
                    "{}scale=64:64,format=rgba",
                    if rate {
                        format!("fps={fps},")
                    } else {
                        String::new()
                    }
                ),
                "-f".into(),
                "rawvideo".into(),
                "-".into(),
            ]);
            args
        };
        let raw = crate::codec_process::capture(
            &self.root,
            &self.ffmpeg,
            &sample(true),
            16 * 1024 * 1024,
        )
        .await?
        .stdout;
        let raw = if raw.is_empty() {
            crate::codec_process::capture(
                &self.root,
                &self.ffmpeg,
                &sample(false),
                16 * 1024 * 1024,
            )
            .await?
            .stdout
        } else {
            raw
        };
        if !raw.is_empty()
            && let Some(key) = key
        {
            self.frames
                .lock()
                .map_err(|_| "video frame cache poisoned")?
                .insert(key, Arc::new(raw.clone()));
        }
        Ok(raw)
    }

    pub async fn timeline(
        &self,
        path: &Path,
    ) -> Result<crate::codec_video_timeline::Timeline, String> {
        Self::validate_source(path)?;
        let key = DecodeKey::read(path, &self.ffmpeg, None);
        if let Some(key) = &key
            && let Some(value) = self
                .timelines
                .lock()
                .map_err(|_| "video timeline cache poisoned")?
                .get(key)
        {
            return Ok(value);
        }
        let value = crate::codec_video_timeline::decode(self, path).await?;
        if let Some(key) = key {
            self.timelines
                .lock()
                .map_err(|_| "video timeline cache poisoned")?
                .insert(key, value.clone());
        }
        Ok(value)
    }

    pub async fn same_image(&self, a: &Path, b: &Path) -> Option<bool> {
        let left = self.timeline(a).await.ok()?;
        let right = self.timeline(b).await.ok()?;
        crate::codec_video_timeline::same_image(&left, &right)
    }

    pub async fn fingerprint(&self, path: &Path) -> Result<Value, String> {
        let raw = self.frames_rgba(path, 10).await?;
        let phash = crate::codec_image::frame_phash(&raw);
        let bytes = if raw.is_empty() {
            std::fs::read(path).map_err(|_| "cannot read undecodable video source")?
        } else {
            raw
        };
        let digest = Sha256::digest(&bytes)
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>();
        Ok(
            json!({"content_key":format!("v:{}", &digest[..32]), "phash":phash,
            "fmt":"video", "extension":".webm"}),
        )
    }

    pub async fn content_key(&self, path: &Path) -> Result<String, String> {
        self.fingerprint(path).await?["content_key"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "missing video content key".into())
    }

    pub async fn perceptual_hash(&self, path: &Path) -> Result<Option<u64>, String> {
        Ok(crate::codec_image::frame_phash(
            &self.frames_rgba(path, 10).await?,
        ))
    }
}
