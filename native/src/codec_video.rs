//! Alpha-faithful video probe and frozen canonical frame stream; no timeline shortcut.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub struct VideoDecoder {
    root: PathBuf,
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
}

impl VideoDecoder {
    pub fn new(root: &Path) -> Result<Self, String> {
        Ok(Self {
            root: root.into(),
            ffmpeg: crate::codec_process::executable("ffmpeg")?,
            ffprobe: crate::codec_process::executable("ffprobe")?,
        })
    }

    pub async fn probe(&self, path: &Path) -> Result<Value, String> {
        if !path.is_absolute()
            || !std::fs::symlink_metadata(path)
                .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
        {
            return Err("video source must be an absolute regular non-symlink file".into());
        }
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
        if crate::codec_format::detect(path)? != "video" {
            return Ok(Vec::new());
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
            _ => return Ok(Vec::new()),
        };
        let args = vec!["-v".into(), "error".into(), "-decoders".into()];
        let captured =
            crate::codec_process::capture(&self.root, &self.ffmpeg, &args, 1024 * 1024).await?;
        if !captured
            .stdout
            .windows(decoder.len())
            .any(|window| window == decoder.as_bytes())
        {
            return Err(format!(
                "ffmpeg has no {decoder} decoder; alpha fidelity cannot be established"
            ));
        }
        Ok(vec!["-c:v".into(), decoder.into()])
    }

    pub async fn frames_rgba(&self, path: &Path, fps: u32) -> Result<Vec<u8>, String> {
        if fps == 0 || fps > 30 {
            return Err("video sample rate must be1..30".into());
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
        if !raw.is_empty() {
            return Ok(raw);
        }
        Ok(crate::codec_process::capture(
            &self.root,
            &self.ffmpeg,
            &sample(false),
            16 * 1024 * 1024,
        )
        .await?
        .stdout)
    }

    pub async fn content_key(&self, path: &Path) -> Result<String, String> {
        let raw = self.frames_rgba(path, 10).await?;
        let bytes = if raw.is_empty() {
            std::fs::read(path).map_err(|_| "cannot read undecodable video source")?
        } else {
            raw
        };
        let digest = Sha256::digest(&bytes)
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>();
        Ok(format!("v:{}", &digest[..32]))
    }
}
