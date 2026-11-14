//! Rust subprocess outcomes retain source error membership without a language runtime.
use crate::codec_error::CodecError;
use crate::codec_process::{Captured, SettledCapture};
use crate::process_owner::WaitOutcome;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub(crate) enum CommandContext {
    Direct,
    Probe,
    Capability,
}
pub(crate) enum VideoFailure {
    Unverified(String),
    NoCodec,
    Header(VideoError),
    Metadata {
        failure: VideoError,
        context: CommandContext,
    },
    MissingDecoder {
        codec: String,
        decoder: &'static str,
    },
    Command {
        tool: PathBuf,
        args: Vec<String>,
        context: CommandContext,
        settled: SettledCapture,
    },
}
pub(crate) fn capture_result(
    tool: &Path,
    args: &[String],
    context: CommandContext,
    settled: SettledCapture,
) -> Result<Captured, VideoFailure> {
    if matches!(&settled.outcome,WaitOutcome::Finished(status) if status.success()) {
        Ok(settled.captured)
    } else {
        Err(VideoFailure::Command {
            tool: tool.into(),
            args: args.into(),
            context,
            settled,
        })
    }
}
fn returncode(status: &std::process::ExitStatus) -> Option<i64> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status
            .code()
            .map(i64::from)
            .or_else(|| status.signal().map(|signal| -i64::from(signal)))
    }
    #[cfg(windows)]
    {
        status.code().map(|code| i64::from(code as u32))
    }
    #[cfg(not(any(unix, windows)))]
    {
        status.code().map(i64::from)
    }
}
#[derive(Debug)]
pub(crate) struct VideoError {
    pub(crate) error: CodecError,
    pub(crate) caught_by_exception: bool,
}
impl VideoError {
    pub(crate) fn known(error: CodecError) -> Self {
        Self {
            error,
            caught_by_exception: true,
        }
    }
    fn unverified(message: impl Into<String>) -> Self {
        Self {
            error: CodecError::unverified(message),
            caught_by_exception: false,
        }
    }
}
fn undecodable(message: String) -> VideoError {
    VideoError::known(CodecError {
        kind: "UndecodableVideo".into(),
        message,
        handled: true,
    })
}

fn command_message(tool: &Path, args: &[String], code: i64) -> String {
    use crate::codec_video_probe::MetadataString;
    let command = std::iter::once(tool.to_string_lossy().into_owned())
        .chain(args.iter().cloned())
        .map(|arg| MetadataString::from(arg.as_str()).repr())
        .collect::<Vec<_>>()
        .join(", ");
    let command = format!("[{command}]");
    if code < 0 {
        let signal = -code;
        #[cfg(unix)]
        let name = match signal {
            1 => Some("SIGHUP"),
            2 => Some("SIGINT"),
            3 => Some("SIGQUIT"),
            4 => Some("SIGILL"),
            6 => Some("SIGABRT"),
            8 => Some("SIGFPE"),
            9 => Some("SIGKILL"),
            11 => Some("SIGSEGV"),
            13 => Some("SIGPIPE"),
            14 => Some("SIGALRM"),
            15 => Some("SIGTERM"),
            _ => None,
        };
        #[cfg(not(unix))]
        let name: Option<&str> = None;
        match name {
            Some(name) => format!("Command '{command}' died with <Signals.{name}: {signal}>."),
            None => format!("Command '{command}' died with unknown signal {signal}."),
        }
    } else {
        format!("Command '{command}' returned non-zero exit status {code}.")
    }
}
impl VideoFailure {
    pub(crate) fn legacy(self) -> String {
        match self {
            Self::Unverified(message) => message,
            Self::Header(failure) | Self::Metadata { failure, .. } => failure.error.message,
            Self::NoCodec => {
                "video container named no codec; alpha fidelity cannot be established".into()
            }
            Self::MissingDecoder { decoder, .. } => {
                format!("ffmpeg has no {decoder} decoder; alpha fidelity cannot be established")
            }
            Self::Command { settled, .. } => match settled.into_legacy() {
                Err(message) => message,
                Ok(_) => "video command failure unexpectedly succeeded".into(),
            },
        }
    }
    pub(crate) fn typed(self, path: &Path) -> CodecError {
        self.classified(path).error
    }
    pub(crate) fn classified(self, path: &Path) -> VideoError {
        let filename = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy();
        match self {
            Self::Unverified(message) => VideoError::unverified(message),
            Self::Header(failure) => failure,
            Self::Metadata { failure, context } => context.classify(path, failure),
            Self::NoCodec => undecodable(format!(
                "{filename}: the container named no video codec, so alpha fidelity cannot be established"
            )),
            Self::MissingDecoder { codec, decoder } => undecodable(format!(
                "{filename}: {codec} keeps alpha in a separate layer and this ffmpeg has no {decoder} decoder, so a decode here cannot be trusted to carry transparency"
            )),
            Self::Command {
                tool,
                args,
                context,
                settled,
            } => {
                let error = match settled.outcome {
                    WaitOutcome::Finished(status) => match returncode(&status) {
                        Some(code) => VideoError::known(CodecError {
                            kind: "CalledProcessError".into(),
                            message: command_message(&tool, &args, code),
                            handled: false,
                        }),
                        None => {
                            return VideoError::unverified(
                                "video command has no known return code",
                            );
                        }
                    },
                    WaitOutcome::Wall(limit)
                        if limit.as_secs() > 0 && limit.subsec_nanos() == 0 =>
                    {
                        let Some(name) = tool.file_name().and_then(|name| name.to_str()) else {
                            return VideoError::unverified("video command tool name is not UTF-8");
                        };
                        VideoError::known(CodecError::media(format!(
                            "{name} timed out after {}s",
                            limit.as_secs()
                        )))
                    }
                    outcome => {
                        return VideoError::unverified(match outcome.status() {
                            Err(message) => message,
                            Ok(_) => "video command outcome was not a source failure".into(),
                        });
                    }
                };
                context.classify(path, error)
            }
        }
    }
}
impl CommandContext {
    fn classify(self, path: &Path, error: VideoError) -> VideoError {
        if !error.caught_by_exception {
            return error;
        }
        match self {
            Self::Direct => error,
            Self::Probe => undecodable(format!(
                "{}: the codec could not be read, so the decoder that preserves its alpha cannot be chosen: {}",
                path.file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy(),
                error.error.message
            )),
            Self::Capability => undecodable(format!(
                "could not ask ffmpeg which decoders it has, so alpha fidelity cannot be established: {}",
                error.error.message
            )),
        }
    }
}

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use super::*;
    use std::time::Duration;
    fn status(code: u32) -> std::process::ExitStatus {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            std::process::ExitStatus::from_raw((code as i32) << 8)
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(code)
        }
    }
    fn failure(outcome: WaitOutcome, context: CommandContext) -> VideoFailure {
        match capture_result(
            Path::new("ffprobe"),
            &["clip café.webm".into()],
            context,
            SettledCapture {
                outcome,
                captured: Captured {
                    stdout: vec![],
                    stderr: vec![],
                },
            },
        ) {
            Err(failure) => failure,
            Ok(_) => panic!("failure unexpectedly succeeded"),
        }
    }
    #[test]
    fn settled_commands_keep_direct_and_alpha_probe_error_membership() {
        let error = failure(WaitOutcome::Finished(status(2)), CommandContext::Direct)
            .typed(Path::new("clip.webm"));
        assert_eq!(error.kind, "CalledProcessError");
        assert_eq!(
            error.message,
            "Command '['ffprobe', 'clip café.webm']' returned non-zero exit status 2."
        );
        assert!(!error.handled);
        let error = failure(WaitOutcome::Finished(status(2)), CommandContext::Probe)
            .typed(Path::new("clip.webm"));
        assert_eq!(error.kind, "UndecodableVideo");
        assert!(error.handled);
        assert!(
            error
                .message
                .starts_with("clip.webm: the codec could not be read")
        );
        let timed = failure(
            WaitOutcome::Wall(Duration::from_secs(300)),
            CommandContext::Direct,
        )
        .typed(Path::new("clip.webm"));
        assert_eq!(timed.kind, "MediaError");
        assert_eq!(timed.message, "ffprobe timed out after 300s");
        assert!(timed.handled);
    }
    #[test]
    fn unknown_custody_and_idle_deadlines_escape_ordinary_codec_catches() {
        for outcome in [
            WaitOutcome::Idle(Duration::from_secs(5)),
            WaitOutcome::Wall(Duration::from_millis(50)),
        ] {
            let failure =
                failure(outcome, CommandContext::Probe).classified(Path::new("clip.webm"));
            assert_eq!(failure.error.kind, "UnverifiedCodecError");
            assert!(!failure.error.handled);
            assert!(!failure.caught_by_exception);
        }
    }
}
