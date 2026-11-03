//! One owned stream-copy remux preserves legacy results and source-specific skip warnings.
use crate::codec_error::CodecError;
use crate::codec_video::VideoDecoder;
use crate::codec_video_encode::Staged;
use crate::codec_video_error::{CommandContext, VideoError, VideoFailure, capture_result};
use pyo3::prelude::*;
use pyo3::types::PyString;
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
    Bytes(Vec<u8>),
    Skipped(VideoReencode),
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
    Python::initialize();
    Python::attach(|py| {
        PyString::new(py, "re-encode of %s would declare %.3fs, over the %.1fs cap (the source under-declares its own length); keeping the original")
            .call_method1("__mod__", ((source.file_name().unwrap_or_default(), duration, 3.0),))
            .and_then(|value| value.extract::<String>())
            .map_err(|error| VideoError::from_python(py, error))
    })
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

    fn read(self, source: &Path, legacy_message: &str) -> Result<Vec<u8>, VideoError> {
        match self {
            Self::Legacy => std::fs::read(source).map_err(|_| native(legacy_message)),
            Self::Source => {
                Python::initialize();
                Python::attach(|py| {
                    crate::codec_lottie_load::source_bytes(py, source)
                        .map_err(|error| VideoError::from_python(py, error))
                })
            }
        }
    }

    fn preserved(self, source: &Path, before: &[u8], bytes: &[u8]) -> Option<VideoReencode> {
        if bytes == before {
            return Some(VideoReencode::unchanged(None));
        }
        if bytes.len() > 256 * 1024 {
            let warning = matches!(self, Self::Source).then(|| format!(
                "re-encode of {} would be {} bytes, over the video cap of 262144; keeping the original",
                source.file_name().unwrap_or_default().to_string_lossy(), bytes.len()
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
        .read(source, "cannot read video preparation source")
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
            Ok(bytes) => Ok(Remuxed::Bytes(bytes)),
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
    let bytes = match decoded {
        Remuxed::Skipped(outcome) => return Ok(outcome),
        Remuxed::Bytes(bytes) => bytes,
    };
    if let Some(outcome) = policy.preserved(source, &before, &bytes) {
        return Ok(outcome);
    }
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
    use pyo3::exceptions::{PyKeyboardInterrupt, PyOSError, PySystemExit, PyTypeError};

    #[test]
    fn only_actual_ordinary_python_errors_become_remux_skip_warnings() {
        Python::initialize();
        Python::attach(|py| {
            for (error, caught_by_exception, kind) in [
                (PyOSError::new_err("controlled read fault"), true, "OSError"),
                (
                    PyTypeError::new_err("controlled read fault"),
                    true,
                    "TypeError",
                ),
                (
                    PyKeyboardInterrupt::new_err("controlled read fault"),
                    false,
                    "KeyboardInterrupt",
                ),
                (
                    PySystemExit::new_err("controlled read fault"),
                    false,
                    "SystemExit",
                ),
            ] {
                let failure = VideoError::from_python(py, error);
                assert_eq!(failure.caught_by_exception, caught_by_exception);
                let result = caught(Path::new("sample.webm"), failure);
                if caught_by_exception {
                    let Remuxed::Skipped(outcome) = result.unwrap() else {
                        panic!("a caught failure must not publish bytes")
                    };
                    assert!(!outcome.rewritten);
                    assert_eq!(
                        outcome.warning.as_deref(),
                        Some("re-encode skipped for sample.webm (video): controlled read fault")
                    );
                } else {
                    let error = result.err().unwrap();
                    assert_eq!(error.kind, kind);
                    assert_eq!(error.message, "controlled read fault");
                    assert!(!error.handled);
                }
            }
        });
        let error = caught(Path::new("sample.webm"), native("unknown custody"))
            .err()
            .unwrap();
        assert_eq!(error.kind, "UnverifiedCodecError");
        assert_eq!(error.message, "unknown custody");
        assert!(!error.handled);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn settled_command_policy_keeps_legacy_false_and_source_actual_causes() {
        use crate::codec_process::{Captured, SettledCapture};
        use crate::process_owner::WaitOutcome;
        use std::time::Duration;
        for policy in [RemuxPolicy::Legacy, RemuxPolicy::Source] {
            for case in ["exit", "wall", "idle"] {
                #[cfg(unix)]
                let status = {
                    use std::os::unix::process::ExitStatusExt;
                    std::process::ExitStatus::from_raw(7 << 8)
                };
                #[cfg(windows)]
                let status = {
                    use std::os::windows::process::ExitStatusExt;
                    std::process::ExitStatus::from_raw(7)
                };
                let outcome = match case {
                    "exit" => WaitOutcome::Finished(status),
                    "wall" => WaitOutcome::Wall(Duration::from_secs(3)),
                    _ => WaitOutcome::Idle(Duration::from_secs(2)),
                };
                let failure = capture_result(
                    Path::new("ffmpeg"),
                    &["sample.webm".into()],
                    CommandContext::Direct,
                    SettledCapture {
                        outcome,
                        captured: Captured {
                            stdout: b"actual stdout".to_vec(),
                            stderr: b"actual stderr".to_vec(),
                        },
                    },
                )
                .err()
                .unwrap();
                let result = policy.command(failure, Path::new("sample.webm"));
                if matches!(policy, RemuxPolicy::Source) && case == "idle" {
                    let error = result.err().unwrap();
                    assert_eq!(error.kind, "UnverifiedCodecError");
                    assert_eq!(
                        error.message,
                        "media adapter made no CPU/IO progress for 2s"
                    );
                    assert!(!error.handled);
                } else {
                    let Remuxed::Skipped(outcome) = result.unwrap() else {
                        panic!("failed command must not publish bytes")
                    };
                    assert!(!outcome.rewritten);
                    assert_eq!(
                        outcome.warning.as_deref(),
                        match (policy, case) {
                            (RemuxPolicy::Legacy, _) => None,
                            (_, "wall") => Some(
                                "re-encode skipped for sample.webm (video): ffmpeg timed out after 3s"
                            ),
                            _ => Some(
                                "re-encode skipped for sample.webm (video): Command '['ffmpeg', 'sample.webm']' returned non-zero exit status 7."
                            ),
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn lossless_probe_errors_and_nonfinite_durations_keep_the_source_remux_catch() {
        for (duration, refused) in [
            (f64::NAN, false),
            (f64::INFINITY, true),
            (f64::NEG_INFINITY, false),
            (3.0, false),
            (3.001, true),
        ] {
            assert_eq!(RemuxPolicy::Source.refuses_duration(duration), refused);
        }
        assert!(RemuxPolicy::Legacy.refuses_duration(f64::NAN));
        for (duration, displayed) in [
            (f64::NAN, "nan"),
            (f64::INFINITY, "inf"),
            (f64::NEG_INFINITY, "-inf"),
        ] {
            assert_eq!(
                duration_warning(Path::new("sample.webm"), duration).unwrap(),
                format!(
                    "re-encode of sample.webm would declare {displayed}s, over the 3.0s cap (the source under-declares its own length); keeping the original"
                )
            );
        }
        let failure = crate::codec_video_probe::parse_stdout(b"{\"format\":null}").unwrap_err();
        let Remuxed::Skipped(outcome) = RemuxPolicy::Source
            .command(
                VideoFailure::Metadata {
                    failure,
                    context: CommandContext::Direct,
                },
                Path::new("sample.webm"),
            )
            .unwrap()
        else {
            panic!("ordinary probe errors must skip remux")
        };
        assert_eq!(
            outcome.warning.as_deref(),
            Some(
                "re-encode skipped for sample.webm (video): 'NoneType' object has no attribute 'get'"
            )
        );
    }

    #[test]
    fn source_duration_text_uses_displayed_probe_and_equality_precedes_cap() {
        assert_eq!(
            duration_warning(Path::new("sample.webm"), 2.5)
                .map_err(|error| error.error)
                .unwrap(),
            "re-encode of sample.webm would declare 2.500s, over the 3.0s cap (the source under-declares its own length); keeping the original"
        );
        let before = vec![1; 262145];
        let large = vec![2; 262145];
        for policy in [RemuxPolicy::Legacy, RemuxPolicy::Source] {
            let unchanged = policy
                .preserved(Path::new("sample.webm"), &before, &before)
                .unwrap();
            assert!(!unchanged.rewritten);
            assert!(unchanged.warning.is_none());
            let oversized = policy
                .preserved(Path::new("sample.webm"), &before, &large)
                .unwrap();
            assert!(!oversized.rewritten);
            assert_eq!(
                oversized.warning.as_deref(),
                match policy {
                    RemuxPolicy::Legacy => None,
                    RemuxPolicy::Source => Some(
                        "re-encode of sample.webm would be 262145 bytes, over the video cap of 262144; keeping the original"
                    ),
                }
            );
            assert!(
                policy
                    .preserved(Path::new("sample.webm"), &before, b"different small bytes")
                    .is_none()
            );
        }
    }
}
