use crate::{logging::RunLog, tooling_environment, tooling_process};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[cfg(target_os = "linux")]
pub(super) fn current_process_start() -> Result<u64, String> {
    std::fs::read_to_string("/proc/self/stat")
        .map_err(|_| "cannot inspect sandbox parent lifetime")?
        .rsplit_once(')')
        .and_then(|(_, rest)| rest.split_whitespace().nth(19))
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| "invalid sandbox parent lifetime".into())
}

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn sandbox_lease(
    command: &mut std::process::Command,
    session: &crate::tooling_sandbox::Session,
) -> Result<Option<std::fs::File>, String> {
    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;
    let file = session.lease_file()?;
    let fd = file.as_raw_fd();
    // Only this already-forked child clears CLOEXEC. Other concurrent spawns
    // cannot inherit another session's lifetime descriptor.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if fd <= 2 || flags < 0 || flags & libc::FD_CLOEXEC == 0 {
        return Err("sandbox lease must begin close-on-exec".into());
    }
    unsafe {
        command.pre_exec(move || {
            let flags = libc::fcntl(fd, libc::F_GETFD);
            if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let birth = current_process_start()?;
    command
        .env("NUMERA_SANDBOX_LEASE_FD", fd.to_string())
        .env("NUMERA_SANDBOX_PARENT_PID", std::process::id().to_string())
        .env("NUMERA_SANDBOX_PARENT_START", birth.to_string());
    Ok(Some(file))
}
#[cfg(windows)]
fn sandbox_lease(
    _command: &mut std::process::Command,
    _session: &crate::tooling_sandbox::Session,
) -> Result<Option<std::fs::File>, String> {
    Ok(None)
}

pub(super) fn sandbox(
    root: &Path,
    args: &[String],
    timeout: Duration,
    log: &RunLog,
) -> Result<i32, String> {
    use crate::tooling_sandbox::{self, Session};
    if std::env::var("NUMERA_DEV_OWNED_WORKER").as_deref() != Ok("1") {
        return Err("sandbox requires the dedicated owned developer worker".into());
    }
    #[cfg(target_os = "linux")]
    {
        let pid = std::env::var("NUMERA_SANDBOX_PARENT_PID")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| *value > 0);
        let birth = std::env::var("NUMERA_SANDBOX_PARENT_START")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0);
        if pid.is_none() || birth.is_none() {
            return Err("sandbox controller lifetime is missing or invalid".into());
        }
    }
    let cancel_path = std::env::var_os("NUMERA_SANDBOX_CANCEL_PATH")
        .map(PathBuf::from)
        .ok_or("sandbox cancellation ownership is missing")?;
    tooling_environment::reject_links(&cancel_path, "sandbox cancellation cannot follow links")?;
    let control = cancel_path
        .parent()
        .ok_or("sandbox cancellation has no owner directory")?;
    let expected = tooling_environment::confined_directory(root, "logs/tmp/tooling", false)?;
    if !control.starts_with(&expected)
        || cancel_path.file_name().is_none_or(|name| name != "stop")
        || !control.join("sandbox-controller.json").is_file()
    {
        return Err("sandbox cancellation is outside its owning controller".into());
    }
    let real_port = crate::panel_session::DEFAULT_PORT;
    let (source, options) = crate::tooling_launch::sandbox_options(args, real_port)?;
    let source = if source.is_absolute() {
        source
    } else {
        root.join(source)
    };
    let scratch = tooling_environment::confined_directory(root, "logs/tmp", true)?;
    tooling_sandbox::sweep_stale(&scratch)?;
    let mut random = [0; 16];
    getrandom::fill(&mut random).map_err(|_| "cannot create sandbox identity")?;
    let identity = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let session = Session::create(
        root,
        &source,
        &scratch.join(format!("{}{}", tooling_sandbox::PREFIX, identity)),
    )?;
    for warning in &session.warnings {
        log.event("WARNING", "sandbox", warning);
    }
    println!(
        "sandbox catalog: {} items cloned; source catalog is not served",
        session.count
    );
    let prepared = (|| -> Result<_, String> {
        let arguments =
            crate::tooling_launch::panel_arguments(&options, &session.directory, real_port)?;
        let template = root.join(".env.example");
        tooling_environment::reject_links(&template, "sandbox template cannot follow links")?;
        let template = std::fs::read_to_string(template)
            .map_err(|_| "sandbox requires its public environment template")?;
        let scrubbed =
            crate::tooling_launch::scrubbed_environment(&std::env::vars().collect(), &template);
        let launch = crate::tooling_launch::plan(root, &arguments, &scrubbed)?;
        let directory = tooling_process::workspace(root)?;
        std::fs::create_dir(directory.join("owner")).map_err(|error| error.to_string())?;
        let mut command = std::process::Command::new(&launch.argv[0]);
        command
            .args(&launch.argv[1..])
            .current_dir(root)
            .env_clear()
            .envs(&launch.environment)
            .stdin(std::process::Stdio::null())
            .stdout(
                std::fs::File::create(directory.join("stdout"))
                    .map_err(|error| error.to_string())?,
            )
            .stderr(
                std::fs::File::create(directory.join("stderr"))
                    .map_err(|error| error.to_string())?,
            );
        for key in [
            "NUMERA_SANDBOX_LEASE_FD",
            "NUMERA_SANDBOX_PARENT_PID",
            "NUMERA_SANDBOX_PARENT_START",
            "NUMERA_DEV_OWNED_WORKER",
            "NUMERA_SANDBOX_CANCEL_PATH",
        ] {
            command.env_remove(key);
        }
        command.env("NUMERA_SANDBOX_OWNED", "1");
        let lease = sandbox_lease(&mut command, &session)?;
        session.retain_for_recovery()?;
        Ok((directory, command, lease))
    })();
    let (directory, mut command, lease) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            return match session.finish() {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!("{error}; sandbox cleanup also failed: {cleanup}")),
            };
        }
    };
    // This process is a dedicated one-child owner, so Linux subreaper custody
    // includes independently owned codec groups without sharing another owner.
    let mut owner = match crate::process_owner::OwnedProcess::spawn_descendants(
        &mut command,
        directory.join("owner"),
    ) {
        Ok(owner) => owner,
        Err(error) => {
            drop(command);
            if crate::process_owner::cleanup_pending(&directory) {
                return Err(format!("{error}; sandbox recovery copy retained"));
            }
            let cleanup = session.finish();
            tooling_process::clean_unspawned(&directory)?;
            return match cleanup {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!("{error}; sandbox cleanup also failed: {cleanup}")),
            };
        }
    };
    drop(command);
    let mut interrupt = crate::tooling_interrupt::Interrupt::install()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let mut controller_died = false;
    let result = runtime.block_on(async {
        tokio::select! {
            biased;
            // The foreground worker has its own session. The existing parent
            // PID/birth watcher must settle the actual panel even if its outer
            // controller dies without being able to signal the stop channel.
            _ = crate::panel_session::sandbox_parent_ended() => {controller_died=true;Ok(130)},
            result = owner.wait(timeout,timeout) => result.map(|status|status.code().unwrap_or(1)),
            result = async {
                loop {
                    tooling_environment::reject_links(&cancel_path,"sandbox cancellation cannot follow links")?;
                    if cancel_path.is_file() { return Ok::<(),String>(()); }
                    tokio::select! {
                        result = interrupt.wait() => return result,
                        _ = tokio::time::sleep(Duration::from_millis(25)) => {},
                    }
                }
            } => result.map(|()|130),
            result = async {
                loop {
                    for name in ["stdout","stderr"] {
                        if std::fs::metadata(directory.join(name)).map_err(|error|error.to_string())?.len()>8*1024*1024 { return Err::<i32,String>("sandbox output exceeded 8MiB capture budget".into()); }
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            } => result,
        }
    });
    let stopped = owner.finish();
    interrupt.close()?;
    drop(owner);
    match stopped {
        Err(cleanup) => match result {
            Err(operation) => Err(format!(
                "{operation}; sandbox cleanup also failed: {cleanup}; recovery copy retained"
            )),
            Ok(_) => Err(format!("{cleanup}; sandbox recovery copy retained")),
        },
        Ok(()) => {
            for name in ["stdout", "stderr"] {
                use std::io::Read;
                let mut bytes = Vec::new();
                std::fs::File::open(directory.join(name))
                    .map_err(|error| error.to_string())?
                    .take(8 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|error| error.to_string())?;
                if bytes.len() > 8 * 1024 * 1024 {
                    return Err("sandbox diagnostics exceeded capture budget; copy retained".into());
                }
                let text = String::from_utf8(bytes)
                    .map_err(|_| "sandbox diagnostics are not UTF-8; copy retained")?;
                let text = crate::logging::redact(&text, &[]);
                if name == "stderr" {
                    eprint!("{text}");
                } else {
                    print!("{text}");
                }
            }
            if controller_died {
                drop(lease);
                tooling_process::clean_unspawned(&directory)?;
                return Err("sandbox controller exited; recovery copy retained".into());
            }
            let cleaned = session.finish();
            drop(lease);
            tooling_process::clean_unspawned(&directory)?;
            match (result, cleaned) {
                (Err(operation), Err(cleanup)) => Err(format!(
                    "{operation}; sandbox cleanup also failed: {cleanup}"
                )),
                (Err(error), _) | (_, Err(error)) => Err(error),
                (Ok(code), Ok(())) => Ok(code),
            }
        }
    }
}
