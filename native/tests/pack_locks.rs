//! Original OS-lock contracts; records never grant ownership and crash release bypasses Rust Drop.
use _native::{locks::Lease, process_owner::OwnedProcess, state_bindings::NativeStateLease};
use pyo3::exceptions::{PyOSError, PyRuntimeError};
use pyo3::prelude::*;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

const FIXTURE: &str = "lock_process_fixture";
const CONTROL: &str = "NUMERA_LOCK_FIXTURE";
const SLOT: &str = "NUMERA_LOCK_SLOT";
const ROLE: &str = "NUMERA_LOCK_ROLE";
const READY: Duration = Duration::from_secs(5);

fn source_identity() {
    let inventory: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-baseline.json"
    ))
    .unwrap();
    assert_eq!(
        inventory["baseline_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    for (path, hash) in [
        (
            "tests/test_pack_locks.py",
            "91b84ceda3c046d3c084454f327028abf4be1f65c943a5965c096e98674f2c7d",
        ),
        (
            "tests/test_pack_locks_exclusion.py",
            "625c4413e22c0d4e2388ef9a13df9eadc8fff2ae746b0269586254397bf18a3e",
        ),
    ] {
        let records = inventory["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|record| record["source"] == path)
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["sha256"], hash);
    }
}

fn old_record(path: &Path, age: u64) {
    let old = std::time::SystemTime::now() - Duration::from_secs(age);
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(old))
        .unwrap();
}

fn assert_busy(result: std::io::Result<Lease>) {
    let error = result
        .err()
        .expect("a second handle acquired the held lock");
    assert!(error.to_string().starts_with("OS lock acquisition failed:"));
    #[cfg(windows)]
    {
        let expected = std::io::Error::from_raw_os_error(
            windows_sys::Win32::Foundation::ERROR_LOCK_VIOLATION as i32,
        );
        assert_eq!(
            error.to_string(),
            format!("OS lock acquisition failed: {expected}")
        );
    }
    #[cfg(not(windows))]
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
}

fn stable_identity(path: &Path) -> (String, String) {
    Python::initialize();
    Python::attach(|py| {
        let metadata = py
            .import("os")
            .unwrap()
            .call_method1("stat", (path,))
            .unwrap();
        let field = |name| {
            metadata
                .getattr(name)
                .unwrap()
                .str()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned()
        };
        (field("st_dev"), field("st_ino"))
    })
}

#[test]
fn original_records_age_unwind_and_stable_file_never_replace_os_exclusion() {
    source_identity();
    let root = Root::new("pack lock records");
    let path = root.0.join("state.json.lock");
    for (record, age) in [
        ("pid=999 (crashed)", 10_000),
        (r#"{"token":"t","pid":999999999,"started":"old"}"#, 10_000),
        (r#"{"token":"t","pid":999999999,"started":"old"}"#, 300),
        ("", 0),
    ] {
        std::fs::write(&path, record).unwrap();
        old_record(&path, age);
        drop(Lease::acquire(&path, "unowned leftover").unwrap());
    }
    let first = Lease::acquire(&path, "first fixture UTC").unwrap();
    let identity = stable_identity(&path);
    for record in [
        br#"{"token":"someone-else","pid":1,"started":"now"}"#.as_slice(),
        b"".as_slice(),
        b"not json at all".as_slice(),
    ] {
        std::fs::write(&path, record).unwrap();
        old_record(&path, 10 * 24 * 3600);
        assert_busy(Lease::acquire(&path, "must refuse"));
        assert_eq!(std::fs::read(&path).unwrap(), record);
        assert_eq!(stable_identity(&path), identity);
    }
    let replacement = br#"{"token":"other","pid":4242,"started":"now"}"#;
    std::fs::write(&path, replacement).unwrap();
    drop(first);
    assert_eq!(std::fs::read(&path).unwrap(), replacement);
    assert!(
        path.is_file(),
        "release must not unlink the stable lock target"
    );
    let second = Lease::acquire(&path, "reacquired").unwrap();
    assert_eq!(stable_identity(&path), identity);
    drop(second);
    let unwind = std::panic::catch_unwind(|| {
        let _held = Lease::acquire(&path, "unwind").unwrap();
        panic!("original lock fixture failure");
    })
    .unwrap_err();
    assert_eq!(
        unwind.downcast_ref::<&str>(),
        Some(&"original lock fixture failure")
    );
    drop(Lease::acquire(&path, "after unwind").unwrap());
    assert_eq!(stable_identity(&path), identity);
    root.clean().unwrap();
}

#[test]
fn original_family_coordinates_are_normalized_and_distinct_from_the_map_lock() {
    source_identity();
    let root = Root::new("pack lock coordinates");
    let family = _native::locks::pack_family_path(&root.0, "cryptoemoji");
    assert_eq!(
        family,
        _native::locks::pack_family_path(&root.0, "cryptoemoji")
    );
    assert_ne!(
        _native::locks::pack_family_path(&root.0, "one"),
        _native::locks::pack_family_path(&root.0, "two")
    );
    let unsafe_base = _native::locks::pack_family_path(&root.0, "../../etc/passwd");
    assert_eq!(unsafe_base.parent().unwrap(), root.0.join(".locks"));
    assert!(
        !unsafe_base
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .contains("..")
    );
    let map = root.0.join(".locks/canonical_map.lock");
    let _family = Lease::acquire(&family, "family").unwrap();
    let map_lease = Lease::acquire(&map, "map").unwrap();
    assert_busy(Lease::acquire(&map, "second map writer"));
    drop(map_lease);
    drop(_family);
    assert!(family.is_file() && map.is_file());
    root.clean().unwrap();
}

fn wait_file(path: &Path) {
    let deadline = Instant::now() + READY;
    loop {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => {
                assert!(metadata.is_file() && !metadata.file_type().is_symlink());
                return;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("cannot inspect fixture barrier: {error}"),
        }
        assert!(
            Instant::now() < deadline,
            "lock fixture barrier exceeded5s: {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn lock_process_fixture() {
    let args = std::env::args().collect::<Vec<_>>();
    if !args.windows(2).any(|pair| pair == ["--exact", FIXTURE]) {
        return;
    }
    let control = PathBuf::from(std::env::var_os(CONTROL).expect("explicit fixture control"));
    let control = _native::tooling_environment::physical_root(&control).unwrap();
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let scratch =
        _native::tooling_environment::confined_directory(project, "logs/tmp", false).unwrap();
    assert!(control.starts_with(&scratch) && control != scratch);
    let slot = std::env::var(SLOT).unwrap().parse::<usize>().unwrap();
    assert!(slot < 4);
    let role = std::env::var(ROLE).unwrap();
    assert!(matches!(role.as_str(), "race" | "crash"));
    let child = control.join(format!("child-{slot}"));
    _native::atomic::write_json(
        &child.join("ready.json"),
        &json!({"pid":std::process::id()}),
    )
    .unwrap();
    wait_file(&control.join("start"));
    match Lease::acquire(&control.join("family.lock"), "child fixture UTC") {
        Ok(lease) => {
            _native::atomic::write_json(
                &child.join("result.json"),
                &json!({"acquired":true,"pid":std::process::id()}),
            )
            .unwrap();
            if role == "crash" {
                std::process::exit(9);
            }
            wait_file(&control.join("release"));
            drop(lease);
        }
        Err(error) => {
            assert_busy(Err(error));
            assert_eq!(role, "race", "crash fixture must acquire the free lock");
            _native::atomic::write_json(
                &child.join("result.json"),
                &json!({"acquired":false,"pid":std::process::id()}),
            )
            .unwrap();
        }
    }
}

fn spawn(root: &Root, control: &Path, slot: usize, role: &str) -> OwnedProcess {
    let child = control.join(format!("child-{slot}"));
    std::fs::create_dir(&child).unwrap();
    let storage = root.0.join("logs/tmp/tooling");
    std::fs::create_dir_all(&storage).unwrap();
    let owner = storage.join(format!(
        "{role}-{}-{slot}",
        control.file_name().unwrap().to_str().unwrap()
    ));
    std::fs::create_dir(&owner).unwrap();
    let inherited = std::env::vars().collect();
    let environment = _native::tooling_test_runner::scrubbed_environment(&inherited);
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", FIXTURE, "--nocapture", "--test-threads=1"])
        .current_dir(&root.0)
        .env_clear()
        .envs(environment)
        .env(CONTROL, control)
        .env(SLOT, slot.to_string())
        .env(ROLE, role)
        .env("TEMP", control)
        .env("TMP", control)
        .env("TMPDIR", control)
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(child.join("stdout.log")).unwrap())
        .stderr(std::fs::File::create(child.join("stderr.log")).unwrap());
    OwnedProcess::spawn(&mut command, owner).unwrap()
}

fn record(control: &Path, slot: usize) -> Value {
    let path = control.join(format!("child-{slot}/result.json"));
    wait_file(&path);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn binding_refuses(path: &Path, holder: Option<u64>) {
    Python::initialize();
    Python::attach(|py| {
        let error = py
            .get_type::<NativeStateLease>()
            .call1((path.to_str().unwrap(), "competing fixture UTC"))
            .unwrap_err();
        assert!(error.is_instance_of::<PyRuntimeError>(py));
        assert!(!error.is_instance_of::<PyOSError>(py));
        let message = error.value(py).str().unwrap().to_str().unwrap().to_owned();
        assert!(message.contains("Refusing to mutate"));
        if let Some(pid) = holder {
            assert!(
                message.contains(&format!("record names pid {pid} ")),
                "{message}"
            );
        }
    });
}

#[test]
fn original_two_and_four_process_contention_cannot_overlap_and_crash_releases() {
    source_identity();
    let root = Root::new("pack lock process barriers");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for (count, stale) in [(2, true), (4, false)] {
        let control = root.0.join(format!("round-{count}"));
        std::fs::create_dir(&control).unwrap();
        let path = control.join("family.lock");
        if stale {
            std::fs::write(
                &path,
                br#"{"pid":999999999,"token":"gone","started":"old"}"#,
            )
            .unwrap();
            old_record(&path, 10 * 24 * 3600);
        }
        let mut owners = (0..count)
            .map(|slot| spawn(&root, &control, slot, "race"))
            .collect::<Vec<_>>();
        for slot in 0..count {
            wait_file(&control.join(format!("child-{slot}/ready.json")));
        }
        std::fs::write(control.join("start"), b"start").unwrap();
        let outcomes = (0..count)
            .map(|slot| record(&control, slot))
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| outcome["acquired"] == true)
                .count(),
            1
        );
        assert!(
            outcomes
                .iter()
                .all(|outcome| outcome["acquired"].is_boolean())
        );
        // Every contender attempted acquisition while the winner still holds the OS lock.
        let winner = outcomes
            .iter()
            .find(|outcome| outcome["acquired"] == true)
            .unwrap();
        let pid = winner["pid"].as_u64().unwrap();
        assert_ne!(pid, u64::from(std::process::id()));
        let ready = control.join(format!(
            "child-{}/ready.json",
            outcomes
                .iter()
                .position(|outcome| outcome["acquired"] == true)
                .unwrap()
        ));
        let ready: Value = serde_json::from_slice(&std::fs::read(ready).unwrap()).unwrap();
        assert_eq!(ready["pid"], pid);
        binding_refuses(&path, Some(pid));
        old_record(&path, 10 * 24 * 3600);
        assert_busy(Lease::acquire(&path, "ancient live holder must refuse"));
        for payload in [b"not json at all".as_slice(), b"".as_slice()] {
            std::fs::write(&path, payload).unwrap();
            binding_refuses(&path, None);
            assert_busy(Lease::acquire(&path, "parent must refuse"));
            assert_eq!(std::fs::read(&path).unwrap(), payload);
        }
        std::fs::write(control.join("release"), b"release").unwrap();
        for owner in &mut owners {
            let status = runtime
                .block_on(owner.wait(Duration::from_secs(8), Duration::from_secs(4)))
                .unwrap();
            owner.finish().unwrap();
            assert_eq!(status.code(), Some(0));
        }
        drop(Lease::acquire(&path, "after release").unwrap());
        assert!(path.is_file());
    }
    let control = root.0.join("crash");
    std::fs::create_dir(&control).unwrap();
    let mut owner = spawn(&root, &control, 0, "crash");
    wait_file(&control.join("child-0/ready.json"));
    std::fs::write(control.join("start"), b"start").unwrap();
    assert_eq!(record(&control, 0)["acquired"], true);
    let status = runtime
        .block_on(owner.wait(Duration::from_secs(8), Duration::from_secs(4)))
        .unwrap();
    owner.finish().unwrap();
    assert_eq!(
        status.code(),
        Some(9),
        "fixture must bypass all Rust destructors"
    );
    let path = control.join("family.lock");
    assert!(path.is_file());
    let deadline = Instant::now() + READY;
    loop {
        match Lease::acquire(&path, "after crash") {
            Ok(lease) => {
                drop(lease);
                break;
            }
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "OS did not release crashed lock: {error}"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    assert_eq!(
        std::fs::read_dir(root.0.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    root.clean().unwrap();
}
