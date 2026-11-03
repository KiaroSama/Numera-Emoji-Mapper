//! Real worker preserves the source catch boundary: handled errors answer; TypeError/EOF escape.
use _native::process_owner::OwnedProcess;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

struct Scratch(PathBuf);
impl Scratch {
    fn clean(self) {
        // Completed cases have removed their workspaces; unexpected evidence must survive.
        assert!(!_native::process_owner::cleanup_pending(&self.0));
        let executable = if cfg!(windows) {
            "numera-emoji.exe"
        } else {
            "numera-emoji"
        };
        let expected = ["assets", "logs", "pyproject.toml", executable]
            .map(std::ffi::OsString::from)
            .into_iter()
            .collect::<BTreeSet<_>>();
        let actual = std::fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, expected);
        for name in expected {
            let path = self.0.join(&name);
            _native::tooling_environment::reject_links(
                &path,
                "IPC fixture contains a link; retained",
            )
            .unwrap();
            let metadata = std::fs::symlink_metadata(&path).unwrap();
            if name == "assets" || name == "logs" {
                assert!(metadata.is_dir());
                assert!(!_native::process_owner::cleanup_pending(&path));
            } else {
                assert!(metadata.is_file());
            }
        }
        std::fs::remove_dir_all(&self.0).expect("clean typed codec IPC scratch");
    }
}
#[test]
fn typed_source_errors_do_not_become_invented_worker_envelopes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/media-continuation/worker.json"
    ))
    .unwrap();
    let path = root
        .join("logs/tmp")
        .join(format!("typed codec worker {}", std::process::id()));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::create_dir(&path).unwrap();
    let scratch = Scratch(path);
    // One isolated executable root makes each operational log attributable without scanning user logs.
    std::fs::create_dir(scratch.0.join("assets")).unwrap();
    for name in ["assets/panel.html", "pyproject.toml"] {
        std::fs::copy(root.join(name), scratch.0.join(name)).unwrap();
    }
    let executable = scratch.0.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut cases = [
        "unsupported-operation",
        "list-operation",
        "preview-list-size",
        "preview-float-fps",
        "corrupt-metadata",
    ]
    .into_iter()
    .map(|name| {
        fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap()
            .clone()
    })
    .collect::<Vec<_>>();
    let lottie: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/lottie-file.json")).unwrap();
    assert_eq!(lottie["source_revision"], fixture["source_revision"]);
    assert_eq!(
        lottie["source_sha256"]["emojikit/media_lottie.py"],
        "5112b55f21d4d241ffe9660d5294de20487d4ea1e9772188fb6bd125f7fef63b"
    );
    for input in [
        "tests/fixtures/native/tgs-load/valid.tgs",
        "tests/fixtures/native/tgs-load/invalid-json.tgs",
        "tests/fixtures/native/animated-key/truncated.tgs",
    ] {
        use sha2::{Digest, Sha256};
        let case = lottie["validations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["input"] == input)
            .unwrap();
        let source = root.join(input);
        let digest = Sha256::digest(std::fs::read(&source).unwrap())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(digest, case["input_sha256"]);
        let mut mapped = json!({"name":input,
            "request":{"operation":"usable","source":source,"format":"animated"}});
        if let Some(expected) = case.get("expected") {
            mapped["expected"] = expected.clone();
        } else {
            mapped["error"] = json!({"kind":case["error_kind"],"message":case["error"]});
        }
        cases.push(mapped);
    }
    let animated: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/animated-key.json"
    ))
    .unwrap();
    assert_eq!(animated["source_revision"], lottie["source_revision"]);
    assert_eq!(
        animated["source_sha256"],
        json!({
            "emojikit/identity.py":"8cbefd550e306e671df1e52af2dd9fe6b4e3647c3aa1b13dc98bf35080d36e89",
            "emojikit/media_lottie.py":"5112b55f21d4d241ffe9660d5294de20487d4ea1e9772188fb6bd125f7fef63b"
        })
    );
    for input in [
        "tests/fixtures/native/animated-key/nonfinite.json",
        "tests/fixtures/native/tgs-load/invalid-json.tgs",
        "tests/fixtures/native/tgs-load/root-list.tgs",
        "tests/fixtures/native/tgs-load/expansion.tgs",
        "tests/fixtures/native/animated-key/truncated.tgs",
    ] {
        let case = animated["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["input"] == input)
            .unwrap();
        let source = root.join(input);
        assert_eq!(
            _native::tooling_manifest::digest(&source).unwrap(),
            case["input_sha256"].as_str().unwrap()
        );
        let mut mapped = json!({"name":format!("fingerprint {input}"),
            "request":{"operation":"fingerprint","source":source,"format":"animated"}});
        if let Some(expected) = case.get("expected") {
            mapped["expected"] = expected.clone();
        } else {
            mapped["error"] = json!({"kind":case["error_kind"],"message":case["error"]});
        }
        cases.push(mapped);
    }
    let video: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/video-codec.json")).unwrap();
    assert_eq!(video["source_revision"], fixture["source_revision"]);
    assert_eq!(
        video["source_sha256"]["emojikit/media.py"],
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53"
    );
    let mut video_inputs = Vec::new();
    for (input, refusal) in [
        ("tests/fixtures/native/video-codec/opaque.webm", false),
        ("tests/fixtures/native/video-codec/vp8.webm", true),
    ] {
        let original = video["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["input"] == input)
            .unwrap();
        let source = root.join(input);
        assert_eq!(
            _native::tooling_manifest::digest(&source).unwrap(),
            original["input_sha256"].as_str().unwrap()
        );
        video_inputs.push((source.clone(), std::fs::read(&source).unwrap()));
        let mut case = json!({"name":input,
            "request":{"operation":"usable","source":source,"format":"video"}});
        if refusal {
            case["error"] = json!({"kind":"MediaError",
                "message":"vp8.webm: codec 'vp8' != 'vp9'"});
        } else {
            case["expected"] = json!(true);
        }
        cases.push(case);
        cases.push(json!({"name":format!("fingerprint {input}"),
            "request":{"operation":"fingerprint","source":source,"format":"video"},
            "expected":{"content_key":original["expected"]["content_key"],
                "phash":original["expected"]["phash"],"fmt":"video","extension":".webm"}}));
        if !refusal {
            cases.push(json!({"name":"prepare unchanged video",
                "request":{"operation":"prepare","source":source,"format":"video","reencode":false},
                "expected":{"content_key":original["expected"]["content_key"],
                    "phash":original["expected"]["phash"],"fmt":"video","extension":".webm"}}));
        }
    }
    let refused_probe: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/video-probe-error.json"
    ))
    .unwrap();
    assert_eq!(refused_probe["source_revision"], video["source_revision"]);
    assert_eq!(
        refused_probe["source_sha256"],
        video["source_sha256"]["emojikit/media.py"]
    );
    assert_eq!(refused_probe["expected_failure"], true);
    assert_eq!(refused_probe["error_kind"], "CalledProcessError");
    let source = root.join(refused_probe["input"].as_str().unwrap());
    assert_eq!(
        _native::tooling_manifest::digest(&source).unwrap(),
        refused_probe["input_sha256"].as_str().unwrap()
    );
    video_inputs.push((source.clone(), std::fs::read(&source).unwrap()));
    cases.push(json!({"name":"fingerprint truncated video",
        "request":{"operation":"fingerprint","source":source,"format":"video"},
        "error":{"kind":"UndecodableVideo","message_prefix":
            "truncated.webm: the codec could not be read, so the decoder that preserves its alpha cannot be chosen: Command '"}}));
    cases.push(json!({"name":"usable truncated video",
        "request":{"operation":"usable","source":source,"format":"video"},
        "error":{"kind":"CalledProcessError"}}));
    cases.push(json!({"name":"preview truncated video",
        "request":{"operation":"preview","source":source,"format":"video",
            "output":"$output","key":"fixture","size":52,"fps":15,"still":true},
        "error":{"kind":"UndecodableVideo","message_prefix":
            "truncated.webm: the codec could not be read, so the decoder that preserves its alpha cannot be chosen: Command '"}}));
    cases.push(json!({"name":"convert truncated video",
        "request":{"operation":"convert","source":source,"format":"video",
            "target_format":"video","output":"$video_output"},
        "error":{"kind":"UndecodableVideo","message_prefix":
            "truncated.webm: the codec could not be read, so the decoder that preserves its alpha cannot be chosen: Command '"}}));
    cases.push(json!({"name":"prepare truncated video",
        "request":{"operation":"prepare","source":source,"format":"video","reencode":false},
        "error":{"kind":"UndecodableVideo","message_prefix":
            "truncated.webm: the codec could not be read, so the decoder that preserves its alpha cannot be chosen: Command '"}}));
    for (name, field, value, kind, message) in [
        (
            "prepare nonstring tint",
            "tint",
            json!(true),
            "ValueError",
            "tint must be #RRGGBB",
        ),
        (
            "prepare invalid tint",
            "tint",
            json!("bad"),
            "MediaError",
            "tint must be #RRGGBB, got 'bad'",
        ),
        (
            "prepare refused tint",
            "tint",
            json!("#1A80FF"),
            "ValueError",
            "requested tint could not be applied",
        ),
        (
            "prepare invalid reencode",
            "reencode",
            json!(1),
            "ValueError",
            "reencode must be boolean",
        ),
    ] {
        let mut request = json!({"operation":"prepare","source":source,"format":"video"});
        request[field] = value;
        cases.push(json!({"name":name,"request":request,"error":{"kind":kind,"message":message}}));
    }
    for (name, request) in [
        (
            "dict operation",
            json!({"operation":{},"source":source,"format":"video"}),
        ),
        (
            "dict preview size",
            json!({"operation":"preview","source":source,"format":"video","key":"fixture","size":{}}),
        ),
        (
            "dict conversion format",
            json!({"operation":"convert","source":source,"format":{"x":1}}),
        ),
    ] {
        cases.push(
            json!({"name":name,"request":request,"error":{"kind":"TypeError",
            "message":"unhashable type: 'dict'"}}),
        );
    }
    cases.push(json!({"name":"compare equal undecodable video bytes",
        "request":{"operation":"compare","source":source,"other":source,"format":"video"},
        "expected":true}));
    for (name, input, format) in [
        ("same_image undecodable video", source.clone(), "video"),
        (
            "same_image undecodable static",
            root.join(fixture["inputs"]["$corrupt"].as_str().unwrap()),
            "static",
        ),
        (
            "same_image invalid vector root",
            root.join("tests/fixtures/native/tgs-load/root-list.tgs"),
            "animated",
        ),
    ] {
        cases.push(json!({"name":name,"request":{"operation":"same_image",
            "source":input,"other":input,"format":format},"expected":null}));
    }
    let mut observed_logs = BTreeSet::new();
    for (index, case) in cases.iter().enumerate() {
        let name = case["name"].as_str().unwrap();
        let workspace = scratch.0.join(index.to_string());
        std::fs::create_dir(&workspace).unwrap();
        let request_path = workspace.join("request.json");
        let response_path = workspace.join("response.json");
        let ownership = workspace.join("owner");
        std::fs::create_dir(&ownership).unwrap();
        let mut request = case["request"].clone();
        for value in request.as_object_mut().unwrap().values_mut() {
            match value.as_str() {
                Some("$source") => {
                    *value = json!(root.join(fixture["inputs"]["$source"].as_str().unwrap()))
                }
                Some("$corrupt") => {
                    *value = json!(root.join(fixture["inputs"]["$corrupt"].as_str().unwrap()))
                }
                Some("$output") => *value = json!(workspace.join("output.webp")),
                Some("$video_output") => *value = json!(workspace.join("output.webm")),
                _ => {}
            }
        }
        _native::atomic::write_json(&request_path, &request).unwrap();
        let stderr_path = workspace.join("stderr.log");
        let stderr = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stderr_path)
            .unwrap();
        let mut command = Command::new(&executable);
        command
            .arg("codec-worker")
            .arg(&request_path)
            .arg(&response_path)
            .current_dir(&root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr))
            .env_clear();
        for key in [
            "PATH",
            "SystemRoot",
            "WINDIR",
            "SYSTEMDRIVE",
            "LD_LIBRARY_PATH",
            "DYLD_LIBRARY_PATH",
            "DYLD_FALLBACK_LIBRARY_PATH",
            "PYTHONHOME",
            "VIRTUAL_ENV",
            "LANG",
            "LC_ALL",
        ] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command
            .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
            .env("PYTHONNOUSERSITE", "1")
            .env("PYTHONSAFEPATH", "1")
            .env("PYTHONUTF8", "1")
            .env("TEMP", &workspace)
            .env("TMP", &workspace)
            .env("TMPDIR", &workspace);
        // Video operations create nested private groups; the outer owner must retain their session.
        let mut owner = OwnedProcess::spawn_session(&mut command, ownership.clone()).unwrap();
        drop(command);
        let status = runtime
            .block_on(owner.wait(Duration::from_secs(15), Duration::from_secs(8)))
            .unwrap();
        owner.finish().unwrap();
        assert!(!ownership.exists());
        let stderr = std::fs::read_to_string(&stderr_path).unwrap();
        let logs = scratch.0.join("logs");
        _native::tooling_environment::reject_links(&logs, "IPC logs contain a link; retained")
            .unwrap();
        let files = std::fs::read_dir(&logs)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<BTreeSet<_>>();
        assert!(
            observed_logs.is_subset(&files),
            "previous logs must survive: {name}"
        );
        assert_eq!(
            files.len(),
            index + 1,
            "exactly one per-run worker log: {name}"
        );
        let added = files.difference(&observed_logs).collect::<Vec<_>>();
        assert_eq!(
            added.len(),
            1,
            "one new log for the completed worker: {name}"
        );
        let path = added[0];
        _native::tooling_environment::reject_links(path, "IPC log is linked; retained").unwrap();
        let metadata = std::fs::symlink_metadata(path).unwrap();
        assert!(metadata.is_file() && !metadata.file_type().is_symlink());
        assert!(metadata.len() <= 128 * 1024);
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert!(filename.starts_with("codec-worker_") && filename.ends_with(".log"));
        let text = std::fs::read_to_string(path).unwrap();
        let run_id = text.lines().next().unwrap().rsplit_once(" run=").unwrap().1;
        assert!(!run_id.is_empty());
        assert!(
            text.lines()
                .all(|line| line.ends_with(&format!(" run={run_id}")))
        );
        // Successful summaries are INFO and need not be echoed; failures identify this new file.
        if !status.success() {
            assert!(
                stderr
                    .lines()
                    .any(|line| line.ends_with(&format!(" run={run_id}"))),
                "failure must identify its new operational log: {name}"
            );
        }
        if name == "prepare refused tint" {
            assert_eq!(
                text.lines()
                    .filter(|line| line.contains(
                        "[WARNING] [codec] repaint not supported for truncated.webm (video)"
                    ))
                    .count(),
                1
            );
            assert_eq!(
                text.lines().filter(|line| line.contains(
                    "outcome=FAILED(1) warnings=1 errors=1 critical=0"
                )).count(), 1
            );
        }
        if name.starts_with("dict ") {
            assert_eq!(
                text.lines()
                    .filter(
                        |line| line.contains("[ERROR] [codec] TypeError: unhashable type: 'dict'")
                    )
                    .count(),
                1
            );
        }
        observed_logs = files;
        if name == "prepare refused tint" {
            assert_eq!(
                stderr
                    .lines()
                    .filter(|line| {
                        *line == "WARNING: repaint not supported for truncated.webm (video)"
                    })
                    .count(),
                1
            );
            assert_eq!(
                stderr
                    .lines()
                    .filter(|line| line.contains(
                        "[WARNING] [codec] repaint not supported for truncated.webm (video)"
                    ))
                    .count(),
                1,
                "preparation warning must reach the file-log sink"
            );
            assert_eq!(stderr.lines().filter(|line| line.contains(
                "outcome=FAILED(1) warnings=1 errors=1 critical=0"
            )).count(), 1, "warning and handled failure must each be counted once");
        } else if name.starts_with("prepare ") {
            assert!(!stderr.contains("repaint not supported for"));
        }
        if let Some(expected) = case.get("expected") {
            assert_eq!(status.code(), Some(0), "{name}");
            let response: Value =
                serde_json::from_slice(&std::fs::read(&response_path).unwrap()).unwrap();
            assert_eq!(response, json!({"ok":true,"result":expected}));
        } else if matches!(
            case["error"]["kind"].as_str(),
            Some("TypeError" | "EOFError" | "CalledProcessError")
        ) {
            assert_eq!(status.code(), Some(1), "{name}");
            assert!(
                !response_path.exists(),
                "source exception must escape without a response: {name}"
            );
            let kind = case["error"]["kind"].as_str().unwrap();
            assert!(
                stderr
                    .lines()
                    .any(|line| line.starts_with(&format!("{kind}: "))),
                "native uncertainty must not replace the expected exception: {name}"
            );
            assert!(
                text.lines()
                    .any(|line| line.contains(&format!("[ERROR] [codec] {kind}: "))),
                "the same exception must reach the operational log: {name}"
            );
            if name.starts_with("dict ") {
                assert!(
                    stderr
                        .lines()
                        .any(|line| line == "TypeError: unhashable type: 'dict'"),
                    "unhandled dict error must preserve its diagnostic: {name}"
                );
                assert_eq!(
                    stderr
                        .lines()
                        .filter(|line| line
                            .contains("[ERROR] [codec] TypeError: unhashable type: 'dict'"))
                        .count(),
                    1,
                    "unhandled cause must reach the structured log sink: {name}"
                );
                assert_eq!(
                    stderr
                        .lines()
                        .filter(|line| line
                            .contains("outcome=FAILED(1) warnings=0 errors=1 critical=0"))
                        .count(),
                    1,
                    "unhandled failure must be counted exactly once: {name}"
                );
            }
        } else {
            assert_eq!(status.code(), Some(1), "{name}");
            let response: Value =
                serde_json::from_slice(&std::fs::read(&response_path).unwrap()).unwrap();
            assert_eq!(response["ok"], false);
            if let Some(prefix) = case["error"]["message_prefix"].as_str() {
                let message = response["error"].as_str().unwrap();
                assert!(message.starts_with(prefix), "{name}: {message}");
                assert!(message.contains("returned non-zero exit status"));
            } else if name != "corrupt-metadata" {
                assert_eq!(response["error"], case["error"]["message"]);
            } else {
                assert!(
                    response["error"]
                        .as_str()
                        .unwrap()
                        .starts_with("cannot identify image file ")
                );
            }
        }
        assert!(!workspace.join("output.webp").exists());
        assert!(!workspace.join("output.webm").exists());
        assert!(!_native::process_owner::cleanup_pending(&workspace));
        // Inner capture retention lives below logs/tmp, outside the shallow ownership query.
        let capture_logs = workspace.join("logs");
        _native::tooling_environment::reject_links(
            &capture_logs,
            "IPC captures are linked; retained",
        )
        .unwrap();
        let mut expected = ["request.json", "stderr.log"]
            .map(std::ffi::OsString::from)
            .into_iter()
            .collect::<BTreeSet<_>>();
        if response_path.exists() {
            expected.insert("response.json".into());
        }
        match std::fs::symlink_metadata(&capture_logs) {
            Ok(metadata) => {
                assert!(metadata.is_dir());
                assert_eq!(
                    std::fs::read_dir(&capture_logs)
                        .unwrap()
                        .map(|entry| entry.unwrap().file_name())
                        .collect::<BTreeSet<_>>(),
                    BTreeSet::from([std::ffi::OsString::from("tmp")])
                );
                let temporary = capture_logs.join("tmp");
                _native::tooling_environment::reject_links(
                    &temporary,
                    "IPC capture storage is linked; retained",
                )
                .unwrap();
                assert_eq!(
                    std::fs::read_dir(temporary).unwrap().count(),
                    0,
                    "unresolved inner capture evidence must survive: {name}"
                );
                expected.insert("logs".into());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("cannot inspect IPC capture evidence; retained: {error}"),
        }
        assert_eq!(
            std::fs::read_dir(&workspace)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<BTreeSet<_>>(),
            expected,
            "unexpected worker evidence must survive: {name}"
        );
        std::fs::remove_dir_all(&workspace).unwrap();
        assert!(!workspace.exists());
    }
    for (source, original) in video_inputs {
        assert_eq!(std::fs::read(source).unwrap(), original);
    }
    scratch.clean();
}
