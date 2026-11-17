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

mod CloneBoundaries {
    use super::*;
    // Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_cloning_does_not_migrate_or_change_source_journal_mode
    #[test]
    fn test_cloning_does_not_migrate_or_change_source_journal_mode() {
        let root = Root::new();
        let source = root.source(&["s:a"], false);
        let db = source.join("catalog.db");
        let connection = rusqlite::Connection::open(&db).unwrap();
        connection
            .execute_batch("PRAGMA journal_mode=DELETE; DROP TABLE meta;")
            .unwrap();
        drop(connection);
        let plan = br#"{"version":1,"targets":[],"known":[],"excluded":[],"moves":[],"held":[]}"#;
        root.file("source/pack_plan.json", plan);
        let before = database_state(&db);
        let bytes = std::fs::read(&db).unwrap();
        let session = root.clone(&source);
        assert_eq!(database_state(&db), before);
        assert_eq!(std::fs::read(&db).unwrap(), bytes);
        assert_eq!(std::fs::read(source.join("pack_plan.json")).unwrap(), plan);
        let copied = Catalog::open(&session.directory.join("catalog.db"), UTC, &root.0).unwrap();
        assert_eq!(copied.all(None).unwrap().len(), 1);
        drop(copied);
        assert_eq!(database_state(&db), before);
        session.finish().unwrap();
    }
    // Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_a_refused_clone_also_leaves_the_source_database_unchanged
    #[test]
    fn test_a_refused_clone_also_leaves_the_source_database_unchanged() {
        let root = Root::new();
        let source = root.source(&["s:a"], false);
        let db = source.join("catalog.db");
        let connection = rusqlite::Connection::open(&db).unwrap();
        connection
            .execute_batch("PRAGMA journal_mode=DELETE")
            .unwrap();
        drop(connection);
        std::fs::remove_file(source.join("media/0.png")).unwrap();
        let before = std::fs::read(&db).unwrap();
        assert!(tooling_sandbox::Session::create(&root.0, &source, &root.destination()).is_err());
        assert_eq!(std::fs::read(&db).unwrap(), before);
        assert!(!root.destination().exists());
    }
    // Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_project_relative_path_is_not_shadowed_by_a_source_relative_file
    #[test]
    fn test_project_relative_path_is_not_shadowed_by_a_source_relative_file() {
        let root = Root::new();
        let source = root.source(&["s:a"], false);
        root.file("media/0.png", b"PROJECT-original");
        root.file("source/media/0.png", b"SOURCE-decoy");
        set_media_path(&source, "media/0.png");
        let session = root.clone(&source);
        assert_eq!(
            std::fs::read(&paths(&session.directory)[0]).unwrap(),
            b"PROJECT-original"
        );
        session.finish().unwrap();
    }
    // Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_data_relative_path_is_not_shadowed_by_a_project_relative_file
    #[test]
    fn test_data_relative_path_is_not_shadowed_by_a_project_relative_file() {
        let root = Root::new();
        let source = root.source(&["s:a"], false);
        root.file("media/0.png", b"PROJECT-decoy");
        root.file("source/media/0.png", b"SOURCE-original");
        set_media_path(&source, "./media/0.png");
        let session = root.clone(&source);
        assert_eq!(
            std::fs::read(&paths(&session.directory)[0]).unwrap(),
            b"SOURCE-original"
        );
        session.finish().unwrap();
    }
    // Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_portable_names_do_not_alias_distinct_catalog_keys
    #[test]
    fn test_portable_names_do_not_alias_distinct_catalog_keys() {
        let root = Root::new();
        let source = root.source(&["s:a_b", "s_a:b"], false);
        let session = root.clone(&source);
        let names = paths(&session.directory);
        assert_eq!(names.len(), 2);
        assert_ne!(names[0], names[1]);
        let bodies = names
            .iter()
            .map(|path| std::fs::read(path).unwrap())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            bodies,
            [b"\x89PNG-0".to_vec(), b"\x89PNG-1".to_vec()]
                .into_iter()
                .collect()
        );
        session.finish().unwrap();
    }
    // Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_wrapper_keeps_ownership_from_marker_through_complete_cleanup
    #[test]
    fn test_wrapper_keeps_ownership_from_marker_through_complete_cleanup() {
        let root = Root::new();
        root.install();
        let source = root.source(&["s:a"], false);
        let mut live = Live::wrapper(&root, &source, port());
        live.ready();
        let clone = active_clone(&root);
        let base = clone.parent().unwrap().to_path_buf();
        let lock = tooling_sandbox::lifetime_lock_path(&clone).unwrap();
        assert_eq!(tooling_sandbox::sweep_stale(&base).unwrap(), 0);
        assert_eq!(
            Catalog::open(&clone.join("catalog.db"), UTC, &root.0)
                .unwrap()
                .all(None)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(live.graceful(), 130);
        assert!(!clone.exists());
        assert!(lock.is_file());
        assert_eq!(tooling_sandbox::sweep_stale(&base).unwrap(), 0);
    }
    // Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_old_marker_protocol_is_left_alone_not_mistaken_for_abandonment
    #[test]
    fn test_old_marker_protocol_is_left_alone_not_mistaken_for_abandonment() {
        let root = Root::new();
        let source = root.source(&["s:a"], false);
        let session = root.clone(&source);
        let directory = session.directory.clone();
        drop(session);
        let marker = directory.join(".sandbox-owner.json");
        let mut document: Value = serde_json::from_slice(&std::fs::read(&marker).unwrap()).unwrap();
        document["version"] = json!(1);
        std::fs::write(marker, serde_json::to_vec(&document).unwrap()).unwrap();
        assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
        assert!(directory.join("catalog.db").is_file());
    }
    // Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_process_death_releases_construction_lease_for_safe_reclamation
    #[test]
    fn test_process_death_releases_construction_lease_for_safe_reclamation() {
        let root = Root::new();
        let source = root.source(&["s:a"], false);
        let mut child = Construction::spawn(&root, &source);
        child.ready();
        let before = runtime::package::digest(&root.destination().join("catalog.db")).unwrap();
        assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
        assert_eq!(
            runtime::package::digest(&root.destination().join("catalog.db")).unwrap(),
            before
        );
        child.stop();
        assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 1);
        assert!(!root.destination().join("catalog.db").exists());
        assert!(
            tooling_sandbox::lifetime_lock_path(&root.destination())
                .unwrap()
                .is_file()
        );
    }
    // Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_real_wrapper_http_save_changes_only_the_owned_clone
    #[test]
    fn test_real_wrapper_http_save_changes_only_the_owned_clone() {
        let root = Root::new();
        root.install();
        let source = root.source(
            &[
                "s:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "s:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            ],
            false,
        );
        let before = database_state(&source.join("catalog.db"));
        let mut live = Live::wrapper(&root, &source, port());
        live.ready();
        let clone = active_clone(&root);
        let (_, page) = live.get("/");
        let page = String::from_utf8(page).unwrap();
        let token = page
            .split("const TOKEN = \"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let body = page
            .split("<script id=\"items-data\" type=\"application/json\">")
            .nth(1)
            .unwrap()
            .split("</script>")
            .next()
            .unwrap();
        let cards: Vec<Value> = serde_json::from_str(body).unwrap();
        let keys = cards
            .iter()
            .filter(|card| card["isLogo"] != true)
            .map(|card| card["key"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(keys.len(), 2);
        let response: Value = reqwest::blocking::Client::new()
            .post(format!("{}/api/save", live.url))
            .header("X-Panel-Token", token)
            .json(&json!({"known":keys,"excluded":[keys[0]],"packs":[]}))
            .send()
            .unwrap()
            .json()
            .unwrap();
        assert_eq!(response["ok"], true, "{response}");
        let connection = rusqlite::Connection::open_with_flags(
            clone.join("catalog.db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let excluded = connection
            .prepare("SELECT content_key FROM items WHERE included=0")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(excluded, [keys[0].clone()]);
        drop(connection);
        assert_eq!(database_state(&source.join("catalog.db")), before);
        assert_eq!(
            tooling_sandbox::sweep_stale(clone.parent().unwrap()).unwrap(),
            0
        );
        live.owner.finish().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let address = live
            .url
            .strip_prefix("http://")
            .unwrap()
            .parse::<std::net::SocketAddr>()
            .unwrap();
        while std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_millis(100))
            .is_ok()
        {
            assert!(
                std::time::Instant::now() < deadline,
                "actual panel still listened ten seconds after controller death"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        // Native safety disposition: abrupt owner death cannot certify every
        // descendant stopped. Its pre-spawn recovery copy is retained rather
        // than promoting a released lease into automatic reclamation proof.
        assert!(clone.join(".sandbox-recovery.json").is_file());
        assert_eq!(
            tooling_sandbox::sweep_stale(clone.parent().unwrap()).unwrap(),
            0
        );
        assert!(clone.join("catalog.db").is_file());
        assert_eq!(database_state(&source.join("catalog.db")), before);
    }
    // Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_panel_exception_cleans_owned_data_and_restores_environment
    #[test]
    fn test_panel_exception_cleans_owned_data_and_restores_environment() {
        let root = Root::new();
        root.install();
        let source = root.source(&["s:a"], false);
        let before = std::env::vars().collect::<BTreeMap<_, _>>();
        let occupied = Legacy::new();
        let result = root.dev(&[
            "panel-sandbox",
            "--source",
            source.to_str().unwrap(),
            "--port",
            &occupied.port.to_string(),
        ]);
        assert_eq!(result.code, 2, "{} {}", result.stdout, result.stderr);
        assert!(std::env::vars().collect::<BTreeMap<_, _>>() == before);
        assert!(
            !std::fs::read_dir(root.0.join("logs/tmp"))
                .unwrap()
                .any(|entry| entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(tooling_sandbox::PREFIX))
        );
        assert!(source.join("catalog.db").is_file());
    }
}

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
