//! Fast, dependency-free Windows PowerShell contract for the real shipped launcher.
#[cfg(windows)]
mod windows {
    use std::ffi::OsString;
    use std::fs::{File, OpenOptions};
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    fn native_stub(executable: &Path) -> Result<(), String> {
        let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
        if arguments != [OsString::from("doctor")] {
            return Err(format!(
                "launcher passed {arguments:?}; expected exactly [doctor]"
            ));
        }
        let package = executable.parent().ok_or("stub has no package directory")?;
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        if cwd.canonicalize().map_err(|e| e.to_string())?
            != package.canonicalize().map_err(|e| e.to_string())?
        {
            return Err("launcher did not select its package working directory".into());
        }
        let mut invocation = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(package.join("invocation.txt"))
            .map_err(|e| e.to_string())?;
        invocation.write_all(b"doctor").map_err(|e| e.to_string())?;
        invocation.sync_all().map_err(|e| e.to_string())?;
        println!("NUMERA_LAUNCHER_FIXTURE_ARGS={arguments:?}");
        println!("[OK] Environment check complete.");
        std::io::stdout().flush().map_err(|e| e.to_string())?;
        Ok(())
    }

    fn capture(path: &Path) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(|e| e.to_string())?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 1024 * 1024 {
            return Err("launcher fixture exceeded its output bound".into());
        }
        Ok(bytes)
    }

    fn diagnostic(label: &str, bytes: &[u8]) {
        println!(
            "{label}_len={} {label}_prefix={:02x?} {label}_text={:?}",
            bytes.len(),
            &bytes[..bytes.len().min(128)],
            String::from_utf8_lossy(&bytes[..bytes.len().min(8192)])
        );
    }

    #[derive(Clone, Copy, Debug)]
    enum Mode {
        Host,
        File,
        Native,
    }

    fn probe(
        parent: &Path,
        executable: &Path,
        script: &str,
        label: &str,
        additional: &[&str],
        mode: Mode,
    ) -> Result<bool, String> {
        let root = parent.join(label);
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let package = root.join("owned package with spaces");
        let cwd = root.join("unrelated working directory");
        let empty_path = root.join("empty PATH");
        let temporary = root.join("owned Windows temporary files");
        for path in [&package, &cwd, &empty_path, &temporary] {
            std::fs::create_dir(path).map_err(|e| e.to_string())?;
        }
        std::fs::copy(executable, package.join("numera-emoji.exe")).map_err(|e| e.to_string())?;
        let launcher = package.join("run.ps1");
        std::fs::write(&launcher, script).map_err(|e| e.to_string())?;
        let system_root = std::env::var_os("SystemRoot").ok_or("Windows SystemRoot is missing")?;
        let powershell =
            PathBuf::from(system_root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let stdout_path = root.join("stdout.bin");
        let stderr_path = root.join("stderr.bin");
        let create = |path| {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|e| e.to_string())
        };
        let mut command = if matches!(mode, Mode::Native) {
            Command::new(package.join("numera-emoji.exe"))
        } else {
            Command::new(&powershell)
        };
        match mode {
            Mode::Native => {
                command.arg("doctor");
            }
            Mode::File => {
                command
                    .args(["-NoLogo", "-NoProfile", "-NonInteractive"])
                    .arg("-File")
                    .arg(launcher)
                    .args(["-NoRelaunch", "-Check"]);
            }
            Mode::Host => {
                command
                    .args(["-NoLogo", "-NoProfile", "-NonInteractive"])
                    .args([
                        "-Command",
                        "[Console]::Out.WriteLine('NUMERA_POWERSHELL_HOST_READY')",
                    ]);
            }
        }
        command
            .current_dir(if matches!(mode, Mode::Native) {
                &package
            } else {
                &cwd
            })
            .env_clear()
            .env("PATH", &empty_path)
            .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
            .env("PYO3_PYTHON", empty_path.join("python-must-not-run"))
            .env("PYTHONHOME", empty_path.join("python-home-must-not-exist"))
            .env(
                "PYTHONPATH",
                empty_path.join("python-modules-must-not-exist"),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::from(create(&stdout_path)?))
            .stderr(Stdio::from(create(&stderr_path)?));
        for key in ["SystemRoot", "WINDIR", "SYSTEMDRIVE"]
            .into_iter()
            .chain(additional.iter().copied())
        {
            if matches!(key, "TEMP" | "TMP") {
                command.env(key, &temporary);
            } else if key == "PATHEXT" {
                command.env(key, ".EXE");
            } else if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        println!("probe={label} mode={mode:?} additional_OS_keys={additional:?}");
        let started = Instant::now();
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        // Command retains the parent copies of redirected files after spawn.
        // Release them now so Windows can delete the settled fixture later.
        drop(command);
        let mut timed_out = false;
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break status;
            }
            let oversized = [&stdout_path, &stderr_path]
                .iter()
                .any(|path| std::fs::metadata(path).is_ok_and(|m| m.len() > 1024 * 1024));
            if started.elapsed() >= Duration::from_secs(30) || oversized {
                timed_out = true;
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
            "probe={label} powershell_exit={:?} elapsed_ms={} invoked_argv={:?}",
            status.code(),
            started.elapsed().as_millis(),
            String::from_utf8_lossy(&invocation)
        );
        diagnostic("stdout", &stdout);
        diagnostic("stderr", &stderr);
        if timed_out {
            eprintln!(
                "probe={label} exceeded 30s/output bound; parent reaped, owned fixture retained at {}",
                root.display()
            );
            return Ok(false);
        }
        let text = String::from_utf8_lossy(&stdout);
        let passed = status.success()
            && match mode {
                Mode::File => {
                    invocation == b"doctor"
                        && text.contains("Environment check complete")
                        && text.contains("NUMERA_LAUNCHER_SCRIPT_ENTERED")
                }
                Mode::Native => {
                    invocation == b"doctor" && text.contains("Environment check complete")
                }
                Mode::Host => text.contains("NUMERA_POWERSHELL_HOST_READY"),
            };
        // The real script invokes this finite, non-forking stub synchronously.
        // Both processes and all parent capture handles are settled before cleanup.
        if !passed {
            eprintln!(
                "probe={label} did not prove settled native execution; owned fixture retained without deletion at {}",
                root.display()
            );
        } else if let Err(error) = std::fs::remove_dir_all(&root) {
            eprintln!(
                "probe={label} output passed but cleanup was refused; owned fixture retained at {}: {error}",
                root.display()
            );
        }
        Ok(passed)
    }

    fn controller(executable: &Path) -> Result<(), String> {
        let path = std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .ok_or("usage: launcher-contract.exe ABSOLUTE_RUN_PS1")?;
        if !path.is_absolute() || !path.is_file() {
            return Err("launcher contract requires the real absolute run.ps1 path".into());
        }
        let script = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let marker = "$ErrorActionPreference = 'Stop'";
        if script.matches(marker).count() != 1 {
            return Err(
                "real launcher startup marker changed; update the diagnostic fixture".into(),
            );
        }
        // Trace only the owned fixture copy. The actual schema2 test runs the
        // shipped script unchanged and still requires the real native doctor.
        let mut script = script.replace(
            marker,
            &format!("[Console]::Out.WriteLine('NUMERA_LAUNCHER_SCRIPT_ENTERED')\n{marker}"),
        );
        for (before, after) in [
            (
                "$candidates = @(",
                "[Console]::Out.WriteLine('NUMERA_TRACE_BEFORE_CANDIDATES')\n$candidates = @(",
            ),
            (
                "$binary = $null",
                "[Console]::Out.WriteLine('NUMERA_TRACE_AFTER_CANDIDATES')\n$binary = $null",
            ),
            (
                "foreach ($candidate in $candidates) {",
                "foreach ($candidate in $candidates) {\n[Console]::Out.WriteLine(\"NUMERA_TRACE_BEFORE_TEST_PATH candidate=$candidate\")",
            ),
            (
                "$item = Get-Item -LiteralPath $candidate -Force",
                "[Console]::Out.WriteLine('NUMERA_TRACE_BEFORE_GET_ITEM')\n$item = Get-Item -LiteralPath $candidate -Force\n[Console]::Out.WriteLine('NUMERA_TRACE_AFTER_GET_ITEM')",
            ),
            (
                "if ($Check) {",
                "[Console]::Out.WriteLine(\"NUMERA_TRACE_CHECK check=$Check\")\nif ($Check) {",
            ),
            (
                "Push-Location -LiteralPath $ScriptRoot",
                "[Console]::Out.WriteLine(\"NUMERA_TRACE_BEFORE_PUSH_LOCATION args=$($CommandArguments -join ',')\")\nPush-Location -LiteralPath $ScriptRoot\n[Console]::Out.WriteLine('NUMERA_TRACE_AFTER_PUSH_LOCATION')",
            ),
            (
                "& $binary @CommandArguments",
                "[Console]::Out.WriteLine(\"NUMERA_TRACE_BEFORE_NATIVE_INVOKE binary=$binary args=$($CommandArguments -join ',')\")\n& $binary @CommandArguments\n[Console]::Out.WriteLine(\"NUMERA_TRACE_AFTER_NATIVE_INVOKE last_exit=$LASTEXITCODE\")",
            ),
        ] {
            if script.matches(before).count() != 1 {
                return Err(format!("real launcher trace marker changed: {before}"));
            }
            script = script.replace(before, after);
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
        let scenarios: &[(&str, &[&str])] = &[
            (
                "windows-host",
                &[
                    "TEMP",
                    "TMP",
                    "USERPROFILE",
                    "APPDATA",
                    "LOCALAPPDATA",
                    "HOMEDRIVE",
                    "HOMEPATH",
                    "PATHEXT",
                    "ComSpec",
                    "ProgramData",
                    "ProgramFiles",
                    "ProgramFiles(x86)",
                    "ProgramW6432",
                    "CommonProgramFiles",
                    "CommonProgramFiles(x86)",
                    "CommonProgramW6432",
                    "PSModulePath",
                    "OS",
                    "PROCESSOR_ARCHITECTURE",
                    "NUMBER_OF_PROCESSORS",
                    "USERNAME",
                    "USERDOMAIN",
                    "PUBLIC",
                    "ALLUSERSPROFILE",
                ],
            ),
            ("pathext-only", &["PATHEXT"]),
            ("minimal", &[]),
        ];
        let mut complete = false;
        for (name, keys) in scenarios {
            let native = probe(
                &root,
                executable,
                &script,
                &format!("{name}-native"),
                keys,
                Mode::Native,
            )?;
            let host = probe(
                &root,
                executable,
                &script,
                &format!("{name}-command"),
                keys,
                Mode::Host,
            )?;
            let file = {
                probe(
                    &root,
                    executable,
                    &script,
                    &format!("{name}-file"),
                    keys,
                    Mode::File,
                )?
            };
            println!(
                "scenario={name} direct_native_verified={native} host_ready={host} actual_script_doctor_verified={file}"
            );
            if *name == "windows-host" {
                complete = native && host && file;
            }
        }
        if let Err(error) = std::fs::remove_dir(&root) {
            eprintln!(
                "Diagnostic probes retained at {} (no broad cleanup attempted): {error}",
                root.display()
            );
        }
        if !complete {
            return Err("real Windows PowerShell launcher did not forward doctor and its completion output with the Windows host prerequisites".into());
        }
        println!(
            "Windows PowerShell host, launcher argv=doctor and completion output verified; PATH stayed empty and Python overrides invalid in every probe"
        );
        Ok(())
    }

    pub fn run() -> Result<(), String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        if executable
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("numera-emoji.exe"))
        {
            native_stub(&executable)
        } else {
            controller(&executable)
        }
    }
}

fn main() -> std::process::ExitCode {
    #[cfg(windows)]
    match windows::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ERROR: {error}");
            1.into()
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("launcher-contract fixture requires native Windows PowerShell");
        1.into()
    }
}
