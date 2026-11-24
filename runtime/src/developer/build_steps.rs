use super::Command;
use crate::{logging::RunLog, tooling_environment, tooling_process};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};

pub struct Tools {
    pub cargo: PathBuf,
    pub rustc: PathBuf,
    pub npm: Option<PathBuf>,
    pub driver: PathBuf,
}
impl Tools {
    pub(super) fn discover() -> Result<Self, String> {
        let required = |name| {
            crate::codec_process::executable(name).map_err(|_| format!("Rust 1.100.0 and Cargo must already be installed ({name} unavailable); no toolchain was installed"))
        };
        Ok(Self {
            cargo: required("cargo")?,
            rustc: required("rustc")?,
            npm: crate::codec_process::executable("npm").ok(),
            driver: std::env::current_exe().map_err(|error| error.to_string())?,
        })
    }
}
pub struct Step {
    pub label: String,
    pub argv: Vec<OsString>,
    pub wall: Duration,
    pub idle: Duration,
}
fn step(label: &str, executable: &Path, args: &[&str], wall: Duration) -> Step {
    let mut argv = vec![executable.into()];
    argv.extend(args.iter().map(OsString::from));
    Step {
        label: label.into(),
        argv,
        wall,
        idle: wall.min(Duration::from_secs(120)),
    }
}

pub fn plan(root: &Path, command: &Command, tools: &Tools) -> Result<Vec<Step>, String> {
    let root = tooling_environment::physical_root(root)?;
    let mut steps = Vec::new();
    match command {
        Command::Check(options) | Command::Test(options) => {
            if matches!(command, Command::Check(_)) {
                if options.format {
                    steps.push(step(
                        "Rust formatting",
                        &tools.cargo,
                        &[
                            "fmt",
                            "--manifest-path",
                            "runtime/Cargo.toml",
                            "--",
                            "--check",
                        ],
                        options.timeout,
                    ));
                }
                if options.lint {
                    steps.push(step(
                        "Clippy",
                        &tools.cargo,
                        &[
                            "clippy",
                            "--manifest-path",
                            "runtime/Cargo.toml",
                            "--locked",
                            "--all-targets",
                            "--",
                            "-D",
                            "warnings",
                        ],
                        options.timeout,
                    ));
                }
            }
            steps.push(step(
                "Build guarded runtime runner",
                &tools.cargo,
                &[
                    "build",
                    "--manifest-path",
                    "runtime/Cargo.toml",
                    "--locked",
                    "--release",
                    "--bin",
                    "numera-runtime-tests",
                ],
                options.timeout,
            ));
            let mut tests = step(
                "Guarded runtime tests",
                &tools.cargo,
                &[
                    "test",
                    "--config",
                    ".cargo/runtime-tests.toml",
                    "--manifest-path",
                    "runtime/Cargo.toml",
                    "--locked",
                ],
                options.timeout,
            );
            for name in &options.tests {
                tests.argv.extend(["--test".into(), name.into()]);
            }
            tests.argv.extend(["--".into(), "--test-threads=1".into()]);
            steps.push(tests);
            if matches!(command, Command::Check(_))
                && options.web
                && let Some(npm) = &tools.npm
            {
                let worker = root.join("worker/node_modules");
                if worker.join("typescript/package.json").is_file()
                    && worker.join("vitest/package.json").is_file()
                {
                    steps.push(step(
                        "Worker type check",
                        npm,
                        &["--prefix", "worker", "run", "typecheck"],
                        options.timeout,
                    ));
                    let config = root.join("assets/jsconfig.json");
                    steps.push(step(
                        "Browser script type check",
                        npm,
                        &[
                            "--prefix",
                            "worker",
                            "exec",
                            "--",
                            "tsc",
                            "-p",
                            config
                                .to_str()
                                .ok_or("browser configuration path must be UTF-8")?,
                            "--noEmit",
                        ],
                        options.timeout,
                    ));
                    steps.push(step(
                        "Worker tests",
                        npm,
                        &["--prefix", "worker", "test"],
                        options.timeout,
                    ));
                }
                if root
                    .join("assets/node_modules/@playwright/test/package.json")
                    .is_file()
                {
                    steps.push(step(
                        "Browser tests",
                        npm,
                        &["--prefix", "assets", "test"],
                        options.timeout,
                    ));
                }
            }
        }
        Command::Build { release } => steps.push(step(
            "Build standalone application",
            &tools.driver,
            if *release {
                &["__execute", "build"]
            } else {
                &["__execute", "build", "--debug"]
            },
            Duration::from_secs(1840),
        )),
        _ => return Err("this developer command has no external process plan".into()),
    }
    Ok(steps)
}

pub fn worker_limit(cores: Option<usize>, original: &BTreeMap<String, String>) -> usize {
    let mut limit = cores.unwrap_or(2).saturating_sub(2).clamp(1, 4);
    for key in ["HOOKMAKER_MAX_TEST_WORKERS", "CARGO_BUILD_JOBS"] {
        if let Some(value) = original
            .get(key)
            .and_then(|value| value.trim().parse::<usize>().ok())
            .filter(|value| *value > 0)
        {
            limit = limit.min(value);
        }
    }
    limit
}
pub(super) const PREREQUISITES: &[&str] = &[
    "PATH",
    "SystemRoot",
    "SYSTEMROOT",
    "WINDIR",
    "PATHEXT",
    "SystemDrive",
    "HOME",
    "USERPROFILE",
    "LOCALAPPDATA",
    "ProgramFiles",
    "ProgramFiles(x86)",
    "LD_LIBRARY_PATH",
    "LANG",
    "LC_ALL",
    "RUSTUP_HOME",
    "CARGO_BUILD_JOBS",
    "HOOKMAKER_MAX_TEST_WORKERS",
    "CARGO_PROFILE_DEV_DEBUG",
    "CARGO_PROFILE_TEST_DEBUG",
    "CARGO_PROFILE_RELEASE_DEBUG",
    "CC",
    "CXX",
    "AR",
    "CMAKE",
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
    "NUMERA_BROWSER_EXECUTABLE",
    "NUMERA_EMOJI_MAPPER_BROWSER_CHANNEL",
    "PLAYWRIGHT_BROWSERS_PATH",
];
pub fn environment(
    root: &Path,
    original: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, String> {
    let selected = original
        .iter()
        .filter(|(key, _)| {
            PREREQUISITES
                .iter()
                .any(|allowed| key.eq_ignore_ascii_case(allowed))
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let mut environment = tooling_environment::environment(root, &selected, false)?;
    for (key, relative) in [
        ("CARGO_HOME", ".cache/cargo"),
        ("CARGO_TARGET_DIR", "runtime/target"),
        ("npm_config_cache", ".cache/npm"),
    ] {
        let path = tooling_environment::confined_directory(root, relative, true)?;
        environment.insert(
            key.into(),
            path.to_str()
                .ok_or("developer cache path must be UTF-8")?
                .into(),
        );
    }
    for (key, value) in [
        ("RUSTUP_AUTO_INSTALL", "0"),
        ("RUSTUP_TOOLCHAIN", "1.100.0"),
        ("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1"),
        ("TELEGRAM_API_BASE", "http://127.0.0.1:9"),
        ("BRAND_LOGO_BOTS", ""),
        ("COLLECTION_PACK_BASE", "YourBrand_Emoji_Packs"),
        ("COIN_PACK_BASE", "cryptoemoji"),
        ("COIN_PACK_TITLE", "@YourBrand Crypto Emoji"),
        ("WRANGLER_SEND_METRICS", "false"),
    ] {
        environment.insert(key.into(), value.into());
    }
    environment.insert(
        "CARGO_BUILD_JOBS".into(),
        worker_limit(
            std::thread::available_parallelism().ok().map(usize::from),
            original,
        )
        .to_string(),
    );
    Ok(environment)
}

pub async fn execute(
    root: &Path,
    steps: &[Step],
    environment: &BTreeMap<String, String>,
    log: &RunLog,
) -> Result<i32, String> {
    for step in steps {
        log.event("INFO", "developer", &format!("Starting {}", step.label));
        let result =
            tooling_process::capture(root, &step.argv, environment, step.wall, step.idle, log)
                .await?;
        for (text, stderr) in [(&result.stdout, false), (&result.stderr, true)] {
            let text = crate::logging::redact(text, &[]);
            if !text.is_empty() {
                log.event("INFO", "developer-output", &text);
                if stderr {
                    eprint!("{text}");
                } else {
                    print!("{text}");
                }
            }
        }
        log.event(
            if result.code == 0 { "INFO" } else { "ERROR" },
            "developer",
            &format!("{} exited {}", step.label, result.code),
        );
        if result.code != 0 {
            return Ok(result.code);
        }
    }
    Ok(0)
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn standard_windows_node_installation_enables_all_installed_web_checks() {
        let node = crate::codec_process::executable("node")
            .expect("Windows contributor checks require the installed Node runtime");
        let installation = node.parent().unwrap();
        assert!(
            installation.join("npm.cmd").is_file(),
            "the real Windows Node installation must supply its npm command shim"
        );
        assert!(
            installation
                .join("node_modules/npm/bin/npm-cli.js")
                .is_file(),
            "the real Windows Node installation must supply the npm CLI"
        );
        let tools = Tools::discover().unwrap();
        assert!(
            tools.npm.is_some(),
            "installed Windows npm must not silently skip the browser and Worker checks"
        );
        let root = tempfile::Builder::new()
            .prefix("developer-windows-web-")
            .tempdir()
            .unwrap();
        for relative in [
            "worker/node_modules/typescript/package.json",
            "worker/node_modules/vitest/package.json",
            "assets/node_modules/@playwright/test/package.json",
        ] {
            let path = root.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"{}").unwrap();
        }
        let steps = plan(
            root.path(),
            &Command::Check(crate::developer::Options::default()),
            &tools,
        )
        .unwrap();
        assert_eq!(
            steps
                .iter()
                .skip(4)
                .map(|step| step.label.as_str())
                .collect::<Vec<_>>(),
            [
                "Worker type check",
                "Browser script type check",
                "Worker tests",
                "Browser tests",
            ]
        );
    }
}
