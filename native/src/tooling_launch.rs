//! Installed-runtime launch policy: native argv only, package-local codec environment, no Python wrapper.
use crate::{logging::RunLog, tooling_environment, tooling_process};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct Launch {
    pub root: PathBuf,
    pub argv: Vec<OsString>,
    pub environment: BTreeMap<String, String>,
}

/// PYO3_PYTHON preserves source subprocess selection, but is not proof that an
/// embedded interpreter uses this venv. Do not switch the installed launcher
/// until explicit runtime home/site-package selection is verified on all targets.
pub fn plan(
    root: &Path,
    args: &[String],
    original: &BTreeMap<String, String>,
) -> Result<Launch, String> {
    let root = tooling_environment::physical_root(root)?;
    tooling_environment::confined_directory(&root, ".venv", false)?;
    tooling_environment::confined_directory(
        &root,
        if cfg!(windows) {
            ".venv/Scripts"
        } else {
            ".venv/bin"
        },
        false,
    )?;
    tooling_environment::confined_directory(&root, "native/runtime", false)?;
    let executable = root.join(if cfg!(windows) {
        "native/runtime/numera-emoji.exe"
    } else {
        "native/runtime/numera-emoji"
    });
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    tooling_environment::reject_links(&executable, "Linked native executable is not supported")?;
    // POSIX venv intentionally links its interpreter to a base runtime; don't
    // reject that external dependency as a cache/temporary-directory escape.
    if cfg!(windows) {
        tooling_environment::reject_links(&python, "Linked Windows interpreter is not supported")?;
    }
    if !executable.is_file() || !python.is_file() {
        return Err(
            "Prebuilt runtime is not installed; install a verified prebuilt package first.".into(),
        );
    }
    let mut environment = tooling_environment::environment(&root, original, false)?;
    environment.insert(
        "PYO3_PYTHON".into(),
        python
            .to_str()
            .ok_or("Package Python path is not UTF-8")?
            .into(),
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    let mut argv = vec![executable.into_os_string()];
    argv.extend(args.iter().map(OsString::from));
    Ok(Launch {
        root,
        argv,
        environment,
    })
}

/// Bounded automation launch; menu/panel Ctrl+C and embedded-interpreter selection require their own installed proof.
pub async fn run(
    launch: &Launch,
    wall: Duration,
    idle: Duration,
    log: &RunLog,
) -> Result<tooling_process::Completed, String> {
    tooling_process::capture(
        &launch.root,
        &launch.argv,
        &launch.environment,
        wall,
        idle,
        log,
    )
    .await
}

/// Foreground user action, not an automation runner. The caller owns cancellation;
/// automated callers must additionally provide their normal wall/idle deadline.
pub async fn foreground(
    launch: &Launch,
    cancel: impl std::future::Future<Output = Result<(), String>>,
    log: &RunLog,
) -> Result<i32, String> {
    use crate::process_owner::OwnedProcess;
    use std::process::{Command, Stdio};
    let executable = launch.argv.first().ok_or("launcher command is empty")?;
    let root = tooling_environment::physical_root(&launch.root)?;
    let directory = tooling_process::workspace(&root)?;
    let owner_directory = directory.join("owner");
    if std::fs::create_dir(&owner_directory).is_err() {
        tooling_process::clean_unspawned(&directory)?;
        return Err("cannot create foreground ownership workspace".into());
    }
    let mut command = Command::new(executable);
    command
        .args(&launch.argv[1..])
        .current_dir(&root)
        .env_clear()
        .envs(&launch.environment)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let mut owner = match OwnedProcess::spawn_console(&mut command, owner_directory) {
        Ok(owner) => owner,
        Err(error) => {
            drop(command);
            tooling_process::clean_unspawned(&directory)?;
            return Err(error);
        }
    };
    drop(command);
    log.event("INFO", "launcher", "foreground child started");
    let result: Result<i32, String> = async {
        tokio::select! {
            biased;
            result = cancel => {
                result?;
                log.event("INFO", "launcher", "foreground cancellation requested");
                owner.interrupt()?;
                // Give the panel's own graceful shutdown a bounded opportunity before
                // mandatory tree termination. Cancellation is never successful work.
                if let Ok(result) = tokio::time::timeout(Duration::from_secs(2), owner.wait_console()).await {
                    result?;
                }
                Ok(130)
            }
            result = owner.wait_console() => Ok(result?.code().unwrap_or(1)),
        }
    }.await;
    // Do not return to another menu action while any child or cleanup is uncertain.
    owner.finish()?;
    tooling_process::clean_unspawned(&directory)?;
    match &result {
        Ok(code) => log.event(
            "INFO",
            "launcher",
            &format!("foreground child exited code={code}"),
        ),
        Err(error) => log.event("ERROR", "launcher", error),
    }
    result
}

pub fn scrubbed_environment(
    original: &BTreeMap<String, String>,
    template: &str,
) -> BTreeMap<String, String> {
    let listed = template
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once('=').map(|(key, _)| key.trim()))
        .collect::<Vec<_>>();
    let mut child = original
        .iter()
        .filter(|(key, _)| {
            let upper = key.to_uppercase();
            !listed.contains(&key.as_str())
                && !["TOKEN", "SECRET", "PASSWORD"]
                    .iter()
                    .any(|part| upper.contains(part))
                && !upper.ends_with("_KEY")
                && upper != "OWNER_ID"
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    child.insert("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into());
    child
}

pub struct SandboxOptions {
    pub port: i64,
    pub all: bool,
    pub with_pack: Vec<i64>,
    pub bot_username: String,
}

pub fn sandbox_options(
    args: &[String],
    real_port: u16,
) -> Result<(PathBuf, SandboxOptions), String> {
    let mut source = PathBuf::from("collection");
    let mut options = SandboxOptions {
        port: i64::from(real_port) + 1,
        all: false,
        with_pack: Vec::new(),
        bot_username: String::new(),
    };
    let mut index = 0;
    while index < args.len() {
        let option = args[index].as_str();
        if option == "--all" {
            options.all = true;
            index += 1;
            continue;
        }
        if !["--source", "--port", "--with-pack", "--bot-username"].contains(&option) {
            return Err(format!("unknown sandbox option: {option}"));
        }
        index += 1;
        let value = args
            .get(index)
            .filter(|value| !value.starts_with("--"))
            .ok_or_else(|| format!("sandbox option {option} requires a value"))?;
        match option {
            "--source" => source = PathBuf::from(value),
            "--port" => {
                options.port = value
                    .parse()
                    .map_err(|_| "sandbox --port must be an integer")?
            }
            "--with-pack" => options.with_pack.push(
                value
                    .parse()
                    .map_err(|_| "sandbox --with-pack must be an integer")?,
            ),
            "--bot-username" => options.bot_username = value.clone(),
            _ => return Err("invalid sandbox option".into()),
        }
        index += 1;
    }
    // Validate before a clone is created; this function accepts presentation
    // flags only, never a forwarded data directory or abbreviated option.
    panel_arguments(&options, Path::new("clone"), real_port)?;
    Ok((source, options))
}

/// The clone path comes only from the owner, never a forwarded --data-dir flag.
pub fn panel_arguments(
    options: &SandboxOptions,
    clone: &Path,
    real_port: u16,
) -> Result<Vec<String>, String> {
    if options.port == i64::from(real_port) {
        return Err(format!(
            "refusing port {real_port}: that is the real panel's port"
        ));
    }
    if !(1..=65535).contains(&options.port) {
        return Err(format!("refusing port {}: not a port", options.port));
    }
    let mut args = vec![
        "panel".into(),
        "--data-dir".into(),
        clone.to_str().ok_or("Sandbox path is not UTF-8")?.into(),
        "--port".into(),
        options.port.to_string(),
        "--no-open".into(),
    ];
    if options.all {
        args.push("--all".into());
    }
    for pack in &options.with_pack {
        args.extend(["--with-pack".into(), pack.to_string()]);
    }
    if !options.bot_username.is_empty() {
        args.extend(["--bot-username".into(), options.bot_username.clone()]);
    }
    Ok(args)
}
