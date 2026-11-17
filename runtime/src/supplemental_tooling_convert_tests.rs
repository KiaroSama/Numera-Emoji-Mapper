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
    for (variant, expected, wall, idle) in [
        ("exit", 3, 5_000, 2_000),
        ("idle", 124, 6_000, 2_000),
        ("wall", 125, 2_000, 5_000),
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
        if variant == "wall" {
            assert!(
                std::fs::read(root.join("output café/.svg_cur"))
                    .unwrap()
                    .len()
                    > 1
            );
        } else {
            assert!(!root.join("output café/.svg_cur").exists());
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
