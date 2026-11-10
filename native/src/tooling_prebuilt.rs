//! Verified offline installation into a fresh package-local environment.
use crate::{
    logging::RunLog, tooling_build::Step, tooling_environment, tooling_install, tooling_manifest,
    tooling_process, tooling_runtime,
};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn steps(root: &Path, base_python: &Path, wheel: &str) -> Result<Vec<Step>, String> {
    let install = tooling_install::offline_install_arguments(root, wheel)?;
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut argv = vec![
        python.into_os_string(),
        "-I".into(),
        "-B".into(),
        "-X".into(),
        "utf8".into(),
    ];
    argv.extend(install.into_iter().map(OsString::from));
    Ok(vec![
        Step {
            argv: vec![
                base_python.as_os_str().to_owned(),
                "-I".into(),
                "-B".into(),
                "-X".into(),
                "utf8".into(),
                "-m".into(),
                "venv".into(),
                root.join(".venv").into_os_string(),
            ],
            wall: Duration::from_secs(60),
            idle: Duration::from_secs(30),
        },
        Step {
            argv,
            wall: Duration::from_secs(180),
            idle: Duration::from_secs(60),
        },
    ])
}

pub fn environment(
    root: &Path,
    inherited: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, String> {
    // A parent PIP_TARGET, user config or index override must not redirect an offline install.
    let mut environment = tooling_runtime::environment(inherited);
    for (key, value) in [
        (
            "PIP_CONFIG_FILE",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        ),
        ("PIP_NO_INPUT", "1"),
        ("PIP_NO_INDEX", "1"),
    ] {
        environment.insert(key.into(), value.into());
    }
    tooling_environment::environment(root, &environment, false)
}

fn base_python(inherited: &BTreeMap<String, String>) -> Result<PathBuf, String> {
    let selected = inherited
        .get("PYO3_PYTHON")
        .filter(|value| !value.is_empty());
    let path = Path::new(selected.map(String::as_str).unwrap_or("python"));
    let path = if path.is_absolute() || path.components().count() > 1 {
        crate::migration_bundle::resolved(path)?
    } else {
        let name = path.to_str().ok_or("bootstrap interpreter must be UTF-8")?;
        crate::codec_process::executable(name.strip_suffix(".exe").unwrap_or(name))?
    };
    if !path.is_file() {
        return Err("offline installation requires an existing Python3.11 interpreter".into());
    }
    Ok(path)
}

async fn execute(
    root: &Path,
    step: &Step,
    environment: &BTreeMap<String, String>,
    log: &RunLog,
) -> Result<i32, String> {
    let result =
        tooling_process::capture(root, &step.argv, environment, step.wall, step.idle, log).await?;
    // Child bodies can contain local paths or external package diagnostics; redact before display.
    print!("{}", crate::logging::redact(&result.stdout, &[]));
    eprint!("{}", crate::logging::redact(&result.stderr, &[]));
    Ok(result.code)
}

async fn install(
    root: &Path,
    inherited: &BTreeMap<String, String>,
    log: &RunLog,
) -> Result<i32, String> {
    let root = tooling_environment::physical_root(root)?;
    tooling_install::require_fresh_environment(&root)?;
    if !root.join("package.json").is_file() {
        return Err("package manifest is missing; extract a complete prebuilt archive".into());
    }
    let manifest = tooling_manifest::verify(&root, &tooling_manifest::embedded_host()?)?;
    let python = base_python(inherited)?;
    let wheel = manifest["wheel"]
        .as_str()
        .ok_or("package manifest has no application wheel")?;
    let mut plan = steps(&root, &python, wheel)?.into_iter();
    let create = plan
        .next()
        .ok_or("offline installer has no environment step")?;
    let dependencies = plan.next().ok_or("offline installer has no wheel step")?;
    let environment = environment(&root, inherited)?;
    let base = tooling_process::capture(
        &root,
        &[
            python.into_os_string(),
            "-I".into(),
            "-B".into(),
            "-X".into(),
            "utf8".into(),
            "--version".into(),
        ],
        &environment,
        Duration::from_secs(10),
        Duration::from_secs(5),
        log,
    )
    .await?;
    if base.code != 0 {
        return Ok(base.code);
    }
    let version = base
        .stdout
        .trim()
        .strip_prefix("Python ")
        .ok_or("bootstrap interpreter did not report Python3.11")?;
    let version = version
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "bootstrap interpreter must be stable Python3.11")?;
    if version.len() != 3 || version[..2] != [3, 11] {
        return Err("offline installation requires Python3.11; no environment was created".into());
    }
    let venv = tooling_environment::confined_directory(&root, ".venv", false)?;
    // Claim the final path exclusively. Renaming a built venv would invalidate its
    // absolute scripts; failed setup stays visible and can never overwrite a prior install.
    std::fs::create_dir(&venv)
        .map_err(|_| "cannot exclusively claim package .venv; existing state preserved")?;
    log.event(
        "INFO",
        "install",
        "claimed fresh package environment; incomplete setup is retained on failure",
    );
    let code = execute(&root, &create, &environment, log).await?;
    if code != 0 {
        return Ok(code);
    }
    let selected = tooling_runtime::selection(&root)?;
    if selected.shared_sites {
        return Err("offline installation requires an isolated package environment".into());
    }
    // Uses the real selected executable before any package can supply an import.
    tooling_runtime::independent(&root, &environment, log).await?;
    let code = execute(&root, &dependencies, &environment, log).await?;
    if code != 0 {
        return Ok(code);
    }
    let binary = root.join("native/runtime").join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
            .map_err(|_| "cannot enable installed native executable")?;
    }
    // Observe the installed wheel through the same fresh runtime as contributor builds.
    let report =
        tooling_runtime::inspect_executable(&root, &binary, &[], &environment, log).await?;
    let source = tooling_install::installed_extension(&root, &report)?;
    let extension = tooling_install::install_extension(&root, &source)?;
    tooling_runtime::inspect_executable(
        &root,
        &binary,
        &[
            "--extension".into(),
            extension
                .to_str()
                .ok_or("extension path must be UTF-8")?
                .into(),
        ],
        &environment,
        log,
    )
    .await?;
    let completed = tooling_process::capture(
        &root,
        &[binary.into_os_string(), "--help".into()],
        &environment,
        Duration::from_secs(10),
        Duration::from_secs(5),
        log,
    )
    .await?;
    if completed.code != 0 {
        return Ok(completed.code);
    }
    if !completed
        .stdout
        .contains("Numera Emoji Mapper native backend")
    {
        return Err("installed native executable returned unexpected startup output".into());
    }
    log.event(
        "INFO",
        "install",
        "offline environment and native wheel checked; no Rust/Cargo compilation performed",
    );
    println!(
        "Installed. Run native/runtime/numera-emoji{} menu",
        if cfg!(windows) { ".exe" } else { "" }
    );
    Ok(0)
}

pub fn run(root: &Path, args: &[String]) -> i32 {
    if args == ["--help"] || args == ["-h"] {
        println!(
            "Usage: numera-emoji install-prebuilt\nVerify this package and install its bundled wheels into a fresh local environment. Existing installations are preserved. Never builds Rust or downloads dependencies."
        );
        return 0;
    }
    let log = std::sync::Arc::new(RunLog::open(root, "install_prebuilt", vec![]));
    log.install_panic_hook();
    let result = (|| {
        if !args.is_empty() {
            return Err("install-prebuilt accepts no arguments".into());
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "cannot initialize offline install owner")?;
        runtime.block_on(install(root, &std::env::vars().collect(), &log))
    })();
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            log.event("ERROR", "install", &error);
            1
        }
    };
    log.finish(code)
}
