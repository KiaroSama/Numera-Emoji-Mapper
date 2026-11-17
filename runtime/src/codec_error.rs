//! Native error envelopes. No interpreter exception or Python ABI participates in execution.
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct CodecError {
    pub kind: String,
    pub message: String,
    pub handled: bool,
}

pub struct WorkerReply {
    pub exit: i32,
    pub envelope: Value,
    pub error: Option<String>,
}

impl CodecError {
    pub fn media(message: impl Into<String>) -> Self {
        Self {
            kind: "MediaError".into(),
            message: message.into(),
            handled: true,
        }
    }
    pub fn value(message: impl Into<String>) -> Self {
        Self {
            kind: "ValueError".into(),
            message: message.into(),
            handled: true,
        }
    }
    pub fn type_error(message: impl Into<String>) -> Self {
        Self {
            kind: "TypeError".into(),
            message: message.into(),
            handled: false,
        }
    }
    pub fn escaped(kind: &str, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            handled: false,
        }
    }
    pub fn unverified(message: impl Into<String>) -> Self {
        Self::escaped("UnverifiedCodecError", message)
    }
    pub fn io(error: std::io::Error) -> Self {
        let kind = match error.kind() {
            std::io::ErrorKind::NotFound => "FileNotFoundError",
            std::io::ErrorKind::PermissionDenied => "PermissionError",
            std::io::ErrorKind::AlreadyExists => "FileExistsError",
            _ => "OSError",
        };
        Self {
            kind: kind.into(),
            message: error.to_string(),
            handled: true,
        }
    }
    pub fn worker_reply(result: Result<Value, Self>) -> Result<WorkerReply, Self> {
        match result {
            Ok(result) => Ok(WorkerReply {
                exit: 0,
                envelope: json!({"ok": true, "result": result}),
                error: None,
            }),
            Err(error) if error.handled => {
                let message = crate::logging::redact(&error.message, &[]);
                Ok(WorkerReply {
                    exit: 1,
                    envelope: json!({"ok": false, "error": message}),
                    error: Some(format!("Media adapter failed: {message}")),
                })
            }
            Err(error) => Err(error),
        }
    }
}
impl From<String> for CodecError {
    fn from(message: String) -> Self {
        Self::unverified(message)
    }
}
impl From<&str> for CodecError {
    fn from(message: &str) -> Self {
        Self::value(message)
    }
}
impl From<std::io::Error> for CodecError {
    fn from(error: std::io::Error) -> Self {
        Self::io(error)
    }
}
impl From<crate::native_raster::RasterError> for CodecError {
    fn from(error: crate::native_raster::RasterError) -> Self {
        Self::media(error.to_string())
    }
}
impl From<crate::native_lottie::LottieError> for CodecError {
    fn from(error: crate::native_lottie::LottieError) -> Self {
        Self {
            kind: error.kind,
            message: error.message,
            handled: error.handled,
        }
    }
}
impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for CodecError {}
