//! Candidate sandbox wrapper; not activation-ready until active-codec and double-failure cleanup gates pass.
//! Inherited close-only lease and parent identity reuse existing process containment.
use crate::{
    logging::RunLog, process_owner::OwnedProcess, tooling_environment, tooling_launch,
    tooling_process, tooling_sandbox::Session,
};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

/// No child can inherit a non-CLOEXEC descriptor from another thread's spawn.
/// Only the already-forked owned child clears its known lease descriptor.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[allow(unsafe_code)]
fn inherit_lease(command: &mut Command, lease: &std::fs::File) -> Result<(), String> {
    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;
    unsafe extern "C" {
        fn fcntl(fd: i32, command: i32, ...) -> i32;
    }
    let fd = lease.as_raw_fd();
    if fd <= 2 {
        return Err("Sandbox lifetime descriptor is invalid".into());
    }
    // SAFETY: fd belongs to the caller's live File. F_GETFD/F_SETFD are
    // async-signal-safe; the closure allocates/logs nothing after fork.
    let flags = unsafe { fcntl(fd, 1) };
    if flags < 0 || flags & 1 == 0 {
        return Err("Sandbox lifetime descriptor must start close-on-exec".into());
    }
    unsafe {
        command.pre_exec(move || {
            let flags = fcntl(fd, 1);
            if flags < 0 || fcntl(fd, 2, flags & !1) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.env("NUMERA_SANDBOX_LEASE_FD", fd.to_string());
    command.env("NUMERA_SANDBOX_PARENT_PID", std::process::id().to_string());
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string("/proc/self/stat")
            .map_err(|_| "Cannot inspect sandbox parent birth identity")?;
        let fields = stat
            .rsplit_once(')')
            .ok_or("Malformed sandbox parent identity")?
            .1
            .split_whitespace()
            .collect::<Vec<_>>();
        let birth = fields
            .get(19)
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .ok_or("Invalid sandbox parent birth identity")?;
        command.env("NUMERA_SANDBOX_PARENT_START", birth.to_string());
    }
    #[cfg(target_os = "macos")]
    {
        let birth = crate::process_owner::process_birth(std::process::id())?
            .ok_or("Cannot inspect Darwin sandbox parent identity")?;
        command.env("NUMERA_SANDBOX_PARENT_START", birth.to_string());
    }
    Ok(())
}

struct Running {
    owner: Option<OwnedProcess>,
    session: Option<Session>,
    _lease: Option<std::fs::File>,
}
impl Running {
    fn stop(&mut self) -> Result<(), String> {
        let stopped = match self.owner.as_mut() {
            Some(owner) => owner.finish(),
            None => Ok(()),
        };
        match stopped {
            Ok(()) => {
                self.owner.take();
                self.session
                    .take()
                    .ok_or("Sandbox session is missing")?
                    .finish()?;
                self._lease.take();
                Ok(())
            }
            // Recovery-only marker was validated before the child existed;
            // this error path never depends on another filesystem write.
            Err(error) => Err(error),
        }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        if self.session.is_some()
            && let Err(error) = self.stop()
        {
            eprintln!(
                "ERROR: {}; sandbox recovery copy retained",
                crate::logging::redact(&error, &[])
            );
        }
    }
}

/// Storage/save lifecycle candidate only: do not activate for operator work yet.
/// Recovery-only marker is established before spawn; active separately-owned
/// codecs still require complete cancellation/parent-death proof.
pub async fn serve(
    root: &Path,
    session: Session,
    options: &tooling_launch::SandboxOptions,
    real_port: u16,
    wall: Duration,
    idle: Duration,
    log: &RunLog,
) -> Result<i32, String> {
    let prepared = (|| {
        if wall.is_zero() || idle.is_zero() || idle > wall {
            return Err("Sandbox needs positive idle <= wall bounds".into());
        }
        if !cfg!(any(windows, target_os = "linux", target_os = "macos")) {
            return Err("Native sandbox lifecycle supports Windows/Linux/macOS only".into());
        }
        let root = tooling_environment::physical_root(root)?;
        let args = tooling_launch::panel_arguments(options, &session.directory, real_port)?;
        let template_path = root.join(".env.example");
        tooling_environment::reject_links(&template_path, "Linked sandbox environment template")?;
        let template = std::fs::read_to_string(&template_path)
            .map_err(|_| "Sandbox needs its public environment template")?;
        let scrubbed = tooling_launch::scrubbed_environment(
            &std::env::vars().collect::<BTreeMap<_, _>>(),
            &template,
        );
        let launch = tooling_launch::plan(&root, &args, &scrubbed)?;
        let directory = tooling_process::workspace(&root)?;
        Ok::<_, String>((root, launch, directory))
    })();
    let (root, launch, directory) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            session.finish()?;
            return Err(error);
        }
    };
    let mut command = Command::new(&launch.argv[0]);
    command
        .args(&launch.argv[1..])
        .current_dir(&root)
        .env_clear()
        .envs(&launch.environment)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for key in [
        "NUMERA_SANDBOX_LEASE_FD",
        "NUMERA_SANDBOX_PARENT_PID",
        "NUMERA_SANDBOX_PARENT_START",
    ] {
        command.env_remove(key);
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let lease = {
        let lease = match session.lease_file() {
            Ok(lease) => lease,
            Err(error) => {
                drop(command);
                tooling_process::clean_unspawned(&directory)?;
                session.finish()?;
                return Err(error);
            }
        };
        if let Err(error) = inherit_lease(&mut command, &lease) {
            drop(command);
            tooling_process::clean_unspawned(&directory)?;
            session.finish()?;
            return Err(error);
        }
        Some(lease)
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let lease = None;
    // Fail before spawning if a reclaimable marker could remain. No stop or
    // cancellation path may need to publish retention evidence after this point.
    if let Err(error) = session.retain_for_recovery() {
        drop(command);
        tooling_process::clean_unspawned(&directory)?;
        session.finish()?;
        return Err(error);
    }
    let owner = match OwnedProcess::spawn(&mut command, directory.clone()) {
        Ok(owner) => owner,
        Err(error) => {
            drop(command);
            // Uncertain spawn leaves the already non-reclaimable recovery copy.
            if !crate::process_owner::cleanup_pending(&directory) {
                session.finish()?;
            }
            tooling_process::clean_unspawned(&directory)?;
            return Err(error);
        }
    };
    drop(command);
    let mut running = Running {
        owner: Some(owner),
        session: Some(session),
        _lease: lease,
    };
    log.event(
        "INFO",
        "sandbox",
        "Native sandbox child started with isolated catalog and scrubbed credentials",
    );
    let result: Result<i32, String> = async {
        let owner = running.owner.as_mut().ok_or("Sandbox process owner is missing")?;
        tokio::select! {
            result = owner.wait(wall, idle) => Ok(result?.code().unwrap_or(1)),
            result = tokio::signal::ctrl_c() => { result.map_err(|_| "Cannot monitor sandbox cancellation")?; Ok(0) }
        }
    }.await;
    running.stop()?;
    log.event(
        "INFO",
        "sandbox",
        "Native sandbox tree stopped and isolated copy removed",
    );
    result
}

pub fn run(
    root: &Path,
    args: &[String],
    real_port: u16,
    wall: Duration,
    idle: Duration,
    log: &RunLog,
) -> Result<u8, String> {
    if args
        .iter()
        .any(|value| matches!(value.as_str(), "--help" | "-h"))
    {
        println!(
            "Usage: numera-emoji panel-sandbox [--source DIR] [--port N] [--all] [--with-pack N] [--bot-username NAME]\nServe a throwaway catalog clone, never its source. Default port is the real panel port plus one."
        );
        return Ok(0);
    }
    if wall.is_zero() || idle.is_zero() || idle > wall {
        return Err("Sandbox needs positive idle <= wall bounds".into());
    }
    let (source, options) = tooling_launch::sandbox_options(args, real_port)?;
    let root = tooling_environment::physical_root(root)?;
    let source = if source.is_absolute() {
        source
    } else {
        root.join(source)
    };
    let scratch = tooling_environment::confined_directory(&root, "logs/tmp", true)?;
    crate::tooling_sandbox::sweep_stale(&scratch)?;
    let mut random = [0; 8];
    getrandom::fill(&mut random).map_err(|_| "Cannot create sandbox session identity")?;
    let name = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Cannot initialize sandbox process monitor")?;
    let session = Session::create(
        &root,
        &source,
        &scratch.join(format!("{}{}", crate::tooling_sandbox::PREFIX, name)),
    )?;
    println!(
        "sandbox catalog: {} items cloned; source catalog is not served",
        session.count
    );
    // Source has no session ceiling. Caller supplies the reviewed automation or
    // operator bound rather than silently introducing another product default.
    let code = runtime.block_on(serve(&root, session, &options, real_port, wall, idle, log))?;
    u8::try_from(code).map_err(|_| "Sandbox child exit code is outside0..255".into())
}
