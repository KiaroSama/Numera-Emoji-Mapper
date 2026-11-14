//! Owned VP9 conversion and pixel-exact stream-copy preparation; originals survive refusal.
use crate::codec_error::CodecError;
use crate::codec_video::VideoDecoder;
use crate::codec_video_error::{CommandContext, VideoFailure, capture_result};
pub use crate::codec_video_remux::{VideoReencode, reencode, reencode_typed};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(crate) struct Staged(pub PathBuf, bool);
impl Drop for Staged {
    fn drop(&mut self) {
        if !self.1 {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}
impl Staged {
    pub(crate) fn new(output: &Path, extension: &str) -> Result<Self, String> {
        if !output.is_absolute() || std::fs::symlink_metadata(output).is_ok() {
            return Err("codec output must be absolute and must not already exist".into());
        }
        let parent = output.parent().ok_or("codec output has no parent")?;
        std::fs::create_dir_all(parent).map_err(|_| "cannot create codec output directory")?;
        loop {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(
                ".video-output-{}-{id}.{extension}",
                std::process::id()
            ));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Self(path, false)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err("cannot stage video output".into()),
            }
        }
    }

    pub(crate) fn for_remux(source: &Path) -> Result<Self, String> {
        let marker = source.with_file_name(format!(
            ".remux-placeholder-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        Self::new(&marker, "webm")
    }

    pub(crate) fn retain(&mut self) {
        self.1 = true;
    }

    pub(crate) fn publish(&self, output: &Path) -> Result<u64, String> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.0)
            .map_err(|_| "cannot inspect staged video output")?;
        file.sync_all().map_err(|_| "cannot flush video output")?;
        let bytes = file
            .metadata()
            .map_err(|_| "cannot inspect video output")?
            .len();
        drop(file);
        crate::atomic::publish_new(&self.0, output)
            .map_err(|_| "cannot publish exclusive video output")?;
        Ok(bytes)
    }

    pub(crate) fn clean(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.0) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("cannot clean staged video output".into()),
        }
    }
}

#[derive(Clone, Copy)]
enum ConvertErrors {
    Legacy,
    Source,
}
impl ConvertErrors {
    fn command(self, failure: VideoFailure, source: &Path) -> CodecError {
        match self {
            Self::Legacy => CodecError::unverified(failure.legacy()),
            Self::Source => failure.typed(source),
        }
    }

    fn oversize(self, message: String) -> CodecError {
        match self {
            Self::Legacy => CodecError::unverified(message),
            Self::Source => CodecError::media(message),
        }
    }
}

pub async fn convert(
    decoder: &VideoDecoder,
    source: &Path,
    output: &Path,
    max_bytes: u64,
) -> Result<Value, String> {
    convert_with(decoder, source, output, max_bytes, ConvertErrors::Legacy)
        .await
        .map_err(|error| error.message)
}

pub async fn convert_typed(
    decoder: &VideoDecoder,
    source: &Path,
    output: &Path,
    max_bytes: u64,
) -> Result<Value, CodecError> {
    convert_with(decoder, source, output, max_bytes, ConvertErrors::Source).await
}

pub fn convert_at(root: Option<&Path>, source: &Path, output: &Path) -> Result<Value, CodecError> {
    let root =
        root.ok_or_else(|| CodecError::value("video worker requires an explicit project root"))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| CodecError::unverified("cannot initialize video worker runtime"))?;
    let decoder = VideoDecoder::new(root).map_err(CodecError::unverified)?;
    runtime.block_on(async {
        convert_typed(&decoder, source, output, 262144).await?;
        decoder.fingerprint_typed(output).await
    })
}

async fn convert_with(
    decoder: &VideoDecoder,
    source: &Path,
    output: &Path,
    max_bytes: u64,
    errors: ConvertErrors,
) -> Result<Value, CodecError> {
    let input = match errors {
        ConvertErrors::Legacy => decoder
            .decoder_args(source)
            .await
            .map_err(CodecError::unverified)?,
        ConvertErrors::Source => decoder.decoder_args_typed(source).await?,
    };
    let mut staged = Staged::new(output, "webm").map_err(CodecError::unverified)?;
    let result = async {
        let mut size = 0;
        for crf in [32, 40, 48, 56, 63] {
            let mut args = vec!["-nostdin".into(), "-v".into(), "error".into(), "-y".into(), "-t".into(), "3.0".into()];
            args.extend(input.clone());
            args.extend(["-i".into(), source.to_string_lossy().into_owned(), "-an".into(), "-vf".into(),
                "fps=30,scale=w=100:h=100:force_original_aspect_ratio=decrease:flags=lanczos,format=rgba,pad=100:100:(ow-iw)/2:(oh-ih)/2:color=0x00000000,format=yuva420p".into(),
                "-c:v".into(), "libvpx-vp9".into(), "-pix_fmt".into(), "yuva420p".into(),
                "-b:v".into(), "0".into(), "-crf".into(), crf.to_string(), "-auto-alt-ref".into(), "0".into(),
                "-deadline".into(), "good".into(), "-cpu-used".into(), "2".into(),
                staged.0.to_string_lossy().into_owned()]);
            // The child can write this sibling before containment is settled. Cancellation
            // or any outer capture error must preserve it, just like stream-copy staging.
            staged.1 = true;
            let settled = crate::codec_process::capture_outcome_input(
                &decoder.root, &decoder.ffmpeg, &args, 1024 * 1024, None,
            ).await.map_err(CodecError::unverified)?;
            staged.1 = false;
            capture_result(&decoder.ffmpeg, &args, CommandContext::Direct, settled)
                .map_err(|failure| errors.command(failure, source))?;
            size = std::fs::metadata(&staged.0)
                .map_err(|_| CodecError::unverified("cannot inspect encoded video"))?.len();
            if size <= max_bytes {
                return staged.publish(output).map(|bytes| json!({"bytes":bytes}))
                    .map_err(CodecError::unverified);
            }
        }
        Err(errors.oversize(format!("could not fit {} under {max_bytes} bytes (got {size}).",
            source.file_name().ok_or_else(|| CodecError::unverified("source has no filename"))?.to_string_lossy())))
    }.await;
    if !staged.1 {
        // A failed explicit cleanup is not permission for Drop to try deleting again.
        staged.1 = true;
        staged.clean().map_err(CodecError::unverified)?;
    }
    result
}

fn prepare_tint(
    source: &Path,
    request: &Value,
    warnings: &mut Vec<String>,
) -> Result<(), CodecError> {
    let Some(tint) = request.get("tint") else {
        return Ok(());
    };
    let tint = tint
        .as_str()
        .ok_or_else(|| CodecError::value("tint must be #RRGGBB"))?;
    crate::codec_tint::parse_tint(tint).map_err(CodecError::media)?;
    warnings.push(format!(
        "repaint not supported for {} (video)",
        source.file_name().unwrap_or_default().to_string_lossy()
    ));
    Err(CodecError::value("requested tint could not be applied"))
}

pub fn prepare_at(
    root: Option<&Path>,
    source: &Path,
    request: &Value,
    warnings: &mut Vec<String>,
) -> Result<Value, CodecError> {
    let should_reencode = request
        .get("reencode")
        .unwrap_or(&Value::Bool(false))
        .as_bool()
        .ok_or_else(|| CodecError::value("reencode must be boolean"))?;
    // A tint-only refusal must not depend on video tools; remux still precedes tint when requested.
    if !should_reencode {
        prepare_tint(source, request, warnings)?;
    }
    let root =
        root.ok_or_else(|| CodecError::value("video worker requires an explicit project root"))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| CodecError::unverified("cannot initialize video worker runtime"))?;
    let decoder = VideoDecoder::new(root).map_err(CodecError::unverified)?;
    runtime.block_on(async {
        if should_reencode {
            let outcome = reencode_typed(&decoder, source).await?;
            warnings.extend(outcome.warning);
            prepare_tint(source, request, warnings)?;
        }
        decoder.fingerprint_typed(source).await
    })
}
