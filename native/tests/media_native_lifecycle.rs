//! Native fixture process verifies parent safety; these are not source media-parity claims.
use _native::media_native_adapter::NativeMediaAdapter;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;

struct Scratch(PathBuf);
impl Scratch {
    fn clean(self) {
        assert!(!_native::process_owner::cleanup_pending(&self.0));
        std::fs::remove_dir_all(&self.0).expect("clean native lifecycle control");
    }
}
#[test]
fn lifecycle_fixture_retains_failed_assertions_and_pending_ownership() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let path = parent.join(format!("native lifecycle retention {}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    let evidence = path.join("diagnostic.txt");
    std::fs::write(&evidence, b"owned unresolved fixture").unwrap();
    let failed = std::panic::catch_unwind(|| {
        let _control = Scratch(path.clone());
        panic!("controlled fixture assertion failure");
    });
    assert!(failed.is_err());
    assert_eq!(
        std::fs::read(&evidence).unwrap(),
        b"owned unresolved fixture"
    );
    let marker = path.join("codec-cleanup-pending.json");
    std::fs::write(&marker, b"{}").unwrap();
    assert!(std::panic::catch_unwind(|| Scratch(path.clone()).clean()).is_err());
    assert_eq!(
        std::fs::read(&evidence).unwrap(),
        b"owned unresolved fixture"
    );
    assert_eq!(std::fs::read(&marker).unwrap(), b"{}");
    // This test owns only a synthetic marker: there is no process to reclaim.
    std::fs::remove_file(marker).unwrap();
    Scratch(path.clone()).clean();
    assert!(!path.exists());
}

fn diagnostic(root: &Path, expected: &serde_json::Value) -> PathBuf {
    let prefix = format!("native-codec-diagnostics-{}-", std::process::id());
    let mut matched = Vec::new();
    let mut inspected = 0;
    for entry in std::fs::read_dir(root.join("logs")).unwrap() {
        let entry = entry.unwrap();
        if !entry
            .file_name()
            .to_str()
            .unwrap_or("")
            .starts_with(&prefix)
        {
            continue;
        }
        inspected += 1;
        assert!(
            inspected <= 64,
            "bounded current-process diagnostic inventory"
        );
        let path = entry.path();
        _native::tooling_environment::physical_root(&path).unwrap();
        let request = path.join("envelope/request.json");
        let bytes = match std::fs::symlink_metadata(&request) {
            Ok(metadata) => {
                assert!(metadata.is_file() && metadata.len() <= 65536);
                _native::tooling_environment::reject_links(
                    &request,
                    "diagnostic request cannot use links",
                )
                .unwrap();
                std::fs::read(&request).unwrap()
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => panic!("cannot inspect owned diagnostic request: {error}"),
        };
        let observed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        if expected
            .as_object()
            .unwrap()
            .iter()
            .all(|(key, value)| observed.get(key) == Some(value))
        {
            _native::tooling_environment::physical_root(&path).unwrap();
            assert!(!_native::process_owner::cleanup_pending(
                &path.join("envelope")
            ));
            assert!(
                !path.join("envelope/owner").exists(),
                "owner must finish before relocation"
            );
            matched.push(path);
        }
    }
    assert_eq!(matched.len(), 1, "one exact request's retained diagnostics");
    matched.pop().unwrap()
}

fn names(path: &Path) -> Vec<String> {
    let mut names = std::fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn clean_diagnostic(path: PathBuf) {
    assert_eq!(names(&path), ["envelope"]);
    assert!(!_native::process_owner::cleanup_pending(
        &path.join("envelope")
    ));
    std::fs::remove_dir_all(path.join("envelope")).unwrap();
    std::fs::remove_dir(&path).unwrap();
    assert!(!path.exists());
}

#[path = "support/process_liveness.rs"]
mod process_liveness;
use process_liveness::assert_stopped as dead;
async fn ready(control: &Path) -> Vec<u32> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match std::fs::read_to_string(control.join("ready")) {
                Ok(text) if text.ends_with('\n') => {
                    let ids = text
                        .lines()
                        .map(|line| line.parse::<u32>().unwrap())
                        .collect::<Vec<_>>();
                    if ids.len() == 2 {
                        return ids;
                    }
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("cannot read fixture readiness: {error}"),
            }
            // Polling only until the explicit ready marker, under the timeout above.
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("fixture readiness timeout")
}

struct FixtureCodec<'a> {
    adapter: NativeMediaAdapter,
    environment: &'a std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString>,
    fixture_id: Option<&'a str>,
}
impl _native::media_adapter::MediaCodec for FixtureCodec<'_> {
    fn with_scratch(&self, scratch: PathBuf) -> Self {
        Self {
            adapter: _native::media_adapter::MediaCodec::with_scratch(&self.adapter, scratch),
            environment: self.environment,
            fixture_id: self.fixture_id,
        }
    }

    async fn call(&self, mut request: serde_json::Value) -> Result<serde_json::Value, String> {
        let (wall, idle) = if let Some(id) = self.fixture_id {
            request["fixture_mode"] = json!("cancel");
            request["fixture_id"] = json!(id);
            (8, 6)
        } else {
            (10, 8)
        };
        self.adapter
            .call_with_environment(
                request,
                Duration::from_secs(wall),
                Duration::from_secs(idle),
                self.environment,
            )
            .await
    }
}

#[test]
fn actual_worker_nested_capture_stays_inside_envelope_and_stops_on_cancellation() {
    let root = _native::tooling_environment::physical_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
    )
    .unwrap();
    let parent = _native::tooling_environment::confined_directory(&root, "logs/tmp", true).unwrap();
    let control = parent.join(format!("nested codec capture {}", std::process::id()));
    std::fs::create_dir(&control).unwrap();
    // Keep the complete control/envelope on any failed ownership assertion.
    let tools = control.join("tools");
    std::fs::create_dir(&tools).unwrap();
    let fixture = PathBuf::from(std::env::var_os("NUMERA_MEDIA_FIXTURE_WORKER").unwrap());
    for name in ["ffmpeg", "ffprobe"] {
        std::fs::copy(
            &fixture,
            tools.join(if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.into()
            }),
        )
        .unwrap();
    }
    let mut environment = std::env::vars_os().collect::<std::collections::BTreeMap<_, _>>();
    let mut paths = vec![tools];
    if let Some(value) = environment.get(std::ffi::OsStr::new("PATH")) {
        paths.extend(std::env::split_paths(value));
    }
    environment.insert("PATH".into(), std::env::join_paths(paths).unwrap());
    environment.insert("EMOJI_FFMPEG_TIMEOUT".into(), "7".into());
    let adapter = NativeMediaAdapter {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji")),
        root: control.clone(),
        scratch: control.join("envelopes"),
    };
    let source = root.join("tests/fixtures/native/video-fingerprint/fast30.webm");
    let original = std::fs::read(&source).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let codec = FixtureCodec {
        adapter: _native::media_adapter::MediaCodec::with_scratch(
            &adapter,
            adapter.scratch.clone(),
        ),
        environment: &environment,
        fixture_id: None,
    };
    let previews = _native::panel_preview::Previews::new(codec, &control, 1);
    for mode in ["fingerprint", "preview", "warm"] {
        let pid = runtime.block_on(async {
            let future = async {
                if mode == "warm" {
                    let view = json!({"view":[{"key":"v:cancellation","fmt":"video","included":true}],
                        "by_key":{"v:cancellation":source}});
                    previews.warm(&view, 15).await;
                    Ok(())
                } else if mode == "preview" {
                    previews
                        .bytes("v:cancellation", &source, 15, false, 52)
                        .await
                        .map(|_| ())
                } else {
                    adapter
                        .call_with_environment(
                            json!({"operation":"fingerprint","source":source,"format":"video"}),
                            Duration::from_secs(10),
                            Duration::from_secs(8),
                            &environment,
                        )
                        .await
                        .map(|_| ())
                }
            };
            tokio::pin!(future);
            let readiness = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match std::fs::read_dir(&adapter.scratch) {
                    Ok(entries) => {
                        for entry in entries {
                            let envelope = entry.unwrap().path();
                            match std::fs::read_to_string(envelope.join("nested-codec-ready")) {
                                Ok(text) if text.ends_with('\n') => {
                                    let captures = envelope.join("logs/tmp");
                                    let captures = std::fs::read_dir(captures)
                                        .unwrap()
                                        .map(|entry| entry.unwrap().path())
                                        .collect::<Vec<_>>();
                                    assert_eq!(captures.len(), 1, "one actual nested capture");
                                    assert!(
                                        captures[0]
                                            .join("owner/codec-cleanup-pending.json")
                                            .is_file()
                                    );
                                    return text.trim_end_matches('\n').parse::<u32>().unwrap();
                                }
                                Ok(_) => {}
                                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                                Err(error) => {
                                    panic!("cannot read nested capture readiness: {error}")
                                }
                            }
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => panic!("cannot inspect nested capture envelopes: {error}"),
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        tokio::select! {
            answer = &mut future => panic!("nested worker ended before cancellation: {answer:?}"),
            pid = readiness => pid.expect("nested codec readiness exceeded5s"),
        }
            // Drop the actual consumer/call and its owner before checking the acknowledged child.
        });
        dead(pid);
        let retained = diagnostic(
            &control,
            &json!({
                "operation":if mode == "fingerprint" { "fingerprint" } else { "preview" },
                "source":source
            }),
        );
        let envelope = retained.join("envelope");
        assert_eq!(
            std::fs::read(envelope.join("nested-codec-ready")).unwrap(),
            format!("{pid}\n").as_bytes()
        );
        assert!(!envelope.join("response.json").exists());
        let captures = std::fs::read_dir(envelope.join("logs/tmp"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(captures.len(), 1);
        assert!(
            captures[0]
                .join("owner/codec-cleanup-pending.json")
                .is_file()
        );
        assert_eq!(names(&captures[0]), ["owner", "stderr.bin", "stdout.bin"]);
        // The outer session reaped the acknowledged inner child before moving its evidence.
        clean_diagnostic(retained);
        assert_eq!(std::fs::read_dir(&adapter.scratch).unwrap().count(), 0);
        if mode != "fingerprint" {
            assert_eq!(
                std::fs::read_dir(control.join("preview")).unwrap().count(),
                0
            );
        }
        assert_eq!(std::fs::read(&source).unwrap(), original);
    }
    drop(previews);
    std::fs::remove_dir_all(control).unwrap();
}

#[test]
fn cancelled_codec_retains_original_envelope_when_diagnostic_relocation_is_blocked() {
    let project = _native::tooling_environment::physical_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
    )
    .unwrap();
    let parent =
        _native::tooling_environment::confined_directory(&project, "logs/tmp", true).unwrap();
    let root = parent.join(format!(
        "native diagnostic relocation {}",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    let id = format!("{}-relocation", std::process::id());
    let control = root.join("logs/tmp").join(format!("media-fixture-{id}"));
    std::fs::create_dir_all(&control).unwrap();
    let adapter = NativeMediaAdapter {
        executable: PathBuf::from(std::env::var_os("NUMERA_MEDIA_FIXTURE_WORKER").unwrap()),
        root: root.clone(),
        scratch: root.join("envelopes"),
    };
    let mut environment = std::env::vars_os().collect::<std::collections::BTreeMap<_, _>>();
    environment.insert("EMOJI_FFMPEG_TIMEOUT".into(), "7".into());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let request = json!({"fixture_mode":"cancel","fixture_id":id});
    let ids = runtime.block_on(async {
        let future = adapter.call_with_environment(
            request.clone(),
            Duration::from_secs(8),
            Duration::from_secs(6),
            &environment,
        );
        tokio::pin!(future);
        let ids = tokio::select! {
            ids = ready(&control) => ids,
            answer = &mut future => panic!("fixture ended before relocation refusal: {answer:?}"),
        };
        std::fs::rename(root.join("logs"), root.join("held-logs")).unwrap();
        std::fs::write(root.join("logs"), b"caller-owned diagnostics blocker").unwrap();
        ids
    });
    ids.into_iter().filter(|pid| *pid != 0).for_each(dead);
    assert_eq!(
        std::fs::read(root.join("logs")).unwrap(),
        b"caller-owned diagnostics blocker"
    );
    let envelopes = std::fs::read_dir(&adapter.scratch)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(
        envelopes.len(),
        1,
        "relocation failure must not erase original evidence"
    );
    assert!(!_native::process_owner::cleanup_pending(&envelopes[0]));
    assert_eq!(names(&envelopes[0]), ["request.json"]);
    let observed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(envelopes[0].join("request.json")).unwrap()).unwrap();
    assert_eq!(observed, request);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn native_adapter_scrubs_bounds_reaps_and_refuses_hostile_outputs() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let parent =
        _native::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
    // Separate test cohorts never enumerate each other's transient diagnostic directories.
    let root = parent.join(format!("hostile codec cohort {}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let executable = PathBuf::from(
        std::env::var_os("NUMERA_MEDIA_FIXTURE_WORKER")
            .expect("hosted CI must compile native/tests/fixtures/media_worker.rs"),
    );
    assert!(executable.is_absolute() && executable.is_file());
    let mut environment = std::env::vars_os().collect::<std::collections::BTreeMap<_, _>>();
    for key in ["GENERAL_BOT_TOKEN", "UNRELATED_PRIVATE_VALUE", "PYTHONPATH"] {
        environment.insert(
            std::ffi::OsString::from(key),
            std::ffi::OsString::from("synthetic-not-forwarded"),
        );
    }
    environment.insert("EMOJI_FFMPEG_TIMEOUT".into(), "7".into());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut modes = vec![
        "environment",
        "idle",
        "wall",
        "cancel",
        "gallery-cancel",
        "descendant",
        "oversize",
        "missing-result",
        "false-success",
        "nonobject",
        "invalid-json",
        "uncaught",
        "error-secret",
        "response-directory",
        "output-missing",
        "output-directory",
    ];
    if cfg!(unix) {
        modes.push("response-link");
        modes.push("cleanup-refusal");
    }
    for (index, mode) in modes.into_iter().enumerate() {
        let id = format!("{}-{index}", std::process::id());
        let path = root.join("logs/tmp").join(format!("media-fixture-{id}"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::create_dir(&path).unwrap();
        let control = Scratch(path);
        let adapter = NativeMediaAdapter {
            executable: executable.clone(),
            root: root.clone(),
            scratch: control.0.join("envelopes"),
        };
        let output = control.0.join("destination.webp");
        let mut request = json!({"fixture_mode":mode,"fixture_id":id});
        if mode.starts_with("output-") || mode == "cleanup-refusal" {
            request["output"] = json!(output);
        }
        if mode == "gallery-cancel" {
            let source = project.join("tests/fixtures/native/static-codec/opaque.png");
            let original = std::fs::read(&source).unwrap();
            let cache = control.0.join("gallery");
            let art = std::collections::BTreeMap::from([("111111111".into(), source.clone())]);
            let doc = json!({"family":"general","set_name":"fixture","emoji":[
                {"custom_emoji_id":"111111111","format":"static"}
            ]});
            let codec = FixtureCodec {
                adapter: _native::media_adapter::MediaCodec::with_scratch(
                    &adapter,
                    adapter.scratch.clone(),
                ),
                environment: &environment,
                fixture_id: Some(&id),
            };
            let ids = runtime.block_on(async {
                let rendering = _native::pack_gallery::render(&doc, &codec, &art, &cache);
                tokio::pin!(rendering);
                let ids = tokio::select! {
                    ids = ready(&control.0) => ids,
                    answer = &mut rendering => panic!("gallery ended before cancellation: {answer:?}"),
                };
                let staging = std::fs::read_dir(&cache).unwrap()
                    .map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
                assert_eq!(staging.len(), 1, "one active gallery render");
                let envelopes = std::fs::read_dir(staging[0].join("codec")).unwrap()
                    .map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
                assert_eq!(envelopes.len(), 1, "one scoped codec envelope");
                assert!(envelopes[0].join("owner/codec-cleanup-pending.json").is_file());
                ids
                // Dropping rendering stops its owner before RenderStaging can clean.
            });
            ids.into_iter().filter(|pid| *pid != 0).for_each(dead);
            assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 0);
            assert!(
                !adapter.scratch.exists(),
                "gallery ignored its scoped scratch"
            );
            assert_eq!(std::fs::read(&source).unwrap(), original);
            let retained = diagnostic(&root, &json!({"fixture_mode":"cancel","fixture_id":id}));
            assert_eq!(names(&retained.join("envelope")), ["request.json"]);
            clean_diagnostic(retained);
            control.clean();
            continue;
        }
        runtime.block_on(async {
            if mode=="cancel" || mode=="descendant" {
                let future=adapter.call_with_environment(request.clone(),Duration::from_secs(8),Duration::from_secs(6),&environment);
                tokio::pin!(future);
                let ids=tokio::select! { ids=ready(&control.0)=>ids,answer=&mut future=>panic!("fixture exited before cancellation: {answer:?}") };
                // Leaving this block drops the underlying future and OwnedProcess before PID checks.
                ids
            } else { Vec::new() }
        }).into_iter().filter(|pid|*pid!=0).for_each(dead);
        if mode != "cancel" && mode != "descendant" {
            let (wall, idle) = if mode == "wall" {
                (Duration::from_millis(300), Duration::from_secs(5))
            } else if mode == "idle" {
                (Duration::from_secs(5), Duration::from_millis(300))
            } else {
                (Duration::from_secs(5), Duration::from_secs(3))
            };
            let answer =
                runtime.block_on(adapter.call_with_environment(request, wall, idle, &environment));
            if mode == "environment" {
                assert_eq!(answer.unwrap(), json!({"scrubbed":true}));
            } else if mode == "cleanup-refusal" {
                // Check the refusal before repairing the owned fault or removing its evidence.
                assert!(!output.exists(), "cleanup failure published output");
                assert!(
                    answer.unwrap_err().contains("clean"),
                    "cleanup fault must be reported"
                );
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let blocked = PathBuf::from(
                        std::fs::read_to_string(control.0.join("blocked-path")).unwrap(),
                    );
                    if blocked.exists() {
                        std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o700))
                            .unwrap();
                    }
                    for entry in std::fs::read_dir(&adapter.scratch).unwrap() {
                        let envelope = entry.unwrap().path();
                        assert!(!_native::process_owner::cleanup_pending(&envelope));
                        std::fs::remove_dir_all(envelope).unwrap();
                    }
                    let retained = diagnostic(&root, &json!({"fixture_mode":mode,"fixture_id":id}));
                    assert_eq!(
                        names(&retained.join("envelope")),
                        ["request.json", "response.json"]
                    );
                    assert_eq!(
                        std::fs::read(retained.join("envelope/response.json")).unwrap(),
                        br#"{"ok":true,"result":{"bytes":7}}"#
                    );
                    clean_diagnostic(retained);
                }
            } else {
                let error = answer.unwrap_err();
                for (name, expected) in [
                    ("false-success", "refused fixture"),
                    ("nonobject", "media adapter failed"),
                    ("invalid-json", "invalid media adapter response"),
                    ("missing-result", "media adapter response has no result"),
                    ("oversize", "media adapter response exceeds64KiB"),
                ] {
                    if mode == name {
                        assert_eq!(error, expected);
                    }
                }
                if mode == "idle" {
                    assert!(error.contains("no CPU/IO progress"), "{error}");
                }
                if mode == "wall" {
                    assert!(error.contains("wall budget"), "{error}");
                }
                if mode == "error-secret" {
                    assert!(error.contains("[REDACTED]"));
                    assert!(!error.contains("abcdefghijklmnopqrstuvwxyz"));
                }
            }
            if matches!(mode, "idle" | "wall") {
                for pid in std::fs::read_to_string(control.0.join("ready"))
                    .unwrap()
                    .lines()
                    .map(|line| line.parse::<u32>().unwrap())
                    .filter(|pid| *pid != 0)
                {
                    dead(pid);
                }
            }
        }
        if mode != "environment" && mode != "cleanup-refusal" {
            let retained = diagnostic(&root, &json!({"fixture_mode":mode,"fixture_id":id}));
            let envelope = retained.join("envelope");
            let actual: serde_json::Value =
                serde_json::from_slice(&std::fs::read(envelope.join("request.json")).unwrap())
                    .unwrap();
            let mut expected_request = json!({"fixture_mode":mode,"fixture_id":id});
            if mode.starts_with("output-") {
                let staged = actual["output"].as_str().map(Path::new).unwrap();
                assert_eq!(staged.file_name().unwrap(), "destination.webp");
                assert!(staged.starts_with(&adapter.scratch));
                expected_request["output"] = actual["output"].clone();
            }
            assert_eq!(actual, expected_request);
            let response = envelope.join("response.json");
            match mode {
                "idle" | "wall" | "cancel" | "descendant" | "uncaught" => {
                    assert_eq!(names(&envelope), ["request.json"]);
                }
                "response-directory" => assert_eq!(names(&response), Vec::<String>::new()),
                "response-link" => {
                    let target = control.0.join("preserved.json");
                    assert_eq!(std::fs::read_link(&response).unwrap(), target);
                    assert_eq!(
                        std::fs::read(&target).unwrap(),
                        br#"{"ok":true,"result":true}"#
                    );
                }
                _ => {
                    let expected = match mode {
                        "oversize" => vec![b'x'; 65537],
                        "missing-result" => br#"{"ok":true}"#.to_vec(),
                        "false-success" => br#"{"ok":false,"error":"refused fixture"}"#.to_vec(),
                        "nonobject" => b"[]".to_vec(),
                        "invalid-json" => b"not json".to_vec(),
                        "error-secret" => br#"{"ok":false,"error":"fixture 123456789:abcdefghijklmnopqrstuvwxyz0123456789 refused"}"#.to_vec(),
                        "output-missing" | "output-directory" => br#"{"ok":true,"result":{"bytes":7}}"#.to_vec(),
                        _ => unreachable!(),
                    };
                    assert_eq!(std::fs::read(&response).unwrap(), expected);
                }
            }
            if mode == "output-directory" {
                assert_eq!(
                    names(&envelope),
                    ["destination.webp", "request.json", "response.json"]
                );
                assert_eq!(
                    names(&envelope.join("destination.webp")),
                    Vec::<String>::new()
                );
            } else if !matches!(mode, "idle" | "wall" | "cancel" | "descendant" | "uncaught") {
                assert_eq!(names(&envelope), ["request.json", "response.json"]);
            }
            // Only this exact, inspected fixture envelope is disposable, after all PID checks.
            clean_diagnostic(retained);
        }
        assert_eq!(
            std::fs::read_dir(&adapter.scratch).unwrap().count(),
            0,
            "{mode}"
        );
        assert!(!output.exists(), "unproved output published: {mode}");
        control.clean();
    }
    assert_eq!(names(&root), ["logs"]);
    assert_eq!(names(&root.join("logs")), ["tmp"]);
    assert!(names(&root.join("logs/tmp")).is_empty());
    std::fs::remove_dir(root.join("logs/tmp")).unwrap();
    std::fs::remove_dir(root.join("logs")).unwrap();
    std::fs::remove_dir(root).unwrap();
}
