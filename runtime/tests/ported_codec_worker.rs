// Ported from native/tests/codec_worker.rs at a32dbab6e3e694f1fac9656997fa2edb6e3b2ede.
// Original source SHA256: c357b7423490dc0ed1cd73233f670da6c42ca3fec64ebe25900573d33cd8df20.
// Original test identities and behavioral assertions retained against the production crate.
extern crate numera_runtime as _native;

// The public native worker boundary preserves source output and refuses malformed input.
use serde_json::{Value, json};
use std::path::PathBuf;

#[test]
fn static_worker_matches_source_fixture_and_refuses_invalid_requests() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(root.join("tests/fixtures/native/static-codec.json")).unwrap(),
    )
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let source = root.join(case["input"].as_str().unwrap());
        let request = json!({"operation":"image_info","source":source,"format":"static"});
        assert_eq!(
            _native::codec_worker::execute(&request).unwrap(),
            json!({"size":case["expected"]["size"],"mode":case["expected"]["mode"],"blank":case["expected"]["blank"]}),
            "{}",
            case["name"]
        );
        let fingerprint = json!({"operation":"fingerprint","source":source,"format":"static"});
        assert_eq!(
            _native::codec_worker::execute(&fingerprint).unwrap(),
            json!({"content_key":case["expected"]["content_key"],"phash":case["expected"]["phash"],"fmt":"static","extension":".png"})
        );
    }
    for request in [
        json!(null),
        json!([]),
        json!({"operation":"unknown"}),
        json!({"operation":"image_info","source":"relative.png","format":"static"}),
        json!({"operation":"image_info","source":root,"format":"static"}),
    ] {
        assert!(
            _native::codec_worker::execute(&request).is_err(),
            "{request}"
        );
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
