//! Explicit contributor commands. No operator configuration, interpreter or historical oracle.
use crate::{logging::RunLog, tooling_environment, tooling_process};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[path = "developer/build_steps.rs"]
mod build_steps;
#[path = "developer/sandbox.rs"]
mod sandbox;
#[path = "developer/similarity.rs"]
mod similarity;

use build_steps::PREREQUISITES;
pub use build_steps::{Step, Tools, environment, execute, plan, worker_limit};
#[cfg(target_os = "linux")]
use sandbox::current_process_start;
use sandbox::sandbox;
pub use similarity::{Benchmark, benchmark, generated_hashes, load_hashes, median};
use similarity::{cardinality, repetitions};

const HELP: &str = "Usage: numera-dev COMMAND [OPTIONS]\nCommands:\n  panel-sandbox [--source DIR] [--port N]  Serve an isolated catalog clone (bounded, credential-free)\n  build [--debug]                         Build the locked standalone application\n  check [--skip-format] [--skip-lint]      Rust formatting, Clippy and guarded tests\n  test [--test NAME | --tests NAME,...]    Guarded serial runtime Cargo tests\n  benchmark-similarity [--catalog PATH]   Read-only catalog or deterministic Rust timings\nSandbox: --all --with-pack N --bot-username NAME --timeout-seconds N (default 1800; maximum 7200).\nCheck/test: --timeout-seconds N (1..7200 per step), --test NAME, --tests NAME,...\nCheck: --no-web skips optional already-installed browser/Worker checks.\nBenchmark: --count N (default 1500, 1..10000), --seed N (default 321), --repeats N (default 5, odd 1..21).\nRequires the installed Rust 1.100.0 toolchain for build/check/test. No prerequisite installation.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub tests: Vec<String>,
    pub timeout: Duration,
    pub format: bool,
    pub lint: bool,
    pub web: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            tests: vec![],
            timeout: Duration::from_secs(1800),
            format: true,
            lint: true,
            web: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Build {
        release: bool,
    },
    Check(Options),
    Test(Options),
    Sandbox {
        args: Vec<String>,
        timeout: Duration,
    },
    Benchmark {
        catalog: Option<PathBuf>,
        count: usize,
        seed: u64,
        repeats: usize,
    },
}

fn option<'a>(args: &'a [String], index: &mut usize) -> Result<(&'a str, Option<&'a str>), String> {
    let argument = args[*index].as_str();
    if let Some((key, value)) = argument.split_once('=') {
        return Ok((key, Some(value)));
    }
    Ok((argument, None))
}
fn value<'a>(
    args: &'a [String],
    index: &mut usize,
    key: &str,
    inline: Option<&'a str>,
) -> Result<&'a str, String> {
    let text = match inline {
        Some(text) => text,
        None => {
            *index += 1;
            args.get(*index)
                .map(String::as_str)
                .ok_or_else(|| format!("{key} requires a value"))?
        }
    };
    if text.is_empty() || text.starts_with("--") {
        return Err(format!("{key} requires a value"));
    }
    Ok(text)
}

pub fn arguments(args: &[String]) -> Result<Command, String> {
    if args.is_empty()
        || args == ["--help"]
        || args == ["-h"]
        || args
            .iter()
            .skip(1)
            .any(|arg| arg == "--help" || arg == "-h")
    {
        return Ok(Command::Help);
    }
    let command = args[0].as_str();
    if command == "panel-sandbox" {
        let mut passed = Vec::new();
        let mut timeout = Duration::from_secs(1800);
        let mut index = 1;
        while index < args.len() {
            let (key, inline) = option(args, &mut index)?;
            if key == "--timeout-seconds" {
                timeout = Duration::from_secs(
                    value(args, &mut index, key, inline)?
                        .parse::<u64>()
                        .ok()
                        .filter(|value| (1..=7200).contains(value))
                        .ok_or("sandbox timeout must be 1..7200 seconds")?,
                );
            } else {
                passed.push(args[index].clone());
            }
            index += 1;
        }
        crate::tooling_launch::sandbox_options(&passed, crate::panel_session::DEFAULT_PORT)?;
        return Ok(Command::Sandbox {
            args: passed,
            timeout,
        });
    }
    if command == "build" {
        return match &args[1..] {
            [] => Ok(Command::Build { release: true }),
            [flag] if flag == "--debug" => Ok(Command::Build { release: false }),
            _ => Err(
                "build accepts only --debug; no dependencies or toolchains are installed".into(),
            ),
        };
    }
    if matches!(command, "check" | "test") {
        let mut options = Options::default();
        let mut index = 1;
        while index < args.len() {
            let (key, inline) = option(args, &mut index)?;
            match key {
                "--skip-format" if command == "check" && inline.is_none() => options.format = false,
                "--skip-lint" if command == "check" && inline.is_none() => options.lint = false,
                "--no-web" if command == "check" && inline.is_none() => options.web = false,
                "--test" | "--tests" => {
                    let selected = value(args, &mut index, key, inline)?;
                    for name in selected.split(',').map(str::trim) {
                        if name.is_empty()
                            || !name.bytes().all(|byte| {
                                byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
                            })
                            || name.starts_with('-')
                        {
                            return Err(
                                "test targets must be explicit Rust integration target names"
                                    .into(),
                            );
                        }
                        if !options.tests.iter().any(|old| old == name) {
                            options.tests.push(name.into());
                        }
                    }
                }
                "--timeout-seconds" => {
                    let seconds = value(args, &mut index, key, inline)?
                        .parse::<u64>()
                        .ok()
                        .filter(|value| (1..=7200).contains(value))
                        .ok_or("timeout must be 1..7200 seconds per step")?;
                    options.timeout = Duration::from_secs(seconds);
                }
                _ => return Err(format!("unsupported {command} option: {key}")),
            }
            index += 1;
        }
        return Ok(if command == "check" {
            Command::Check(options)
        } else {
            Command::Test(options)
        });
    }
    if command == "benchmark-similarity" {
        let (mut catalog, mut count, mut seed, mut repeats) = (None, 1500, 321, 5);
        let mut index = 1;
        while index < args.len() {
            let (key, inline) = option(args, &mut index)?;
            // Historical --c explicitly selects catalog; --count has no
            // abbreviated spelling. Keep this compatibility local to benchmark.
            let key = if key == "--c" { "--catalog" } else { key };
            let text = if key == "--catalog" && inline == Some("") {
                "."
            } else {
                value(args, &mut index, key, inline)?
            };
            match key {
                "--catalog" => catalog = Some(PathBuf::from(text)),
                "--count" => count = text.parse().map_err(|_| "count must be 1..10000")?,
                "--seed" => {
                    seed = text
                        .parse()
                        .map_err(|_| "seed must be an unsigned 64-bit integer")?
                }
                "--repeats" => {
                    repeats = text.parse().map_err(|_| "repeats must be odd and 1..21")?
                }
                _ => return Err(format!("unsupported benchmark option: {key}")),
            }
            index += 1;
        }
        cardinality(count)?;
        repetitions(repeats)?;
        if catalog.is_some()
            && args.iter().skip(1).any(|arg| {
                arg == "--count"
                    || arg.starts_with("--count=")
                    || arg == "--seed"
                    || arg.starts_with("--seed=")
            })
        {
            return Err(
                "catalog input cannot be combined with generated count/seed options".into(),
            );
        }
        return Ok(Command::Benchmark {
            catalog,
            count,
            seed,
            repeats,
        });
    }
    Err(format!("unknown developer command: {command}"))
}

fn source_root() -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    for directory in executable.ancestors().skip(1) {
        let manifest = directory.join("runtime/Cargo.toml");
        if manifest.is_file() {
            tooling_environment::reject_links(&manifest, "developer manifest cannot follow links")?;
            return tooling_environment::physical_root(directory);
        }
    }
    Err("numera-dev requires a source checkout with runtime/Cargo.toml beside its executable ancestry".into())
}
fn inherited() -> Result<BTreeMap<String, String>, String> {
    let mut values = BTreeMap::new();
    for key in PREREQUISITES {
        if let Some(value) = std::env::var_os(key) {
            values.insert(
                (*key).into(),
                value
                    .into_string()
                    .map_err(|_| format!("developer prerequisite {key} must be UTF-8"))?,
            );
        }
    }
    Ok(values)
}
fn work(root: &Path, args: &[String], log: &RunLog) -> Result<i32, String> {
    let command = arguments(args)?;
    match command {
        Command::Sandbox { args, timeout } => return sandbox(root, &args, timeout, log),
        Command::Help => {
            println!("{HELP}");
            return Ok(0);
        }
        Command::Benchmark {
            catalog,
            count,
            seed,
            repeats,
        } => {
            let path = catalog.map(|path| {
                if path.is_absolute() {
                    path
                } else {
                    root.join(path)
                }
            });
            let hashes = if let Some(path) = path {
                load_hashes(&path)?
            } else {
                generated_hashes(count, seed)?
            };
            let report = benchmark(&hashes, repeats)?;
            log.event("INFO", "benchmark-similarity", &report.line());
            println!("{}", report.line());
            return Ok(0);
        }
        _ => {}
    }
    let tools = Tools::discover()?;
    let environment = environment(root, &inherited()?)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let version = runtime.block_on(tooling_process::capture(
        root,
        &[tools.rustc.clone().into_os_string(), "--version".into()],
        &environment,
        Duration::from_secs(10),
        Duration::from_secs(5),
        log,
    ))?;
    if version.code != 0 || version.stdout.split_whitespace().nth(1) != Some("1.100.0") {
        return Err("Rust 1.100.0 must already be installed; no toolchain was installed".into());
    }
    if let Command::Build { release } = command {
        let binary = crate::package::build(root, release)?;
        let probe = runtime.block_on(tooling_process::capture(
            root,
            &[binary.into_os_string(), "--help".into()],
            &environment,
            Duration::from_secs(10),
            Duration::from_secs(5),
            log,
        ))?;
        if probe.code != 0 || !probe.stdout.contains("Numera Emoji Mapper native backend") {
            return Err("Built application failed its startup probe".into());
        }
        log.event(
            "INFO",
            "developer",
            "Standalone application built and startup checked",
        );
        return Ok(0);
    }
    let guard = root.join(".cargo/runtime-tests.toml");
    tooling_environment::reject_links(&guard, "runtime test guard cannot follow links")?;
    if !guard.is_file() {
        return Err(
            "guarded tests require .cargo/runtime-tests.toml; unguarded execution was refused"
                .into(),
        );
    }
    let steps = plan(root, &command, &tools)?;
    if matches!(command, Command::Check(_)) {
        for label in ["Worker tests", "Browser tests"] {
            if !steps.iter().any(|step| step.label == label) {
                println!(
                    "[SKIP] {label}: optional dependencies are unavailable or web checks were explicitly disabled."
                );
            }
        }
    }
    runtime.block_on(execute(root, &steps, &environment, log))
}

/// Worker entry for explicit contributor commands; the binary's outer owner supplies cancellation.
pub fn run(root: &Path, args: &[String]) -> i32 {
    let log = std::sync::Arc::new(RunLog::open(root, "developer", vec![]));
    log.install_panic_hook();
    let result = tooling_environment::physical_root(root).and_then(|root| work(&root, args, &log));
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            log.event("ERROR", "developer", &error);
            if error.contains("Rust 1.100.0") { 2 } else { 1 }
        }
    };
    log.finish(code);
    code
}

pub fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let worker = args.first().is_some_and(|arg| arg == "__execute");
    if worker {
        if std::env::var("NUMERA_DEV_OWNED_WORKER").as_deref() != Ok("1") {
            eprintln!("ERROR: Internal worker requires an owned numera-dev invocation");
            return 2.into();
        }
        args.remove(0);
    }
    let command = match arguments(&args) {
        Ok(Command::Help) => {
            println!("{HELP}");
            return std::process::ExitCode::SUCCESS;
        }
        Ok(command) => command,
        Err(error) => {
            eprintln!("ERROR: {}", crate::logging::redact(&error, &[]));
            return 2.into();
        }
    };
    let root = match source_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("ERROR: {error}");
            return 2.into();
        }
    };
    if worker {
        return (run(&root, &args) as u8).into();
    }
    let result = (|| -> Result<i32, String> {
        let original = inherited()?;
        let mut environment = if matches!(command, Command::Sandbox { .. }) {
            let template = root.join(".env.example");
            tooling_environment::reject_links(&template, "sandbox template cannot follow links")?;
            let template = std::fs::read_to_string(template)
                .map_err(|_| "sandbox requires its public environment template")?;
            let scrubbed =
                crate::tooling_launch::scrubbed_environment(&std::env::vars().collect(), &template);
            tooling_environment::environment(&root, &scrubbed, false)?
        } else {
            environment(&root, &original)?
        };
        environment.insert("NUMERA_DEV_OWNED_WORKER".into(), "1".into());
        let control = if matches!(command, Command::Sandbox { .. }) {
            #[cfg(target_os = "linux")]
            let birth = current_process_start()?;
            let directory = tooling_process::workspace(&root)?;
            std::fs::write(
                directory.join("sandbox-controller.json"),
                serde_json::json!({"version":1,"directory":directory,"cancel":"stop"}).to_string(),
            )
            .map_err(|error| error.to_string())?;
            environment.insert(
                "NUMERA_SANDBOX_CANCEL_PATH".into(),
                directory
                    .join("stop")
                    .to_str()
                    .ok_or("sandbox control path must be UTF-8")?
                    .into(),
            );
            #[cfg(target_os = "linux")]
            {
                environment.insert(
                    "NUMERA_SANDBOX_PARENT_PID".into(),
                    std::process::id().to_string(),
                );
                environment.insert("NUMERA_SANDBOX_PARENT_START".into(), birth.to_string());
            }
            Some(directory)
        } else {
            None
        };
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        tooling_environment::reject_links(&executable, "developer executable cannot follow links")?;
        let mut argv = vec![executable.into_os_string(), "__execute".into()];
        argv.extend(args.into_iter().map(OsString::from));
        let launch = crate::tooling_launch::Launch {
            root: root.clone(),
            argv,
            environment,
        };
        let log = RunLog::open(&root, "developer-owner", vec![]);
        let mut interrupt = crate::tooling_interrupt::Interrupt::install()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        let wall = match command {
            Command::Build { .. } => Duration::from_secs(1840),
            Command::Benchmark { .. } => Duration::from_secs(300),
            Command::Sandbox { timeout, .. } => timeout + Duration::from_secs(30),
            Command::Check(options) | Command::Test(options) => {
                options.timeout.saturating_mul(10) + Duration::from_secs(30)
            }
            Command::Help => Duration::from_secs(10),
        };
        let mut timed_out = false;
        let cancellation = async {
            tokio::select! {
                result = interrupt.wait() => result?,
                _ = tokio::time::sleep(wall) => { timed_out = true; }
            }
            if let Some(directory) = &control {
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(directory.join("stop"))
                    .map_err(|_| "cannot signal owned sandbox cancellation")?;
                file.write_all(b"stop")
                    .and_then(|()| file.sync_all())
                    .map_err(|_| "cannot persist owned sandbox cancellation")?;
            }
            Ok(())
        };
        let result = runtime.block_on(crate::tooling_launch::foreground(
            &launch,
            cancellation,
            &log,
        ));
        interrupt.close()?;
        let observed = result?;
        if let Some(directory) = control {
            tooling_process::clean_unspawned(&directory)?;
        }
        let code = if timed_out {
            log.event(
                "ERROR",
                "developer",
                "Developer command exceeded its bounded deadline",
            );
            124
        } else {
            observed
        };
        log.finish(code);
        Ok(code)
    })();
    match result {
        Ok(code) => (code as u8).into(),
        Err(error) => {
            eprintln!("ERROR: {}", crate::logging::redact(&error, &[]));
            1.into()
        }
    }
}
