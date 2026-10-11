//! Successful short children retain ownership while Darwin tears down task accounting.
#[cfg(target_os = "macos")]
#[test]
fn successful_children_reap_exit_transition_before_removing_scratch() {
    use _native::process_owner::OwnedProcess;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let python = std::env::var_os("PYO3_PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|| project.join(".venv/bin/python"));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    // Exit timing is nondeterministic; a bounded set exercises actual fast process teardown.
    for index in 0..12 {
        let scratch = project
            .join("logs")
            .join(format!("darwin-exit-{}-{index}", std::process::id()));
        std::fs::create_dir(&scratch).unwrap();
        let mut command = Command::new(&python);
        command
            .args(["-c", "pass"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut owner = OwnedProcess::spawn(&mut command, scratch.clone()).unwrap();
        assert!(
            runtime
                .block_on(owner.wait(Duration::from_secs(5), Duration::from_secs(3)))
                .unwrap()
                .success()
        );
        owner.finish().unwrap();
        assert!(!scratch.exists());
    }
}
