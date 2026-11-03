//! Original private-download guarantees through the public upload/readback boundary.
use _native::{media_adapter::MediaCodec, telegram::Telegram, upload_effect::UploadEffect};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;
#[path = "support/read_only_api.rs"]
mod read_only_api;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use read_only_api::{Fixture, Reply, TOKEN};
use tooling_fixture::Root;

#[derive(Clone)]
struct Observe {
    observed: mpsc::Sender<PathBuf>,
    release: std::sync::Arc<std::sync::Mutex<mpsc::Receiver<()>>>,
    directory: PathBuf,
    scratch: PathBuf,
    outcome: &'static str,
}
impl MediaCodec for Observe {
    fn with_scratch(&self, scratch: PathBuf) -> Self {
        assert_eq!(
            scratch.parent().and_then(Path::parent),
            Some(self.directory.as_path())
        );
        Self {
            scratch,
            ..self.clone()
        }
    }
    async fn call(&self, request: Value) -> Result<Value, String> {
        assert_eq!(request["operation"], "same_image");
        assert_eq!(request["format"], "static");
        let path = PathBuf::from(request["source"].as_str().unwrap());
        assert_eq!(
            path.parent().and_then(Path::parent),
            Some(self.directory.as_path())
        );
        assert_eq!(self.scratch, path.parent().unwrap().join("codec"));
        assert!(path.is_file(), "download did not exist at the codec seam");
        assert_eq!(std::fs::read(&path).unwrap(), b"synthetic downloaded bytes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for private in [&path, path.parent().unwrap()] {
                assert_eq!(
                    std::fs::metadata(private).unwrap().permissions().mode() & 0o077,
                    0,
                    "download file or directory permits another account"
                );
            }
        }
        self.observed.send(path.clone()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(6))
            .map_err(|_| "fixture codec release exceeded6s")?;
        match self.outcome {
            "error" => Err("controlled codec refusal".into()),
            "panic" => panic!("controlled codec unwind"),
            "cleanup" => {
                // A controlled non-file substitution must fail cleanup visibly,
                // not turn a correct comparison into a successful operation.
                std::fs::remove_file(&path).unwrap();
                std::fs::create_dir(&path).unwrap();
                Ok(json!(true))
            }
            "pending" => {
                std::fs::create_dir_all(&self.scratch).unwrap();
                _native::atomic::write_json(
                    &self.scratch.join("codec-cleanup-pending.json"),
                    &json!({"synthetic":true}),
                )
                .unwrap();
                Err("controlled unresolved codec custody".into())
            }
            _ => Ok(json!(true)),
        }
    }
}
fn replies() -> Vec<Reply> {
    vec![
        Reply::api(
            "getFile",
            "file_id",
            "fixture-file",
            json!({"file_path":"synthetic.bin"}),
        ),
        Reply::file("synthetic.bin", b"synthetic downloaded bytes"),
    ]
}

#[test]
fn original_downloads_are_private_distinct_and_removed_after_success_error_or_unwind() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = Root::new("private publication downloads café");
    let directory = root.0.join("tmp");
    std::fs::create_dir(&directory).unwrap();
    let sentinel = directory.join("caller.dl");
    std::fs::write(&sentinel, b"caller bytes").unwrap();
    let (observed, receiver) = mpsc::channel();
    for outcomes in [
        ["ok", "ok"],
        ["error", "panic"],
        ["cleanup", "ok"],
        ["pending", "ok"],
    ] {
        std::thread::scope(|scope| {
            let mut releases = Vec::new();
            let mut children = Vec::new();
            for outcome in outcomes {
                let (release, wait) = mpsc::channel();
                releases.push(release);
                let media = Observe {
                    observed: observed.clone(),
                    release: std::sync::Arc::new(std::sync::Mutex::new(wait)),
                    directory: directory.clone(),
                    scratch: PathBuf::new(),
                    outcome,
                };
                let data = root.0.clone();
                children.push(scope.spawn(move || {
                    let mut server = Fixture::new(replies());
                    let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        UploadEffect {
                            tg: &tg,
                            media: &media,
                            data: &data,
                            runtime: &runtime,
                        }
                        .same(
                            &json!({"file_id":"fixture-file"}),
                            Path::new("unused.png"),
                            "static",
                        )
                    }));
                    assert_eq!(server.finish().unwrap().len(), 2);
                    match outcome {
                        "panic" => assert!(result.is_err()),
                        "error" => {
                            assert_eq!(result.unwrap().unwrap_err(), "controlled codec refusal")
                        }
                        "cleanup" => {
                            assert!(result.unwrap().unwrap_err().contains("cleanup failed"))
                        }
                        "pending" => {
                            assert!(result.unwrap().unwrap_err().contains("custody unresolved"))
                        }
                        _ => assert_eq!(result.unwrap().unwrap(), Some(true)),
                    }
                    runtime.shutdown_timeout(Duration::from_secs(1));
                }));
            }
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let paths = (0..2)
                .map(|_| {
                    receiver
                        .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                        .unwrap()
                })
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(paths.len(), 2, "simultaneous downloads shared a path");
            for path in &paths {
                assert!(path.is_file());
            }
            assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 3);
            for release in releases {
                release.send(()).unwrap();
            }
            for child in children {
                child.join().unwrap();
            }
            let mut retained = 0;
            for path in paths {
                let workspace = path.parent().unwrap();
                if path.is_file() {
                    assert!(
                        outcomes.contains(&"pending"),
                        "owned download survived its call"
                    );
                    assert_eq!(std::fs::read(&path).unwrap(), b"synthetic downloaded bytes");
                    let codec = workspace.join("codec");
                    let marker = codec.join("codec-cleanup-pending.json");
                    assert_eq!(
                        serde_json::from_slice::<Value>(&std::fs::read(&marker).unwrap()).unwrap(),
                        json!({"synthetic":true})
                    );
                    assert_eq!(std::fs::read_dir(&codec).unwrap().count(), 1);
                    std::fs::remove_file(marker).unwrap();
                    std::fs::remove_dir(codec).unwrap();
                    std::fs::remove_file(&path).unwrap();
                    std::fs::remove_dir(workspace).unwrap();
                    retained += 1;
                } else if path.is_dir() {
                    assert!(outcomes.contains(&"cleanup"));
                    assert!(std::fs::read_dir(&path).unwrap().next().is_none());
                    std::fs::remove_dir(&path).unwrap();
                    std::fs::remove_dir(workspace).unwrap();
                    retained += 1;
                } else {
                    assert!(!path.exists(), "owned download survived its call");
                    assert!(
                        !workspace.exists(),
                        "owned download workspace survived its call"
                    );
                }
            }
            assert_eq!(
                retained,
                usize::from(outcomes.contains(&"cleanup") || outcomes.contains(&"pending"))
            );
        });
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"caller bytes");
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
    }
    root.clean().unwrap();
}
