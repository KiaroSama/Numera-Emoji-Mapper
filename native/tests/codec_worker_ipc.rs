//! The real internal executable publishes exclusively before its owner removes scratch.
use _native::process_owner::OwnedProcess;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
fn executable_codec_envelope_is_owned_bounded_and_exclusive() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let directory = root
        .join("logs")
        .join(format!("native codec ipc {}", std::process::id()));
    std::fs::create_dir_all(root.join("logs")).unwrap();
    std::fs::create_dir(&directory).unwrap();
    let source = root.join("tests/fixtures/native/static-codec/alpha-edge.png");
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    let expected = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "alpha-edge")
        .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for (index, request, existing, expected_code) in [
        (
            0,
            json!({"operation":"fingerprint","source":source,"format":"static"}),
            false,
            0,
        ),
        (
            1,
            json!({"operation":"image_info","source":"relative.png","format":"static"}),
            false,
            1,
        ),
        (
            2,
            json!({"operation":"fingerprint","source":source,"format":"static"}),
            true,
            2,
        ),
    ] {
        let workspace = directory.join(index.to_string());
        std::fs::create_dir(&workspace).unwrap();
        let input = workspace.join("request.json");
        let output = workspace.join("response.json");
        _native::atomic::write_json(&input, &request).unwrap();
        if existing {
            std::fs::write(&output, b"preserve").unwrap();
        }
        let mut command = Command::new(env!("CARGO_BIN_EXE_numera-emoji"));
        command
            .arg("codec-worker")
            .arg(&input)
            .arg(&output)
            .current_dir(&workspace)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
            .env("PYTHONUTF8", "1")
            .env("TEMP", &workspace)
            .env("TMP", &workspace)
            .env("TMPDIR", &workspace);
        for (name, _) in std::env::vars_os() {
            let upper = name.to_string_lossy().to_uppercase();
            if ["TOKEN", "SECRET", "PASSWORD", "API_KEY"]
                .iter()
                .any(|key| upper.contains(key))
            {
                command.env_remove(name);
            }
        }
        let mut owner = OwnedProcess::spawn(&mut command, workspace.clone()).unwrap();
        {
            let status = runtime
                .block_on(owner.wait(Duration::from_secs(20), Duration::from_secs(10)))
                .unwrap();
            assert_eq!(status.code(), Some(expected_code));
            let bytes = std::fs::read(&output).unwrap();
            if existing {
                assert_eq!(bytes, b"preserve");
            } else {
                let response: Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(response["ok"], expected_code == 0);
                if expected_code == 0 {
                    assert_eq!(
                        response["result"],
                        json!({"content_key":expected["expected"]["content_key"],"phash":expected["expected"]["phash"],"fmt":"static","extension":".png"})
                    );
                } else {
                    assert_eq!(response["error"], "source must be absolute");
                }
            }
        }
        owner.finish().unwrap();
        assert!(!workspace.exists());
    }
    std::fs::remove_dir(&directory).unwrap();
}
