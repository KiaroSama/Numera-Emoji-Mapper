//! Supplementary actual conversion ownership regression, separate from the1516 IDs.
//! Actual protected-runner RED is required before an ownership policy change.
use super::*;
use crate::{media_adapter::MediaCodec, media_native_adapter::NativeMediaAdapter};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::sync::{Arc, Mutex};

use crate::supplemental_worker_fixture as worker_fixture;

const CASE: &str = "coin_workflows::tests::conversion_session_proof::cancellation_stops_the_actual_converter_and_its_separate_renderer_session";
const ID: &str = "outer-conversion-owner";

struct ControlledCodec(NativeMediaAdapter);
impl MediaCodec for ControlledCodec {
    fn with_scratch(&self, scratch: PathBuf) -> Self {
        Self(self.0.with_scratch(scratch))
    }
    async fn call(&self, mut request: serde_json::Value) -> Result<serde_json::Value> {
        request["fixture_mode"] = json!("watchdog-owner");
        request["fixture_id"] = json!(ID);
        let environment = std::env::vars_os().collect::<BTreeMap<OsString, OsString>>();
        self.0
            .call_with_environment(
                request,
                Duration::from_secs(30),
                Duration::from_secs(20),
                &environment,
            )
            .await
    }
}

fn converter_host() -> bool {
    let Some(worker) = std::env::var_os("NUMERA_CONVERSION_OWNER_FIXTURE") else {
        return false;
    };
    let root = std::env::current_dir().unwrap();
    std::fs::write(root.join("converter-ready"), std::process::id().to_string()).unwrap();
    let config = Config::load_with_environment(
        root.clone(),
        &BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]),
    )
    .unwrap();
    let log = RunLog::open(&root, "controlled-converter", vec![]);
    let result = crate::convert_cli::run_with_codec(
        &config,
        &[
            "--in".into(),
            root.join("input").to_str().unwrap().into(),
            "--out".into(),
            root.join("output").to_str().unwrap().into(),
        ],
        &log,
        |scratch| {
            ControlledCodec(NativeMediaAdapter {
                executable: worker.into(),
                root: root.clone(),
                scratch,
            })
        },
    );
    panic!("converter returned before the acknowledged renderer was cancelled: {result:?}");
}

#[derive(Clone, Debug)]
struct Identity {
    pid: u32,
    #[cfg(target_os = "linux")]
    session: u32,
    #[cfg(target_os = "linux")]
    birth: u64,
    #[cfg(windows)]
    handle: Arc<std::os::windows::io::OwnedHandle>,
}
impl Identity {
    fn live(pid: u32) -> Self {
        assert_process(pid, false);
        #[cfg(target_os = "linux")]
        {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
            let fields = stat
                .rsplit_once(") ")
                .unwrap()
                .1
                .split_whitespace()
                .collect::<Vec<_>>();
            Self {
                pid,
                session: fields[3].parse().unwrap(),
                birth: fields[19].parse().unwrap(),
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::{FromRawHandle, OwnedHandle};
            use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};
            // SAFETY: pin the acknowledged live fixture with a read-only synchronization
            // handle; ownership closes it exactly once, never sends a signal by PID.
            #[allow(unsafe_code)]
            let handle = unsafe {
                let raw = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
                assert!(!raw.is_null());
                OwnedHandle::from_raw_handle(raw)
            };
            Self {
                pid,
                handle: Arc::new(handle),
            }
        }
    }

    fn stopped(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            match std::fs::read_to_string(format!("/proc/{}/stat", self.pid)) {
                Ok(stat) => {
                    let fields = stat
                        .rsplit_once(") ")
                        .unwrap()
                        .1
                        .split_whitespace()
                        .collect::<Vec<_>>();
                    fields[19].parse::<u64>().unwrap() != self.birth || fields[0] == "Z"
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound
                        || error.raw_os_error() == Some(3) =>
                {
                    true
                }
                Err(error) => {
                    panic!("cannot observe acknowledged renderer identity {self:?}: {error}")
                }
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
            use windows_sys::Win32::System::Threading::WaitForSingleObject;
            // SAFETY: the pinned fixture handle is live for this read-only query.
            #[allow(unsafe_code)]
            let state = unsafe { WaitForSingleObject(self.handle.as_raw_handle(), 0) };
            assert!(
                state == WAIT_OBJECT_0 || state == WAIT_TIMEOUT,
                "cannot inspect acknowledged fixture PID {}: unexpected wait state {state}",
                self.pid
            );
            state == WAIT_OBJECT_0
        }
    }
}

// Safe cleanup even if readiness/admission assertions unwind. Release is a fixture-only
// file observed by this exact acknowledged renderer; it is not a PID-based kill fallback.
struct Release(PathBuf);
impl Drop for Release {
    fn drop(&mut self) {
        let _ = std::fs::write(self.0.join("release"), b"fixture cleanup only");
    }
}

#[test]
fn cancellation_stops_the_actual_converter_and_its_separate_renderer_session() {
    if converter_host() {
        return;
    }
    let worker = worker_fixture::worker();
    let root = Scratch::new();
    let control = root.0.join("logs/tmp").join(format!("media-fixture-{ID}"));
    std::fs::create_dir_all(&control).unwrap();
    let release = Release(control.clone());
    std::fs::create_dir(root.0.join("input")).unwrap();
    std::fs::write(root.0.join("input/art.svg"), b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\"><rect width=\"8\" height=\"8\" fill=\"blue\"/></svg>").unwrap();
    let mut environment = worker_fixture::test_environment();
    environment.insert(
        "NUMERA_CONVERSION_OWNER_FIXTURE".into(),
        worker.to_str().unwrap().into(),
    );
    let launch = tooling_launch::Launch {
        root: root.0.clone(),
        argv: vec![
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            CASE.into(),
            "--nocapture".into(),
        ],
        environment,
    };
    let observed = Arc::new(Mutex::new(None));
    let seen = observed.clone();
    let ready = control.join("ready");
    let host_ready = root.0.join("converter-ready");
    let mut cancel: Pin<Box<dyn Future<Output = Result<()>>>> = Box::pin(async move {
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                if let Ok(text) = std::fs::read_to_string(&ready) {
                    let ids = text.lines().filter_map(|s| s.parse::<u32>().ok()).collect::<Vec<_>>();
                    if ids.len() == 2 {
                        assert_eq!(ids[1], 0);
                        let host = Identity::live(std::fs::read_to_string(&host_ready).unwrap().parse().unwrap());
                        let renderer = Identity::live(ids[0]);
                        #[cfg(target_os = "linux")]
                        {
                            assert_eq!(host.session, host.pid);
                            assert_eq!(renderer.session, renderer.pid);
                            assert_ne!(host.session, renderer.session, "real codec worker must establish its actual separate session");
                        }
                        eprintln!("acknowledged before cancellation: converter={host:?}, renderer={renderer:?}");
                        *seen.lock().unwrap() = Some((host, renderer));
                        return;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.map_err(|_| "actual renderer never acknowledged readiness".to_string())?;
        Ok(())
    });
    let log = RunLog::open(&root.0, "actual-nested-conversion-cancel", vec![]);
    let result = runtime().block_on(owned(&launch, None, &mut cancel, &log));
    // Capture actual state BEFORE fixture release, and before a fail-safe exit could mask it.
    let identities = observed.lock().unwrap().clone();
    let stopped_before_release = identities.as_ref().map(|(_, renderer)| renderer.stopped());
    eprintln!(
        "post-owner cancellation, BEFORE release: identities={identities:?}, renderer_stopped={stopped_before_release:?}"
    );
    drop(release);
    if let Some((_, renderer)) = &identities {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !renderer.stopped() {
            assert!(
                std::time::Instant::now() < deadline,
                "bounded fixture release failed: {renderer:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    assert_eq!(result.unwrap(), 130);
    let (host, _) =
        identities.expect("actual converter/renderer readiness must precede cancellation");
    assert!(
        host.stopped(),
        "actual converter survived cancellation: {host:?}"
    );
    assert!(
        !control.join("expired").exists(),
        "fixture fail-safe cannot satisfy cancellation"
    );
    assert_eq!(
        stopped_before_release,
        Some(true),
        "actual separately-sessioned renderer survived owned cancellation"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn descendant_session_refuses_a_preexisting_child_without_touching_it() {
    const NAME: &str = "coin_workflows::tests::conversion_session_proof::descendant_session_refuses_a_preexisting_child_without_touching_it";
    const ROLE: &str = "NUMERA_COIN_FOREIGN_CHECK";
    let role = std::env::var(ROLE).unwrap_or_default();
    if role == "hold" {
        let root = std::env::current_dir().unwrap();
        std::fs::write(root.join("foreign-ready"), std::process::id().to_string()).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while !root.join("foreign-release").is_file() {
            assert!(
                std::time::Instant::now() < deadline,
                "foreign fixture release expired"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        return;
    }
    let root = Scratch::new();
    let mut environment = worker_fixture::test_environment();
    let command = vec![
        std::env::current_exe().unwrap().into_os_string(),
        "--exact".into(),
        NAME.into(),
        "--nocapture".into(),
    ];
    let log = RunLog::open(&root.0, "foreign-child-admission", vec![]);
    if role != "driver" {
        // A fresh exact-selected process isolates the process-global admission
        // precondition from all other libtest cases, without a new worker test.
        environment.insert(ROLE.into(), "driver".into());
        let result = runtime()
            .block_on(tooling_process::capture(
                &root.0,
                &command,
                &environment,
                Duration::from_secs(15),
                Duration::from_secs(10),
                &log,
            ))
            .unwrap();
        assert_eq!(result.code, 0, "{}{}", result.stdout, result.stderr);
        assert!(result.stdout.contains("1 passed"));
        return;
    }
    let launch = tooling_launch::Launch {
        root: root.0.clone(),
        argv: vec![
            std::env::current_exe().unwrap().into_os_string(),
            "--help".into(),
        ],
        environment: environment.clone(),
    };
    let mut cancel: Pin<Box<dyn Future<Output = Result<()>>>> = Box::pin(std::future::pending());
    // A real success before creating the foreign child proves admission is
    // connected and not already refusing due to an unrelated prior child.
    assert_eq!(
        runtime()
            .block_on(owned(
                &launch,
                Some(&root.0.join("help.txt")),
                &mut cancel,
                &log
            ))
            .unwrap(),
        0
    );
    environment.insert(ROLE.into(), "hold".into());
    struct Child(std::process::Child, PathBuf);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = std::fs::write(&self.1, b"owned fixture release");
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            loop {
                if matches!(self.0.try_wait(), Ok(Some(_))) {
                    return;
                }
                if std::time::Instant::now() >= deadline {
                    // This is the unreaped Child created above, never a fresh
                    // numeric-PID lookup. Failure-only fixture cleanup.
                    let _ = self.0.kill();
                    let _ = self.0.wait();
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
    let child = Command::new(&command[0])
        .args(&command[1..])
        .current_dir(&root.0)
        .env_clear()
        .envs(&environment)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let foreign = Child(child, root.0.join("foreign-release"));
    let expected = foreign.0.id().to_string();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::fs::read_to_string(root.0.join("foreign-ready"))
        .ok()
        .as_deref()
        != Some(expected.as_str())
    {
        assert!(
            std::time::Instant::now() < deadline,
            "foreign child did not acknowledge"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let identity = Identity::live(foreign.0.id());
    let absent = root.0.join("must-not-start");
    assert!(!absent.exists());
    let refused = tooling_launch::Launch {
        root: root.0.clone(),
        argv: vec![absent.into_os_string()],
        environment: environment.clone(),
    };
    let error = runtime()
        .block_on(owned(&refused, None, &mut cancel, &log))
        .unwrap_err();
    assert_eq!(
        error,
        "test descendant owner refuses preexisting child processes"
    );
    assert!(
        !identity.stopped(),
        "admission touched the preexisting child"
    );
    assert_eq!(
        std::fs::read_to_string(root.0.join("foreign-ready")).unwrap(),
        expected
    );
    let retained = std::fs::read_dir(root.0.join("logs/tmp/tooling"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(retained.len(), 1);
    assert!(
        retained[0]
            .join("owner/codec-cleanup-pending.json")
            .is_file()
    );
    drop(foreign);
    assert!(identity.stopped());
    // Known pre-spawn refusal: release only the synthetic recovery evidence.
    std::fs::remove_dir_all(&retained[0]).unwrap();
}
