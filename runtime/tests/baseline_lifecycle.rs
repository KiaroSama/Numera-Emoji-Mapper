//! Original sandbox-construction, installed-runtime and test-admission boundaries.
#![allow(non_snake_case)]
#[allow(dead_code)]
mod baseline_lifecycle_support;
#[allow(dead_code)]
mod baseline_tooling_support;
use baseline_lifecycle_support::*;
use baseline_tooling_support::*;
use numera_runtime as runtime;
use runtime::{catalog::Catalog, tooling_environment, tooling_sandbox};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[path = "baseline_lifecycle_support/clone_boundaries.rs"]
mod CloneBoundaries;

mod PrebuiltEnvironment {
    use super::*;
    // Original: tests.test_prebuilt_environment.PrebuiltEnvironment.test_existing_installation_refuses_before_writing
    #[test]
    fn test_existing_installation_refuses_before_writing() {
        let root = Root::new();
        root.install();
        let source = root.file(
            "opaque-native-artifact",
            b"verified opaque package artifact",
        );
        let package = package_fixture(&root, &source, None);
        let destination = root.0.join("installed");
        std::fs::create_dir(&destination).unwrap();
        std::fs::write(
            destination.join("operator-config"),
            b"previous working installation",
        )
        .unwrap();
        assert!(runtime::package::install(&package, &destination).is_err());
        assert_eq!(
            std::fs::read(destination.join("operator-config")).unwrap(),
            b"previous working installation"
        );
        assert_eq!(std::fs::read_dir(&destination).unwrap().count(), 1);
        assert!(!root.0.join(".installed.install.lock").exists());
    }
    // Original: tests.test_prebuilt_environment.PrebuiltEnvironment.test_owned_temp_and_caches_override_external_values
    #[test]
    fn test_owned_temp_and_caches_override_external_values() {
        let root = Root::new();
        let before = BTreeMap::from([
            ("TEMP".into(), "/outside".into()),
            ("CARGO_HOME".into(), "/outside".into()),
            ("KEEP".into(), "unchanged".into()),
        ]);
        let actual = tooling_environment::environment(&root.0, &before, true).unwrap();
        assert_eq!(before["TEMP"], "/outside");
        assert_eq!(actual["KEEP"], "unchanged");
        for key in [
            "TEMP",
            "TMP",
            "TMPDIR",
            "XDG_CACHE_HOME",
            "CARGO_HOME",
            "CARGO_TARGET_DIR",
            "RUSTUP_HOME",
            "HOMEBREW_CACHE",
            "HOMEBREW_TEMP",
            "CCACHE_DIR",
            "SCCACHE_DIR",
            "npm_config_cache",
        ] {
            let path =
                tooling_environment::physical_root(std::path::Path::new(&actual[key])).unwrap();
            assert!(path.starts_with(&root.0), "{key}: {}", path.display());
            assert!(path.is_dir());
        }
        // Python/pip caches retire with their interpreter dependency.
        for key in ["PIP_CACHE_DIR", "PYTHONPYCACHEPREFIX"] {
            assert!(!actual.contains_key(key));
        }
    }
    // Original: tests.test_prebuilt_environment.PrebuiltEnvironment.test_linked_cache_refuses_before_using_outside_directory
    #[test]
    fn test_linked_cache_refuses_before_using_outside_directory() {
        let root = Root::new();
        let outside = root.0.join("outside");
        std::fs::create_dir(&outside).unwrap();
        let link = root.0.join(".cache");
        directory_link(&outside, &link);
        let error = tooling_environment::confined_directory(&root.0, ".cache", false).unwrap_err();
        assert!(error.contains("Linked project-local storage"), "{error}");
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
        remove_directory_link(&link);
    }
}

mod NativeInstall {
    use super::*;
    // Original: tests.test_native_install.NativeInstall.test_installed_launcher_probe_and_refusal_work_without_cargo
    #[test]
    fn test_installed_launcher_probe_and_refusal_work_without_cargo() {
        let root = Root::new();
        root.install();
        let caller = Root::new();
        let checked = execute_binary(&caller.0, &root.0.join(app_name()), &["doctor"], None);
        assert_eq!(checked.code, 0, "{}", checked.stderr);
        assert!(checked.stdout.contains("Environment check complete"));
        std::fs::remove_file(root.0.join(app_name())).unwrap();
        let source = root.source(&["s:a"], false);
        let refused = root.dev(&[
            "panel-sandbox",
            "--source",
            source.to_str().unwrap(),
            "--port",
            &port().to_string(),
        ]);
        assert_eq!(refused.code, 1, "{} {}", refused.stdout, refused.stderr);
        assert!(
            refused
                .stderr
                .contains("Standalone native executable is missing"),
            "{}",
            refused.stderr
        );
        for path in ["runtime/target", "native/target", ".venv"] {
            assert!(!root.0.join(path).exists());
        }
    }
    // Original: tests.test_native_install.NativeInstall.test_primary_doctor_and_conversion_use_installed_runtime_without_cargo
    #[test]
    fn test_primary_doctor_and_conversion_use_installed_runtime_without_cargo() {
        let root = Root::new();
        root.install();
        let caller = Root::new();
        let checked = execute_binary(&caller.0, &root.0.join(app_name()), &["doctor"], None);
        assert_eq!(checked.code, 0, "{}", checked.stderr);
        assert!(checked.stdout.contains("Environment check complete"));
        root.raster("input with spaces/art.png", [30, 90, 220, 255]);
        let input = caller.file(
            "scripted-input",
            format!(
                "a2\n{}\n{}\nexit\n",
                root.0.join("input with spaces").display(),
                root.0.join("output with spaces").display()
            )
            .as_bytes(),
        );
        let result = execute_binary(&caller.0, &root.0.join(app_name()), &["menu"], Some(&input));
        assert_eq!(result.code, 0, "{} {}", result.stdout, result.stderr);
        assert!(result.stdout.contains("Conversion finished"));
        assert_eq!(
            image::open(root.0.join("output with spaces/art.png"))
                .unwrap()
                .into_rgba8()
                .dimensions(),
            (100, 100)
        );
        std::fs::remove_file(root.0.join(app_name())).unwrap();
        let source = root.source(&["s:a"], false);
        let refused = root.dev(&[
            "panel-sandbox",
            "--source",
            source.to_str().unwrap(),
            "--port",
            &port().to_string(),
        ]);
        assert_eq!(refused.code, 1, "{} {}", refused.stdout, refused.stderr);
        assert!(
            refused
                .stderr
                .contains("Standalone native executable is missing")
        );
        for path in ["runtime/target", ".venv", "emojikit"] {
            assert!(!root.0.join(path).exists());
        }
    }
    // Original: tests.test_native_install.NativeInstall.test_checkout_extension_keeps_repaired_wheel_libraries
    #[test]
    fn test_checkout_extension_keeps_repaired_wheel_libraries() {
        let root = Root::new();
        root.install();
        let executable = root.file(
            "opaque-native-artifact",
            b"fixture opaque native executable",
        );
        let library = root.file(
            "dependencies/libssl-fixture.so",
            b"fixture shared dependency",
        );
        let package = package_fixture(&root, &executable, Some(library.parent().unwrap()));
        let metadata = runtime::package::verify(&package).unwrap();
        assert_eq!(
            metadata["files"]["lib/libssl-fixture.so"],
            runtime::package::digest(&library).unwrap()
        );
        let installed = runtime::package::install(&package, &root.0.join("installed")).unwrap();
        assert_eq!(
            std::fs::read(installed).unwrap(),
            std::fs::read(executable).unwrap()
        );
        assert_eq!(
            std::fs::read(root.0.join("installed/lib/libssl-fixture.so")).unwrap(),
            std::fs::read(library).unwrap()
        );
        assert_eq!(
            std::fs::read_dir(root.0.join("installed/lib"))
                .unwrap()
                .count(),
            1
        );
        let installed_manifest = runtime::package::verify(&root.0.join("installed")).unwrap();
        assert_eq!(installed_manifest["files"], metadata["files"]);
        assert!(installed_manifest["notices"].as_array().unwrap().len() >= 2);
    }
    // Original: tests.test_native_install.NativeInstall.test_installed_binary_runs_after_source_removed_and_failed_replace_preserves_it
    #[test]
    fn test_installed_binary_runs_after_source_removed_and_failed_replace_preserves_it() {
        let root = Root::new();
        root.install();
        let source = root.0.join(app_name());
        let package = package_fixture(&root, &source, None);
        let installed = runtime::package::install(&package, &root.0.join("installed")).unwrap();
        assert_eq!(
            std::fs::read(&installed).unwrap(),
            std::fs::read(&source).unwrap()
        );
        std::fs::remove_file(source).unwrap();
        let help = execute_binary(installed.parent().unwrap(), &installed, &["--help"], None);
        assert_eq!(help.code, 0, "{}", help.stderr);
        assert!(help.stdout.contains("identity-repair"));
        let status = execute_binary(installed.parent().unwrap(), &installed, &["status"], None);
        assert_eq!(status.code, 3, "{} {}", status.stdout, status.stderr);
        assert!(status.stdout.contains("STALE roster"));
        assert!(!root.0.join("installed/collection/catalog.db").exists());
        let logs = std::fs::read_dir(root.0.join("installed/logs"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("status_")
                    && path.extension().is_some_and(|suffix| suffix == "log")
            })
            .collect::<Vec<_>>();
        assert_eq!(logs.len(), 1);
        let text = std::fs::read_to_string(&logs[0]).unwrap();
        assert!(text.contains("[runtime] native backend started"), "{text}");
        assert!(text.contains("exit=3"));
        let before = std::fs::read(&installed).unwrap();
        // A real schema2 replacement refuses to sweep a used installation,
        // preserving both its executable and its newly created runtime logs.
        assert!(runtime::package::install(&package, installed.parent().unwrap()).is_err());
        assert_eq!(std::fs::read(&installed).unwrap(), before);
        assert_eq!(std::fs::read_to_string(&logs[0]).unwrap(), text);
        assert!(runtime::package::verify(installed.parent().unwrap()).is_ok());
    }
}

mod BuildProcessOwner {
    #[cfg(windows)]
    use super::*;
    // Original: tests.test_build_process_owner.BuildProcessOwner.test_success_reaps_detached_stdio_descendant
    #[test]
    #[cfg_attr(not(windows), ignore = "native Windows Job ownership")]
    fn test_success_reaps_detached_stdio_descendant() {
        #[cfg(not(windows))]
        panic!("requires actual Windows Job ownership");
        #[cfg(windows)]
        {
            use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
            use std::time::{Duration, Instant};
            use windows_sys::Win32::{
                Foundation::WAIT_OBJECT_0,
                System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
            };
            let root = Root::new();
            let mut environment = machine();
            environment.insert(
                "NUMERA_DETACHED_PARENT_FIXTURE".into(),
                root.0.to_string_lossy().into_owned(),
            );
            if let Ok(marker) = std::env::var("NUMERA_RUNTIME_TEST_ADMISSION") {
                environment.insert("NUMERA_RUNTIME_TEST_ADMISSION".into(), marker);
            }
            let argv = vec![
                std::env::current_exe().unwrap().into_os_string(),
                "detached_parent_fixture".into(),
                "--exact".into(),
                "--test-threads=1".into(),
            ];
            let log = runtime::logging::RunLog::open(&root.0, "detached-build-owner", vec![]);
            let (result, inspection) = executor().block_on(async {
                tokio::join!(
                    runtime::tooling_process::capture(
                        &root.0,
                        &argv,
                        &environment,
                        Duration::from_secs(15),
                        Duration::from_secs(10),
                        &log
                    ),
                    async {
                        let deadline = Instant::now() + Duration::from_secs(10);
                        let ready = root.0.join("descendant-ready");
                        while !ready.is_file() {
                            assert!(
                                Instant::now() < deadline,
                                "actual detached descendant did not become ready"
                            );
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                        let pid = std::fs::read_to_string(ready)
                            .unwrap()
                            .parse::<u32>()
                            .unwrap();
                        let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
                        assert!(!raw.is_null());
                        let original = unsafe { OwnedHandle::from_raw_handle(raw) };
                        std::fs::write(root.0.join("inspection-ready"), b"pinned").unwrap();
                        original
                    }
                )
            });
            let result = result.unwrap();
            assert_eq!(result.code, 0, "{}", result.stderr);
            assert_eq!(
                unsafe { WaitForSingleObject(inspection.as_raw_handle(), 0) },
                WAIT_OBJECT_0,
                "successful build left its original detached-stdio descendant alive"
            );
            log.finish(result.code);
            let text = std::fs::read_dir(root.0.join("logs"))
                .unwrap()
                .filter_map(|entry| std::fs::read_to_string(entry.unwrap().path()).ok())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(text.contains("owned child started"));
            assert!(text.contains("exit=0"));
        }
    }
}

mod DirectRunsRefuse {
    use super::*;
    // Original: tests.test_suite_guard.DirectRunsRefuse.test_no_test_module_starts_unittest_from_its_main_block
    #[test]
    fn test_no_test_module_starts_unittest_from_its_main_block() {
        let root = Root::new();
        let canary = root.0.join("body-must-not-run");
        let mut environment = machine();
        environment.insert(
            "NUMERA_ADMISSION_CANARY".into(),
            canary.to_string_lossy().into_owned(),
        );
        let result = direct_test_child(
            &root,
            &["admission_canary_fixture", "--exact"],
            &environment,
        );
        assert_eq!(
            result.code, 2,
            "unguarded libtest ran: {} {}",
            result.stdout, result.stderr
        );
        assert!(
            !canary.exists(),
            "unguarded test body performed effects before admission"
        );
        assert!(
            result.stderr.contains("isolated Rust test runner"),
            "{}",
            result.stderr
        );
        // A listing-looking value consumed by --skip must not bypass startup
        // admission and then execute the selected real fixture body.
        for value in ["--list", "--help"] {
            let disguised = direct_test_child(
                &root,
                &["--skip", value, "admission_canary_fixture", "--exact"],
                &environment,
            );
            assert_eq!(
                disguised.code, 2,
                "listing value bypassed admission: {} {}",
                disguised.stdout, disguised.stderr
            );
            assert!(
                disguised.stderr.contains("isolated Rust test runner"),
                "libtest argument refusal cannot substitute for startup admission: {}",
                disguised.stderr
            );
            assert!(!canary.exists());
        }
        for option in ["--list", "--help"] {
            let listed = direct_test_child(&root, &[option], &environment);
            assert_eq!(listed.code, 0, "{}", listed.stderr);
            assert!(!canary.exists());
        }
        let marker = std::env::var("NUMERA_RUNTIME_TEST_ADMISSION")
            .expect("actual outer runner must establish admission before test modules");
        environment.insert("NUMERA_RUNTIME_TEST_ADMISSION".into(), marker);
        environment.remove("NUMERA_EMOJI_MAPPER_NO_DOTENV");
        let unguarded_configuration = direct_test_child(
            &root,
            &["admission_canary_fixture", "--exact"],
            &environment,
        );
        assert_eq!(
            unguarded_configuration.code, 2,
            "admission without dotenv isolation ran a test body: {} {}",
            unguarded_configuration.stdout, unguarded_configuration.stderr
        );
        assert!(
            unguarded_configuration
                .stderr
                .contains("isolated Rust test runner")
        );
        assert!(!canary.exists());
        environment.insert("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into());
        let positive = direct_test_child(
            &root,
            &["admission_canary_fixture", "--exact"],
            &environment,
        );
        assert_eq!(positive.code, 0, "{}", positive.stderr);
        assert_eq!(std::fs::read(&canary).unwrap(), b"admitted body");
        for manifest in [
            "runtime/Cargo.toml",
            "tools/ci/Cargo.toml",
            "tools/test-inventory/Cargo.toml",
        ] {
            let metadata = metadata_for(manifest);
            for target in metadata["packages"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|package| package["targets"].as_array().unwrap())
                .filter(|target| target["test"] == true)
            {
                let path = std::path::Path::new(target["src_path"].as_str().unwrap());
                let file = syn::parse_file(&std::fs::read_to_string(path).unwrap()).unwrap();
                let module = file
                    .items
                    .iter()
                    .find_map(|item| match item {
                        syn::Item::Mod(module) if module.ident == "test_admission" => Some(module),
                        _ => None,
                    })
                    .unwrap_or_else(|| {
                        panic!(
                            "actual Cargo test root lacks pre-main admission: {}",
                            path.display()
                        )
                    });
                assert!(
                    module.content.is_none(),
                    "an inline namesake cannot substitute for real admission"
                );
                let relative = module
                    .attrs
                    .iter()
                    .find_map(|attribute| {
                        if let syn::Meta::NameValue(value) = &attribute.meta
                            && value.path.is_ident("path")
                            && let syn::Expr::Lit(value) = &value.value
                            && let syn::Lit::Str(text) = &value.lit
                        {
                            Some(text.value())
                        } else {
                            None
                        }
                    })
                    .expect("each real crate root must explicitly own the common admission source");
                assert_eq!(
                    path.parent()
                        .unwrap()
                        .join(relative)
                        .canonicalize()
                        .unwrap(),
                    project()
                        .join("runtime/tests/support/test_admission.rs")
                        .canonicalize()
                        .unwrap()
                );
            }
        }
    }
}
mod DotenvDetector {
    use super::*;
    // Original: tests.test_suite_guard.DotenvDetector.test_an_imported_unittest_counts_as_a_test_run_and_nothing_else_does
    #[test]
    fn test_an_imported_unittest_counts_as_a_test_run_and_nothing_else_does() {
        let root = Root::new();
        root.install();
        root.file(".env.example", b"SYNTHETIC_SUITE_CANARY=\n");
        root.file(".env", b"SYNTHETIC_SUITE_CANARY=local-fixture-value\n");
        let before = std::env::vars().collect::<BTreeMap<_, _>>();
        // Native admission uses an explicit guarded process context in place
        // of Python's imported-module registry. Ordinary application startup
        // remains able to read its own local operator configuration.
        let ordinary = root.run(
            &["doctor"],
            &[("NUMERA_EMOJI_MAPPER_NO_DOTENV", "0".into())],
        );
        let guarded = root.run(
            &["doctor"],
            &[("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1".into())],
        );
        assert_eq!(ordinary.code, 0, "{}", ordinary.stderr);
        assert_eq!(guarded.code, 0, "{}", guarded.stderr);
        assert!(
            !ordinary.stdout.contains("unset: SYNTHETIC_SUITE_CANARY"),
            "{}",
            ordinary.stdout
        );
        assert!(
            guarded.stdout.contains("unset: SYNTHETIC_SUITE_CANARY"),
            "{}",
            guarded.stdout
        );
        assert!(std::env::vars().collect::<BTreeMap<_, _>>() == before);
    }
}

// A technical admitted-body probe, not another original identity.
#[test]
fn admission_canary_fixture() {
    if let Ok(path) = std::env::var("NUMERA_ADMISSION_CANARY") {
        std::fs::write(path, b"admitted body").unwrap();
    }
}

// Technical self-spawned fixtures, never additional original-case identities.
#[test]
#[allow(
    clippy::zombie_processes,
    reason = "Intentional successful-parent exit: the outer actual Windows Job owner must terminate/reap the original separately pinned descendant; waiting here would remove the behavior under test"
)]
fn detached_parent_fixture() {
    let Ok(directory) = std::env::var("NUMERA_DETACHED_PARENT_FIXTURE") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["detached_grandchild_fixture", "--exact", "--test-threads=1"])
        .env_remove("NUMERA_DETACHED_PARENT_FIXTURE")
        .env("NUMERA_DETACHED_GRANDCHILD_FIXTURE", &directory)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !directory.join("inspection-ready").is_file() {
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("parent fixture did not receive original-handle inspection acknowledgement");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
#[test]
fn detached_grandchild_fixture() {
    let Ok(directory) = std::env::var("NUMERA_DETACHED_GRANDCHILD_FIXTURE") else {
        return;
    };
    std::fs::write(
        std::path::Path::new(&directory).join("descendant-ready"),
        std::process::id().to_string(),
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_secs(300));
}

// Technical child body only; it represents no additional original test ID.
#[test]
fn construction_child_fixture() {
    let Ok(directory) = std::env::var("NUMERA_CONSTRUCTION_FIXTURE") else {
        return;
    };
    let root = std::path::PathBuf::from(directory);
    let _session = tooling_sandbox::Session::create(
        &root,
        &root.join("source"),
        &root.join("panel-sandbox-dest"),
    )
    .unwrap();
    std::fs::write(root.join("construction-ready"), b"READY\n").unwrap();
    std::thread::sleep(std::time::Duration::from_secs(300));
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
