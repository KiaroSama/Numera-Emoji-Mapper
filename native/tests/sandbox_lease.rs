//! The inherited open-file description excludes reclaimers until native ownership ends.
#[cfg(target_os = "linux")]
mod linux {
    use _native::sandbox_lease;
    use sha2::{Digest, Sha256};
    use std::os::fd::AsRawFd;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command};
    use std::time::{Duration, Instant};

    unsafe extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
        fn fcntl(fd: i32, command: i32, ...) -> i32;
    }

    struct Owner(Child, PathBuf);
    impl Drop for Owner {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
            let _ = std::fs::remove_dir_all(&self.1);
        }
    }

    fn wait_file(path: &Path, child: &mut Child) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !path.is_file() {
            assert!(
                child.try_wait().unwrap().is_none(),
                "lease child exited before readiness"
            );
            assert!(Instant::now() < deadline, "lease readiness exceeded5s");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn inherited_lease_child() {
        let Ok(raw) = std::env::var("NUMERA_TEST_LEASE_ROOT") else {
            return;
        };
        let root = PathBuf::from(raw);
        if std::env::var("NUMERA_TEST_WRONG_OFD").as_deref() == Ok("1") {
            assert!(sandbox_lease::inherit(&root.join("clone")).is_err());
            std::fs::write(root.join("wrong-ready"), b"rejected").unwrap();
            return;
        }
        let lease = sandbox_lease::inherit(&root.join("clone"))
            .unwrap()
            .unwrap();
        assert!(
            unsafe { fcntl(lease.as_raw_fd(), 1) } & 1 != 0,
            "native lease must be CLOEXEC"
        );
        std::fs::write(root.join("ready"), b"ready").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !root.join("release").is_file() {
            assert!(Instant::now() < deadline, "release signal exceeded5s");
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(lease); // Never unlock the shared open-file description explicitly.
    }

    #[test]
    fn inherited_lease_survives_parent_close_until_native_child_finishes() {
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let root = project
            .join("logs")
            .join(format!("sandbox-lease-{}", std::process::id()));
        std::fs::create_dir_all(root.join("clone")).unwrap();
        let root = root.canonicalize().unwrap();
        let clone = root.join("clone");
        let digest = Sha256::digest(clone.to_str().unwrap().as_bytes());
        let name = digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let leases = root.join(".panel-sandbox-leases");
        std::fs::create_dir(&leases).unwrap();
        let path = leases.join(format!("{name}.sandbox-lifetime.lock"));
        std::fs::write(
            clone.join(".sandbox-owner.json"),
            serde_json::json!({"version":2,"directory":clone}).to_string(),
        )
        .unwrap();
        let file = std::fs::OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        assert_eq!(unsafe { flock(file.as_raw_fd(), 2 | 4) }, 0);
        let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
        let birth = stat
            .rsplit_once(')')
            .unwrap()
            .1
            .split_whitespace()
            .nth(19)
            .unwrap();
        let wrong = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        assert_eq!(unsafe { fcntl(wrong.as_raw_fd(), 2, 0) }, 0);
        let wrong_child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "linux::inherited_lease_child", "--nocapture"])
            .env("NUMERA_TEST_LEASE_ROOT", &root)
            .env("NUMERA_TEST_WRONG_OFD", "1")
            .env("NUMERA_SANDBOX_LEASE_FD", wrong.as_raw_fd().to_string())
            .env("NUMERA_SANDBOX_PARENT_PID", std::process::id().to_string())
            .env("NUMERA_SANDBOX_PARENT_START", birth)
            .spawn()
            .unwrap();
        let mut wrong_owner = Owner(wrong_child, root.join("unused"));
        wait_file(&root.join("wrong-ready"), &mut wrong_owner.0);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = wrong_owner.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "wrong-descriptor child did not finish"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(wrong);
        assert_eq!(unsafe { fcntl(file.as_raw_fd(), 2, 0) }, 0); // Fixture-only handoff.
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "linux::inherited_lease_child", "--nocapture"])
            .env("NUMERA_TEST_LEASE_ROOT", &root)
            .env("NUMERA_SANDBOX_LEASE_FD", file.as_raw_fd().to_string())
            .env("NUMERA_SANDBOX_PARENT_PID", std::process::id().to_string())
            .env("NUMERA_SANDBOX_PARENT_START", birth)
            .spawn()
            .unwrap();
        let mut owner = Owner(child, root.clone());
        wait_file(&root.join("ready"), &mut owner.0);
        drop(file); // Simulate SIGKILL releasing only the wrapper's descriptor.
        let contender = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        assert_eq!(unsafe { flock(contender.as_raw_fd(), 2 | 4) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().kind(),
            std::io::ErrorKind::WouldBlock
        );
        std::fs::write(root.join("release"), b"release").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = owner.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline, "lease child did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(unsafe { flock(contender.as_raw_fd(), 2 | 4) }, 0);
    }
}
