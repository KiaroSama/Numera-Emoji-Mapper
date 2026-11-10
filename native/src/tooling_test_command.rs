//! One original Cargo test invocation with retained split diagnostics and untrusted observations.
use crate::{logging::RunLog, tooling_environment, tooling_process};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

struct Options {
    baseline: String,
    cargo: Vec<String>,
    selectors: Vec<String>,
    libtest: Vec<String>,
}
fn error(message: &str) -> String {
    format!("test-cargo refused: {message}; retained inputs are untrusted, not coverage acceptance")
}
fn text(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_control)
}
fn arguments(args: &[String]) -> Result<Options, String> {
    if args.len() > 1024 || args.iter().map(String::len).sum::<usize>() > 65536 {
        return Err(error("argument budget exceeded"));
    }
    if args.len() < 3 || args[0] != "--baseline" || args[2] != "--" {
        return Err(error("use --baseline SHA -- [original Cargo test options]"));
    }
    let baseline = &args[1];
    if baseline.len() != 40
        || !baseline
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(error("baseline must be exact lowercase40hex"));
    }
    let rest = &args[3..];
    let boundary = rest.iter().position(|s| s == "--").unwrap_or(rest.len());
    let cargo = rest[..boundary].to_vec();
    let libtest = rest.get(boundary + 1..).unwrap_or(&[]).to_vec();
    crate::tooling_test_observation::validate_args(&libtest)?;
    let mut selectors = Vec::new();
    let mut seen = BTreeSet::new();
    let mut filter = false;
    let mut index = 0;
    while index < cargo.len() {
        let key = cargo[index].as_str();
        index += 1;
        if matches!(key, "--lib" | "--locked" | "--no-fail-fast") {
            if !seen.insert((key.to_owned(), String::new())) {
                return Err(error("duplicate Cargo option"));
            }
            if key == "--lib" {
                selectors.push(key.into());
            }
        } else if matches!(key, "--config" | "--manifest-path" | "--bin" | "--test") {
            let value = cargo
                .get(index)
                .filter(|s| text(s) && !s.starts_with('-'))
                .ok_or_else(|| error("Cargo option requires a control-free value"))?;
            index += 1;
            if !seen.insert((key.to_owned(), value.clone())) {
                return Err(error("duplicate Cargo option/target"));
            }
            match key {
                "--config" if value == ".cargo/tests.toml" => {}
                "--manifest-path" if value == "native/Cargo.toml" => {}
                "--bin" | "--test"
                    if !value
                        .chars()
                        .any(|c| matches!(c, '*' | '?' | '[' | ']' | '/' | '\\' | ':')) =>
                {
                    selectors.extend([key.into(), value.clone()]);
                }
                _ => {
                    return Err(error(
                        "only the native manifest, test config and exact named targets are supported",
                    ));
                }
            }
        } else if !key.starts_with('-') && text(key) && !filter {
            filter = true;
        } else {
            return Err(error(
                "unsupported Cargo option or multiple test-name filters",
            ));
        }
    }
    for (key, value) in [
        ("--config", ".cargo/tests.toml"),
        ("--manifest-path", "native/Cargo.toml"),
        ("--locked", ""),
    ] {
        if !seen.contains(&(key.to_owned(), value.to_owned())) {
            return Err(error(
                "explicit native --manifest-path, .cargo/tests.toml and --locked are required",
            ));
        }
    }
    Ok(Options {
        baseline: baseline.clone(),
        cargo,
        selectors,
        libtest,
    })
}
fn host() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") if cfg!(target_env = "msvc") => Ok("x86_64-pc-windows-msvc"),
        ("linux", "x86_64") if cfg!(target_env = "gnu") => Ok("x86_64-unknown-linux-gnu"),
        // Darwin's nested descendant custody is still an explicit acceptance blocker.
        _ => Err(error(
            "this host lacks the admitted native Cargo observation owner",
        )),
    }
}
fn runner_config(triple: &str, values: &[&str]) -> Result<String, String> {
    if !matches!(
        triple,
        "x86_64-pc-windows-msvc" | "x86_64-unknown-linux-gnu"
    ) || values.len() != 5
        || values.iter().any(|s| !text(s))
    {
        return Err(error(
            "runner requires an admitted host and five literal control-free arguments",
        ));
    }
    // For these control-free strings, JSON quoting is also valid TOML basic-string quoting.
    let array = serde_json::to_string(values)
        .map_err(|_| error("cannot encode literal runner arguments"))?;
    Ok(format!(
        "[target.{triple}]\nrunner = {array}\n[target.'cfg(windows)']\nrunner = {array}\n[target.'cfg(unix)']\nrunner = {array}\n"
    ))
}
fn cargo_arguments(options: &Options, config: &Path) -> Result<Vec<String>, String> {
    let config = config
        .to_str()
        .filter(|s| text(s))
        .ok_or_else(|| error("runner config path must be UTF-8"))?;
    let mut args = vec!["test".into()];
    args.extend(options.cargo.iter().cloned());
    args.extend([
        "--message-format=json".into(),
        "--config".into(),
        config.into(),
    ]);
    if !options.libtest.is_empty() {
        args.push("--".into());
        args.extend(options.libtest.iter().cloned());
    }
    Ok(args)
}
fn observed_targets(plan: &serde_json::Value) -> Result<(), String> {
    if !plan["targets"]
        .as_array()
        .is_some_and(|a| a.iter().any(|t| t["mode"] == "observe"))
    {
        return Err(error(
            "the selected plan has no observed target; noncoverage harnesses cannot be the only test evidence",
        ));
    }
    Ok(())
}
fn environment(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut inherited = crate::tooling_build::inherited()?;
    inherited.extend(crate::tooling_test_runner::scrubbed_environment(
        &crate::tooling_test_runner::inherited_runtime()?,
    ));
    let mut environment = tooling_environment::environment(root, &inherited, false)?;
    // Keep the installed shared toolchain, but all compilation output stays project-contained.
    environment.insert(
        "CARGO_TARGET_DIR".into(),
        root.join("native/target")
            .to_str()
            .ok_or_else(|| error("target path must be UTF-8"))?
            .into(),
    );
    environment.insert("RUSTUP_AUTO_INSTALL".into(), "0".into());
    let workers = environment
        .get("HOOKMAKER_MAX_TEST_WORKERS")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(2)
        .clamp(1, 2);
    environment.insert("CARGO_BUILD_JOBS".into(), workers.to_string());
    Ok(environment)
}
async fn execute(root: &Path, args: &[String], log: &RunLog) -> Result<i32, String> {
    let options = arguments(args)?;
    let triple = host()?;
    let root = tooling_environment::physical_root(root)?;
    let cwd = tooling_environment::physical_root(
        &std::env::current_dir().map_err(|_| error("cannot inspect Cargo CWD"))?,
    )?;
    if cwd != root {
        return Err(error(
            "invoke from the physical project root to preserve Cargo configuration/CWD",
        ));
    }
    let app = std::env::current_exe().map_err(|_| error("cannot locate runner executable"))?;
    let name = crate::tooling_test_observation::manifest::relative(&root, &app)?;
    crate::tooling_test_observation::manifest::regular(&app)?;
    if cfg!(windows) && name.starts_with("native/target/") {
        return Err(error(
            "Windows requires a project-local copied driver outside native/target so Cargo can replace its binary",
        ));
    }
    let environment = environment(&root)?;
    let cargo = crate::codec_process::executable("cargo")?;
    let parent = tooling_environment::confined_directory(&root, "logs/tmp", true)?;
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|_| error("cannot allocate Cargo invocation identity"))?;
    let id = random
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let stage = parent.join(format!("test-cargo-{id}"));
    std::fs::create_dir(&stage)
        .map_err(|_| error("cannot create exclusive Cargo observation stage"))?;
    eprintln!("Cargo invocation inputs retained at {}", stage.display());
    let plan = stage.join("plan.json");
    let stdout = stage.join("build.log");
    let stderr = stage.join("stderr.log");
    let config = stage.join("runner.toml");
    let path_text = |p: &Path| {
        p.to_str()
            .filter(|s| text(s))
            .map(str::to_owned)
            .ok_or_else(|| error("invocation paths must be control-free UTF-8"))
    };
    let mut producer = vec![
        "--baseline".into(),
        options.baseline.clone(),
        "--output".into(),
        path_text(&plan)?,
    ];
    producer.extend(options.selectors.iter().cloned());
    crate::tooling_test_plan::produce(&root, &producer, log).await?;
    let (_, plan_bytes, _) = crate::tooling_test_observation::manifest::read_file(
        &plan,
        crate::tooling_test_observation::manifest::SMALL_LIMIT,
        true,
    )?;
    observed_targets(
        &serde_json::from_slice(&plan_bytes)
            .map_err(|_| error("generated PLAN is invalid JSON"))?,
    )?;
    let paths = [
        path_text(&app)?,
        "test-runner".into(),
        "--observe-cargo".into(),
        path_text(&plan)?,
        path_text(&stdout)?,
    ];
    let config_bytes = runner_config(
        triple,
        &paths.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    crate::atomic::write_bytes(&config, config_bytes.as_bytes())
        .map_err(|_| error("cannot persist exclusive runner config"))?;
    let argv = std::iter::once(cargo.into_os_string())
        .chain(
            cargo_arguments(&options, &config)?
                .into_iter()
                .map(OsString::from),
        )
        .collect::<Vec<_>>();
    let done = tooling_process::capture_retained(
        &root,
        &argv,
        &environment,
        (&stdout, &stderr),
        Duration::from_secs(900),
        Duration::from_secs(180),
        log,
    )
    .await?;
    // Raw Cargo metadata and foreign diagnostics stay local; console capture may be uploaded by CI.
    log.event(
        "INFO",
        "test-cargo",
        &format!(
            "Cargo exit={} stdout_bytes={} stderr_bytes={}; inspect retained private split logs",
            done.code,
            done.stdout.len(),
            done.stderr.len()
        ),
    );
    if done.code != 0 {
        // Report only bounded, redacted compiler summaries. Source excerpts,
        // test bodies and arbitrary child output stay in the private split logs.
        for message in done
            .stdout
            .lines()
            .filter_map(|line| {
                let value: serde_json::Value = serde_json::from_str(line).ok()?;
                (value["reason"] == "compiler-message" && value["message"]["level"] == "error")
                    .then(|| value["message"]["message"].as_str().map(str::to_owned))?
            })
            .chain(
                done.stderr
                    .lines()
                    .filter_map(|line| line.strip_prefix("error: ").map(str::to_owned)),
            )
            .take(8)
        {
            let message = message.chars().take(512).collect::<String>();
            log.event(
                "ERROR",
                "cargo-diagnostic",
                &crate::logging::redact(&message, &[]),
            );
        }
    }
    // Cargo exit0 does not establish observed cases, source parity or an authenticated CI receipt.
    log.event(
        "INFO",
        "test-cargo",
        "original Cargo invocation settled; observations remain untrusted",
    );
    let postflight = crate::tooling_test_cargo::postflight::check(
        &root,
        &plan,
        &stdout,
        &done.stderr,
        done.code,
    );
    if let Err(error) = postflight {
        log.event("ERROR", "test-cargo-postflight", &error);
        if done.code == 0 {
            return Ok(1);
        }
    }
    Ok(done.code)
}
pub fn run(root: &Path, args: &[String]) -> i32 {
    if args == ["--help"] || args == ["-h"] {
        println!(
            "Usage: numera-emoji test-cargo --baseline SHA -- --config .cargo/tests.toml --manifest-path native/Cargo.toml --locked [--lib] [--bin NAME]... [--test NAME]... [FILTER] [-- LIBTEST_ARGUMENTS]\nOne original Cargo invocation; split logs and case observations remain untrusted. No build or test runs for help."
        );
        return 0;
    }
    let log = std::sync::Arc::new(RunLog::open(root, "test-cargo", vec![]));
    log.install_panic_hook();
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| error("cannot initialize Cargo observation runtime"))
        .and_then(|runtime| runtime.block_on(execute(root, args, &log)));
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            log.event("ERROR", "test-cargo", &error);
            1
        }
    };
    log.finish(code);
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(extra: &[&str]) -> Vec<String> {
        [
            "--baseline",
            &"a".repeat(40),
            "--",
            "--config",
            ".cargo/tests.toml",
            "--manifest-path",
            "native/Cargo.toml",
            "--locked",
        ]
        .into_iter()
        .chain(extra.iter().copied())
        .map(str::to_owned)
        .collect()
    }
    #[test]
    fn exact_cargo_selectors_supply_the_same_prebuild_plan_without_a_second_schedule() {
        let actual = arguments(&args(&[
            "--lib",
            "--test",
            "one",
            "--test",
            "two",
            "filter",
            "--",
            "--test-threads=2",
            "--nocapture",
        ]))
        .unwrap();
        assert_eq!(
            actual.selectors,
            ["--lib", "--test", "one", "--test", "two"]
        );
        assert_eq!(actual.cargo.last().unwrap(), "filter");
        assert_eq!(actual.libtest, ["--test-threads=2", "--nocapture"]);
        let default = arguments(&args(&[])).unwrap();
        assert!(default.selectors.is_empty());
        assert!(
            observed_targets(
                &serde_json::json!({"targets":[{"mode":"observe"},{"mode":"noncoverage"}]})
            )
            .is_ok()
        );
        for value in [
            serde_json::json!({"targets":[]}),
            serde_json::json!({"targets":[{"mode":"noncoverage"}]}),
            serde_json::json!({}),
        ] {
            assert!(observed_targets(&value).is_err());
        }
        for extra in [
            vec!["--doc"],
            vec!["--all-targets"],
            vec!["--features", "x"],
            vec!["--target", "other"],
            vec!["--test", "*"],
            vec!["--lib", "--lib"],
            vec!["--config", "other.toml"],
            vec!["--", "--logfile", "mine"],
            vec!["--", "--list"],
            vec!["filter", "other"],
        ] {
            assert!(arguments(&args(&extra)).is_err(), "{extra:?}");
        }
        assert!(arguments(&["--baseline".into(), "bad".into(), "--".into()]).is_err());
    }
    #[test]
    fn literal_runner_arrays_preserve_spaces_unicode_and_windows_backslashes() {
        let values = [
            r"C:\project café\native\target\debug\numera-emoji.exe",
            "test-runner",
            "--observe-cargo",
            r"C:\project café\logs\tmp\plan.json",
            r"C:\project café\logs\tmp\build.log",
        ];
        let config = runner_config("x86_64-pc-windows-msvc", &values).unwrap();
        assert!(config.starts_with("[target.x86_64-pc-windows-msvc]\nrunner = ["));
        for line in config.lines().filter_map(|s| s.strip_prefix("runner = ")) {
            assert_eq!(serde_json::from_str::<Vec<String>>(line).unwrap(), values);
        }
        assert_eq!(
            config
                .lines()
                .filter(|s| s.starts_with("runner = "))
                .count(),
            3
        );
        assert!(!config.contains("${"));
        assert!(runner_config("bad\n[target.other]", &values).is_err());
        let mut bad = values;
        bad[3] = "bad\npath";
        assert!(runner_config("x86_64-pc-windows-msvc", &bad).is_err());
    }
    #[test]
    fn cargo_injection_precedes_libtest_arguments_and_does_not_duplicate_tests() {
        let original = args(&["--test", "one", "--no-fail-fast", "--", "--exact", "case"]);
        let options = arguments(&original).unwrap();
        let invocation = cargo_arguments(&options, Path::new("/owned/runner.toml")).unwrap();
        assert_eq!(invocation.first().unwrap(), "test");
        assert_eq!(invocation.iter().filter(|s| *s == "test").count(), 1);
        assert_eq!(invocation.iter().filter(|s| *s == "--test").count(), 1);
        let boundary = invocation.iter().position(|s| s == "--").unwrap();
        assert_eq!(&invocation[boundary + 1..], ["--exact", "case"]);
        assert_eq!(
            &invocation[boundary - 3..boundary],
            ["--message-format=json", "--config", "/owned/runner.toml"]
        );
    }
}
