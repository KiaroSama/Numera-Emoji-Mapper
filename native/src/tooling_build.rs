//! Explicit contributor build, separate from the application binary being replaced.
use crate::{
    logging::RunLog, tooling_environment, tooling_install, tooling_process, tooling_runtime,
};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct Step {
    pub argv: Vec<OsString>,
    pub wall: Duration,
    pub idle: Duration,
}

pub fn worker_limit(cores: Option<usize>, environment: &BTreeMap<String, String>) -> usize {
    let mut limit = cores.unwrap_or(2).saturating_sub(2).clamp(1, 4);
    for key in ["HOOKMAKER_MAX_TEST_WORKERS", "CARGO_BUILD_JOBS"] {
        if let Some(value) = environment
            .get(key)
            .and_then(|value| value.trim().parse::<i64>().ok())
            .filter(|value| *value > 0)
        {
            limit = limit.min(value as usize);
        }
    }
    limit
}

pub fn environment(
    root: &Path,
    original: &BTreeMap<String, String>,
    python: &Path,
    scripts: &Path,
    cores: Option<usize>,
) -> Result<BTreeMap<String, String>, String> {
    let mut result = tooling_environment::environment(root, original, true)?;
    result.insert(
        "PYO3_PYTHON".into(),
        python
            .to_str()
            .ok_or("Build interpreter path is not UTF-8")?
            .into(),
    );
    let mut paths = vec![scripts.to_path_buf()];
    if let Some(path) = original.get("PATH") {
        paths.extend(std::env::split_paths(path));
    }
    result.insert(
        "PATH".into(),
        std::env::join_paths(paths)
            .map_err(|e| e.to_string())?
            .to_str()
            .ok_or("Build PATH is not UTF-8")?
            .into(),
    );
    result.insert(
        "CARGO_BUILD_JOBS".into(),
        worker_limit(cores, original).to_string(),
    );
    let temporary = tooling_environment::confined_directory(root, "logs/native-build", true)?;
    for key in ["TEMP", "TMP", "TMPDIR"] {
        result.insert(
            key.into(),
            temporary
                .to_str()
                .ok_or("Build temporary path is not UTF-8")?
                .into(),
        );
    }
    Ok(result)
}

pub fn wheel_steps(root: &Path, python: &Path, maturin_present: bool) -> Vec<Step> {
    let mut steps = Vec::new();
    if !maturin_present {
        steps.push(Step {
            argv: vec![
                python.as_os_str().into(),
                "-I".into(),
                "-B".into(),
                "-X".into(),
                "utf8".into(),
                "-m".into(),
                "pip".into(),
                "install".into(),
                "maturin==1.15.0".into(),
            ],
            wall: Duration::from_secs(180),
            idle: Duration::from_secs(120),
        });
    }
    steps.push(Step {
        argv: vec![
            python.as_os_str().into(),
            "-I".into(),
            "-B".into(),
            "-X".into(),
            "utf8".into(),
            "-m".into(),
            "maturin".into(),
            "build".into(),
            "--release".into(),
            "--locked".into(),
            "--interpreter".into(),
            python.as_os_str().into(),
            "--target-dir".into(),
            root.join("native/target/wheel-build").into_os_string(),
            "--out".into(),
            root.join("native/target/wheels").into_os_string(),
        ],
        wall: Duration::from_secs(600),
        idle: Duration::from_secs(120),
    });
    steps
}

pub fn executable_step(root: &Path) -> Step {
    Step {
        argv: vec![
            "cargo".into(),
            "build".into(),
            "--manifest-path".into(),
            root.join("native/Cargo.toml").into_os_string(),
            "--release".into(),
            "--locked".into(),
            "--bin".into(),
            "numera-emoji".into(),
        ],
        wall: Duration::from_secs(600),
        idle: Duration::from_secs(120),
    }
}

pub fn wheel(root: &Path, version: &str) -> Result<PathBuf, String> {
    let prefix = format!("numera_emoji_core-{version}-");
    let directory = root.join("native/target/wheels");
    tooling_environment::reject_links(&directory, "Linked native wheel directory")?;
    let mut wheels = std::fs::read_dir(directory)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    wheels.retain(|path| {
        path.file_name()
            .and_then(|v| v.to_str())
            .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".whl"))
    });
    if wheels.len() != 1 {
        return Err(
            "Expected one wheel for this platform; clean native/target/wheels and rebuild.".into(),
        );
    }
    let wheel = wheels.remove(0);
    tooling_environment::reject_links(&wheel, "Linked native wheel")?;
    if !wheel.is_file() {
        return Err("Native wheel is not a regular file".into());
    }
    Ok(wheel)
}

pub fn arguments(args: &[String]) -> Result<bool, String> {
    match args {
        [] => Ok(false),
        [option] if option == "--install-maturin" => Ok(true),
        _ => Err("numera-build accepts only --install-maturin".into()),
    }
}

pub fn driver_location(root: &Path, executable: &Path) -> Result<(), String> {
    if !executable.is_absolute()
        || !executable.starts_with(root)
        || executable.file_stem().and_then(|name| name.to_str()) != Some("numera-build")
    {
        return Err("build requires the separate project-local numera-build driver".into());
    }
    Ok(())
}

pub(crate) fn inherited() -> Result<BTreeMap<String, String>, String> {
    const KEYS: &[&str] = &[
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "PATHEXT",
        "HOME",
        "USERPROFILE",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
        "DYLD_FALLBACK_LIBRARY_PATH",
        "LANG",
        "LC_ALL",
        "RUSTUP_HOME",
        "RUSTUP_TOOLCHAIN",
        "CARGO_HOME",
        "HOOKMAKER_MAX_TEST_WORKERS",
        "CARGO_BUILD_JOBS",
        "CARGO_PROFILE_DEV_DEBUG",
        "CARGO_PROFILE_TEST_DEBUG",
        "CARGO_PROFILE_RELEASE_DEBUG",
        "INCLUDE",
        "LIB",
        "LIBPATH",
        "VSINSTALLDIR",
        "VCINSTALLDIR",
        "VCToolsInstallDir",
        "WindowsSdkDir",
        "WindowsSDKVersion",
        "UniversalCRTSdkDir",
        "UCRTVersion",
        "PKG_CONFIG_PATH",
        "CC",
        "CXX",
        "AR",
        "SDKROOT",
        "MACOSX_DEPLOYMENT_TARGET",
    ];
    let mut values = BTreeMap::new();
    for key in KEYS {
        if let Some(value) = std::env::var_os(key) {
            values.insert(
                (*key).into(),
                value
                    .into_string()
                    .map_err(|_| "build prerequisite must be UTF-8")?,
            );
        }
    }
    Ok(values)
}

async fn compiler(
    root: &Path,
    original: &BTreeMap<String, String>,
    log: &RunLog,
) -> Result<PathBuf, String> {
    let cargo = crate::codec_process::executable("cargo")?;
    let Ok(rustup) = crate::codec_process::executable("rustup") else {
        let directory = crate::migration_bundle::resolved(
            cargo.parent().ok_or("compiler has no bin directory")?,
        )?;
        for name in ["rustc", "rustdoc"] {
            if !directory
                .join(if cfg!(windows) {
                    format!("{name}.exe")
                } else {
                    name.into()
                })
                .is_file()
            {
                return Err(format!("installed compiler directory is missing {name}"));
            }
        }
        return Ok(directory);
    };
    let mut environment = tooling_environment::environment(root, original, false)?;
    environment.insert("RUSTUP_AUTO_INSTALL".into(), "0".into());
    let mut directory = None;
    for tool in ["cargo", "rustc", "rustdoc"] {
        let completed = tooling_process::capture(
            root,
            &[rustup.clone().into_os_string(), "which".into(), tool.into()],
            &environment,
            Duration::from_secs(10),
            Duration::from_secs(5),
            log,
        )
        .await?;
        if completed.code != 0 {
            return Err(format!(
                "installed Rust toolchain is missing {tool}; no toolchain was installed"
            ));
        }
        let path = Path::new(completed.stdout.trim());
        if !path.is_absolute() || !path.is_file() {
            return Err("rustup did not return an installed compiler file".into());
        }
        let parent = crate::migration_bundle::resolved(
            path.parent().ok_or("compiler has no bin directory")?,
        )?;
        if directory.as_ref().is_some_and(|value| value != &parent) {
            return Err("installed compiler tools disagree on their toolchain".into());
        }
        directory = Some(parent);
    }
    directory.ok_or("installed compiler directory is unavailable".into())
}

async fn build(
    root: &Path,
    consent: bool,
    original: &BTreeMap<String, String>,
    log: &RunLog,
) -> Result<i32, String> {
    let driver = std::env::current_exe().map_err(|e| e.to_string())?;
    let driver = crate::migration_bundle::resolved(&driver)?;
    driver_location(root, &driver)?;
    for directory in [
        "native/target/wheel-build",
        "native/target/wheels",
        "native/target/release",
        "native/runtime",
        "emojikit",
        "numera_emoji_core.libs",
    ] {
        tooling_environment::confined_directory(root, directory, false)?;
    }
    let compiler = compiler(root, original, log).await?;
    let report = tooling_runtime::inspect(root, &[], original, log).await?;
    let selected = tooling_runtime::selection(root)?;
    let scripts = Path::new(
        report["scripts"]
            .as_str()
            .ok_or("runtime report has no scripts directory")?,
    );
    let mut environment = environment(
        root,
        original,
        &selected.executable,
        scripts,
        std::thread::available_parallelism().ok().map(usize::from),
    )?;
    let mut paths = vec![compiler.clone()];
    paths.extend(std::env::split_paths(
        environment.get("PATH").ok_or("build PATH is missing")?,
    ));
    environment.insert(
        "PATH".into(),
        std::env::join_paths(paths)
            .map_err(|e| e.to_string())?
            .into_string()
            .map_err(|_| "build PATH must be UTF-8")?,
    );
    environment.insert("RUSTUP_AUTO_INSTALL".into(), "0".into());
    environment.insert(
        "RUSTUP_TOOLCHAIN".into(),
        compiler
            .parent()
            .and_then(|p| p.to_str())
            .ok_or("compiler root must be UTF-8")?
            .into(),
    );
    environment.insert(
        "RUSTC".into(),
        compiler
            .join(if cfg!(windows) { "rustc.exe" } else { "rustc" })
            .to_str()
            .ok_or("compiler path must be UTF-8")?
            .into(),
    );
    environment.insert("PYTHONDONTWRITEBYTECODE".into(), "1".into());
    environment.insert("PYTHONNOUSERSITE".into(), "1".into());
    environment.insert("PYTHONSAFEPATH".into(), "1".into());
    environment.insert("PIP_NO_INPUT".into(), "1".into());
    environment.insert(
        "PIP_CONFIG_FILE".into(),
        if cfg!(windows) { "NUL" } else { "/dev/null" }.into(),
    );
    let maturin = report["maturin_present"]
        .as_bool()
        .ok_or("runtime report has no Maturin availability")?
        && report["maturin_version"].as_str() == Some("1.15.0");
    if !maturin && !consent {
        return Err("Maturin 1.15.0 is required; use --install-maturin to approve installation into the selected .venv".into());
    }
    log.event(
        "INFO",
        "build",
        "building the selected interpreter's locked wheel",
    );
    let code = execute(
        root,
        &wheel_steps(root, &selected.executable, maturin),
        &environment,
        log,
    )
    .await?;
    if code != 0 {
        return Ok(code);
    }
    let wheel = wheel(root, env!("CARGO_PKG_VERSION"))?;
    let install = Step {
        argv: vec![
            selected.executable.clone().into_os_string(),
            "-I".into(),
            "-B".into(),
            "-X".into(),
            "utf8".into(),
            "-m".into(),
            "pip".into(),
            "install".into(),
            "--no-deps".into(),
            "--force-reinstall".into(),
            wheel.into_os_string(),
        ],
        wall: Duration::from_secs(60),
        idle: Duration::from_secs(30),
    };
    let code = execute(root, &[install], &environment, log).await?;
    if code != 0 {
        return Ok(code);
    }
    let source = tooling_install::installed_extension(root, &report)?;
    let extension = tooling_install::install_extension(root, &source)?;
    tooling_runtime::inspect(
        root,
        &[
            "--extension".into(),
            extension
                .to_str()
                .ok_or("native extension path must be UTF-8")?
                .into(),
        ],
        original,
        log,
    )
    .await?;
    log.event(
        "INFO",
        "build",
        "installed native extension origin and required API checked",
    );
    let mut executable = executable_step(root);
    executable.argv[0] = compiler
        .join(if cfg!(windows) { "cargo.exe" } else { "cargo" })
        .into_os_string();
    let code = execute(root, &[executable], &environment, log).await?;
    if code != 0 {
        return Ok(code);
    }
    let built = root.join("native/target/release").join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    let installed = tooling_install::install_executable(root, &built)?;
    let probe = tooling_process::capture(
        root,
        &[installed.into_os_string(), "--help".into()],
        &environment,
        Duration::from_secs(10),
        Duration::from_secs(5),
        log,
    )
    .await?;
    if probe.code != 0 {
        return Ok(probe.code);
    }
    if !probe.stdout.contains("Numera Emoji Mapper native backend") {
        return Err("installed native executable returned unexpected startup output".into());
    }
    log.event(
        "INFO",
        "build",
        "required native module and executable installed and checked",
    );
    Ok(0)
}

pub fn run(root: &Path, args: &[String]) -> i32 {
    if args == ["--help"] || args == ["-h"] {
        println!(
            "Usage: numera-build [--install-maturin]\nContributor build candidate; installation is blocked until approved signing, identity, logo and rollback verification are integrated. No automatic environment or toolchain creation."
        );
        return 0;
    }
    let log = std::sync::Arc::new(RunLog::open(root, "build_native", vec![]));
    log.install_panic_hook();
    let result = (|| {
        let consent = arguments(args)?;
        let root = tooling_environment::physical_root(root)?;
        let original = inherited()?;
        if crate::codec_process::executable("cargo").is_err() {
            log.event("ERROR", "build", "Rust 1.99+ and the platform linker are required. Install Rust, then run numera-build.");
            return Ok(2);
        }
        tooling_install::require_installation_admission()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        runtime.block_on(build(&root, consent, &original, &log))
    })();
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            log.event("ERROR", "build", &format!("Native build failed: {error}"));
            1
        }
    };
    log.finish(code);
    code
}

/// Explicit contributor invocation only. No automatic prerequisite installation.
pub async fn execute(
    root: &Path,
    steps: &[Step],
    environment: &BTreeMap<String, String>,
    log: &RunLog,
) -> Result<i32, String> {
    for step in steps {
        let result =
            tooling_process::capture(root, &step.argv, environment, step.wall, step.idle, log)
                .await?;
        if result.code != 0 {
            log.event(
                "ERROR",
                "build",
                &format!("build step failed exit={}", result.code),
            );
            return Ok(result.code);
        }
    }
    Ok(0)
}
