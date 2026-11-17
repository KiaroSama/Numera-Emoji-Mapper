//! Active capture contracts retained from native/src/tooling_process_retained_tests.rs.
//! Caller-selected retained capture paths are gone; pin actual capture files read-only
//! while the child is acknowledged, without adding a production retained-output API.
use crate::tooling_fixture::Root;
use _native::{logging::RunLog, tooling_process};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const LIMIT: usize = 8 * 1024 * 1024;
const FIXTURE: &str = "capture_contracts::retained_fixture";
const CONTRACT: &str = "capture_contracts::retained_contract_fixture";

fn argv(case: &str) -> Vec<OsString> {
    vec![
        std::env::current_exe().unwrap().into_os_string(),
        "--exact".into(),
        case.into(),
        "--nocapture".into(),
        "--test-threads=1".into(),
    ]
}
fn selected(case: &str) -> bool {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .any(|a| a == ["--exact", case])
}
fn control() -> PathBuf {
    std::env::var_os("NUMERA_RETAINED_CONTROL").unwrap().into()
}

#[test]
fn retained_fixture() {
    if !selected(FIXTURE) || std::env::var("NUMERA_RETAINED_FIXTURE").as_deref() != Ok("1") {
        return;
    }
    std::fs::write(control().join("ready"), b"ready").unwrap();
    for (stdout, bytes) in [
        (true, "stdout café\n"),
        (false, "stderr separate\n"),
        (true, "stdout done\n"),
    ] {
        if stdout {
            std::io::stdout().write_all(bytes.as_bytes()).unwrap();
        } else {
            std::io::stderr().write_all(bytes.as_bytes()).unwrap();
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    std::process::exit(7);
}

#[test]
fn retained_contract_fixture() {
    if !selected(CONTRACT) {
        return;
    }
    let Ok(mode) = std::env::var("NUMERA_RETAINED_CASE") else {
        return;
    };
    assert!(matches!(
        mode.as_str(),
        "overflow-stdout" | "overflow-stderr" | "invalid-stdout" | "invalid-stderr"
    ));
    let directory = control();
    std::io::stdout().flush().unwrap();
    std::io::stderr().flush().unwrap();
    std::fs::write(directory.join("ready"), b"ready").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let prefix = loop {
        if let Ok(raw) = std::fs::read_to_string(directory.join("prefix"))
            && let Ok(prefix) = raw.parse::<usize>()
        {
            break prefix;
        }
        assert!(Instant::now() < deadline, "capture acknowledgement missing");
        std::thread::sleep(Duration::from_millis(10));
    };
    let bytes = if mode.starts_with("overflow-") {
        vec![b'x'; LIMIT + 1 - prefix]
    } else {
        b"ok\xff\n".to_vec()
    };
    if mode.ends_with("stdout") {
        let mut stream = std::io::stdout().lock();
        stream.write_all(&bytes).unwrap();
        stream.flush().unwrap();
    } else {
        let mut stream = std::io::stderr().lock();
        stream.write_all(&bytes).unwrap();
        stream.flush().unwrap();
    }
    if mode.starts_with("overflow-") {
        std::thread::sleep(Duration::from_secs(25));
        std::fs::write(directory.join("fallback"), b"overflow was not stopped").unwrap();
        std::process::exit(92);
    }
    std::process::exit(0);
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
async fn ready(path: &Path) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if std::fs::read(path).ok().as_deref() == Some(b"ready") {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("actual capture child readiness");
}
fn captures(root: &Path) -> [PathBuf; 2] {
    let paths = std::fs::read_dir(root.join("logs/tmp/tooling"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(paths.len(), 1, "one exclusive acknowledged command");
    [paths[0].join("stdout"), paths[0].join("stderr")]
}
fn bytes(file: &mut std::fs::File) -> Vec<u8> {
    file.rewind().unwrap();
    let mut raw = Vec::new();
    file.take((LIMIT + 2) as u64).read_to_end(&mut raw).unwrap();
    raw
}
fn empty(root: &Path) {
    assert_eq!(
        std::fs::read_dir(root.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn retained_capture_keeps_direct_bytes_status_and_failure_evidence() {
    // Keep the exact behavior declaration. The removed retained-file API is
    // replaced only by read-only fixture observation of real capture's own files.
    let root = Root::new("retained active capture");
    let directory = root.0.join("control");
    std::fs::create_dir(&directory).unwrap();
    let log = RunLog::open(&root.0, "retained-active-capture", vec![]);
    let rt = runtime();
    let mut environment = crate::fixture::test_environment();
    environment.insert("NUMERA_RETAINED_FIXTURE".into(), "1".into());
    environment.insert(
        "NUMERA_RETAINED_CONTROL".into(),
        directory.to_str().unwrap().into(),
    );
    let command = argv(FIXTURE);
    let (done, mut observed) = rt.block_on(async {
        let capture = tooling_process::capture(
            &root.0,
            &command,
            &environment,
            Duration::from_secs(5),
            Duration::from_secs(3),
            &log,
        );
        let inspect = async {
            ready(&directory.join("ready")).await;
            captures(&root.0).map(|p| std::fs::File::open(p).unwrap())
        };
        tokio::join!(capture, inspect)
    });
    let done = done.unwrap();
    assert_eq!(done.code, 7);
    assert!(done.stdout.contains("stdout café\n"));
    assert!(done.stdout.contains("stdout done\n"));
    assert_eq!(done.stderr, "stderr separate\n");
    assert_eq!(bytes(&mut observed[0]), done.stdout.as_bytes());
    assert_eq!(bytes(&mut observed[1]), done.stderr.as_bytes());
    drop(observed);
    empty(&root.0);

    // Same exclusive absent executable proves that conservative marker retention
    // is not mistaken for an actual surviving child, then releases only that fixture.
    let missing = [root.0.join("absent-executable").into_os_string()];
    assert!(
        rt.block_on(tooling_process::capture(
            &root.0,
            &missing,
            &environment,
            Duration::from_secs(5),
            Duration::from_secs(3),
            &log
        ))
        .is_err()
    );
    let owners = std::fs::read_dir(root.0.join("logs/tmp/tooling"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(owners.len(), 1);
    assert!(owners[0].join("owner/codec-cleanup-pending.json").is_file());
    assert!(root.clean().is_err());
    std::fs::remove_dir_all(&owners[0]).unwrap();

    capture_contracts(&root.0, &environment, &rt, &log);
    drop(log);
    root.clean().unwrap();
}

fn capture_contracts(
    root: &Path,
    environment: &BTreeMap<String, String>,
    runtime: &tokio::runtime::Runtime,
    log: &RunLog,
) {
    for mode in [
        "overflow-stdout",
        "overflow-stderr",
        "invalid-stdout",
        "invalid-stderr",
    ] {
        let directory = root.join(mode);
        std::fs::create_dir(&directory).unwrap();
        let mut environment = environment.clone();
        environment.insert("NUMERA_RETAINED_CASE".into(), mode.into());
        environment.insert(
            "NUMERA_RETAINED_CONTROL".into(),
            directory.to_str().unwrap().into(),
        );
        let command = argv(CONTRACT);
        let (result, (prefix, mut file)) = runtime.block_on(async {
            let capture = tooling_process::capture(
                root,
                &command,
                &environment,
                Duration::from_secs(8),
                Duration::from_secs(4),
                log,
            );
            let inspect = async {
                ready(&directory.join("ready")).await;
                let paths = captures(root);
                let path = &paths[usize::from(mode.ends_with("stderr"))];
                let file = std::fs::File::open(path).unwrap();
                let prefix = usize::try_from(file.metadata().unwrap().len()).unwrap();
                std::fs::write(directory.join("prefix"), prefix.to_string()).unwrap();
                (prefix, file)
            };
            tokio::join!(capture, inspect)
        });
        let error = result.err().expect("invalid capture must refuse");
        let raw = bytes(&mut file);
        if mode.starts_with("overflow-") {
            assert_eq!(error, "Tool child output exceeded8MiB capture budget");
            assert_eq!(raw.len(), LIMIT + 1);
            assert!(raw[prefix..].iter().all(|b| *b == b'x'));
        } else {
            assert_eq!(
                error,
                format!(
                    "Non-UTF-8 tool output at byte{} (exit Some(0)); cannot safely decode",
                    prefix + 2
                )
            );
            assert_eq!(&raw[prefix..], b"ok\xff\n");
        }
        assert!(!directory.join("fallback").exists());
        drop(file);
        empty(root);
    }
    // The3 old externally supplied retained-file progress scenarios disappear
    // with that API. Actual owner progress3 are retained separately by the
    // supplemental protected process_owner progress driver, not faked here.
}
