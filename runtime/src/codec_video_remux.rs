//! One owned stream-copy remux preserves legacy results and source-specific skip warnings.
use crate::codec_error::CodecError;
use crate::codec_video::VideoDecoder;
use crate::codec_video_encode::Staged;
use crate::codec_video_error::{CommandContext, VideoError, VideoFailure, capture_result};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

#[derive(Debug)]
pub struct VideoReencode {
    pub rewritten: bool,
    pub warning: Option<String>,
}
impl VideoReencode {
    fn unchanged(warning: Option<String>) -> Self {
        Self {
            rewritten: false,
            warning,
        }
    }
}

enum Remuxed {
    Content(RemuxContent),
    Skipped(VideoReencode),
}

const VIDEO_BYTES: usize = 256 * 1024;

struct RemuxContent {
    digest: String,
    size: u64,
    bytes: Option<Vec<u8>>,
}

impl RemuxContent {
    fn read(path: &Path) -> std::io::Result<Self> {
        let mut file = std::fs::File::open(path)?;
        let mut hash = Sha256::new();
        let mut size = 0_u64;
        let mut bytes = Some(Vec::new());
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = match file.read(&mut buffer) {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            if count == 0 {
                break;
            }
            size = size
                .checked_add(count as u64)
                .ok_or_else(|| std::io::Error::other("remux output byte count overflow"))?;
            hash.update(&buffer[..count]);
            if size > VIDEO_BYTES as u64 {
                // Preserve the complete digest/count for equality and refusal ordering;
                // oversized output never needs to be held in memory or published.
                bytes = None;
            } else if let Some(bytes) = &mut bytes {
                bytes.extend_from_slice(&buffer[..count]);
            }
        }
        let digest = hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(Self {
            digest,
            size,
            bytes,
        })
    }
}

#[derive(Clone, Copy)]
enum RemuxPolicy {
    Legacy,
    Source,
}

fn native(message: impl Into<String>) -> VideoError {
    VideoError {
        error: CodecError::unverified(message),
        caught_by_exception: false,
    }
}

fn caught(source: &Path, failure: VideoError) -> Result<Remuxed, CodecError> {
    if !failure.caught_by_exception {
        return Err(failure.error);
    }
    Ok(Remuxed::Skipped(VideoReencode::unchanged(Some(format!(
        "re-encode skipped for {} (video): {}",
        source.file_name().unwrap_or_default().to_string_lossy(),
        failure.error.message
    )))))
}

fn duration_warning(source: &Path, duration: f64) -> Result<String, VideoError> {
    Ok(format!(
        "re-encode of {} would declare {}s, over the 3.0s cap (the source under-declares its own length); keeping the original",
        source.file_name().unwrap_or_default().to_string_lossy(),
        crate::codec_video_validate::fixed(duration, 3)
    ))
}

impl RemuxPolicy {
    fn refuses_duration(self, duration: f64) -> bool {
        duration > 3.0 || (matches!(self, Self::Legacy) && !duration.is_finite())
    }

    fn command(self, failure: VideoFailure, source: &Path) -> Result<Remuxed, CodecError> {
        match self {
            Self::Legacy => Ok(Remuxed::Skipped(VideoReencode::unchanged(None))),
            Self::Source => caught(source, failure.classified(source)),
        }
    }

    fn digest(self, source: &Path, legacy_message: &str) -> Result<String, VideoError> {
        crate::codec_format::file_sha256(source).map_err(|error| match self {
            Self::Legacy => native(legacy_message),
            Self::Source => VideoError::known(error),
        })
    }

    fn read(self, source: &Path, legacy_message: &str) -> Result<RemuxContent, VideoError> {
        RemuxContent::read(source).map_err(|error| match self {
            Self::Legacy => native(legacy_message),
            Self::Source => VideoError::known(CodecError::io(error)),
        })
    }

    fn preserved(
        self,
        source: &Path,
        before: &str,
        content: &RemuxContent,
    ) -> Option<VideoReencode> {
        if content.digest == before {
            return Some(VideoReencode::unchanged(None));
        }
        if content.size > VIDEO_BYTES as u64 {
            let warning = matches!(self, Self::Source).then(|| format!(
                "re-encode of {} would be {} bytes, over the video cap of 262144; keeping the original",
                source.file_name().unwrap_or_default().to_string_lossy(), content.size
            ));
            return Some(VideoReencode::unchanged(warning));
        }
        None
    }
}

pub async fn reencode(decoder: &VideoDecoder, source: &Path) -> Result<bool, String> {
    reencode_with(decoder, source, RemuxPolicy::Legacy)
        .await
        .map(|outcome| outcome.rewritten)
        .map_err(|error| error.message)
}

pub async fn reencode_typed(
    decoder: &VideoDecoder,
    source: &Path,
) -> Result<VideoReencode, CodecError> {
    reencode_with(decoder, source, RemuxPolicy::Source).await
}

async fn reencode_with(
    decoder: &VideoDecoder,
    source: &Path,
    policy: RemuxPolicy,
) -> Result<VideoReencode, CodecError> {
    VideoDecoder::validate_source(source).map_err(CodecError::unverified)?;
    // The original initial Path.read_bytes is outside its ordinary-codec-error catch.
    let before = policy
        .digest(source, "cannot read video preparation source")
        .map_err(|failure| failure.error)?;
    let mut staged = Staged::for_remux(source).map_err(CodecError::unverified)?;
    staged.retain();
    let decoded = async {
        let args = vec![
            "-nostdin".into(),
            "-v".into(),
            "error".into(),
            "-y".into(),
            "-i".into(),
            source.to_string_lossy().into_owned(),
            "-c".into(),
            "copy".into(),
            staged.0.to_string_lossy().into_owned(),
        ];
        let settled = crate::codec_process::capture_outcome_input(
            &decoder.root,
            &decoder.ffmpeg,
            &args,
            1024 * 1024,
            None,
        )
        .await
        .map_err(CodecError::unverified)?;
        if let Err(failure) =
            capture_result(&decoder.ffmpeg, &args, CommandContext::Direct, settled)
        {
            return policy.command(failure, source);
        }
        let info = match decoder
            .probe_core(&staged.0, CommandContext::Direct)
            .await
            .map_err(CodecError::unverified)?
        {
            Ok(info) => info,
            Err(failure) => return policy.command(failure, source),
        };
        let duration = info.duration;
        if policy.refuses_duration(duration) {
            if matches!(policy, RemuxPolicy::Legacy) {
                return Ok(Remuxed::Skipped(VideoReencode::unchanged(None)));
            }
            // Source probes again ONLY to render this warning; the first result chose refusal.
            let info = match decoder
                .probe_core(&staged.0, CommandContext::Direct)
                .await
                .map_err(CodecError::unverified)?
            {
                Ok(info) => info,
                Err(failure) => return policy.command(failure, source),
            };
            return match duration_warning(source, info.duration) {
                Ok(warning) => Ok(Remuxed::Skipped(VideoReencode::unchanged(Some(warning)))),
                Err(failure) => caught(source, failure),
            };
        }
        if matches!(policy, RemuxPolicy::Source) {
            VideoDecoder::validate_source(&staged.0).map_err(CodecError::unverified)?;
        }
        match policy.read(&staged.0, "cannot read remux output") {
            Ok(content) => Ok(Remuxed::Content(content)),
            Err(failure) if matches!(policy, RemuxPolicy::Source) => caught(source, failure),
            Err(failure) => Err(failure.error),
        }
    }
    .await;
    // Any outer/unknown failure or cancellation retains the sibling as before.
    let decoded = decoded?;
    // Retain even across explicit cleanup failure: Drop must not retry destructive removal.
    staged.retain();
    staged.clean().map_err(CodecError::unverified)?;
    let content = match decoded {
        Remuxed::Skipped(outcome) => return Ok(outcome),
        Remuxed::Content(content) => content,
    };
    if let Some(outcome) = policy.preserved(source, &before, &content) {
        return Ok(outcome);
    }
    let bytes = content
        .bytes
        .ok_or_else(|| CodecError::unverified("bounded remux output is incomplete"))?;
    // Final persistence is outside the codec-error catch and remains the existing atomic write.
    crate::atomic::write_bytes(source, &bytes)
        .map_err(|_| CodecError::unverified("cannot replace video preparation source"))?;
    Ok(VideoReencode {
        rewritten: true,
        warning: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sparse_source(size: u64) -> Staged {
        let marker = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("logs/tmp/bounded-remux-test.webm");
        let staged = Staged::new(&marker, "webm").unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&staged.0)
            .unwrap()
            .set_len(size)
            .unwrap();
        staged
    }

    #[test]
    fn remux_scan_retains_only_admissible_output_and_hashes_the_complete_stream() {
        for size in [
            0,
            VIDEO_BYTES as u64,
            VIDEO_BYTES as u64 + 1,
            VIDEO_BYTES as u64 * 2 + 17,
        ] {
            let source = sparse_source(size);
            let content = RemuxContent::read(&source.0).unwrap();
            assert_eq!(content.size, size);
            assert_eq!(
                content.digest,
                crate::codec_format::file_sha256(&source.0).unwrap()
            );
            assert_eq!(
                content.bytes.as_ref().map(Vec::len),
                (size <= VIDEO_BYTES as u64).then_some(size as usize)
            );
            source.clean().unwrap();
        }
    }

    #[test]
    fn unchanged_oversized_remux_precedes_the_original_size_warning() {
        let source = sparse_source(VIDEO_BYTES as u64 + 1);
        let content = RemuxContent::read(&source.0).unwrap();
        let unchanged = RemuxPolicy::Source
            .preserved(&source.0, &content.digest, &content)
            .unwrap();
        assert!(!unchanged.rewritten);
        assert!(unchanged.warning.is_none());
        let refused = RemuxPolicy::Source
            .preserved(Path::new("sample.webm"), &"0".repeat(64), &content)
            .unwrap();
        assert_eq!(
            refused.warning.as_deref(),
            Some(
                "re-encode of sample.webm would be 262145 bytes, over the video cap of 262144; keeping the original"
            )
        );
        assert!(
            RemuxPolicy::Legacy
                .preserved(&source.0, &"0".repeat(64), &content)
                .unwrap()
                .warning
                .is_none()
        );
        source.clean().unwrap();
    }
}
