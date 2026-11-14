//! Source exception identity/catch policy travels with errors, never inferred from message strings.
use pyo3::prelude::*;
use serde_json::{Value, json};

pub struct WorkerReply {
    pub exit: i32,
    pub envelope: Value,
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct CodecError {
    pub kind: String,
    pub message: String,
    pub handled: bool,
}
impl CodecError {
    pub fn worker_reply(result: Result<Value, Self>) -> Result<WorkerReply, Self> {
        match result {
            Ok(result) => Ok(WorkerReply {
                exit: 0,
                envelope: json!({"ok":true,"result":result}),
                error: None,
            }),
            Err(error) if error.handled => {
                let message = crate::logging::redact(&error.message, &[]);
                Ok(WorkerReply {
                    exit: 1,
                    envelope: json!({"ok":false,"error":message}),
                    error: Some(format!("Media adapter failed: {message}")),
                })
            }
            Err(error) => Err(error),
        }
    }
    pub fn media(message: impl Into<String>) -> Self {
        Self {
            kind: "MediaError".into(),
            message: message.into(),
            handled: true,
        }
    }
    pub fn escaped(kind: &str, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            handled: false,
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
    pub fn from_python(py: Python<'_>, error: PyErr) -> Self {
        let handled = error.is_instance_of::<pyo3::exceptions::PyOSError>(py)
            || error.is_instance_of::<pyo3::exceptions::PyValueError>(py)
            || error.is_instance_of::<pyo3::exceptions::PyRuntimeError>(py);
        Self {
            kind: error
                .get_type(py)
                .name()
                .map(|name| name.to_string())
                .unwrap_or_else(|_| "PythonError".into()),
            message: error
                .value(py)
                .str()
                .map(|text| text.to_string())
                .unwrap_or_else(|_| "external codec failed".into()),
            handled,
        }
    }
    pub(crate) fn unverified(message: impl Into<String>) -> Self {
        // Legacy String-only codecs have not proved their source exception class. Never invent one.
        Self {
            kind: "UnverifiedCodecError".into(),
            message: message.into(),
            handled: false,
        }
    }
}
impl From<&str> for CodecError {
    fn from(message: &str) -> Self {
        Self::value(message)
    }
}
impl From<String> for CodecError {
    fn from(message: String) -> Self {
        Self::unverified(message)
    }
}
impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for CodecError {}
