//! Original fidelity refusals/recovery at public decoder and catalog boundaries.
use _native::{catalog::Catalog, codec_video::VideoDecoder};
use pyo3::prelude::*;
use serde_json::json;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::Path;

pub const CASES: [&str; 6] = [
    "fidelity_missing_vp9",
    "fidelity_missing_vp8",
    "fidelity_probe_recovery",
    "fidelity_no_codec",
    "fidelity_capability_recovery",
    "fidelity_api_recovery",
];

fn command_failure(directory: &Path, stage: &str, index: usize, tool: &str) -> String {
    let args = std::fs::read_to_string(directory.join(format!("{stage}-{index}.args"))).unwrap();
    let executable = _native::codec_process::executable(tool).unwrap();
    Python::initialize();
    Python::attach(|py| {
        let command = pyo3::types::PyList::empty(py);
        command.append(executable.as_os_str()).unwrap();
        for arg in args.lines() {
            command.append(arg).unwrap();
        }
        let exception = py
            .import("subprocess")
            .unwrap()
            .getattr("CalledProcessError")
            .unwrap()
            .call1((1, command))
            .unwrap();
        exception.str().unwrap().to_string()
    })
}

struct StatState {
    kind: &'static str,
    phase: &'static str,
    probe: usize,
    directory: std::path::PathBuf,
    calls: std::sync::atomic::AtomicUsize,
    sizes: std::sync::atomic::AtomicUsize,
}
impl StatState {
    fn failure(&self) -> PyErr {
        match self.kind {
            "OSError" => pyo3::exceptions::PyOSError::new_err("controlled video stat"),
            "TypeError" => pyo3::exceptions::PyTypeError::new_err("controlled video stat"),
            "KeyboardInterrupt" => {
                pyo3::exceptions::PyKeyboardInterrupt::new_err("controlled video stat")
            }
            _ => pyo3::exceptions::PySystemExit::new_err("controlled video stat"),
        }
    }
}
#[pyclass]
struct StatValue(std::sync::Arc<StatState>);
#[pymethods]
impl StatValue {
    #[getter]
    fn st_size(&self) -> PyResult<u64> {
        self.0
            .sizes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(self.0.failure())
    }
}
#[pyclass]
struct StatFault(std::sync::Arc<StatState>);
#[pymethods]
impl StatFault {
    #[pyo3(signature=(*_args, **_kwargs))]
    fn __call__(
        &self,
        py: Python<'_>,
        _args: &Bound<'_, pyo3::types::PyTuple>,
        _kwargs: Option<&Bound<'_, pyo3::types::PyDict>>,
    ) -> PyResult<Py<StatValue>> {
        assert!(
            self.0
                .directory
                .join(format!("probe-{}.pid", self.0.probe))
                .is_file(),
            "stat must follow the observed probe"
        );
        self.0
            .calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.0.phase == "stat" {
            return Err(self.0.failure());
        }
        Py::new(py, StatValue(self.0.clone()))
    }
}
struct RestoreStat(Py<PyAny>, Py<PyAny>);
impl Drop for RestoreStat {
    fn drop(&mut self) {
        Python::attach(|py| {
            self.0
                .bind(py)
                .setattr("stat", self.1.bind(py))
                .expect("restore external video stat")
        });
    }
}
fn patch_stat(
    directory: &Path,
    probe: usize,
    kind: &'static str,
    phase: &'static str,
) -> (RestoreStat, std::sync::Arc<StatState>) {
    Python::initialize();
    Python::attach(|py| {
        py.import("json").unwrap();
        let class = py.import("pathlib").unwrap().getattr("Path").unwrap();
        let original = class.getattr("stat").unwrap().unbind();
        let state = std::sync::Arc::new(StatState {
            kind,
            phase,
            probe,
            directory: directory.into(),
            calls: std::sync::atomic::AtomicUsize::new(0),
            sizes: std::sync::atomic::AtomicUsize::new(0),
        });
        class
            .setattr("stat", Py::new(py, StatFault(state.clone())).unwrap())
            .unwrap();
        (RestoreStat(class.unbind(), original), state)
    })
}

pub fn check(directory: &Path, case: &str, runtime: &tokio::runtime::Runtime) {
    assert!(CASES.contains(&case));
    let opaque = directory.join("clip.webm");
    let translucent = directory.join("translucent.webm");
    let before = std::fs::read(&opaque).unwrap();
    let faint = b"\x1a\x45\xdf\xa3 undecoded translucent fixture";
    std::fs::write(&translucent, faint).unwrap();
    let decoder = VideoDecoder::new(directory).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    runtime.block_on(async {
        match case {
            "fidelity_missing_vp9" | "fidelity_missing_vp8" => {
                let mut keys = Vec::new();
                for path in [&opaque, &translucent] {
                    let result = decoder.fingerprint_typed(path).await;
                    let error = result.as_ref().unwrap_err();
                    let (codec, name) = if case == "fidelity_missing_vp9" {
                        ("vp9", "libvpx-vp9")
                    } else {
                        ("vp8", "libvpx")
                    };
                    assert_eq!(error.kind, "UndecodableVideo");
                    assert!(error.handled);
                    assert_eq!(error.message, format!(
                        "{}: {codec} keeps alpha in a separate layer and this ffmpeg has no {name} decoder, so a decode here cannot be trusted to carry transparency",
                        path.file_name().unwrap().to_str().unwrap()
                    ));
                    keys.push(result.as_ref().ok().cloned());
                    let reply = _native::codec_error::CodecError::worker_reply(result).unwrap();
                    assert_eq!(reply.exit, 1);
                    assert_eq!(reply.envelope["ok"], false);
                }
                assert_eq!(keys, [None, None], "a refusal must not invent a byte hash");
                let db = directory.join("r01.db");
                // Match the original pre-insert boundary: no key means no Catalog is even opened.
                let result: Result<(), String> = async {
                    let key = decoder.content_key(&opaque).await?;
                    let mut catalog = Catalog::open(&db, "fixture UTC", directory)?;
                    catalog.insert(
                        &json!({"content_key":key,"fmt":"video","file_path":opaque}),
                        "fixture UTC",
                    )?;
                    Ok(())
                }
                .await;
                assert!(
                    result
                        .unwrap_err()
                        .contains("alpha fidelity cannot be established")
                );
                assert!(!db.exists());
                assert!(!directory.join("r01.db-wal").exists());
                assert!(!directory.join("r01.db-shm").exists());
            }
            "fidelity_no_codec" => {
                let error = decoder.fingerprint_typed(&opaque).await.unwrap_err();
                assert_eq!(error.kind, "UndecodableVideo");
                assert_eq!(error.message, "clip.webm: the container named no video codec, so alpha fidelity cannot be established");
                assert!(error.handled);
            }
            "fidelity_probe_recovery" | "fidelity_capability_recovery" => {
                // Reuse this exact instance/file/tool: a fresh decoder would hide a poisoned cache.
                if case == "fidelity_probe_recovery" {
                    let (restore, sentinel) = patch_stat(directory, 1, "OSError", "stat");
                    let error = tokio::time::timeout(deadline.saturating_duration_since(std::time::Instant::now()),
                        _native::codec_video_validate::validate_typed(&decoder, &opaque)).await.unwrap().unwrap_err();
                    assert_eq!(sentinel.calls.load(std::sync::atomic::Ordering::SeqCst), 0, "failed probe must not reach stat");
                    drop(restore);
                    assert_eq!(error.kind, "CalledProcessError");
                    assert_eq!(error.message, command_failure(directory, "probe", 1, "ffprobe"));
                    assert!(!error.handled);
                    assert!(_native::codec_error::CodecError::worker_reply(Err(error)).is_err());
                    for (index, (kind, phase)) in [("OSError", "stat"), ("TypeError", "size"),
                        ("KeyboardInterrupt", "stat"), ("SystemExit", "size")].into_iter().enumerate() {
                        let (restore, state) = patch_stat(directory, index + 2, kind, phase);
                        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                        assert!(!remaining.is_zero());
                        let result = if kind == "SystemExit" {
                            let root = directory.to_path_buf();
                            let source = opaque.clone();
                            let mut task = tokio::task::spawn_blocking(move || _native::codec_video_validate::usable_at(Some(&root), &source));
                            match tokio::time::timeout(remaining, &mut task).await {
                                Ok(result) => result.unwrap().map(|_| ()),
                                Err(error) => {
                                    // A running blocking task cannot be aborted; settle it before restoring stat.
                                    let _ = task.await;
                                    panic!("video size cohort deadline exceeded: {error}");
                                }
                            }
                        } else {
                            tokio::time::timeout(remaining, _native::codec_video_validate::validate_typed(&decoder, &opaque)).await.unwrap()
                        };
                        drop(restore);
                        let error = result.unwrap_err();
                        assert_eq!(error.kind, kind);
                        assert_eq!(error.message, "controlled video stat");
                        assert_eq!(error.handled, kind == "OSError");
                        assert_eq!(state.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
                        assert_eq!(state.sizes.load(std::sync::atomic::Ordering::SeqCst), usize::from(phase == "size"));
                        assert!(!directory.join("capability-1.pid").exists());
                        assert!(!directory.join("decode-1.pid").exists(), "size failure must stop before blank decoding");
                        Python::attach(|py| {
                            let size = py.import("pathlib").unwrap().getattr("Path").unwrap().call1((opaque.clone(),)).unwrap()
                                .call_method0("stat").unwrap().getattr("st_size").unwrap().extract::<u64>().unwrap();
                            assert_eq!(size, before.len() as u64, "stat patch must be restored");
                        });
                    }
                } else {
                    let error = decoder.fingerprint_typed(&opaque).await.unwrap_err();
                    assert_eq!(error.kind, "UndecodableVideo");
                    assert!(error.handled);
                    assert_eq!(error.message, format!(
                        "could not ask ffmpeg which decoders it has, so alpha fidelity cannot be established: {}",
                        command_failure(directory, "capability", 1, "ffmpeg")
                    ));
                }
                for _ in 0..2 {
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    assert!(!remaining.is_zero());
                    assert_eq!(
                        tokio::time::timeout(remaining, decoder.decoder_args(&opaque)).await.unwrap().unwrap(),
                        ["-c:v", "libvpx-vp9"]
                    );
                }
            }
            "fidelity_api_recovery" => {
                let error = decoder.fingerprint_typed(&opaque).await.unwrap_err();
                assert_eq!(error.kind, "UndecodableVideo");
                assert!(error.handled);
                assert_eq!(error.message, format!(
                    "could not ask ffmpeg which decoders it has, so alpha fidelity cannot be established: {}",
                    command_failure(directory, "capability", 1, "ffmpeg")
                ));
                assert_eq!(_native::codec_error::CodecError::worker_reply(Err(error)).unwrap().exit, 1);
                let fingerprint = decoder.fingerprint_typed(&opaque).await.unwrap();
                // Same independently fixed RGBA sample/key as existing codec_success, not a target golden.
                assert_eq!(
                    fingerprint["content_key"],
                    "v:769f9a9a414a35bdb6e3ee62419e9c1e"
                );
                assert_eq!(fingerprint["phash"], 0);
                assert_eq!(
                    decoder.content_key(&opaque).await.unwrap(),
                    fingerprint["content_key"]
                );
                assert_eq!(
                    json!(decoder.perceptual_hash(&opaque).await.unwrap()),
                    fingerprint["phash"]
                );
                let tool = _native::codec_process::executable("ffmpeg").unwrap();
                let expected_tools = directory.parent().unwrap().parent().unwrap().join("tools");
                assert_eq!(tool.parent(), Some(expected_tools.as_path()));
                let tool_bytes = std::fs::read(&tool).unwrap();
                let previous = tool.metadata().unwrap().modified().unwrap();
                let changed = std::time::SystemTime::UNIX_EPOCH
                    + std::time::Duration::from_secs(1_700_000_000);
                assert_ne!(previous, changed);
                {
                    let file = std::fs::File::options().write(true).open(&tool).unwrap();
                    file.set_modified(changed).unwrap();
                }
                assert_eq!(tool.metadata().unwrap().modified().unwrap(), changed);
                let result = _native::codec_video_validate::is_blank_typed(&decoder, &opaque).await;
                {
                    let file = std::fs::File::options().write(true).open(&tool).unwrap();
                    file.set_modified(previous).unwrap();
                }
                assert_eq!(tool.metadata().unwrap().modified().unwrap(), previous);
                assert_eq!(std::fs::read(&tool).unwrap(), tool_bytes);
                let error = result.unwrap_err();
                assert_eq!(error.kind, "UndecodableVideo");
                assert_eq!(error.message, format!(
                    "clip.webm: the codec could not be read, so the decoder that preserves its alpha cannot be chosen: {}",
                    command_failure(directory, "probe", 3, "ffprobe")
                ));
                assert!(error.handled);
            }
            _ => unreachable!(),
        }
    });
    assert_eq!(std::fs::read(&opaque).unwrap(), before);
    assert_eq!(std::fs::read(&translucent).unwrap(), faint);
    let (probes, capabilities, decodes) = match case {
        "fidelity_missing_vp9" | "fidelity_missing_vp8" => (3, 1, 0),
        "fidelity_probe_recovery" => (6, 1, 0),
        "fidelity_no_codec" => (1, 0, 0),
        "fidelity_capability_recovery" => (2, 2, 0),
        "fidelity_api_recovery" => (3, 2, 1),
        _ => unreachable!(),
    };
    let mut expected = ["clip.webm", "translucent.webm", "logs"]
        .map(OsString::from)
        .into_iter()
        .collect::<BTreeSet<_>>();
    for (stage, count) in [
        ("probe", probes),
        ("capability", capabilities),
        ("decode", decodes),
    ] {
        for index in 1..=count {
            let prefix = format!("{stage}-{index}");
            let pid = std::fs::read_to_string(directory.join(format!("{prefix}.pid")))
                .unwrap()
                .parse()
                .unwrap();
            super::process_liveness::assert_stopped(pid);
            for suffix in ["pid", "args"] {
                expected.insert(format!("{prefix}.{suffix}").into());
            }
            let args = std::fs::read_to_string(directory.join(format!("{prefix}.args"))).unwrap();
            let args = args.lines().collect::<Vec<_>>();
            match stage {
                "probe" => {
                    assert_eq!(
                        &args[..6],
                        [
                            "-v",
                            "error",
                            "-show_entries",
                            "stream=index,codec_type,codec_name,width,height,avg_frame_rate:format=duration,format_name",
                            "-of",
                            "json"
                        ]
                    );
                    let path = if case.starts_with("fidelity_missing_") && index == 2 {
                        &translucent
                    } else {
                        &opaque
                    };
                    assert_eq!(args[6], path.to_str().unwrap());
                    assert_eq!(args.len(), 7);
                }
                "capability" => assert_eq!(args, ["-v", "error", "-decoders"]),
                "decode" => {
                    let input = args.iter().position(|arg| *arg == "-i").unwrap();
                    let codec = args
                        .windows(2)
                        .position(|pair| pair == ["-c:v", "libvpx-vp9"])
                        .unwrap();
                    assert!(codec < input);
                    assert_eq!(args[input + 1], opaque.to_str().unwrap());
                    assert!(
                        args.windows(2)
                            .any(|pair| pair == ["-vf", "fps=10,scale=64:64,format=rgba"])
                    );
                }
                _ => unreachable!(),
            }
        }
    }
    assert_eq!(
        std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<BTreeSet<_>>(),
        expected
    );
    assert_eq!(
        std::fs::read_dir(directory.join("logs/tmp"))
            .unwrap()
            .count(),
        0
    );
    assert!(!_native::process_owner::cleanup_pending(directory));
    std::fs::write(directory.join("checked"), b"ok\n").unwrap();
}
