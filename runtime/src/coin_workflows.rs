//! Coin operator orchestration; conversion and publishing policy stay in the existing native commands.
use crate::{
    config::Config, logging::RunLog, process_owner::OwnedProcess, tooling_launch, tooling_process,
};
use std::{
    future::Future,
    io::{BufRead, Write},
    path::{Path, PathBuf},
    pin::Pin,
    process::{Command, Stdio},
    time::Duration,
};

type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Svg,
    Png,
    Build,
    Map,
    Links,
}
#[derive(Default)]
struct Transcript {
    last: Option<String>,
    review: Vec<String>,
    lines: Vec<String>,
}
fn prefix(line: &str, label: &str) -> bool {
    line.get(..label.len())
        .is_some_and(|s| s.eq_ignore_ascii_case(label))
}
fn transcript(path: &Path) -> Transcript {
    let Ok(file) = std::fs::File::open(path) else {
        return Transcript::default();
    };
    let mut out = Transcript::default();
    for line in std::io::BufReader::new(file)
        .lines()
        .map_while(std::result::Result::ok)
    {
        if prefix(&line, "REVIEW:") || prefix(&line, "QUARANTINE:") {
            out.review.push(line.clone());
        }
        out.last = Some(line.clone());
        out.lines.push(line);
    }
    out
}
trait Steps {
    async fn phase(&mut self, phase: Phase) -> Result<i32>;
    async fn backoff(&mut self, duration: Duration) -> Result<bool>;
    fn cursor(&mut self) -> i32;
    fn marker(&mut self) -> String;
    fn capture(&mut self, phase: Phase) -> Transcript;
    fn png_exists(&self) -> bool;
    fn skip_file(&self) -> PathBuf;
    fn note(&mut self, text: &str) -> Result<()>;
}
async fn rebuild(steps: &mut impl Steps) -> Result<i32> {
    let mut previous = steps.cursor();
    let mut stalled = 0;
    for attempt in 1..=200 {
        steps.note(&format!(
            "=== build attempt {attempt} @ {} ===",
            crate::logging::iso_utc()
        ))?;
        let code = steps.phase(Phase::Build).await?;
        steps.note(&format!(
            "=== build attempt {attempt} exited code={code} ==="
        ))?;
        if code == 0 {
            steps.note("=== build complete; mapping + filling inventory ===")?;
            let map = steps.phase(Phase::Map).await?;
            if map != 0 {
                steps.note(&format!("=== map failed (exit {map}); links skipped ==="))?;
                return Ok(map);
            }
            steps.note("=== sending final combined links ===")?;
            let links = steps.phase(Phase::Links).await?;
            if links != 0 {
                steps.note(&format!("=== links failed (exit {links}) ==="))?;
                return Ok(links);
            }
            steps.note("=== ALL DONE ===")?;
            return Ok(0);
        }
        if code != 3 {
            steps.note(&format!(
                "=== terminal error (exit {code}); not retrying ==="
            ))?;
        } else {
            let current = steps.cursor();
            stalled = if current > previous { 0 } else { stalled + 1 };
            previous = current;
            if stalled >= 2 {
                steps.note(&format!("=== no progress at plan position {current} for {stalled} checkpoints; stopping ==="))?;
            } else if attempt >= 200 {
                steps.note(&format!("=== gave up after {attempt} attempts ==="))?;
            } else {
                if !steps.backoff(Duration::from_secs(15)).await? {
                    return Ok(130);
                }
                continue;
            }
        }
        steps.note(&format!("=== BUILD INCOMPLETE (last exit {code} after {attempt} attempts); map + links skipped ==="))?;
        return Ok(code);
    }
    unreachable!("the 200th attempt always exits before another attempt")
}
async fn convert(steps: &mut impl Steps) -> Result<i32> {
    let (mut exit, mut finished) = (0, false);
    let mut killed = Vec::new();
    let mut last = Transcript::default();
    for attempt in 1..=100 {
        let code = steps.phase(Phase::Svg).await?;
        if code == 130 {
            return Ok(130);
        }
        if code == 124 {
            let stuck = steps.marker();
            steps.note(&format!("[watchdog] '{stuck}' made no progress for 120s (iter {attempt}) - killed and reaped."))?;
            killed.push(stuck);
            continue;
        }
        last = steps.capture(Phase::Svg);
        if code == 125 {
            steps
                .note("[watchdog] conversion exceeded its 3600s wall bound - see emoji_err.txt")?;
            exit = 1;
            break;
        }
        if last.last.as_deref().is_some_and(|s| prefix(s, "DONE:")) {
            steps.note(&format!(
                "[watchdog] SVG conversion complete: {}",
                last.last.as_deref().unwrap_or_default()
            ))?;
            exit = exit.max(code);
            finished = true;
        } else {
            steps.note(&format!(
                "[watchdog] converter exited (code {code}) without finishing - see emoji_err.txt"
            ))?;
            exit = code.max(1);
        }
        break;
    }
    // After a final idle kill the last attempt's partial capture still carries its review notices.
    if !finished && last.last.is_none() {
        last = steps.capture(Phase::Svg);
    }
    for review in last.review {
        steps.note(&format!("[watchdog] {review}"))?;
        exit = exit.max(3);
    }
    if !killed.is_empty() {
        steps.note(&format!(
            "[watchdog] REVIEW killed sources: {} - listed in {}; delete a line to retry one.",
            killed.join(", "),
            steps.skip_file().display()
        ))?;
        exit = exit.max(3);
    }
    if !finished {
        exit = exit.max(1);
        steps.note(&format!(
            "[watchdog] SVG conversion did NOT complete - giving up (exit {exit})."
        ))?;
        return Ok(exit);
    }
    if steps.png_exists() {
        let mut code = steps.phase(Phase::Png).await?;
        if code == 130 {
            return Ok(130);
        }
        for line in steps.capture(Phase::Png).lines {
            steps.note(&line)?;
        }
        if [124, 125].contains(&code) {
            steps.note(
                "[watchdog] PNG conversion stopped at its owned timeout - sources need review.",
            )?;
            code = 1;
        }
        exit = exit.max(code);
    }
    Ok(exit)
}

fn cursor(path: &Path) -> i32 {
    let Some(value) = std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
    else {
        return -1;
    };
    // PowerShell's [int] cast accepts null/missing as0, booleans, numeric text and rounded numeric JSON.
    let value = &value["cursor"];
    if value.is_null() {
        return 0;
    }
    if let Some(n) = value.as_i64() {
        return i32::try_from(n).unwrap_or(-1);
    }
    if let Some(n) = value.as_u64() {
        return i32::try_from(n).unwrap_or(-1);
    }
    if let Some(b) = value.as_bool() {
        return i32::from(b);
    }
    if let Some(s) = value.as_str() {
        return if s.trim().is_empty() {
            0
        } else {
            s.trim().parse().unwrap_or(-1)
        };
    }
    value
        .as_f64()
        .map(f64::round_ties_even)
        .filter(|n| n.is_finite() && *n >= f64::from(i32::MIN) && *n <= f64::from(i32::MAX))
        .map_or(-1, |n| n as i32)
}
fn exit_code(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
}
async fn owned(
    launch: &tooling_launch::Launch,
    append: Option<&Path>,
    cancel: &mut Pin<Box<dyn Future<Output = Result<()>> + '_>>,
    log: &RunLog,
) -> Result<i32> {
    // A cancellation already observed between phases must never start another child.
    tokio::select! {
        biased;
        stopped = cancel.as_mut() => { stopped?; return Ok(130) }
        _ = std::future::ready(()) => {}
    }
    let executable = launch.argv.first().ok_or("workflow command is empty")?;
    let directory = tooling_process::workspace(&launch.root)?;
    let owner_directory = directory.join("owner");
    let setup = (|| -> Result<Command> {
        std::fs::create_dir(&owner_directory).map_err(|e| e.to_string())?;
        let mut command = Command::new(executable);
        command
            .args(&launch.argv[1..])
            .current_dir(&launch.root)
            .env_clear()
            .envs(&launch.environment)
            .stdin(Stdio::null());
        if let Some(path) = append {
            crate::tooling_environment::reject_links(path, "Linked coin workflow log")?;
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| e.to_string())?;
            command
                .stdout(file.try_clone().map_err(|e| e.to_string())?)
                .stderr(file);
        } else {
            command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
        }
        Ok(command)
    })();
    let mut command = match setup {
        Ok(command) => command,
        Err(error) => {
            tooling_process::clean_unspawned(&directory)?;
            return Err(error);
        }
    };
    // These CLI workflows are dedicated automation processes with one child
    // cohort. Keep graceful session interruption AND custody of nested codec
    // sessions. Existing descendant admission refuses other child ownership.
    let mut owner = match OwnedProcess::spawn_descendant_session(&mut command, owner_directory) {
        Ok(owner) => owner,
        Err(error) => {
            drop(command);
            tooling_process::clean_unspawned(&directory)?;
            return Err(error);
        }
    };
    drop(command);
    let result = async {
        tokio::select! {
            biased;
            stopped = cancel.as_mut() => {
                stopped?; log.event("INFO","coin-workflow","interrupted; stopping the owned phase"); owner.interrupt()?;
                let _ = tokio::time::timeout(Duration::from_secs(2),owner.wait_console()).await;
                Ok(130)
            }
            status = owner.wait_console() => Ok(exit_code(status?)),
        }
    }.await;
    owner.finish()?;
    tooling_process::clean_unspawned(&directory)?;
    result
}
struct Native<'a> {
    config: &'a Config,
    log: &'a RunLog,
    coin: PathBuf,
    output: PathBuf,
    forwarded: Vec<String>,
    append: Option<PathBuf>,
    cancel: Pin<Box<dyn Future<Output = Result<()>> + 'a>>,
}
impl Native<'_> {
    fn capture_path(&self, phase: Phase) -> PathBuf {
        self.coin.join(if phase == Phase::Png {
            "emoji_png_log.txt"
        } else {
            "emoji_conv.txt"
        })
    }
}
impl Steps for Native<'_> {
    async fn phase(&mut self, phase: Phase) -> Result<i32> {
        let args = if matches!(phase, Phase::Svg | Phase::Png) {
            let source = self.coin.join(if phase == Phase::Svg {
                "logos/svg"
            } else {
                "logos/png"
            });
            let stderr = self.coin.join(if phase == Phase::Svg {
                "emoji_err.txt"
            } else {
                "emoji_png_err.txt"
            });
            vec![
                "native-convert-owner".into(),
                "--in".into(),
                source.to_string_lossy().into_owned(),
                "--out".into(),
                self.output.to_string_lossy().into_owned(),
                "--stdout".into(),
                self.capture_path(phase).to_string_lossy().into_owned(),
                "--stderr".into(),
                stderr.to_string_lossy().into_owned(),
            ]
        } else {
            let mut args = vec![
                "coin-rebuild".into(),
                match phase {
                    Phase::Build => "build",
                    Phase::Map => "map",
                    Phase::Links => "links",
                    _ => unreachable!(),
                }
                .into(),
            ];
            args.extend(self.forwarded.clone());
            args
        };
        let launch = tooling_launch::plan(&self.config.root, &args, &std::env::vars().collect())?;
        owned(&launch, self.append.as_deref(), &mut self.cancel, self.log).await
    }
    async fn backoff(&mut self, duration: Duration) -> Result<bool> {
        tokio::select! {biased;result=self.cancel.as_mut()=>{result?;Ok(false)},_=tokio::time::sleep(duration)=>Ok(true)}
    }
    fn cursor(&mut self) -> i32 {
        cursor(&self.coin.join("rebuild_dedup_state.json"))
    }
    fn marker(&mut self) -> String {
        let marker = self.output.join(".svg_cur");
        let label = crate::tooling_environment::reject_links(&marker, "Linked converter marker")
            .ok()
            .and_then(|()| std::fs::read_to_string(marker).ok())
            .unwrap_or_default();
        let label = label.trim();
        if label.is_empty() {
            "(unknown)".into()
        } else {
            label.into()
        }
    }
    fn capture(&mut self, phase: Phase) -> Transcript {
        transcript(&self.capture_path(phase))
    }
    fn png_exists(&self) -> bool {
        self.coin.join("logos/png").exists()
    }
    fn skip_file(&self) -> PathBuf {
        self.output.join(".svg_skip.txt")
    }
    fn note(&mut self, text: &str) -> Result<()> {
        let safe = crate::logging::redact(text, &self.config.secret_values());
        if let Some(path) = &self.append {
            crate::tooling_environment::reject_links(path, "Linked coin workflow log")?;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| e.to_string())?;
            writeln!(file, "{safe}").map_err(|e| e.to_string())?;
        } else {
            println!("{safe}");
        }
        Ok(())
    }
}
/// The primary application dispatch preserves full native terminal status codes.
pub fn run(config: &Config, command: &str, args: &[String], log: &RunLog) -> Result<i32> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "Usage: numera-emoji {command}{}",
            if command == "coin-rebuild-loop" {
                " [--coin-dir DIR] [--emoji-dir DIR] [--token-env KEY]"
            } else {
                ""
            }
        );
        println!(
            "{}",
            if command == "coin-rebuild-loop" {
                "Resume build only on exit 3, then map and send links after success. Build may delete old packs and upload; interrupting keeps resume state."
            } else {
                "Watch SVG conversion with 120 seconds for progress and a 3600 second wall bound, then PNG fallback. Quarantined sources require review (exit 3)."
            }
        );
        return Ok(0);
    }
    let mut coin = config.root.join("coins");
    let mut i = 0;
    if command == "coin-rebuild-loop" {
        while i < args.len() {
            if !["--coin-dir", "--emoji-dir", "--token-env"].contains(&args[i].as_str()) {
                return Err(format!("unknown coin-rebuild-loop argument: {}", args[i]));
            }
            let option = &args[i];
            i += 1;
            let value = args
                .get(i)
                .ok_or_else(|| format!("{option} requires a value"))?;
            if option == "--coin-dir" {
                coin = config.path(value)
            }
            i += 1;
        }
    } else if command != "coin-convert" || !args.is_empty() {
        return Err("coin-convert accepts only --help".into());
    }
    crate::tooling_environment::physical_root(&config.root)?;
    crate::tooling_environment::reject_links(&coin, "Linked coin workflow directory")?;
    if !coin.is_dir() {
        return Err("coin workflow directory is missing".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let mut driver = Native {
            config,
            log,
            output: coin.join("logos/emoji"),
            coin: coin.clone(),
            forwarded: args.to_vec(),
            append: (command == "coin-rebuild-loop").then(|| coin.join("rebuild_dedup_out.txt")),
            cancel: Box::pin(async { tokio::signal::ctrl_c().await.map_err(|e| e.to_string()) }),
        };
        if command == "coin-convert" {
            convert(&mut driver).await
        } else {
            rebuild(&mut driver).await
        }
    })
}

#[cfg(test)]
#[path = "../tests/coin_workflows_support/mod.rs"]
mod tests;
