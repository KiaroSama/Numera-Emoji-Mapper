//! Explicit Cargo test runner; never loads operator configuration or changes the parent environment.
use crate::{logging::RunLog, tooling_environment, tooling_process};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

const RUNTIME_KEYS: &[&str] = &[
    "PATH",
    "SystemRoot",
    "SYSTEMROOT",
    "WINDIR",
    "PATHEXT",
    "LD_LIBRARY_PATH",
    "DYLD_LIBRARY_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
    "PYTHONHOME",
    "PYO3_PYTHON",
    "LANG",
    "LC_ALL",
    "NUMERA_STATIC_CONVERT_ORACLE",
    "NUMERA_PROVIDER_IMAGE_ORACLE",
    "NUMERA_LOTTIE_PREVIEW_ORACLE",
    "NUMERA_GALLERY_CODEC_ORACLE",
    "NUMERA_MEDIA_FIXTURE_WORKER",
    "NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS",
    "NUMERA_REAL_CATALOG_FIXTURE",
    "NUMERA_REAL_COIN_DIRECTORY",
    "NUMERA_REAL_VIDEO_FIXTURE",
    "HOOKMAKER_MAX_TEST_WORKERS",
];

pub fn scrubbed_environment(original: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut child = RUNTIME_KEYS
        .iter()
        .filter(|&&key| !cfg!(windows) || key != "SYSTEMROOT")
        .filter_map(|&key| {
            let value = original.get(key).or_else(|| {
                cfg!(windows)
                    .then(|| {
                        original
                            .iter()
                            .find(|(name, _)| name.eq_ignore_ascii_case(key))
                    })
                    .flatten()
                    .map(|(_, value)| value)
            })?;
            Some((key.to_owned(), value.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    for (key, value) in [
        ("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1"),
        ("TELEGRAM_API_BASE", "http://127.0.0.1:9"),
        ("BRAND_LOGO_BOTS", ""),
        ("COLLECTION_PACK_BASE", "YourBrand_Emoji_Packs"),
        ("COIN_PACK_BASE", "cryptoemoji"),
        ("COIN_PACK_TITLE", "@YourBrand Crypto Emoji"),
        ("PYTHONUTF8", "1"),
        ("PYTHONNOUSERSITE", "1"),
        ("PYTHONSAFEPATH", "1"),
        ("RUST_BACKTRACE", "1"),
    ] {
        child.insert(key.into(), value.into());
    }
    child
}

pub fn arguments(args: &[String], ceiling: usize) -> Result<Vec<String>, String> {
    if ceiling == 0 || ceiling > 2 {
        return Err("test worker ceiling must be1..2".into());
    }
    let mut result = args.to_vec();
    let mut present = false;
    let mut index = 0;
    while index < result.len() {
        if result[index] == "--test-threads" {
            index += 1;
            let value = result
                .get_mut(index)
                .ok_or("--test-threads requires a value")?;
            let count = value
                .parse::<usize>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or("--test-threads must be a positive integer")?;
            *value = count.min(ceiling).to_string();
            present = true;
        } else if let Some(value) = result[index].strip_prefix("--test-threads=") {
            let count = value
                .parse::<usize>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or("--test-threads must be a positive integer")?;
            result[index] = format!("--test-threads={}", count.min(ceiling));
            present = true;
        }
        index += 1;
    }
    if !present {
        result.push(format!("--test-threads={ceiling}"));
    }
    Ok(result)
}

pub(crate) fn inherited_runtime() -> Result<BTreeMap<String, String>, String> {
    let mut values = BTreeMap::new();
    for name in RUNTIME_KEYS {
        if let Some(value) = std::env::var_os(name) {
            values.insert(
                (*name).into(),
                value
                    .into_string()
                    .map_err(|_| format!("test prerequisite {name} must be UTF-8"))?,
            );
        }
    }
    Ok(values)
}

/// Only the outer runner may remove suite artifacts, after capture verified descendant quiescence.
pub fn clean_suite_workspace(root: &Path, workspace: &Path, passed: bool) -> Result<(), String> {
    tooling_environment::reject_links(workspace, "suite workspace cannot follow links")?;
    let root = tooling_environment::physical_root(root)?;
    let workspace = tooling_environment::physical_root(workspace)?;
    let parent = tooling_environment::confined_directory(&root, "logs/tmp/tooling", false)?;
    let prefix = format!("child-{}-", std::process::id());
    if workspace.parent() != Some(parent.as_path())
        || !workspace
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix(&prefix))
            .is_some_and(|suffix| {
                !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
            })
    {
        return Err("suite workspace does not belong to this runner".into());
    }
    let members = std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect suite workspace")?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|_| "cannot inspect suite workspace")?;
    if members.is_empty() {
        return std::fs::remove_dir(workspace)
            .map_err(|_| "cannot remove empty suite workspace".into());
    }
    if !passed {
        eprintln!(
            "ERROR: failed suite artifacts retained at {}",
            workspace.display()
        );
        return Ok(());
    }
    fn inspect(path: &Path, depth: usize, left: &mut usize) -> Result<(), String> {
        if depth > 16 || *left == 0 {
            return Err("suite cleanup inventory exceeded its bound".into());
        }
        *left -= 1;
        tooling_environment::reject_links(path, "suite artifacts cannot follow links")?;
        let metadata =
            std::fs::symlink_metadata(path).map_err(|_| "cannot inspect suite artifact")?;
        if metadata.is_dir() {
            if crate::process_owner::cleanup_pending(path) {
                return Err("suite child custody remains unresolved".into());
            }
            for entry in std::fs::read_dir(path).map_err(|_| "cannot inspect suite directory")? {
                inspect(
                    &entry.map_err(|_| "cannot inspect suite member")?.path(),
                    depth + 1,
                    left,
                )?;
            }
        } else if !metadata.is_file() {
            return Err("unsupported suite artifact".into());
        }
        Ok(())
    }
    let mut left = 2048;
    for path in &members {
        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("native-browser-"))
            || !path.is_dir()
        {
            return Err("unexpected suite artifact; workspace retained".into());
        }
        inspect(path, 0, &mut left)?;
        let ready = path.join("cleanup-ready");
        if !ready.is_file() {
            return Err("browser shutdown acknowledgement is not a regular file".into());
        }
        if std::fs::metadata(&ready)
            .map_err(|_| "browser shutdown is not acknowledged; workspace retained")?
            .len()
            != 25
            || std::fs::read(ready).map_err(|_| "cannot read browser shutdown acknowledgement")?
                != b"native-browser-stopped-v1"
        {
            return Err("browser shutdown is not acknowledged; workspace retained".into());
        }
    }
    std::fs::remove_dir_all(workspace).map_err(|_| "suite artifacts could not be removed".into())
}

pub fn plan(root: &Path, args: &[String]) -> i32 {
    if args == ["--help"] || args == ["-h"] {
        println!(
            "Usage: numera-emoji test-plan --baseline SHA --output FILE [--lib] [--bin NAME]... [--test NAME]...\nCreate a pre-build input claim from offline Cargo metadata and tracked public files; not verified coverage."
        );
        return 0;
    }
    let log = std::sync::Arc::new(RunLog::open(root, "test-plan", vec![]));
    log.install_panic_hook();
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "cannot initialize test plan runtime".to_owned())
        .and_then(|runtime| runtime.block_on(crate::tooling_test_plan::produce(root, args, &log)));
    let code = match result {
        Ok(path) => {
            log.event("INFO", "test-plan", "untrusted pre-build plan preserved");
            println!("test plan retained at {}", path.display());
            0
        }
        Err(error) => {
            log.event("ERROR", "test-plan", &error);
            1
        }
    };
    log.finish(code);
    code
}

pub fn run(root: &Path, args: &[String]) -> i32 {
    if args == ["--help"] || args == ["-h"] {
        println!(
            "Usage: numera-emoji test-runner [--observe MANIFEST | --observe-cargo PLAN BUILD_LOG] EXECUTABLE [LIBTEST_ARGUMENTS]\nOptional compiled-case observations remain untrusted, not verified migration coverage."
        );
        return 0;
    }
    let log = std::sync::Arc::new(RunLog::open(root, "test-runner", vec![]));
    log.install_panic_hook();
    let result = (|| {
        let (manifest, cargo, args) = match args.first().map(String::as_str) {
            Some("--observe") => {
                let manifest = args.get(1).filter(|value| !value.is_empty()).ok_or(
                    "test-runner --observe requires MANIFEST EXECUTABLE [LIBTEST_ARGUMENTS]",
                )?;
                (Some(Path::new(manifest)), None, &args[2..])
            }
            Some("--observe-cargo") => {
                let plan = args.get(1).filter(|value| !value.is_empty())
                    .ok_or("test-runner --observe-cargo requires PLAN BUILD_LOG EXECUTABLE [LIBTEST_ARGUMENTS]")?;
                let build = args.get(2).filter(|value| !value.is_empty())
                    .ok_or("test-runner --observe-cargo requires PLAN BUILD_LOG EXECUTABLE [LIBTEST_ARGUMENTS]")?;
                (None, Some((Path::new(plan), Path::new(build))), &args[3..])
            }
            _ => (None, None, args),
        };
        // Linux and Windows use descendant custody; Darwin uses the existing process-group
        // owner plus each lifecycle test's own verified child owner and cleanup markers.
        if !cfg!(any(windows, target_os = "linux", target_os = "macos")) {
            return Err("POSIX test-runner is blocked: nested-session cleanup is not verified; no child started".into());
        }
        let executable = args.first().ok_or("test-runner requires an executable")?;
        let executable = Path::new(executable);
        let executable = if executable.is_absolute() {
            executable.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|_| "cannot locate test executable")?
                .join(executable)
        };
        tooling_environment::reject_links(&executable, "test executable cannot follow links")?;
        let root = tooling_environment::physical_root(root)?;
        let executable = crate::migration_bundle::resolved(&executable)?;
        if !executable.starts_with(&root) || !executable.is_file() {
            return Err("test executable must be a regular project-contained file".into());
        }
        let observed_executable = executable.clone();
        let inherited = inherited_runtime()?;
        let ceiling = inherited
            .get("HOOKMAKER_MAX_TEST_WORKERS")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(2)
            .min(2);
        #[cfg(target_os = "linux")]
        let mut argv = vec![
            std::env::current_exe()
                .map_err(|_| "cannot locate native test bootstrap")?
                .into_os_string(),
            "test-net-bootstrap".into(),
            "--".into(),
            executable.into_os_string(),
        ];
        #[cfg(not(target_os = "linux"))]
        let mut argv = vec![executable.into_os_string()];
        let test_args = arguments(&args[1..], ceiling)?;
        let prefix = argv.len();
        argv.extend(test_args.iter().map(OsString::from));
        let mut environment =
            tooling_environment::environment(&root, &scrubbed_environment(&inherited), false)?;
        let browsers =
            tooling_environment::confined_directory(&root, ".cache/ms-playwright", true)?;
        environment.insert(
            "PLAYWRIGHT_BROWSERS_PATH".into(),
            browsers
                .to_str()
                .ok_or("browser cache path must be UTF-8")?
                .into(),
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "cannot initialize test owner runtime")?;
        // Same conservative ceiling as the existing hosted checkpoint; narrower test bounds still apply.
        let cwd = tooling_environment::physical_root(
            &std::env::current_dir().map_err(|_| "cannot inspect test working directory")?,
        )?;
        if !cwd.starts_with(&root) {
            return Err("test working directory must stay project-contained".into());
        }
        let started = (manifest.is_some() || cargo.is_some()).then(std::time::Instant::now);
        let cargo_manifest = if let Some((plan, build)) = cargo {
            match crate::tooling_test_cargo::prepare(&root, plan, build, &observed_executable)? {
                crate::tooling_test_cargo::Selection::Observe(path) => Some(path),
                crate::tooling_test_cargo::Selection::NonCoverage { reason } => {
                    log.event(
                        "INFO",
                        "test-observation",
                        &format!("explicit noncoverage target: {reason}"),
                    );
                    None
                }
            }
        } else {
            None
        };
        let manifest = manifest.or(cargo_manifest.as_deref());
        let mut observation = if let Some(manifest) = manifest {
            let parent =
                tooling_environment::confined_directory(&root, "logs/test-observations", true)?;
            let mut random = [0_u8; 16];
            getrandom::fill(&mut random)
                .map_err(|_| "cannot allocate test observation identity")?;
            let name = random
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let store = parent.join(name);
            std::fs::create_dir(&store)
                .map_err(|_| "cannot create exclusive test observation storage")?;
            eprintln!("test observation retained at {}", store.display());
            Some(crate::tooling_test_observation::Observation::begin(
                &root,
                &store,
                manifest,
                &observed_executable,
                &test_args,
            )?)
        } else {
            None
        };
        let remaining_wall = || {
            started.map_or(Ok(Duration::from_secs(900)), |started| {
                Duration::from_secs(900)
                    .checked_sub(started.elapsed())
                    .filter(|remaining| !remaining.is_zero())
                    .ok_or("test observation exhausted suite budget".to_owned())
            })
        };
        let workspace = tooling_process::workspace(&root)?;
        let mut custody_settled = true;
        let execution = (|| {
            environment.insert(
                "NUMERA_TEST_WORKSPACE".into(),
                workspace
                    .to_str()
                    .ok_or("suite workspace must be UTF-8")?
                    .into(),
            );
            if let Some(observation) = observation.as_mut() {
                let mut listing = argv[..prefix].to_vec();
                listing.extend(observation.list_args().into_iter().map(OsString::from));
                let wall = remaining_wall()?.min(Duration::from_secs(30));
                custody_settled = false;
                #[cfg(any(windows, target_os = "linux"))]
                let listed = runtime.block_on(tooling_process::capture_descendants(
                    &cwd,
                    &listing,
                    &environment,
                    wall,
                    wall,
                    &log,
                ))?;
                #[cfg(not(any(windows, target_os = "linux")))]
                let listed = runtime.block_on(tooling_process::capture(
                    &cwd,
                    &listing,
                    &environment,
                    wall,
                    wall,
                    &log,
                ))?;
                custody_settled = true;
                if listed.code != 0 {
                    print!("{}", crate::logging::redact(&listed.stdout, &[]));
                    eprint!("{}", crate::logging::redact(&listed.stderr, &[]));
                }
                argv.truncate(prefix);
                argv.extend(
                    observation
                        .run_args(listed.code)?
                        .into_iter()
                        .map(OsString::from),
                );
            }
            let wall = remaining_wall()?;
            custody_settled = false;
            #[cfg(any(windows, target_os = "linux"))]
            let completed = runtime.block_on(tooling_process::capture_descendants(
                &cwd,
                &argv,
                &environment,
                wall,
                wall.min(Duration::from_secs(180)),
                &log,
            ))?;
            #[cfg(not(any(windows, target_os = "linux")))]
            let completed = runtime.block_on(tooling_process::capture(
                &cwd,
                &argv,
                &environment,
                wall,
                wall.min(Duration::from_secs(180)),
                &log,
            ))?;
            custody_settled = true;
            print!("{}", crate::logging::redact(&completed.stdout, &[]));
            eprint!("{}", crate::logging::redact(&completed.stderr, &[]));
            let observation_error = observation.and_then(|observation| {
                observation
                    .finish(completed.code)
                    .and_then(|report| {
                        if let Some(manifest) = cargo_manifest.as_deref() {
                            crate::tooling_test_cargo::complete(&root, manifest, &report)?;
                        }
                        Ok(report)
                    })
                    .err()
            });
            if let Some(error) = &observation_error {
                log.event("ERROR", "test-observation", error);
            }
            Ok::<i32, String>(if completed.code == 0 && observation_error.is_some() {
                1
            } else {
                completed.code
            })
        })();
        // An owner error may retain descendants even when this separate suite workspace is empty.
        if !custody_settled {
            return Err(format!(
                "{}; suite artifacts retained at {}",
                execution
                    .err()
                    .unwrap_or_else(|| "test child custody is unresolved".into()),
                workspace.display()
            ));
        }
        let cleanup = clean_suite_workspace(
            &root,
            &workspace,
            execution.as_ref().is_ok_and(|code| *code == 0),
        );
        match (execution, cleanup) {
            (Ok(code), Ok(())) => Ok(code),
            (Err(error), Ok(())) => Err(format!(
                "{error}; inspect retained test observations and nonempty suite artifacts"
            )),
            (result, Err(cleanup)) => Err(format!(
                "{}; {cleanup}; suite artifacts retained at {}",
                result
                    .err()
                    .unwrap_or_else(|| "suite cleanup failed".into()),
                workspace.display()
            )),
        }
    })();
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            log.event("ERROR", "test-runner", &error);
            1
        }
    };
    log.finish(code);
    code
}
