//! Real Windows kernel handles: inspection failure retains custody until retry.
#![allow(non_snake_case)]
mod BuildProcessOwner {
    // Original: tests.test_build_process_owner.BuildProcessOwner.test_partial_member_inventory_closes_already_pinned_handles
    #[test]
    #[cfg_attr(
        not(windows),
        ignore = "Native Windows kernel handle ownership replaces the original portable Windows API mock"
    )]
    fn test_partial_member_inventory_closes_already_pinned_handles() {
        #[cfg(not(windows))]
        panic!(
            "requires actual Windows Job and process handles; never claim a simulated Linux pass"
        );
        #[cfg(windows)]
        {
            use crate::process_owner::OwnedProcess;
            use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
            use std::time::{Duration, Instant};
            use windows_sys::Win32::{
                Foundation::{GetHandleInformation, WAIT_OBJECT_0, WAIT_TIMEOUT},
                System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
            };
            let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap();
            let parent =
                crate::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
            let root = parent.join(format!("windows-inventory-fault-{}", std::process::id()));
            std::fs::create_dir(&root).unwrap();
            std::fs::create_dir(root.join("assets")).unwrap();
            std::fs::copy(
                project.join("assets/panel.html"),
                root.join("assets/panel.html"),
            )
            .unwrap();
            let original = crate::media_native_adapter::native_executable(
                project,
                &std::env::current_exe().unwrap(),
            )
            .unwrap();
            let binary = root.join("numera-emoji.exe");
            std::fs::copy(original, &binary).unwrap();
            drop(
                crate::catalog::Catalog::open(&root.join("data/catalog.db"), "fixture", &root)
                    .unwrap(),
            );
            let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = reserve.local_addr().unwrap().port();
            drop(reserve);
            let workspace = root.join("owner");
            std::fs::create_dir(&workspace).unwrap();
            let mut command = std::process::Command::new(binary);
            command
                .args([
                    "panel",
                    "--data-dir",
                    root.join("data").to_str().unwrap(),
                    "--port",
                    &port.to_string(),
                    "--no-open",
                ])
                .env_clear()
                .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            for key in ["SystemRoot", "WINDIR"] {
                if let Ok(value) = std::env::var(key) {
                    command.env(key, value);
                }
            }
            let mut owner = OwnedProcess::spawn(&mut command, workspace.clone()).unwrap();
            let pid = owner.child.as_ref().unwrap().id();
            let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
            assert!(!raw.is_null());
            let inspection = unsafe { OwnedHandle::from_raw_handle(raw) };
            let deadline = Instant::now() + Duration::from_secs(10);
            while std::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).is_err() {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(10));
            }
            let (job, member) = owner.tree.owned_handles();
            owner.tree.fail_next_member_query();
            assert_eq!(owner.finish().unwrap_err(), "fixture member query failed");
            let valid = |handle| {
                let mut flags = 0;
                unsafe { GetHandleInformation(handle, &mut flags) != 0 }
            };
            assert!(
                valid(job) && valid(member),
                "unknown inventory must retain real Job/member custody"
            );
            assert!(workspace.is_dir());
            assert_eq!(
                unsafe { WaitForSingleObject(inspection.as_raw_handle(), 0) },
                WAIT_TIMEOUT
            );
            owner.finish().unwrap();
            assert!(!workspace.exists());
            assert_eq!(
                unsafe { WaitForSingleObject(inspection.as_raw_handle(), 0) },
                WAIT_OBJECT_0
            );
            assert!(
                !valid(member),
                "successful retry did not close original pinned process handle"
            );
            assert!(valid(job), "Job custody ended before the owner was dropped");
            drop(owner);
            assert!(
                !valid(job),
                "owner drop leaked the original Job kernel handle"
            );
            drop(inspection);
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
