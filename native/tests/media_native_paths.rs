//! Filesystem/envelope safety is an additive native contract, not a relaxation of source parity.
use _native::{media_native_adapter::NativeMediaAdapter, process_owner::OwnedProcess};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

struct Scratch(PathBuf);
impl Scratch {
    fn clean(self) {
        let envelopes = self.0.join("envelopes");
        assert_eq!(std::fs::read_dir(envelopes).unwrap().count(), 0);
        assert!(!_native::process_owner::cleanup_pending(&self.0));
        std::fs::remove_dir_all(&self.0).expect("clean media path fixture");
    }
}

#[test]
fn native_output_cannot_escape_or_alias_the_owned_envelope() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let path = root
        .join("logs/tmp")
        .join(format!("native media paths {}", std::process::id()));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::create_dir(&path).unwrap();
    let scratch = Scratch(path);
    let source = root.join("tests/fixtures/native/static-codec/alpha-edge.png");
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji"));
    let adapter = NativeMediaAdapter {
        root: root.clone(),
        executable: executable.clone(),
        scratch: scratch.0.join("envelopes"),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for output in [
        scratch.0.join("../traversal.webp"),
        scratch.0.join("response.json"),
        scratch.0.join("codec-cleanup-pending.json"),
    ] {
        let result = runtime.block_on(adapter.call_with_budget(
            json!({"operation":"preview","source":source,"output":output,"key":"ok"}),
            Duration::from_secs(5),
            Duration::from_secs(3),
        ));
        assert!(result.is_err(), "{output:?}");
        assert!(!output.exists());
    }
    assert_eq!(std::fs::read_dir(&adapter.scratch).unwrap().count(), 0);
    for (index, name) in [
        "response.json",
        "codec-cleanup-pending.json",
        "outside-envelope.webp",
    ]
    .into_iter()
    .enumerate()
    {
        let workspace = scratch.0.join(format!("direct-{index}"));
        std::fs::create_dir(&workspace).unwrap();
        let request = workspace.join("request.json");
        let response = workspace.join("response.json");
        let output = if name == "outside-envelope.webp" {
            scratch.0.join(name)
        } else {
            workspace.join(name)
        };
        _native::atomic::write_json(
            &request,
            &json!({"operation":"preview","source":source,"output":output,"key":"ok"}),
        )
        .unwrap();
        let mut command = Command::new(&executable);
        command
            .arg("codec-worker")
            .arg(&request)
            .arg(&response)
            .current_dir(&root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1");
        let ownership = workspace.join("owner");
        std::fs::create_dir(&ownership).unwrap();
        let mut owner = OwnedProcess::spawn(&mut command, ownership.clone()).unwrap();
        let status = runtime
            .block_on(owner.wait(Duration::from_secs(10), Duration::from_secs(5)))
            .unwrap();
        owner.finish().unwrap();
        assert!(!ownership.exists());
        assert_eq!(status.code(), Some(1));
        let answer: Value = serde_json::from_slice(&std::fs::read(response).unwrap()).unwrap();
        assert_eq!(answer["ok"], false);
        assert_eq!(
            answer["error"],
            if name == "outside-envelope.webp" {
                "codec output must stay in its owned envelope directory"
            } else {
                "codec output must not alias its request, response or ownership marker"
            }
        );
        if name == "outside-envelope.webp" {
            assert!(!output.exists());
        }
        std::fs::remove_dir_all(&workspace).unwrap();
        assert!(!workspace.exists());
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let mut environment = std::collections::BTreeMap::new();
        environment.insert(
            "EMOJI_FFMPEG_TIMEOUT".into(),
            std::ffi::OsString::from_vec(vec![0xff]),
        );
        let error = runtime
            .block_on(adapter.call_with_environment(
                json!({"operation":"fingerprint","source":source}),
                Duration::from_secs(5),
                Duration::from_secs(3),
                &environment,
            ))
            .unwrap_err();
        assert_eq!(error, "codec timeout is not UTF-8");
        assert_eq!(std::fs::read_dir(&adapter.scratch).unwrap().count(), 0);
        let linked_root = scratch.0.join("linked-root");
        std::os::unix::fs::symlink(&root, &linked_root).unwrap();
        let linked_root_adapter = NativeMediaAdapter {
            root: linked_root.clone(),
            executable: executable.clone(),
            scratch: scratch.0.join("must-not-create"),
        };
        assert!(
            runtime
                .block_on(
                    linked_root_adapter.call(json!({"operation":"fingerprint","source":source}))
                )
                .is_err()
        );
        assert!(!linked_root_adapter.scratch.exists());
        std::fs::remove_file(linked_root).unwrap();
        let physical = scratch.0.join("physical");
        std::fs::create_dir(&physical).unwrap();
        let linked = scratch.0.join("linked");
        std::os::unix::fs::symlink(&physical, &linked).unwrap();
        let linked_adapter = NativeMediaAdapter {
            root,
            executable,
            scratch: linked.clone(),
        };
        assert!(
            runtime
                .block_on(linked_adapter.call(json!({"operation":"fingerprint","source":source})))
                .is_err()
        );
        let output = linked.join("result.webp");
        assert!(
            runtime
                .block_on(adapter.call(
                    json!({"operation":"preview","source":source,"output":output,"key":"ok"})
                ))
                .is_err()
        );
        assert_eq!(std::fs::read_dir(physical).unwrap().count(), 0);
        std::fs::remove_file(linked).unwrap();
    }
    scratch.clean();
}
