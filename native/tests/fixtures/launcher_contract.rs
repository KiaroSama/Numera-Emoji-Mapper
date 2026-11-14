//! Dependency-free regression for the real launcher in Windows PowerShell 5.1.
#[cfg(windows)]
mod windows {
    use std::ffi::OsString;
    use std::fs::{File, OpenOptions};
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    fn stub(executable: &Path) -> Result<u8, String> {
        let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
        if arguments != [OsString::from("doctor")] {
            return Err(format!(
                "launcher passed {arguments:?}; expected exactly [doctor]"
            ));
        }
        let package = executable.parent().ok_or("stub has no package directory")?;
        if std::env::current_dir()
            .map_err(|e| e.to_string())?
            .canonicalize()
            .map_err(|e| e.to_string())?
            != package.canonicalize().map_err(|e| e.to_string())?
        {
            return Err("launcher did not select its package working directory".into());
        }
        let extensions = std::env::var("PATHEXT").unwrap_or_default();
        if !extensions
            .split(';')
            .any(|value| value.eq_ignore_ascii_case(".EXE"))
        {
            return Err("native child did not receive executable .EXE recognition".into());
        }
        let mut record = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(package.join("invocation.txt"))
            .map_err(|e| e.to_string())?;
        record.write_all(b"doctor").map_err(|e| e.to_string())?;
        record.sync_all().map_err(|e| e.to_string())?;
        println!("NUMERA_LAUNCHER_FIXTURE_ARGS={arguments:?}");
        println!("[OK] Environment check complete.");
        std::io::stdout().flush().map_err(|e| e.to_string())?;
        std::env::var("NUMERA_LAUNCHER_FIXTURE_EXIT")
            .unwrap_or_else(|_| "0".into())
            .parse::<u8>()
            .map_err(|e| e.to_string())
    }

    fn capture(path: &Path) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(|e| e.to_string())?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 1024 * 1024 {
            return Err("launcher output exceeded 1MiB".into());
        }
        Ok(bytes)
    }

    fn probe(
        root: &Path,
        executable: &Path,
        script: &Path,
        label: &str,
        path_ext: Option<&str>,
        exit: u8,
    ) -> Result<bool, String> {
        let root = root.join(label);
        let package = root.join("owned package with spaces");
        let cwd = root.join("unrelated working directory");
        let empty_path = root.join("empty PATH");
        for directory in [&package, &cwd, &empty_path] {
            std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        }
        std::fs::copy(executable, package.join("numera-emoji.exe")).map_err(|e| e.to_string())?;
        std::fs::copy(script, package.join("run.ps1")).map_err(|e| e.to_string())?;
        // Invoke the unchanged shipped script, then observe restored state in
        // the same PowerShell process. The package test also uses -File directly.
        let harness = root.join("scope-contract.ps1");
        std::fs::write(
            &harness,
            r#"param([string]$Launcher)
$before = [Environment]::GetEnvironmentVariable('PATHEXT', [EnvironmentVariableTarget]::Process)
$location = $ExecutionContext.SessionState.Path.CurrentLocation.Path
& $Launcher -NoRelaunch -Check
$forwarded = [int]$LASTEXITCODE
$after = [Environment]::GetEnvironmentVariable('PATHEXT', [EnvironmentVariableTarget]::Process)
if (-not [String]::Equals($before, $after, [StringComparison]::Ordinal)) {
    [Console]::Error.WriteLine('PATHEXT was not restored exactly')
    exit 98
}
if ($location -cne $ExecutionContext.SessionState.Path.CurrentLocation.Path) {
    [Console]::Error.WriteLine('PowerShell working location was not restored')
    exit 97
}
[Console]::Out.WriteLine('NUMERA_SCOPED_STATE_RESTORED')
exit $forwarded
"#,
        )
        .map_err(|e| e.to_string())?;
        let powershell =
            PathBuf::from(std::env::var_os("SystemRoot").ok_or("Windows SystemRoot is missing")?)
                .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let stdout_path = root.join("stdout.bin");
        let stderr_path = root.join("stderr.bin");
        let create = |path| {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|e| e.to_string())
        };
        let mut command = Command::new(powershell);
        command
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
            .arg(harness)
            .arg(package.join("run.ps1"))
            .current_dir(cwd)
            .env_clear()
            .env("PATH", &empty_path)
            .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
            .env("NUMERA_LAUNCHER_FIXTURE_EXIT", exit.to_string())
            .env("PYO3_PYTHON", empty_path.join("python-must-not-run"))
            .env("PYTHONHOME", empty_path.join("python-home-must-not-exist"))
            .env(
                "PYTHONPATH",
                empty_path.join("python-modules-must-not-exist"),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::from(create(&stdout_path)?))
            .stderr(Stdio::from(create(&stderr_path)?));
        for key in ["SystemRoot", "WINDIR", "SYSTEMDRIVE"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        if let Some(value) = path_ext {
            command.env("PATHEXT", value);
        }
        let started = Instant::now();
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        drop(command);
        let mut exceeded = false;
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break status;
            }
            let oversized = [&stdout_path, &stderr_path]
                .iter()
                .any(|path| std::fs::metadata(path).is_ok_and(|m| m.len() > 1024 * 1024));
            // Cold PS 5.1 startup reached 29s with this deliberately minimal
            // environment on hosted CI. Keep a finite bound with startup margin.
            if started.elapsed() >= Duration::from_secs(45) || oversized {
                exceeded = true;
                let _ = child.kill();
                break child.wait().map_err(|e| e.to_string())?;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        drop(child);
        let stdout = capture(&stdout_path)?;
        let stderr = capture(&stderr_path)?;
        let invocation = std::fs::read(package.join("invocation.txt")).unwrap_or_default();
        println!(
            "case={label} original_PATHEXT={path_ext:?} expected_exit={exit} actual_exit={:?} elapsed_ms={} argv={:?}",
            status.code(),
            started.elapsed().as_millis(),
            String::from_utf8_lossy(&invocation)
        );
        for (name, bytes) in [("stdout", &stdout), ("stderr", &stderr)] {
            println!(
                "{name}_len={} {name}_prefix={:02x?} {name}_text={:?}",
                bytes.len(),
                &bytes[..bytes.len().min(128)],
                String::from_utf8_lossy(&bytes[..bytes.len().min(8192)])
            );
        }
        let output = String::from_utf8_lossy(&stdout);
        let passed = !exceeded
            && status.code() == Some(i32::from(exit))
            && invocation == b"doctor"
            && output.contains("Environment check complete")
            && output.contains("NUMERA_SCOPED_STATE_RESTORED")
            && std::fs::read_dir(&empty_path)
                .map_err(|e| e.to_string())?
                .next()
                .is_none();
        if passed {
            if let Err(error) = std::fs::remove_dir_all(&root) {
                eprintln!(
                    "Passed fixture retained at {} because cleanup was refused: {error}",
                    root.display()
                );
            }
        } else {
            eprintln!(
                "Failed fixture retained without broad deletion at {}",
                root.display()
            );
        }
        Ok(passed)
    }

    fn controller(executable: &Path) -> Result<u8, String> {
        let script = std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .ok_or("usage: launcher-contract.exe ABSOLUTE_RUN_PS1")?;
        if !script.is_absolute() || !script.is_file() {
            return Err("real absolute run.ps1 path required".into());
        }
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "numera launcher café {} {stamp}",
            std::process::id()
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let mut passed = true;
        for (label, extensions, exit) in [
            ("unset", None, 0),
            ("missing-exe-nonzero", Some(".CMD;.BAT"), 7),
            ("existing-exe", Some(".eXe;.COM"), 0),
        ] {
            passed &= probe(&root, executable, &script, label, extensions, exit)?;
        }
        if let Err(error) = std::fs::remove_dir(&root) {
            eprintln!(
                "Owned fixture evidence retained at {}: {error}",
                root.display()
            );
        }
        if !passed {
            return Err(
                "real PowerShell launcher doctor/output/exit or scoped-state contract failed"
                    .into(),
            );
        }
        println!(
            "Real Windows PowerShell launcher passed unset/missing/existing .EXE, scoped restoration and nonzero exit propagation with empty PATH and invalid Python overrides"
        );
        Ok(0)
    }

    pub fn run() -> Result<u8, String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        if executable
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("numera-emoji.exe"))
        {
            stub(&executable)
        } else {
            controller(&executable)
        }
    }
}

fn main() -> std::process::ExitCode {
    #[cfg(windows)]
    match windows::run() {
        Ok(code) => code.into(),
        Err(error) => {
            eprintln!("ERROR: {error}");
            1.into()
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("launcher-contract requires native Windows PowerShell");
        1.into()
    }
}
