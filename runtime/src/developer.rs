//! Explicit contributor commands. No operator configuration, interpreter or historical oracle.
use crate::{logging::RunLog, tooling_environment, tooling_process};
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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

pub struct Tools {
    pub cargo: PathBuf,
    pub rustc: PathBuf,
    pub npm: Option<PathBuf>,
    pub driver: PathBuf,
}
impl Tools {
    fn discover() -> Result<Self, String> {
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
const PREREQUISITES: &[&str] = &[
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

fn cardinality(count: usize) -> Result<(), String> {
    if !(1..=10000).contains(&count) {
        Err("Benchmark requires 1..10000 hashes".into())
    } else {
        Ok(())
    }
}
fn repetitions(repeats: usize) -> Result<(), String> {
    if repeats == 0 || repeats > 21 || repeats.is_multiple_of(2) {
        Err("repeats must be odd and 1..21".into())
    } else {
        Ok(())
    }
}
pub fn generated_hashes(count: usize, seed: u64) -> Result<Vec<u64>, String> {
    cardinality(count)?;
    let mut state = seed;
    Ok((0..count)
        .map(|_| {
            state = state.wrapping_add(0x9e3779b97f4a7c15);
            let mut value = state;
            value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
            value ^ (value >> 31)
        })
        .collect())
}
fn decimal(bytes: &[u8]) -> Result<u64, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "catalog phash contains an invalid integer")?
        .trim();
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text.as_bytes()[1..]),
        Some(b'+') => (false, &text.as_bytes()[1..]),
        _ => (false, text.as_bytes()),
    };
    let mut number = 0_u64;
    let mut previous_digit = false;
    if digits.is_empty() {
        return Err("catalog phash contains an invalid integer".into());
    }
    for (index, byte) in digits.iter().copied().enumerate() {
        if byte.is_ascii_digit() {
            number = number.wrapping_mul(10).wrapping_add(u64::from(byte - b'0'));
            previous_digit = true;
        } else if byte == b'_'
            && previous_digit
            && digits.get(index + 1).is_some_and(u8::is_ascii_digit)
        {
            previous_digit = false;
        } else {
            return Err("catalog phash contains an invalid integer".into());
        }
    }
    Ok(if negative {
        number.wrapping_neg()
    } else {
        number
    })
}
fn real(value: f64) -> Result<u64, String> {
    if !value.is_finite() {
        return Err("catalog phash contains a nonfinite number".into());
    }
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1023 - 52;
    let mantissa = (bits & ((1_u64 << 52) - 1)) | (1_u64 << 52);
    let magnitude = if exponent >= 64 || exponent <= -64 {
        0
    } else if exponent >= 0 {
        mantissa.wrapping_shl(exponent as u32)
    } else {
        mantissa >> (-exponent as u32)
    };
    Ok(if value.is_sign_negative() {
        magnitude.wrapping_neg()
    } else {
        magnitude
    })
}
pub fn load_hashes(path: &Path) -> Result<Vec<u64>, String> {
    tooling_environment::reject_links(path, "benchmark catalog cannot follow links")?;
    if !path.is_file() {
        return Err("benchmark catalog must be an existing regular file".into());
    }
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| error.to_string())?;
    db.busy_timeout(Duration::from_secs(5))
        .map_err(|error| error.to_string())?;
    let mut statement = db.prepare("SELECT phash FROM items WHERE phash IS NOT NULL ORDER BY position,content_key LIMIT 10001").map_err(|error| error.to_string())?;
    let mut rows = statement.query([]).map_err(|error| error.to_string())?;
    let mut hashes = Vec::new();
    while let Some(row) = rows.next().map_err(|error| error.to_string())? {
        let hash = match row.get_ref(0).map_err(|error| error.to_string())? {
            ValueRef::Integer(value) => value as u64,
            ValueRef::Real(value) => real(value)?,
            ValueRef::Text(bytes) | ValueRef::Blob(bytes) => decimal(bytes)?,
            ValueRef::Null => return Err("catalog returned an unexpected null phash".into()),
        };
        hashes.push(hash);
    }
    cardinality(hashes.len())?;
    Ok(hashes)
}
pub fn median(samples: &[f64]) -> Result<f64, String> {
    if samples.is_empty()
        || samples.len().is_multiple_of(2)
        || samples
            .iter()
            .any(|sample| !sample.is_finite() || *sample < 0.0)
    {
        return Err("Benchmark clock returned invalid elapsed samples".into());
    }
    let mut samples = samples.to_vec();
    samples.sort_by(f64::total_cmp);
    Ok(samples[samples.len() / 2])
}
#[derive(Debug)]
pub struct Benchmark {
    pub hashes: usize,
    pub samples: usize,
    pub median_secs: f64,
    pub order: Vec<usize>,
    pub candidates: usize,
}
impl Benchmark {
    pub fn line(&self) -> String {
        let checksum = self
            .order
            .iter()
            .fold(0xcbf29ce484222325_u64, |value, index| {
                (value ^ *index as u64).wrapping_mul(0x100000001b3)
            });
        format!(
            "Rust similarity: hashes={} samples={} median={:.6}s order_checksum={checksum:016x} shortlist_candidates={}",
            self.hashes, self.samples, self.median_secs, self.candidates
        )
    }
}
pub fn benchmark(hashes: &[u64], repeats: usize) -> Result<Benchmark, String> {
    cardinality(hashes.len())?;
    repetitions(repeats)?;
    let mut samples = Vec::with_capacity(repeats);
    let mut order = None;
    for _ in 0..repeats {
        let started = Instant::now();
        let result = crate::similarity::greedy(std::hint::black_box(hashes));
        samples.push(started.elapsed().as_secs_f64());
        if result.len() != hashes.len()
            || result.iter().copied().collect::<BTreeSet<_>>() != (0..hashes.len()).collect()
        {
            return Err("Similarity returned an invalid permutation".into());
        }
        if order.as_ref().is_some_and(|order| order != &result) {
            return Err("Similarity order changed between identical repeated inputs".into());
        }
        order = Some(result);
    }
    let values = hashes.iter().copied().map(Some).collect::<Vec<_>>();
    let mut candidates = 0;
    for query in hashes.iter().take(20) {
        for threshold in [-1, 0, 16] {
            candidates += crate::similarity::near(&values, *query, threshold)?.len();
        }
    }
    Ok(Benchmark {
        hashes: hashes.len(),
        samples: repeats,
        median_secs: median(&samples)?,
        order: order.ok_or("benchmark produced no order")?,
        candidates,
    })
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

#[cfg(target_os = "linux")]
fn current_process_start() -> Result<u64, String> {
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

fn sandbox(root: &Path, args: &[String], timeout: Duration, log: &RunLog) -> Result<i32, String> {
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
