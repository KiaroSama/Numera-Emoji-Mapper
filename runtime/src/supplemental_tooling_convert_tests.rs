//! Preserved native watchdog matrix over the actual standalone conversion owner and shared Rust worker.
// Original native/src/tooling_convert_tests.rs; separate Python runtime selection was retired.
use crate::{logging::RunLog, tooling_convert, tooling_environment};
use std::path::Path;
use std::time::Duration;

#[path = "../tests/supplemental_native_support/process_liveness.rs"]
#[allow(unsafe_code)]
mod process_liveness;

#[test]
fn installed_converter_argv_exit_and_marker_deadlines_reap_acknowledged_tree() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let parent = tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
    let fixture = crate::supplemental_worker_fixture::worker();
    assert!(fixture.is_file());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    owned_path_controls(&parent, &fixture, &runtime);
    for (variant, expected, wall, idle) in [
        ("exit", 3, 5_000, 2_000),
        ("idle", 124, 6_000, 2_000),
        ("wall", 125, 2_000, 5_000),
        ("wall-staged", 125, 2_000, 5_000),
    ] {
        let root = parent.join(format!(
            "converter watchdog café {variant} {}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        let root = tooling_environment::physical_root(&root).unwrap();
        let installed = root.join(if cfg!(windows) {
            "installation/numera-emoji.exe"
        } else {
            "installation/numera-emoji"
        });
        std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
        std::fs::copy(&fixture, &installed).unwrap();
        let source = root.join("input with spaces");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("sentinel"), b"caller source bytes").unwrap();
        let log = RunLog::open(&root, "converter-watchdog-test", vec![]);
        let mut environment = crate::supplemental_worker_fixture::test_environment();
        environment.insert("NUMERA_CONVERT_FIXTURE".into(), variant.into());
        let limits = tooling_convert::Watchdog {
            wall: Duration::from_millis(wall),
            idle: Duration::from_millis(idle),
            poll: Duration::from_millis(20),
        };
        let result = runtime
            .block_on(async {
                tokio::time::timeout(
                    Duration::from_secs(15),
                    tooling_convert::run_attempt_with_launch(
                        &root,
                        Path::new("input with spaces"),
                        Path::new("output café"),
                        Path::new("stdout.txt"),
                        Path::new("stderr.txt"),
                        &log,
                        ((&environment, limits), |launch_root, args, env| {
                            let mut argv = vec![installed.clone().into_os_string()];
                            argv.extend(args.iter().map(std::ffi::OsString::from));
                            Ok(crate::tooling_launch::Launch {
                                root: launch_root.into(),
                                argv,
                                environment: tooling_environment::environment(
                                    launch_root,
                                    env,
                                    false,
                                )?,
                            })
                        }),
                    ),
                )
                .await
                .expect("watchdog test exceeded its outer bound")
            })
            .unwrap();
        assert_eq!(result, expected, "{variant}");
        let actual = std::fs::read_to_string(root.join("argv.txt")).unwrap();
        assert_eq!(
            actual,
            format!(
                "make-emoji-pngs\n--in\n{}\n--out\n{}",
                source.display(),
                root.join("output café").display()
            )
        );
        let pids = std::fs::read_to_string(root.join("ready")).unwrap();
        let pids = pids
            .lines()
            .map(|pid| pid.parse::<u32>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(pids.len(), if variant == "exit" { 1 } else { 2 });
        for pid in pids {
            process_liveness::assert_stopped(pid);
        }
        assert_eq!(
            std::fs::read(source.join("sentinel")).unwrap(),
            b"caller source bytes"
        );
        assert!(
            std::fs::read_to_string(root.join("stdout.txt"))
                .unwrap()
                .contains("converter café 😀")
        );
        assert!(
            std::fs::read_to_string(root.join("stderr.txt"))
                .unwrap()
                .contains("fixture stderr")
        );
        assert_eq!(
            std::fs::read_dir(root.join("logs/tmp/tooling"))
                .unwrap()
                .count(),
            0
        );
        assert!(!crate::process_owner::cleanup_pending(&root));
        if matches!(variant, "wall" | "wall-staged") {
            assert!(
                std::fs::read(root.join("output café/.svg_cur"))
                    .unwrap()
                    .len()
                    > 1
            );
        } else {
            assert!(!root.join("output café/.svg_cur").exists());
        }
        if variant == "wall-staged" {
            assert_eq!(
                std::fs::read(root.join("output café/marker-update-opened")).unwrap(),
                b"source-2.svg"
            );
            assert!(
                std::fs::read(root.join("output café/.svg_cur.next"))
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                std::fs::read(root.join("output café/.svg_cur")).unwrap(),
                b"source-1.svg"
            );
        }
        log.finish(result);
        let path = log.path.clone().unwrap();
        drop(log);
        assert!(
            std::fs::read_to_string(path)
                .unwrap()
                .contains(&format!("native conversion attempt exited code={expected}"))
        );
        // Assertion failures retain this exact root; remove only after all custody checks.
        std::fs::remove_dir_all(root).unwrap();
    }
}

// Additional variants of the existing watchdog case; no new libtest identity.
fn owned_path_controls(parent: &Path, fixture: &Path, runtime: &tokio::runtime::Runtime) {
    let root = parent.join(format!("converter path café {}", std::process::id()));
    let outside = parent.join(format!("converter outside {}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&outside).unwrap();
    let root = tooling_environment::physical_root(&root).unwrap();
    let outside = tooling_environment::physical_root(&outside).unwrap();
    std::fs::write(outside.join("precious"), b"outside unchanged").unwrap();
    std::fs::create_dir(root.join("input with spaces")).unwrap();
    std::fs::write(
        root.join("input with spaces/sentinel"),
        b"caller source bytes",
    )
    .unwrap();
    let log = RunLog::open(&root, "converter-path-admission", vec![]);
    let mut environment = crate::supplemental_worker_fixture::test_environment();
    environment.insert("NUMERA_CONVERT_FIXTURE".into(), "exit".into());
    let limits = tooling_convert::Watchdog {
        wall: Duration::from_secs(5),
        idle: Duration::from_secs(2),
        poll: Duration::from_millis(20),
    };
    let attempt = |owner: &Path, output: &Path, stdout: &Path, stderr: &Path| {
        runtime.block_on(tooling_convert::run_attempt_with_launch(
            owner,
            Path::new("input with spaces"),
            output,
            stdout,
            stderr,
            &log,
            ((&environment, limits), |launch_root, args, env| {
                let mut argv = vec![fixture.as_os_str().to_owned()];
                argv.extend(args.iter().map(std::ffi::OsString::from));
                Ok(crate::tooling_launch::Launch {
                    root: launch_root.into(),
                    argv,
                    environment: tooling_environment::environment(launch_root, env, false)?,
                })
            }),
        ))
    };
    let output = root.join("output café");
    let stdout = root.join("stdout.txt");
    let stderr = root.join("stderr.txt");
    assert_eq!(
        tooling_convert::owned_path(&root, Path::new("new parent/capture.txt"), false).unwrap(),
        root.join("new parent/capture.txt")
    );
    assert!(!root.join("new parent").exists());
    for refused in [
        outside.join("output"),
        root.join("../outside"),
        root.join("nested/../../outside"),
    ] {
        assert!(
            attempt(&root, &refused, &stdout, &stderr)
                .unwrap_err()
                .contains("inside its owning project")
        );
    }
    assert!(attempt(&root, &output, &outside.join("stdout"), &stderr).is_err());
    assert!(attempt(&root, &output, &stdout, &outside.join("stderr")).is_err());
    let linked = root.join("linked");
    #[cfg(target_os = "linux")]
    std::os::unix::fs::symlink(&outside, &linked).unwrap();
    #[cfg(windows)]
    junction::create(&outside, &linked).unwrap();
    assert!(
        attempt(&root, &linked.join("output"), &stdout, &stderr)
            .unwrap_err()
            .contains("Linked project-local storage")
    );
    #[cfg(target_os = "linux")]
    std::fs::remove_file(&linked).unwrap();
    #[cfg(windows)]
    std::fs::remove_dir(&linked).unwrap();
    let capture = root.join("same.capture");
    std::fs::write(&capture, b"existing capture must survive refusal").unwrap();
    let alias = capture.canonicalize().unwrap();
    assert!(
        attempt(&root, &output, &capture, &alias)
            .unwrap_err()
            .contains("captures must be distinct")
    );
    assert_eq!(
        std::fs::read(&capture).unwrap(),
        b"existing capture must survive refusal"
    );
    #[cfg(windows)]
    {
        use std::path::{Component, PathBuf, Prefix};
        let verbatim = root.canonicalize().unwrap();
        assert_ne!(
            verbatim, root,
            "real Windows canonicalize must expose the alias being tested"
        );
        assert_eq!(
            tooling_convert::owned_path(&root, &verbatim.join("new parent/capture.txt"), false)
                .unwrap(),
            root.join("new parent/capture.txt")
        );
        let mut refused = vec![
            PathBuf::from(r"\\.\NUL"),
            PathBuf::from(r"\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy1\output"),
            PathBuf::from(r"\\outside.invalid\share\output"),
        ];
        // A raw ordinary DOS component, not verbatim: do not let Win32 trim
        // its ASCII spaces into parent traversal before lexical admission.
        let mut dos_parent = root.as_os_str().to_owned();
        dos_parent.push(r"\..  \outside");
        refused.push(PathBuf::from(dos_parent));
        // Construct raw verbatim suffixes with OsString, not PathBuf::push,
        // which itself normalizes appended paths for a verbatim prefix.
        for suffix in [
            r".\output",
            r".. \outside",
            "trailing.",
            "trailing ",
            "capture:stream",
            "NUL",
            "NUL.txt",
            "COM1.log",
            "COM¹",
            "LPT².tmp",
            "CONOUT$",
            "foo/../../outside",
        ] {
            let mut raw = verbatim.as_os_str().to_owned();
            raw.push(r"\");
            raw.push(suffix);
            refused.push(PathBuf::from(raw));
        }
        if let Some(Component::Prefix(prefix)) = root.components().next()
            && let Prefix::Disk(drive) = prefix.kind()
        {
            refused.push(PathBuf::from(format!(
                "{}:relative-output",
                char::from(drive)
            )));
        }
        for path in refused {
            assert!(
                attempt(&root, &path, &stdout, &stderr)
                    .unwrap_err()
                    .contains("inside its owning project")
            );
        }
        assert!(!output.exists());
        assert!(!stdout.exists());
        assert!(!stderr.exists());
        assert!(
            !root.join("ready").exists(),
            "no refused path may start the actual worker"
        );
        // Actual child positive: same physical owner/root with mixed DOS and
        // verbatim destination/captures, still exact worker argv and exit3.
        assert_eq!(
            attempt(
                &verbatim,
                &verbatim.join("output café"),
                &stdout,
                &verbatim.join("stderr.txt")
            )
            .unwrap(),
            3
        );
        assert_eq!(
            std::fs::read_to_string(root.join("argv.txt")).unwrap(),
            format!(
                "make-emoji-pngs\n--in\n{}\n--out\n{}",
                root.join("input with spaces").display(),
                output.display()
            )
        );
        let pid = std::fs::read_to_string(root.join("ready"))
            .unwrap()
            .parse()
            .unwrap();
        process_liveness::assert_stopped(pid);
        assert!(
            std::fs::read_to_string(&stdout)
                .unwrap()
                .contains("converter café 😀")
        );
        assert!(
            std::fs::read_to_string(&stderr)
                .unwrap()
                .contains("fixture stderr")
        );
    }
    #[cfg(target_os = "linux")]
    {
        assert!(!output.exists());
        assert!(!stdout.exists());
        assert!(!stderr.exists());
        assert!(!root.join("ready").exists());
    }
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 1);
    assert_eq!(
        std::fs::read(outside.join("precious")).unwrap(),
        b"outside unchanged"
    );
    assert_eq!(
        std::fs::read(root.join("input with spaces/sentinel")).unwrap(),
        b"caller source bytes"
    );
    assert!(!crate::process_owner::cleanup_pending(&root));
    if let Ok(entries) = std::fs::read_dir(root.join("logs/tmp/tooling")) {
        assert_eq!(entries.count(), 0);
    }
    log.finish(0);
    drop(log);
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(outside).unwrap();
}
