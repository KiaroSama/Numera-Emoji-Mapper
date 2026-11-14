//! Read-only public command refuses invalid arguments and occupied output before reading Git.
use _native::process_owner::OwnedProcess;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
fn source_inventory_public_help_and_refusals_leave_output_untouched() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let scratch = parent.join(format!("source inventory cli {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    std::fs::create_dir(scratch.join("assets")).unwrap();
    std::fs::write(scratch.join("assets/panel.html"), b"isolated fixture").unwrap();
    std::fs::write(scratch.join("pyproject.toml"), b"# isolated fixture\n").unwrap();
    let executable = scratch.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-compat"), &executable).unwrap();
    let output = scratch.join("occupied.json");
    std::fs::write(&output, b"preserved").unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut environment = std::collections::BTreeMap::new();
    for name in [
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
        "DYLD_FALLBACK_LIBRARY_PATH",
        "PYTHONHOME",
    ] {
        if let Ok(value) = std::env::var(name) {
            environment.insert(name.to_owned(), value);
        }
    }
    environment.insert("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into());
    environment.insert("PYTHONNOUSERSITE".into(), "1".into());
    environment.insert("PYTHONSAFEPATH".into(), "1".into());
    let environment =
        _native::tooling_environment::environment(&scratch, &environment, false).unwrap();
    let cases: Vec<(Vec<String>, i32, &str)> = vec![
        (vec!["--help".into()], 0, "Read pinned Git Python sources"),
        (vec!["--bad".into()], 2, "unknown source inventory option"),
        (vec!["--baseline".into()], 2, "requires a value"),
        (vec![], 2, "requires --baseline"),
        (
            vec!["--baseline".into(), "HEAD".into()],
            2,
            "exact lowercase40hex",
        ),
        (
            vec![
                "--baseline".into(),
                "a".repeat(40),
                "--output".into(),
                output.to_str().unwrap().into(),
            ],
            2,
            "output already exists",
        ),
        (
            vec![
                "--baseline".into(),
                "a".repeat(40),
                "--baseline".into(),
                "a".repeat(40),
            ],
            2,
            "duplicate source inventory",
        ),
    ];
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for (index, (args, expected, text)) in cases.into_iter().enumerate() {
            let owner_path = scratch.join(format!("owner{index}"));
            std::fs::create_dir(&owner_path).unwrap();
            let out = scratch.join(format!("out{index}"));
            let err = scratch.join(format!("err{index}"));
            let mut command = Command::new(&executable);
            command
                .arg("source-inventory")
                .args(args)
                .current_dir(&scratch)
                .env_clear()
                .envs(&environment)
                .stdin(Stdio::null())
                .stdout(Stdio::from(std::fs::File::create(&out).unwrap()))
                .stderr(Stdio::from(std::fs::File::create(&err).unwrap()));
            let mut owner = OwnedProcess::spawn(&mut command, owner_path.clone()).unwrap();
            drop(command);
            let status = runtime
                .block_on(owner.wait(Duration::from_secs(5), Duration::from_secs(3)))
                .unwrap();
            owner.finish().unwrap();
            assert_eq!(status.code(), Some(expected));
            let output_text =
                std::fs::read_to_string(out).unwrap() + &std::fs::read_to_string(err).unwrap();
            assert!(output_text.contains(text), "{output_text}");
            assert!(!owner_path.exists());
            assert_eq!(std::fs::read(&output).unwrap(), b"preserved");
        }
    }));
    // Unverified ownership or a failed assertion retains the exact input and captures.
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    std::fs::remove_dir_all(scratch).unwrap();
}
