use super::*;
mod fixture {
    use crate as _native;
    include!("../tests/support/tooling_fixture.rs");
}
#[test]
fn retained_paths_refuse_existing_same_foreign_and_linked_outputs_without_clobber() {
    let root = fixture::Root::new("retained capture paths");
    let foreign = fixture::Root::new("foreign capture paths");
    let directory = root.0.join("logs/tmp/capture");
    std::fs::create_dir_all(&directory).unwrap();
    let output = directory.join("stdout.log");
    let errors = directory.join("stderr.log");
    std::fs::write(&output, b"original").unwrap();
    assert!(retained_paths(&root.0, &output, &errors).is_err());
    assert_eq!(std::fs::read(&output).unwrap(), b"original");
    std::fs::remove_file(&output).unwrap();
    assert!(retained_paths(&root.0, &output, &output).is_err());
    assert!(retained_paths(&root.0, &output, &foreign.0.join("stderr.log")).is_err());
    assert!(retained_paths(&root.0, &directory, &errors).is_err());
    let protected = directory.join("private");
    std::fs::create_dir(&protected).unwrap();
    assert!(
        retained_paths(
            &root.0,
            &protected.join("out.log"),
            &protected.join("err.log")
        )
        .is_err()
    );
    #[cfg(unix)]
    {
        let link = directory.join("linked.log");
        std::os::unix::fs::symlink(&output, &link).unwrap();
        assert!(retained_paths(&root.0, &link, &errors).is_err());
        std::fs::remove_file(link).unwrap();
    }
    assert!(retained_paths(&root.0, &output, &errors).is_ok());
    assert!(!output.exists());
    assert!(!errors.exists());
    foreign.clean().unwrap();
    root.clean().unwrap();
}
#[test]
fn retained_fixture() {
    if std::env::var("NUMERA_RETAINED_FIXTURE").as_deref() != Ok("1") {
        return;
    }
    use std::io::Write;
    // Spaced writes exercise either stream's activity without a CPU-burning progress loop.
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
#[cfg(any(windows, target_os = "linux"))]
#[test]
fn retained_capture_keeps_direct_bytes_status_and_failure_evidence() {
    // A parallel libtest process cannot safely own Linux's process-global subreaper.
    if std::env::var("NUMERA_RETAINED_DRIVER").as_deref() != Ok("1") {
        dedicated_driver();
        return;
    }
    let root = fixture::Root::new("retained capture");
    let directory = root.0.join("logs/tmp/capture");
    std::fs::create_dir_all(&directory).unwrap();
    let output = directory.join("stdout.log");
    let errors = directory.join("stderr.log");
    let log = RunLog::open(&root.0, "retained-capture-test", vec![]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut environment =
        crate::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
    environment.insert("NUMERA_RETAINED_FIXTURE".into(), "1".into());
    let argv = [
        std::env::current_exe().unwrap().into_os_string(),
        "--exact".into(),
        "tooling_process::retained_tests::retained_fixture".into(),
        "--nocapture".into(),
        "--test-threads=1".into(),
    ];
    let done = runtime
        .block_on(capture_retained(
            &root.0,
            &argv,
            &environment,
            (&output, &errors),
            Duration::from_secs(5),
            Duration::from_secs(3),
            &log,
        ))
        .unwrap();
    assert_eq!(done.code, 7);
    assert!(done.stdout.contains("stdout café\n"));
    assert_eq!(done.stderr, "stderr separate\n");
    assert_eq!(std::fs::read(&output).unwrap(), done.stdout.as_bytes());
    assert_eq!(std::fs::read(&errors).unwrap(), done.stderr.as_bytes());
    assert!(
        runtime
            .block_on(capture_retained(
                &root.0,
                &argv,
                &environment,
                (&output, &errors),
                Duration::from_secs(5),
                Duration::from_secs(3),
                &log
            ))
            .is_err()
    );
    assert_eq!(std::fs::read(&errors).unwrap(), b"stderr separate\n");
    assert_eq!(
        std::fs::read_dir(root.0.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    let missing = [root.0.join("absent-executable").into_os_string()];
    let failed_out = directory.join("failed-out.log");
    let failed_err = directory.join("failed-err.log");
    assert!(
        runtime
            .block_on(capture_retained(
                &root.0,
                &missing,
                &environment,
                (&failed_out, &failed_err),
                Duration::from_secs(5),
                Duration::from_secs(3),
                &log
            ))
            .is_err()
    );
    assert!(failed_out.is_file());
    assert!(failed_err.is_file());
    let owners = std::fs::read_dir(root.0.join("logs/tmp/tooling"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(owners.len(), 1);
    assert!(owners[0].join("owner/codec-cleanup-pending.json").is_file());
    assert!(root.clean().is_err());
    // The exclusive absent executable could never start; only this synthetic recovery may be removed.
    std::fs::remove_dir_all(&owners[0]).unwrap();
    capture_contracts(&root.0, &environment, &runtime, &log);
    runtime.block_on(crate::process_owner::progress_tests::exercise(
        &root.0,
        &contract_command(),
        &environment,
    ));
    println!("retained capture/deadline contract cohort completed");
    drop(log);
    root.clean().unwrap();
}

#[cfg(any(windows, target_os = "linux"))]
fn contract_command() -> Vec<OsString> {
    vec![
        std::env::current_exe().unwrap().into_os_string(),
        "--exact".into(),
        "tooling_process::retained_tests::retained_contract_fixture".into(),
        "--nocapture".into(),
        "--test-threads=1".into(),
    ]
}

#[cfg(any(windows, target_os = "linux"))]
fn dedicated_driver() {
    let root = fixture::Root::new("retained dedicated driver");
    let directory = root.0.join("logs/tmp/capture");
    std::fs::create_dir_all(&directory).unwrap();
    let output = directory.join("stdout.log");
    let errors = directory.join("stderr.log");
    let mut environment =
        crate::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
    environment.insert("NUMERA_RETAINED_DRIVER".into(), "1".into());
    let command = vec![
        std::env::current_exe().unwrap().into_os_string(),
        "--exact".into(),
        "tooling_process::retained_tests::retained_capture_keeps_direct_bytes_status_and_failure_evidence".into(),
        "--nocapture".into(),
        "--test-threads=1".into(),
    ];
    let log = RunLog::open(&root.0, "retained-driver-test", vec![]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    // Session ownership leaves Linux subreaper admission to the otherwise child-free driver.
    // Retain outer diagnostics even when inner libtest catches a panic and returns101.
    let result = runtime.block_on(capture_owned(
        &root.0,
        &command,
        &environment,
        Duration::from_secs(90),
        Duration::from_secs(20),
        &log,
        (
            OwnedProcess::spawn_session,
            Stdio::null(),
            Some(retained_paths(&root.0, &output, &errors).unwrap()),
        ),
    ));
    let done = result.unwrap();
    assert_eq!(
        done.code, 0,
        "dedicated driver failed; retained raw logs: {directory:?}"
    );
    assert!(
        done.stdout
            .contains("retained capture/deadline contract cohort completed")
    );
    assert_eq!(std::fs::read(output).unwrap(), done.stdout.as_bytes());
    assert_eq!(std::fs::read(errors).unwrap(), done.stderr.as_bytes());
    drop(log);
    root.clean().unwrap();
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn retained_contract_fixture() {
    let Ok(mode) = std::env::var("NUMERA_RETAINED_CASE") else {
        return;
    };
    let directory = PathBuf::from(std::env::var_os("NUMERA_RETAINED_CONTROL").unwrap());
    if mode == "quiet" {
        std::fs::write(directory.join("ready"), b"ready").unwrap();
        // No polling/heartbeat in the leaf: either would contaminate CPU/Job-IO attribution.
        std::thread::sleep(Duration::from_secs(25));
        std::fs::write(directory.join("fallback"), b"fixture fallback expired").unwrap();
        std::process::exit(91);
    }
    use std::io::Write;
    std::io::stdout().flush().unwrap();
    std::io::stderr().flush().unwrap();
    let stdout = mode.ends_with("stdout");
    let path = directory.join(if stdout { "stdout.log" } else { "stderr.log" });
    let prefix = std::fs::metadata(path).unwrap().len();
    std::fs::write(directory.join("ready"), prefix.to_string()).unwrap();
    let bytes = if mode.starts_with("overflow-") {
        // Cross the cap only on the final byte; earlier monitor ticks cannot truncate the payload.
        vec![b'x'; usize::try_from(CAPTURE_LIMIT + 1 - prefix).unwrap()]
    } else {
        assert!(mode.starts_with("invalid-"));
        b"ok\xff\n".to_vec()
    };
    if stdout {
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

#[cfg(any(windows, target_os = "linux"))]
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
        let directory = root.join("logs/tmp").join(mode);
        std::fs::create_dir(&directory).unwrap();
        let output = directory.join("stdout.log");
        let errors = directory.join("stderr.log");
        let mut environment = environment.clone();
        environment.insert("NUMERA_RETAINED_CASE".into(), mode.into());
        environment.insert(
            "NUMERA_RETAINED_CONTROL".into(),
            directory.to_str().unwrap().into(),
        );
        let result = runtime.block_on(capture_retained(
            root,
            &contract_command(),
            &environment,
            (&output, &errors),
            Duration::from_secs(8),
            Duration::from_secs(4),
            log,
        ));
        let error = result.err().expect("invalid capture must refuse");
        assert!(!directory.join("fallback").exists());
        let prefix: usize = std::fs::read_to_string(directory.join("ready"))
            .unwrap()
            .parse()
            .unwrap();
        let bytes = std::fs::read(if mode.ends_with("stdout") {
            &output
        } else {
            &errors
        })
        .unwrap();
        if mode.starts_with("overflow-") {
            assert_eq!(error, "Tool child output exceeded8MiB capture budget");
            assert_eq!(bytes.len() as u64, CAPTURE_LIMIT + 1);
            assert!(bytes[prefix..].iter().all(|byte| *byte == b'x'));
        } else {
            assert_eq!(
                error,
                format!(
                    "Non-UTF-8 tool output at byte{} (exit Some(0)); cannot safely decode",
                    prefix + 2
                )
            );
            assert_eq!(&bytes[prefix..], b"ok\xff\n");
        }
        assert_eq!(
            std::fs::read_dir(root.join("logs/tmp/tooling"))
                .unwrap()
                .count(),
            0
        );
    }
    // Separate streams catch a lost stdout OR stderr size signal; ordinary child IO cannot
    // substitute for the driver writes. Stable tree attribution is checked by the owner cohort.
    for (wall, stdout) in [(false, true), (false, false), (true, true)] {
        runtime.block_on(capture_file_progress(root, environment, log, wall, stdout));
    }
}

#[cfg(any(windows, target_os = "linux"))]
async fn capture_file_progress(
    root: &Path,
    environment: &BTreeMap<String, String>,
    log: &RunLog,
    wall: bool,
    stdout: bool,
) {
    use std::cell::Cell;
    use std::io::Write;
    use std::time::Instant;
    let directory = root.join("logs/tmp").join(if wall {
        "capture-wall"
    } else if stdout {
        "capture-idle-stdout"
    } else {
        "capture-idle-stderr"
    });
    std::fs::create_dir(&directory).unwrap();
    let output = directory.join("stdout.log");
    let errors = directory.join("stderr.log");
    let mut environment = environment.clone();
    environment.insert("NUMERA_RETAINED_CASE".into(), "quiet".into());
    environment.insert(
        "NUMERA_RETAINED_CONTROL".into(),
        directory.to_str().unwrap().into(),
    );
    let command = contract_command();
    let done = Cell::new(false);
    let capture = async {
        let result = capture_retained(
            root,
            &command,
            &environment,
            (&output, &errors),
            Duration::from_secs(if wall { 3 } else { 10 }),
            Duration::from_secs(2),
            log,
        )
        .await;
        done.set(true);
        result
    };
    let writer = async {
        let deadline = Instant::now() + Duration::from_secs(4);
        while std::fs::read(directory.join("ready")).ok().as_deref() != Some(b"ready") {
            if done.get() || Instant::now() >= deadline {
                return Err("capture leaf did not become ready".to_owned());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let began = Instant::now();
        let mut last_write = began;
        let mut writes = 0_u32;
        while !done.get() && began.elapsed() < Duration::from_secs(4) {
            let path = if stdout { &output } else { &errors };
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(path)
                .map_err(|e| e.to_string())?;
            file.write_all(b"driver-owned progress\n")
                .map_err(|e| e.to_string())?;
            writes += 1;
            last_write = Instant::now();
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        Ok((began.elapsed(), writes, last_write))
    };
    let started = Instant::now();
    let (result, writer) = tokio::join!(capture, writer);
    let error = result
        .err()
        .expect("quiet fixture must be stopped, not accepted");
    let (writing, writes, last_write) = writer.unwrap();
    assert!(writes >= 2);
    if wall {
        assert_eq!(error, "media adapter exceeded 3s wall budget");
        assert!(started.elapsed() >= Duration::from_secs(3));
    } else {
        assert_eq!(error, "media adapter made no CPU/IO progress for 2s");
        assert!(
            writing >= Duration::from_secs(4),
            "capture must outlive the original idle clock"
        );
        assert!(
            last_write.elapsed() >= Duration::from_secs(2),
            "idle must advance from the last file change, not the original start"
        );
    }
    assert!(!directory.join("fallback").exists());
    let bytes = std::fs::read(if stdout { output } else { errors }).unwrap();
    assert!(
        bytes
            .windows(b"driver-owned progress\n".len())
            .any(|part| part == b"driver-owned progress\n")
    );
    assert_eq!(
        std::fs::read_dir(root.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
}
