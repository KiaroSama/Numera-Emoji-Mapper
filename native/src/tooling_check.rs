//! Native orchestration of the retained source checks; coverage is not replaced with Cargo-only checks.
use crate::{
    logging::RunLog, tooling_build::Step, tooling_environment, tooling_process, tooling_runtime,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Options {
    pub python: Option<PathBuf>,
    pub timeout: Duration,
    pub tests: Vec<String>,
    pub skip_compile: bool,
    pub skip_lint: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            python: None,
            timeout: Duration::from_secs(1800),
            tests: Vec::new(),
            skip_compile: false,
            skip_lint: false,
        }
    }
}

pub enum Arguments {
    Check(Options),
    Exit(i32),
}

fn suite_names(values: &[String]) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for value in values.iter().flat_map(|value| value.split(',')) {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let value = value.strip_prefix("tests.").unwrap_or(value);
        let value = value.strip_suffix(".py").unwrap_or(value);
        if value.split('.').any(|part| {
            part.is_empty()
                || !part.chars().all(|ch| ch == '_' || ch.is_alphanumeric())
                || part
                    .chars()
                    .next()
                    .is_none_or(|ch| ch != '_' && !ch.is_alphabetic())
        }) {
            return Err("check --tests requires dotted test names, never paths or options".into());
        }
        names.push(format!("tests.{value}"));
    }
    if names.is_empty() {
        return Err("check --tests requires at least one test name".into());
    }
    Ok(names)
}

pub fn arguments(args: &[String]) -> Result<Arguments, String> {
    let mut options = Options::default();
    let mut index = 0;
    while index < args.len() {
        let (name, attached) = args[index]
            .split_once('=')
            .map_or((args[index].as_str(), None), |(name, value)| {
                (name, Some(value))
            });
        index += 1;
        match name {
            "--help" | "-h" if attached.is_none() => return Ok(Arguments::Exit(0)),
            "--skip-compile" if attached.is_none() => options.skip_compile = true,
            "--skip-lint" if attached.is_none() => options.skip_lint = true,
            "--timeout-seconds" => {
                let value = if let Some(value) = attached {
                    value
                } else {
                    let value = args
                        .get(index)
                        .ok_or("check --timeout-seconds requires a positive integer")?;
                    index += 1;
                    value
                };
                let seconds = value
                    .parse::<u32>()
                    .ok()
                    .filter(|value| *value > 0 && *value <= i32::MAX as u32)
                    .ok_or("check --timeout-seconds requires a positive 32-bit integer")?;
                options.timeout = Duration::from_secs(u64::from(seconds));
            }
            "--tests" => {
                let mut values = attached
                    .map(|value| vec![value.to_owned()])
                    .unwrap_or_default();
                while index < args.len() && !args[index].starts_with('-') {
                    values.push(args[index].clone());
                    index += 1;
                }
                suite_names(&values)?;
                options.tests.extend(values);
            }
            "--python" => {
                let value = if let Some(value) = attached {
                    value
                } else {
                    let value = args
                        .get(index)
                        .ok_or("check --python requires an executable path or name")?;
                    index += 1;
                    value
                };
                if value.is_empty() || value.starts_with('-') || value.chars().any(char::is_control)
                {
                    return Err("check --python requires an executable path or name".into());
                }
                options.python = Some(value.into());
            }
            _ => return Err(format!("unknown check option: {name}")),
        }
    }
    Ok(Arguments::Check(options))
}

/// The source locates modules without importing their targets and uses actual Python truthiness.
pub(crate) fn prerequisites(
    util: &pyo3::Bound<'_, pyo3::PyAny>,
    no_browser: bool,
) -> pyo3::PyResult<bool> {
    use pyo3::prelude::*;
    for name in ["numpy", "ruff", "emojikit._native", "playwright"] {
        if no_browser && name == "playwright" {
            break;
        }
        if !util.call_method1("find_spec", (name,))?.is_truthy()? {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn steps(python: &Path, ruff: &Path, sources: &[String]) -> Result<Vec<Step>, String> {
    plan(python, Some(ruff), sources, &Options::default())
}

/// Skipped stages have no prerequisite or discovery side effects; selected suites stay package-qualified.
pub fn plan(
    python: &Path,
    ruff: Option<&Path>,
    sources: &[String],
    options: &Options,
) -> Result<Vec<Step>, String> {
    if options.timeout.is_zero() {
        return Err("check timeout must be positive".into());
    }
    if !options.skip_compile && sources.is_empty() {
        return Err("no Python source files were discovered".into());
    }
    for source in sources {
        crate::source_test_inventory::source_path(source)?;
        if source.starts_with('-') || source.chars().any(char::is_control) {
            return Err("check source must be a relative Python file inside the repository".into());
        }
    }
    let mut compile = vec![
        python.as_os_str().to_owned(),
        "-X".into(),
        "utf8".into(),
        "-m".into(),
        "compileall".into(),
        "-q".into(),
    ];
    compile.extend(sources.iter().map(std::ffi::OsString::from));
    let mut tests = vec![python.as_os_str().to_owned()];
    tests.extend(["-X", "utf8", "-m", "unittest"].map(std::ffi::OsString::from));
    if options.tests.is_empty() {
        tests.extend(
            ["discover", "-s", "tests", "-t", ".", "-p", "test_*.py"].map(std::ffi::OsString::from),
        );
    } else {
        tests.extend(
            suite_names(&options.tests)?
                .into_iter()
                .map(std::ffi::OsString::from),
        );
        tests.push("-v".into());
    }
    let step = |argv| Step {
        argv,
        wall: options.timeout,
        idle: options.timeout.min(Duration::from_secs(180)),
    };
    let mut result = Vec::new();
    if !options.skip_compile {
        result.push(step(compile));
    }
    if !options.skip_lint {
        let ruff = ruff.ok_or("check requires ruff when lint is not skipped")?;
        result.push(step(vec![
            ruff.as_os_str().to_owned(),
            "check".into(),
            ".".into(),
        ]));
    }
    result.push(step(tests));
    Ok(result)
}

fn ruff(root: &Path) -> Result<PathBuf, String> {
    let local = root.join(if cfg!(windows) {
        ".venv/Scripts/ruff.exe"
    } else {
        ".venv/bin/ruff"
    });
    tooling_environment::reject_links(&local, "check linter cannot follow links")?;
    match std::fs::symlink_metadata(&local) {
        Ok(metadata) if metadata.is_file() => Ok(local),
        Ok(_) => Err("check linter must be a regular executable".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            crate::codec_process::executable("ruff")
        }
        Err(_) => Err("cannot inspect check linter".into()),
    }
}

pub fn entry(root: &Path, args: &[String]) -> i32 {
    let parsed = arguments(args);
    if let Ok(Arguments::Exit(code)) = &parsed {
        println!(
            "Usage: numera-emoji check [--python FILE|NAME] [--tests NAME[,NAME] ...] [--skip-compile] [--skip-lint] [--timeout-seconds N]\nRun retained Python source compilation, ruff and hermetic unittest discovery with the tests package guard. Explicit interpreter selection requires a compatible CPython3.11 venv matching the linked runtime. Never installs dependencies or builds Rust."
        );
        return *code;
    }
    let log = std::sync::Arc::new(RunLog::open(root, "check", vec![]));
    log.install_panic_hook();
    let result = parsed.and_then(|parsed| match parsed {
        Arguments::Check(options) => {
            run_with_options(root, &std::env::vars().collect(), &options, &log)
        }
        Arguments::Exit(code) => Ok(code),
    });
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            log.event("ERROR", "checks", &error);
            1
        }
    };
    log.finish(code)
}

/// Runs only after an explicit D1/check request. It never installs or compiles Rust.
pub fn run(root: &Path, inherited: &BTreeMap<String, String>, log: &RunLog) -> Result<i32, String> {
    run_with_options(root, inherited, &Options::default(), log)
}

pub fn run_with_options(
    root: &Path,
    inherited: &BTreeMap<String, String>,
    options: &Options,
    log: &RunLog,
) -> Result<i32, String> {
    // Validate caller-constructed options before creating cache or process workspaces.
    plan(
        Path::new("python"),
        Some(Path::new("ruff")),
        &["tests/__init__.py".into()],
        options,
    )?;
    if !cfg!(any(windows, target_os = "linux")) {
        return Err("native checks are blocked: complete descendant custody is not verified on this platform; no checks started".into());
    }
    let root = tooling_environment::physical_root(root)?;
    let selected = tooling_runtime::selected(&root, options.python.as_deref(), inherited)?;
    let mut environment = crate::tooling_test_runner::scrubbed_environment(inherited);
    environment.insert("GIT_TERMINAL_PROMPT".into(), "0".into());
    environment.insert("GIT_OPTIONAL_LOCKS".into(), "0".into());
    environment.remove("PYTHONHOME");
    environment.insert(
        "PYO3_PYTHON".into(),
        selected
            .executable
            .to_str()
            .ok_or("check interpreter path must be UTF-8")?
            .into(),
    );
    let mut environment = tooling_environment::environment(&root, &environment, false)?;
    // compileall is the one intended bytecode writer; don't leave an inherited
    // global bytecode prefix pointing outside the owning project.
    environment.remove("PYTHONPYCACHEPREFIX");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "cannot initialize check process owner")?;
    runtime.block_on(async {
        tooling_environment::confined_directory(&root, "native/runtime", false)?;
        let binary = root.join(if cfg!(windows) {
            "native/runtime/numera-emoji.exe"
        } else {
            "native/runtime/numera-emoji"
        });
        tooling_environment::reject_links(&binary, "check executable cannot follow links")?;
        if !binary.is_file() {
            return Err("check requires the installed executable; no checks started".into());
        }
        let mut probe = vec!["--checks".into()];
        if environment
            .get("NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS")
            .map(String::as_str)
            == Some("1")
        {
            probe.push("--no-browser".into());
        }
        if options.python.is_some() {
            probe.extend([
                "--python".into(),
                selected
                    .executable
                    .to_str()
                    .ok_or("check interpreter path must be UTF-8")?
                    .into(),
            ]);
        }
        let report =
            match tooling_runtime::inspect_executable(&root, &binary, &probe, &environment, log)
                .await
            {
                Ok(report) => report,
                Err(error) => {
                    log.event(
                        "ERROR",
                        "checks",
                        &format!(
                            "check prerequisite probe failed: {error}; no source checks started"
                        ),
                    );
                    return Ok(2);
                }
            };
        if report["check_prerequisites"] != true {
            log.event(
                "ERROR",
                "checks",
                "Missing check dependencies; no source checks started",
            );
            return Ok(2);
        }
        let mut sources = Vec::new();
        if !options.skip_compile {
            let git = crate::codec_process::executable("git")?;
            let listed = tooling_process::capture(
                &root,
                &[
                    git.into_os_string(),
                    "-c".into(),
                    "core.quotePath=false".into(),
                    "ls-files".into(),
                    "--cached".into(),
                    "--others".into(),
                    "--exclude-standard".into(),
                    "-z".into(),
                    "--".into(),
                    "*.py".into(),
                ],
                &environment,
                Duration::from_secs(10),
                Duration::from_secs(5),
                log,
            )
            .await?;
            if listed.code != 0 {
                return Ok(listed.code);
            }
            for source in listed
                .stdout
                .split('\0')
                .filter(|source| !source.is_empty())
            {
                crate::source_test_inventory::source_path(source)?;
                if source.starts_with('-') || source.chars().any(char::is_control) {
                    return Err("invalid check source path".into());
                }
                let path = root.join(source);
                tooling_environment::reject_links(&path, "check source cannot follow links")?;
                match std::fs::symlink_metadata(&path) {
                    Ok(metadata) if metadata.is_file() => sources.push(source.to_owned()),
                    Ok(_) => return Err("check source is not a regular file".into()),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => return Err("cannot inspect check source".into()),
                }
            }
            if !listed.stdout.is_empty() && !listed.stdout.ends_with('\0') {
                return Err("Git returned an incomplete check source list".into());
            }
            sources.sort();
            sources.dedup();
        }
        let ruff = if options.skip_lint {
            None
        } else {
            Some(ruff(&root)?)
        };
        let mut labels = Vec::new();
        if !options.skip_compile {
            labels.push("Byte-compile all sources");
        }
        if !options.skip_lint {
            labels.push("Lint (ruff)");
        }
        labels.push("Unit tests");
        for (label, step) in labels.into_iter().zip(plan(
            &selected.executable,
            ruff.as_deref(),
            &sources,
            options,
        )?) {
            // Safe-path suppresses CWD for explicit unittest names. Admit only this
            // physical root, not any inherited PYTHONPATH or a different tests package.
            let mut step_environment = environment.clone();
            if label == "Unit tests" {
                step_environment.insert(
                    "PYTHONPATH".into(),
                    root.to_str().ok_or("check root must be UTF-8")?.into(),
                );
                step_environment.insert("PYTHONDONTWRITEBYTECODE".into(), "1".into());
            }
            println!("==> {label}");
            log.event("INFO", "checks", label);
            #[cfg(any(windows, target_os = "linux"))]
            let completed = tooling_process::capture_descendants(
                &root,
                &step.argv,
                &step_environment,
                step.wall,
                step.idle,
                log,
            )
            .await?;
            #[cfg(not(any(windows, target_os = "linux")))]
            let completed = tooling_process::capture(
                &root,
                &step.argv,
                &step_environment,
                step.wall,
                step.idle,
                log,
            )
            .await?;
            print!("{}", crate::logging::redact(&completed.stdout, &[]));
            eprint!("{}", crate::logging::redact(&completed.stderr, &[]));
            if completed.code != 0 {
                return Ok(completed.code);
            }
        }
        println!("[OK] All retained source checks passed.");
        Ok(0)
    })
}
