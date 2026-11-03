//! Source-specific video command errors preserve actual outcomes separately from legacy diagnostics.
use crate::codec_error::CodecError;
use crate::codec_process::{Captured, SettledCapture};
use crate::process_owner::WaitOutcome;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};
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
    if matches!(&settled.outcome, WaitOutcome::Finished(status) if status.success()) {
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

fn command_exception(
    py: Python<'_>,
    tool: &Path,
    args: &[String],
    code: i64,
    captured: &Captured,
) -> PyResult<PyErr> {
    let command = PyList::empty(py);
    command.append(tool.as_os_str())?;
    for arg in args {
        command.append(arg)?;
    }
    let exception = py
        .import("subprocess")?
        .getattr("CalledProcessError")?
        .call1((
            code,
            command,
            PyBytes::new(py, &captured.stdout),
            PyBytes::new(py, &captured.stderr),
        ))?;
    let exception = exception.cast_into::<pyo3::exceptions::PyBaseException>()?;
    Ok(PyErr::from_value(exception.into_any()))
}

#[derive(Debug)]
pub(crate) struct VideoError {
    pub(crate) error: CodecError,
    pub(crate) caught_by_exception: bool,
}
impl VideoError {
    fn known(error: CodecError) -> Self {
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

    pub(crate) fn from_python(py: Python<'_>, error: PyErr) -> Self {
        let caught_by_exception = error.is_instance_of::<pyo3::exceptions::PyException>(py);
        Self {
            error: CodecError::from_python(py, error),
            caught_by_exception,
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
            // Header detection precedes the source probe wrapper; do not relabel its exception.
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
                    WaitOutcome::Finished(status) => {
                        let Some(code) = returncode(&status) else {
                            return VideoError::unverified(
                                "video command has no known return code",
                            );
                        };
                        Python::initialize();
                        Python::attach(|py| {
                            match command_exception(py, &tool, &args, code, &settled.captured) {
                                Ok(error) => {
                                    if !error.is_instance_of::<pyo3::exceptions::PyException>(py) {
                                        return Err(VideoError::from_python(py, error));
                                    }
                                    let message = match error.value(py).str() {
                                        Ok(message) => message.to_string(),
                                        Err(error) => {
                                            return Err(VideoError::from_python(py, error));
                                        }
                                    };
                                    let kind = match error.get_type(py).name() {
                                        Ok(kind) => kind.to_string(),
                                        Err(error) => {
                                            return Err(VideoError::from_python(py, error));
                                        }
                                    };
                                    let handled = error
                                        .is_instance_of::<pyo3::exceptions::PyOSError>(py)
                                        || error
                                            .is_instance_of::<pyo3::exceptions::PyValueError>(py)
                                        || error
                                            .is_instance_of::<pyo3::exceptions::PyRuntimeError>(py);
                                    Ok(VideoError::known(CodecError {
                                        kind,
                                        message,
                                        handled,
                                    }))
                                }
                                Err(error) => Err(VideoError::from_python(py, error)),
                            }
                        })
                    }
                    WaitOutcome::Wall(limit)
                        if limit.as_secs() > 0 && limit.subsec_nanos() == 0 =>
                    {
                        let Some(name) = tool.file_name().and_then(|name| name.to_str()) else {
                            return VideoError::unverified("video command tool name is not UTF-8");
                        };
                        Ok(VideoError::known(CodecError::media(format!(
                            "{name} timed out after {}s",
                            limit.as_secs()
                        ))))
                    }
                    outcome => {
                        return VideoError::unverified(match outcome.status() {
                            Err(message) => message,
                            Ok(_) => "video command outcome was not a source failure".into(),
                        });
                    }
                };
                let error = match error {
                    Ok(error) => error,
                    Err(error) => return error,
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
    fn settled(outcome: WaitOutcome) -> SettledCapture {
        SettledCapture {
            outcome,
            captured: Captured {
                stdout: b"out\xff".to_vec(),
                stderr: b"err\xfe".to_vec(),
            },
        }
    }
    fn refused(outcome: WaitOutcome, context: CommandContext) -> VideoFailure {
        match capture_result(
            Path::new("ffprobe"),
            &["clip.webm".into()],
            context,
            settled(outcome),
        ) {
            Err(error) => error,
            Ok(_) => panic!("a failed command became a successful capture"),
        }
    }

    #[test]
    fn successful_capture_and_real_subprocess_exception_retain_the_observed_fields() {
        let good = capture_result(
            Path::new("ffprobe"),
            &[],
            CommandContext::Direct,
            settled(WaitOutcome::Finished(status(0))),
        );
        let captured = match good {
            Ok(captured) => captured,
            Err(_) => panic!("successful capture refused"),
        };
        assert_eq!(captured.stdout, b"out\xff");
        assert_eq!(captured.stderr, b"err\xfe");
        Python::initialize();
        Python::attach(|py| {
            let args = ["-v".into(), "error".into(), "fixture café.webm".into()];
            let error = command_exception(py, Path::new("ffprobe"), &args, 7, &captured).unwrap();
            let value = error.value(py);
            assert_eq!(
                value
                    .getattr("returncode")
                    .unwrap()
                    .extract::<i64>()
                    .unwrap(),
                7
            );
            assert_eq!(
                value
                    .getattr("cmd")
                    .unwrap()
                    .extract::<Vec<String>>()
                    .unwrap(),
                ["ffprobe", "-v", "error", "fixture café.webm"]
            );
            for (field, expected) in [
                ("output", b"out\xff".as_slice()),
                ("stdout", b"out\xff".as_slice()),
                ("stderr", b"err\xfe".as_slice()),
            ] {
                assert_eq!(
                    value
                        .getattr(field)
                        .unwrap()
                        .cast_into::<pyo3::types::PyBytes>()
                        .unwrap()
                        .as_bytes(),
                    expected
                );
            }
        });
        let error = refused(WaitOutcome::Finished(status(7)), CommandContext::Direct)
            .typed(Path::new("clip.webm"));
        assert_eq!(error.kind, "CalledProcessError");
        assert_eq!(
            error.message,
            "Command '['ffprobe', 'clip.webm']' returned non-zero exit status 7."
        );
        assert!(!error.handled);
        assert!(crate::codec_error::CodecError::worker_reply(Err(error)).is_err());
        assert_eq!(
            refused(WaitOutcome::Finished(status(7)), CommandContext::Direct).legacy(),
            "external video codec failed with exit 7"
        );
    }

    #[test]
    fn source_wrappers_apply_to_command_failures_but_not_native_idle_or_unknowns() {
        for (context, prefix) in [
            (
                CommandContext::Probe,
                "clip.webm: the codec could not be read, so the decoder that preserves its alpha cannot be chosen: ",
            ),
            (
                CommandContext::Capability,
                "could not ask ffmpeg which decoders it has, so alpha fidelity cannot be established: ",
            ),
        ] {
            let error =
                refused(WaitOutcome::Finished(status(1)), context).typed(Path::new("clip.webm"));
            assert_eq!(error.kind, "UndecodableVideo");
            assert_eq!(
                error.message,
                format!(
                    "{prefix}Command '['ffprobe', 'clip.webm']' returned non-zero exit status 1."
                )
            );
            assert!(error.handled);
            let error = refused(WaitOutcome::Wall(Duration::from_secs(3)), context)
                .typed(Path::new("clip.webm"));
            assert_eq!(error.kind, "UndecodableVideo");
            assert_eq!(error.message, format!("{prefix}ffprobe timed out after 3s"));
            let error = refused(WaitOutcome::Idle(Duration::from_secs(2)), context)
                .typed(Path::new("clip.webm"));
            assert_eq!(error.kind, "UnverifiedCodecError");
            assert_eq!(
                error.message,
                "media adapter made no CPU/IO progress for 2s"
            );
            assert!(!error.handled);
        }
        let error = refused(
            WaitOutcome::Wall(Duration::from_secs(3)),
            CommandContext::Direct,
        )
        .typed(Path::new("clip.webm"));
        assert_eq!(error.kind, "MediaError");
        assert_eq!(error.message, "ffprobe timed out after 3s");
        assert!(error.handled);
        assert_eq!(
            refused(
                WaitOutcome::Wall(Duration::from_secs(3)),
                CommandContext::Probe
            )
            .legacy(),
            "media adapter exceeded 3s wall budget"
        );
        let error = refused(
            WaitOutcome::Wall(Duration::from_millis(1)),
            CommandContext::Probe,
        )
        .typed(Path::new("clip.webm"));
        assert_eq!(error.kind, "UnverifiedCodecError");
        assert!(!error.handled);
        assert_eq!(
            VideoFailure::Unverified("native admission".into())
                .typed(Path::new("clip.webm"))
                .kind,
            "UnverifiedCodecError"
        );
    }

    #[test]
    fn remux_exception_catch_is_distinct_from_worker_handling_and_native_unknowns() {
        Python::initialize();
        Python::attach(|py| {
            for (error, caught, handled) in [
                (pyo3::exceptions::PyOSError::new_err("read"), true, true),
                (pyo3::exceptions::PyTypeError::new_err("type"), true, false),
                (
                    pyo3::exceptions::PyKeyboardInterrupt::new_err("interrupt"),
                    false,
                    false,
                ),
                (
                    pyo3::exceptions::PySystemExit::new_err("exit"),
                    false,
                    false,
                ),
            ] {
                let error = VideoError::from_python(py, error);
                assert_eq!(error.caught_by_exception, caught);
                assert_eq!(error.error.handled, handled);
            }
        });
        for failure in [
            VideoFailure::Unverified("native custody".into()),
            refused(
                WaitOutcome::Idle(Duration::from_secs(2)),
                CommandContext::Direct,
            ),
        ] {
            let error = failure.classified(Path::new("clip.webm"));
            assert!(!error.caught_by_exception);
            assert_eq!(error.error.kind, "UnverifiedCodecError");
        }
        let error = refused(WaitOutcome::Finished(status(7)), CommandContext::Direct)
            .classified(Path::new("clip.webm"));
        assert!(error.caught_by_exception);
        assert_eq!(error.error.kind, "CalledProcessError");
        assert!(!error.error.handled);
    }

    #[test]
    fn actual_metadata_errors_keep_direct_identity_and_only_exception_context_wraps() {
        for context in [CommandContext::Direct, CommandContext::Probe] {
            let failure =
                crate::codec_video_probe::parse_stdout(br#"{"format":null}"#).unwrap_err();
            let error =
                VideoFailure::Metadata { failure, context }.classified(Path::new("clip.webm"));
            assert!(error.caught_by_exception);
            match context {
                CommandContext::Direct => {
                    assert_eq!(error.error.kind, "AttributeError");
                    assert_eq!(
                        error.error.message,
                        "'NoneType' object has no attribute 'get'"
                    );
                    assert!(!error.error.handled);
                }
                _ => {
                    assert_eq!(error.error.kind, "UndecodableVideo");
                    assert_eq!(
                        error.error.message,
                        "clip.webm: the codec could not be read, so the decoder that preserves its alpha cannot be chosen: 'NoneType' object has no attribute 'get'"
                    );
                    assert!(error.error.handled);
                }
            }
        }
        Python::initialize();
        Python::attach(|py| {
            for error in [
                pyo3::exceptions::PyKeyboardInterrupt::new_err("interrupt"),
                pyo3::exceptions::PySystemExit::new_err("exit"),
            ] {
                let kind = error.get_type(py).name().unwrap().to_string();
                let error = VideoFailure::Metadata {
                    failure: VideoError::from_python(py, error),
                    context: CommandContext::Probe,
                }
                .classified(Path::new("clip.webm"));
                assert_eq!(error.error.kind, kind);
                assert!(!error.caught_by_exception);
                assert!(!error.error.handled);
            }
        });
    }

    #[test]
    fn actual_platform_exit_codes_do_not_invent_or_lose_signal_identity() {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            let signaled = std::process::ExitStatus::from_raw(15);
            assert_eq!(returncode(&signaled), Some(-15));
            let error = refused(WaitOutcome::Finished(signaled), CommandContext::Direct)
                .typed(Path::new("clip.webm"));
            assert_eq!(error.kind, "CalledProcessError");
            assert!(error.message.ends_with("died with <Signals.SIGTERM: 15>."));
            assert!(!error.handled);
        }
        #[cfg(windows)]
        {
            let crashed = status(0xc0000005);
            assert_eq!(returncode(&crashed), Some(3_221_225_477));
            let error = refused(WaitOutcome::Finished(crashed), CommandContext::Direct)
                .typed(Path::new("clip.webm"));
            assert_eq!(error.kind, "CalledProcessError");
            assert!(
                error
                    .message
                    .ends_with("returned non-zero exit status 3221225477.")
            );
            assert!(!error.handled);
        }
    }
}
