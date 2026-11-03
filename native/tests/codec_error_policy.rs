//! Catch/envelope decisions come from the pinned main oracle, not a handwritten native table.
use _native::codec_error::CodecError;
use pyo3::prelude::*;
use serde_json::Value;

#[test]
fn typed_exception_catch_policy_matches_every_pinned_main_outcome() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/media-continuation/main-policy.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(fixture["source_runtime"], "CPython3.11.9");
    assert_eq!(
        fixture["source_sha256"]["emojikit/media_bridge.py"],
        "8d7f86244ed498b32381ba78a6321622aa28d9b72c1a1adba9a48c39565dd5b7"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    Python::initialize();
    for case in cases {
        let result = if let Some(kind) = case["request"]["fault"].as_str() {
            let message = case["request"]["message"].as_str().unwrap();
            let error = match kind {
                "ValueError" => pyo3::exceptions::PyValueError::new_err(message.to_owned()),
                "RuntimeError" => pyo3::exceptions::PyRuntimeError::new_err(message.to_owned()),
                "OSError" => pyo3::exceptions::PyOSError::new_err(message.to_owned()),
                "TypeError" => pyo3::exceptions::PyTypeError::new_err(message.to_owned()),
                "EOFError" => pyo3::exceptions::PyEOFError::new_err(message.to_owned()),
                "KeyError" => pyo3::exceptions::PyKeyError::new_err(message.to_owned()),
                _ => panic!("uninventoried fixture exception"),
            };
            Err(Python::attach(|py| CodecError::from_python(py, error)))
        } else {
            Ok(case["request"]["result"].clone())
        };
        match CodecError::worker_reply(result) {
            Ok(reply) => {
                assert_eq!(
                    reply.exit,
                    case["exit"].as_i64().unwrap() as i32,
                    "{}",
                    case["name"]
                );
                assert_eq!(reply.envelope, case["response"], "{}", case["name"]);
                assert_eq!(case["writes"].as_array().unwrap(), &vec![reply.envelope]);
                assert_eq!(
                    reply.error.into_iter().collect::<Vec<_>>(),
                    serde_json::from_value::<Vec<String>>(case["errors"].clone()).unwrap()
                );
            }
            Err(error) => {
                assert_eq!(error.kind, case["escaped"]["kind"].as_str().unwrap());
                assert_eq!(error.message, case["escaped"]["message"].as_str().unwrap());
                assert!(case["response"].is_null());
                assert!(case["writes"].as_array().unwrap().is_empty());
                assert!(case["errors"].as_array().unwrap().is_empty());
            }
        }
    }
    let error = Python::attach(|py| {
        CodecError::from_python(
            py,
            pyo3::exceptions::PyFileNotFoundError::new_err("controlled subclass"),
        )
    });
    assert_eq!(error.kind, "FileNotFoundError");
    assert!(
        error.handled,
        "source catches OSError subclasses, not only exact class names"
    );
}
