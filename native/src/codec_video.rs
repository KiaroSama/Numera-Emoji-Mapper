//! Alpha-faithful video probe and frozen canonical frame stream; no timeline shortcut.
use crate::codec_error::CodecError;
use crate::codec_video_cache::{DecodeKey, FileIdentity, WorkingSet};
use crate::codec_video_error::{CommandContext, VideoFailure as DecodeFailure, capture_result};
pub use crate::codec_video_probe::VideoInfo;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

fn fingerprint_result(bytes: &[u8], phash: Option<u64>) -> Value {
    let digest = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    json!({"content_key":format!("v:{}", &digest[..32]), "phash":phash,
        "fmt":"video", "extension":".webm"})
}

/// Apply identity policy to the canonical sample stream; only empty successful samples use bytes.
pub fn fingerprint_frames_typed(path: &Path, raw: &[u8]) -> Result<Value, CodecError> {
    if raw.is_empty() {
        pyo3::Python::initialize();
        let bytes = pyo3::Python::attach(|py| {
            crate::codec_lottie_load::source_bytes(py, path)
                .map_err(|error| CodecError::from_python(py, error))
        })?;
        return Ok(fingerprint_result(&bytes, None));
    }
    let phash = crate::codec_raster::frame_phash_typed(raw)?;
    Ok(fingerprint_result(raw, phash))
}

pub fn fingerprint_at(root: Option<&Path>, path: &Path) -> Result<Value, CodecError> {
    let root =
        root.ok_or_else(|| CodecError::value("video worker requires an explicit project root"))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| CodecError::unverified("cannot initialize video worker runtime"))?;
    let decoder = VideoDecoder::new(root).map_err(CodecError::unverified)?;
    runtime.block_on(decoder.fingerprint_typed(path))
}

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
        self.probe_settled(path).await?
    }

    pub(crate) async fn probe_settled(&self, path: &Path) -> Result<Result<Value, String>, String> {
        self.probe_core(path, CommandContext::Direct)
            .await
            .map(|result| {
                result
                    .map_err(DecodeFailure::legacy)
                    .and_then(|info| info.to_value().map_err(|error| error.message))
            })
    }

    pub async fn probe_typed(&self, path: &Path) -> Result<Value, CodecError> {
        self.probe_info_typed(path).await?.to_value()
    }

    /// Source metadata without narrowing Python integers, strings or nonfinite floats to JSON.
    pub async fn probe_info_typed(&self, path: &Path) -> Result<VideoInfo, CodecError> {
        self.probe_core(path, CommandContext::Direct)
            .await
            .map_err(CodecError::unverified)?
            .map_err(|error| error.typed(path))
    }

    pub(crate) async fn probe_core(
        &self,
        path: &Path,
        context: CommandContext,
    ) -> Result<Result<VideoInfo, DecodeFailure>, String> {
        Self::validate_source(path)?;
        let args = vec!["-v".into(),"error".into(),"-show_entries".into(),
            "stream=index,codec_type,codec_name,width,height,avg_frame_rate:format=duration,format_name".into(),
            "-of".into(),"json".into(),path.to_string_lossy().into_owned()];
        let settled = crate::codec_process::capture_outcome_input(
            &self.root,
            &self.ffprobe,
            &args,
            1024 * 1024,
            None,
        )
        .await?;
        let captured = match capture_result(&self.ffprobe, &args, context, settled) {
            Ok(captured) => captured,
            Err(error) => return Ok(Err(error)),
        };
        Ok(crate::codec_video_probe::parse_stdout(&captured.stdout)
            .map_err(|failure| DecodeFailure::Metadata { failure, context }))
    }

    pub async fn decoder_args(&self, path: &Path) -> Result<Vec<String>, String> {
        self.decoder_args_settled(path).await?
    }

    pub async fn decoder_args_typed(&self, path: &Path) -> Result<Vec<String>, CodecError> {
        self.decoder_args_core(path, crate::codec_video_timeline::Policy::Source)
            .await
            .map_err(CodecError::unverified)?
            .map_err(|error| error.typed(path))
    }

    pub(crate) async fn decoder_args_settled(
        &self,
        path: &Path,
    ) -> Result<Result<Vec<String>, String>, String> {
        self.decoder_args_core(path, crate::codec_video_timeline::Policy::Legacy)
            .await
            .map(|result| result.map_err(DecodeFailure::legacy))
    }

    pub(crate) async fn decoder_args_core(
        &self,
        path: &Path,
        policy: crate::codec_video_timeline::Policy,
    ) -> Result<Result<Vec<String>, DecodeFailure>, String> {
        Self::validate_source(path)?;
        let format = match policy {
            crate::codec_video_timeline::Policy::Legacy => crate::codec_format::detect(path)?,
            crate::codec_video_timeline::Policy::Source => {
                match crate::codec_format::detect_external(path) {
                    Ok(format) => format,
                    Err(error) => {
                        return Ok(Err(DecodeFailure::Header(pyo3::Python::attach(|py| {
                            crate::codec_video_error::VideoError::from_python(py, error)
                        }))));
                    }
                }
            }
        };
        if format != "video" {
            return Ok(Ok(Vec::new()));
        }
        let key = DecodeKey::read(path, &self.ffmpeg, None);
        if let Some(key) = &key
            && let Some(args) = self
                .decoders
                .lock()
                .map_err(|_| "video decoder cache poisoned")?
                .get(key)
        {
            return Ok(Ok(args));
        }
        let info = match self.probe_core(path, CommandContext::Probe).await? {
            Ok(info) => info,
            Err(error) => return Ok(Err(error)),
        };
        let decoder = match info.alpha_decoder() {
            Ok(None) => return Ok(Err(DecodeFailure::NoCodec)),
            Ok(Some("")) => {
                if let Some(key) = key {
                    self.decoders
                        .lock()
                        .map_err(|_| "video decoder cache poisoned")?
                        .insert(key, Vec::new());
                }
                return Ok(Ok(Vec::new()));
            }
            Ok(Some(decoder)) => decoder,
            Err(failure) => {
                return Ok(Err(DecodeFailure::Metadata {
                    failure,
                    context: CommandContext::Probe,
                }));
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
            let settled = crate::codec_process::capture_outcome_input(
                &self.root,
                &self.ffmpeg,
                &args,
                1024 * 1024,
                None,
            )
            .await?;
            let captured =
                match capture_result(&self.ffmpeg, &args, CommandContext::Capability, settled) {
                    Ok(captured) => captured,
                    Err(error) => return Ok(Err(error)),
                };
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
            let codec = if decoder == "libvpx-vp9" {
                "vp9"
            } else {
                "vp8"
            }
            .into();
            return Ok(Err(DecodeFailure::MissingDecoder { codec, decoder }));
        }
        let args = vec!["-c:v".into(), decoder.into()];
        if let Some(key) = key {
            self.decoders
                .lock()
                .map_err(|_| "video decoder cache poisoned")?
                .insert(key, args.clone());
        }
        Ok(Ok(args))
    }

    pub async fn frames_rgba(&self, path: &Path, fps: u32) -> Result<Vec<u8>, String> {
        self.frames_rgba_core(path, fps, crate::codec_video_timeline::Policy::Legacy)
            .await
            .map_err(DecodeFailure::legacy)
    }

    pub async fn frames_rgba_typed(&self, path: &Path, fps: u32) -> Result<Vec<u8>, CodecError> {
        self.frames_rgba_core(path, fps, crate::codec_video_timeline::Policy::Source)
            .await
            .map_err(|error| error.typed(path))
    }

    async fn frames_rgba_core(
        &self,
        path: &Path,
        fps: u32,
        policy: crate::codec_video_timeline::Policy,
    ) -> Result<Vec<u8>, DecodeFailure> {
        if fps == 0 || fps > 30 {
            return Err(DecodeFailure::Unverified(
                "video sample rate must be1..30".into(),
            ));
        }
        Self::validate_source(path).map_err(DecodeFailure::Unverified)?;
        let key = DecodeKey::read(path, &self.ffmpeg, Some(fps));
        if let Some(key) = &key
            && let Some(raw) = self
                .frames
                .lock()
                .map_err(|_| DecodeFailure::Unverified("video frame cache poisoned".into()))?
                .get(key)
        {
            return Ok((*raw).clone());
        }
        let decoder = self
            .decoder_args_core(path, policy)
            .await
            .map_err(DecodeFailure::Unverified)??;
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
        let capture = async |args: Vec<String>| {
            let settled = crate::codec_process::capture_outcome_input(
                &self.root,
                &self.ffmpeg,
                &args,
                16 * 1024 * 1024,
                None,
            )
            .await
            .map_err(DecodeFailure::Unverified)?;
            capture_result(&self.ffmpeg, &args, CommandContext::Direct, settled)
                .map(|captured| captured.stdout)
        };
        let raw = capture(sample(true)).await?;
        let raw = if raw.is_empty() {
            capture(sample(false)).await?
        } else {
            raw
        };
        if !raw.is_empty()
            && let Some(key) = key
        {
            self.frames
                .lock()
                .map_err(|_| DecodeFailure::Unverified("video frame cache poisoned".into()))?
                .insert(key, Arc::new(raw.clone()));
        }
        Ok(raw)
    }

    pub async fn timeline(
        &self,
        path: &Path,
    ) -> Result<crate::codec_video_timeline::Timeline, String> {
        self.timeline_core(path, crate::codec_video_timeline::Policy::Legacy)
            .await
            .map_err(|error| error.error.message)
    }

    pub async fn timeline_typed(
        &self,
        path: &Path,
    ) -> Result<crate::codec_video_timeline::Timeline, CodecError> {
        self.timeline_core(path, crate::codec_video_timeline::Policy::Source)
            .await
            .map_err(|error| error.error)
    }

    async fn timeline_core(
        &self,
        path: &Path,
        policy: crate::codec_video_timeline::Policy,
    ) -> Result<crate::codec_video_timeline::Timeline, crate::codec_video_error::VideoError> {
        use crate::codec_video_timeline::unknown;
        Self::validate_source(path).map_err(unknown)?;
        let key = DecodeKey::read(path, &self.ffmpeg, None);
        if let Some(key) = &key
            && let Some(value) = self
                .timelines
                .lock()
                .map_err(|_| unknown("video timeline cache poisoned"))?
                .get(key)
        {
            return Ok(value);
        }
        let value = crate::codec_video_timeline::decode(self, path, policy).await?;
        if let Some(key) = key {
            self.timelines
                .lock()
                .map_err(|_| unknown("video timeline cache poisoned"))?
                .insert(key, value.clone());
        }
        Ok(value)
    }

    pub async fn same_image(&self, a: &Path, b: &Path) -> Option<bool> {
        self.compare_core(a, b, crate::codec_video_timeline::Policy::Legacy)
            .await
            .ok()
            .flatten()
    }

    /// Source `except Exception` is undecidable; interruption and native custody errors escape.
    pub async fn same_image_typed(&self, a: &Path, b: &Path) -> Result<Option<bool>, CodecError> {
        match self
            .compare_core(a, b, crate::codec_video_timeline::Policy::Source)
            .await
        {
            Ok(same) => Ok(same),
            Err(error) if error.caught_by_exception => Ok(None),
            Err(error) => Err(error.error),
        }
    }

    async fn compare_core(
        &self,
        a: &Path,
        b: &Path,
        policy: crate::codec_video_timeline::Policy,
    ) -> Result<Option<bool>, crate::codec_video_error::VideoError> {
        let left = self.timeline_core(a, policy).await?;
        let right = self.timeline_core(b, policy).await?;
        crate::codec_video_timeline::compare(&left, &right, policy)
    }

    pub async fn fingerprint(&self, path: &Path) -> Result<Value, String> {
        let raw = self.frames_rgba(path, 10).await?;
        let phash = crate::codec_image::frame_phash(&raw);
        let bytes = if raw.is_empty() {
            std::fs::read(path).map_err(|_| "cannot read undecodable video source")?
        } else {
            raw
        };
        Ok(fingerprint_result(&bytes, phash))
    }

    pub async fn fingerprint_typed(&self, path: &Path) -> Result<Value, CodecError> {
        let raw = self.frames_rgba_typed(path, 10).await?;
        fingerprint_frames_typed(path, &raw)
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
