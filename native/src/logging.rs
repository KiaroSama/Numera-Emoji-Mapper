//! New UTF-8 UTC log per execution, with redaction before any output sink.
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

pub fn utc() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%d %H:%M:%S UTC")
        .to_string()
}
pub fn iso_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}
fn token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}
pub fn redact(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_owned();
    let mut secrets = secrets.iter().collect::<Vec<_>>();
    secrets.sort_by_key(|s| std::cmp::Reverse(s.chars().count()));
    for secret in secrets {
        if !secret.is_empty() {
            out = out.replace(secret, "[REDACTED]")
        }
    }
    let mut result = String::new();
    let chars = out.chars().collect::<Vec<_>>();
    let mut i = 0;
    while i < chars.len() {
        let bot = chars[i..].starts_with(&['b', 'o', 't']) && (i == 0 || !token_char(chars[i - 1]));
        let digits = i + if bot { 3 } else { 0 };
        let mut j = digits;
        while j < chars.len() && crate::emoji_ids::decimal(chars[j]) {
            j += 1
        }
        if j - digits >= 6 && chars.get(j) == Some(&':') {
            let mut end = j + 1;
            while end < chars.len() && token_char(chars[end]) {
                end += 1
            }
            if end - j > 20 {
                // The original URL pass retains /bot.../; standalone bot prefixes are removed.
                if bot && i > 0 && chars[i - 1] == '/' && chars.get(end) == Some(&'/') {
                    result.push_str("bot");
                }
                result.push_str("[REDACTED]");
                i = end;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}
/// Resolve the original call-time setting without changing the process environment.
pub fn retention_days(raw: Option<&str>) -> Result<(u64, Option<String>), String> {
    let (value, warning) =
        crate::config_integer::resolve(raw, "EMOJI_LOG_RETENTION_DAYS", 30, Some(0), None);
    let days = value
        .parse::<u64>()
        .map_err(|_| "log retention exceeds native duration range; refusing pruning")?;
    Ok((days, warning))
}

/// Prune only direct regular log files; linked entries are never deletion candidates.
pub fn prune_old_logs(root: &Path, days: u64) -> Result<usize, String> {
    if days == 0 {
        return Ok(0);
    }
    let age = days
        .checked_mul(86400)
        .ok_or("log retention exceeds native duration range; refusing pruning")?;
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(age))
        .ok_or("log retention exceeds native clock range; refusing pruning")?;
    let root = crate::tooling_environment::physical_root(root)?;
    let directory = root.join("logs");
    crate::tooling_environment::reject_links(&directory, "log retention cannot follow links")?;
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(format!("cannot inspect log retention directory: {error}")),
    };
    let mut removed = 0;
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let matches = path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| {
                if cfg!(windows) {
                    value.to_ascii_lowercase().ends_with(".log")
                } else {
                    value.ends_with(".log")
                }
            });
        if !matches {
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                continue;
            }
        }
        if metadata.modified().is_ok_and(|modified| modified < cutoff)
            && std::fs::remove_file(&path).is_ok()
        {
            removed += 1;
        }
    }
    Ok(removed)
}

fn script_label(name: &str) -> String {
    let mut label = String::new();
    let mut replacing = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            label.push(ch);
            replacing = false;
        } else if !replacing {
            label.push('_');
            replacing = true;
        }
    }
    let label = label.trim_matches('_');
    if label.is_empty() {
        "script".into()
    } else {
        label.into()
    }
}

struct LogState {
    file: Option<File>,
    counts: [u64; 3],
}
#[derive(Clone)]
pub struct RunLog {
    state: std::sync::Arc<Mutex<LogState>>,
    secrets: Vec<String>,
    pub path: Option<PathBuf>,
    started: Instant,
    run_id: String,
}
impl RunLog {
    pub fn open(root: &Path, name: &str, secrets: Vec<String>) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut random = [0_u8; 4];
        let run_id = match getrandom::fill(&mut random) {
            Ok(()) => format!(
                "{}-{sequence:x}",
                random
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            ),
            Err(_) => {
                eprintln!("WARNING: log run entropy unavailable; using process-local identity");
                format!("{}-{sequence:x}", std::process::id())
            }
        };
        let name = script_label(name);
        let directory = root.join("logs");
        let mut path = None;
        let file = (|| {
            std::fs::create_dir_all(&directory)?;
            let date = chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S_UTC");
            for suffix in 0..1000 {
                let candidate =
                    directory.join(format!("{name}_{date}_{}_{suffix}.log", std::process::id()));
                match OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&candidate)
                {
                    Ok(file) => {
                        path = Some(candidate);
                        return Ok(file);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(e),
                }
            }
            Err(std::io::Error::other("cannot allocate new log filename"))
        })();
        let file = match file {
            Ok(file) => Some(file),
            Err(e) => {
                eprintln!(
                    "WARNING: file logging unavailable: {}",
                    redact(&e.to_string(), &secrets)
                );
                None
            }
        };
        let log = Self {
            state: std::sync::Arc::new(Mutex::new(LogState {
                file,
                counts: [0; 3],
            })),
            secrets,
            path,
            started: Instant::now(),
            run_id,
        };
        log.event("INFO", "runtime", "native backend started");
        log
    }
    pub fn apply_retention(&self, root: &Path, raw: &str) {
        let result = retention_days(Some(raw)).and_then(|(days, warning)| {
            if let Some(warning) = warning {
                let message = warning
                    .strip_prefix("WARNING: ")
                    .unwrap_or(&warning)
                    .trim_end();
                self.event("WARNING", "logging", message);
            }
            prune_old_logs(root, days)
        });
        match result {
            Ok(0) => {}
            Ok(removed) => self.event("DEBUG", "logging", &format!("pruned_logs={removed}")),
            Err(error) => self.event("WARNING", "logging", &error),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, LogState> {
        self.state.lock().unwrap_or_else(|error| {
            eprintln!("WARNING: log state was poisoned; recovering diagnostic output");
            error.into_inner()
        })
    }

    fn write(&self, state: &mut LogState, level: &str, component: &str, message: &str) {
        let text = format!(
            "[{}] [{level}] [{component}] {} run={}\n",
            utc(),
            redact(message, &self.secrets),
            self.run_id
        );
        if let Some(file) = state.file.as_mut()
            && let Err(e) = file.write_all(text.as_bytes()).and_then(|_| file.flush())
        {
            eprintln!(
                "WARNING: log write failed: {}",
                redact(&e.to_string(), &self.secrets)
            );
        }
        if ["WARNING", "ERROR", "CRITICAL"].contains(&level) {
            eprint!("{text}")
        }
    }

    pub fn event(&self, level: &str, component: &str, message: &str) {
        let mut state = self.state();
        if let Some(index) = ["WARNING", "ERROR", "CRITICAL"]
            .iter()
            .position(|name| *name == level)
        {
            state.counts[index] = state.counts[index].saturating_add(1);
        }
        self.write(&mut state, level, component, message);
    }

    /// Install only in the executable entry point, never when importing the extension.
    pub fn install_panic_hook(self: &std::sync::Arc<Self>) {
        let owner = std::sync::Arc::downgrade(self);
        let secrets = self.secrets.clone();
        let run_id = self.run_id.clone();
        std::panic::set_hook(Box::new(move |info| {
            let thread = std::thread::current();
            let payload = info
                .payload()
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| info.payload().downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic payload");
            let location = info
                .location()
                .map(|value| format!("{}:{}:{}", value.file(), value.line(), value.column()))
                .unwrap_or_else(|| "unknown".into());
            let message = redact(
                &format!(
                    "UNCAUGHT panic thread={} at={location}: {payload}\n{}",
                    thread.name().unwrap_or("?"),
                    std::backtrace::Backtrace::force_capture(),
                ),
                &secrets,
            );
            let mut written = false;
            if let Some(log) = owner.upgrade() {
                let guard = match log.state.try_lock() {
                    Ok(guard) => Some(guard),
                    Err(std::sync::TryLockError::Poisoned(error)) => Some(error.into_inner()),
                    Err(std::sync::TryLockError::WouldBlock) => None,
                };
                if let Some(mut state) = guard {
                    state.counts[2] = state.counts[2].saturating_add(1);
                    if let Some(file) = state.file.as_mut() {
                        let text =
                            format!("[{}] [CRITICAL] [panic] {message} run={run_id}\n", utc());
                        written = file
                            .write_all(text.as_bytes())
                            .and_then(|_| file.flush())
                            .is_ok();
                    }
                }
            }
            // Do not delegate to the default hook: it prints the unredacted payload.
            let text = format!(
                "[{}] [CRITICAL] [panic] {message} log_unavailable={} run={run_id}\n",
                utc(),
                !written,
            );
            let _ = std::io::stderr().write_all(text.as_bytes());
        }));
    }

    pub fn finish(&self, code: i32) -> i32 {
        let mut state = self.state();
        let [warnings, errors, critical] = state.counts;
        let outcome = if code == 0 {
            "OK".into()
        } else {
            format!("FAILED({code})")
        };
        // A nonzero summary remains ERROR, but is not another diagnostic to count.
        self.write(
            &mut state,
            if code == 0 { "INFO" } else { "ERROR" },
            "runtime",
            &format!(
                "exit={code} duration_ms={} outcome={outcome} warnings={warnings} errors={errors} critical={critical}",
                self.started.elapsed().as_millis()
            ),
        );
        code
    }
}

#[cfg(all(test, not(numera_native_runtime)))]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Duration;

    const FIXTURE: &str = "logging::tests::panic_child_fixture";
    const CONTROL: &str = "NUMERA_LOGGING_FIXTURE_ROOT";
    const MODE: &str = "NUMERA_LOGGING_FIXTURE_MODE";
    const LITERAL: &str = "synthetic-never-issued-panic-credential";
    const TOKEN: &str = "1234567890:AAxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";

    #[test]
    fn panic_child_fixture() {
        if !std::env::args()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|pair| pair == ["--exact", FIXTURE])
        {
            return;
        }
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let parent =
            crate::tooling_environment::confined_directory(project, "logs/tmp", false).unwrap();
        let root = crate::tooling_environment::physical_root(&PathBuf::from(
            std::env::var_os(CONTROL).unwrap(),
        ))
        .unwrap();
        assert!(root.starts_with(&parent) && root != parent);
        let mode = std::env::var(MODE).unwrap();
        assert!(
            [
                "ordinary",
                "thread",
                "reentrant",
                "dropped",
                "non-string",
                "uncaught"
            ]
            .contains(&mode.as_str())
        );
        let log = Arc::new(RunLog::open(&root, "panic-fixture", vec![LITERAL.into()]));
        let path = log.path.clone().unwrap();
        log.install_panic_hook();
        let trigger: fn() = || panic!("controlled {LITERAL} {TOKEN}");
        if mode == "uncaught" {
            crate::atomic::write_json(
                &root.join("observed.json"),
                &serde_json::json!({"path":path,"mode":mode}),
            )
            .unwrap();
            trigger();
            unreachable!("panic fixture returned without unwinding");
        }
        if mode == "dropped" {
            drop(log);
            assert!(std::panic::catch_unwind(trigger).is_err());
        } else {
            match mode.as_str() {
                "thread" => {
                    let child = std::thread::Builder::new()
                        .name("panic-fixture-thread".into())
                        .spawn(trigger)
                        .unwrap();
                    assert!(child.join().is_err());
                }
                "reentrant" => {
                    let guard = log.state();
                    assert!(std::panic::catch_unwind(trigger).is_err());
                    drop(guard);
                }
                "non-string" => {
                    assert!(std::panic::catch_unwind(|| std::panic::panic_any(17_u8)).is_err())
                }
                _ => assert!(std::panic::catch_unwind(trigger).is_err()),
            }
            assert_eq!(log.finish(17), 17);
            drop(log);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            drop(
                OpenOptions::new()
                    .write(true)
                    .share_mode(0)
                    .open(&path)
                    .unwrap(),
            );
        }
        crate::atomic::write_json(
            &root.join("observed.json"),
            &serde_json::json!({"path":path,"mode":mode}),
        )
        .unwrap();
        std::process::exit(17);
    }

    #[test]
    fn uncaught_panic_capture_redacts_all_sinks_without_deadlock_or_retaining_the_log() {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let parent =
            crate::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
        let root = parent.join(format!(
            "panic logging é {} {}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let inherited = std::env::vars().collect::<BTreeMap<_, _>>();
        let environment = crate::tooling_test_runner::scrubbed_environment(&inherited);
        let log = RunLog::open(&root, "panic-owner", vec![LITERAL.into()]);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        for mode in [
            "ordinary",
            "thread",
            "reentrant",
            "dropped",
            "non-string",
            "uncaught",
        ] {
            let case = root.join(mode);
            std::fs::create_dir(&case).unwrap();
            let mut environment =
                crate::tooling_environment::environment(&case, &environment, false).unwrap();
            environment.insert(CONTROL.into(), case.to_str().unwrap().into());
            environment.insert(MODE.into(), mode.into());
            let completed = runtime
                .block_on(crate::tooling_process::capture(
                    &case,
                    &[
                        std::env::current_exe().unwrap().into_os_string(),
                        "--exact".into(),
                        FIXTURE.into(),
                        "--nocapture".into(),
                        "--test-threads=1".into(),
                    ],
                    &environment,
                    Duration::from_secs(8),
                    Duration::from_secs(4),
                    &log,
                ))
                .unwrap();
            assert_eq!(
                completed.code,
                if mode == "uncaught" { 101 } else { 17 },
                "{mode}: {}",
                completed.stderr
            );
            let report: serde_json::Value =
                serde_json::from_slice(&std::fs::read(case.join("observed.json")).unwrap())
                    .unwrap();
            assert_eq!(report["mode"], mode);
            let path = PathBuf::from(report["path"].as_str().unwrap());
            assert_eq!(path.parent().unwrap(), case.join("logs"));
            let text = std::fs::read_to_string(path).unwrap();
            for sink in [&completed.stdout, &completed.stderr, &text] {
                assert!(
                    !sink.contains(LITERAL) && !sink.contains(TOKEN),
                    "raw credential in {mode}"
                );
            }
            assert!(completed.stderr.contains("[CRITICAL] [panic]"));
            if mode == "non-string" {
                assert!(text.contains("non-string panic payload"));
            } else {
                assert!(
                    completed
                        .stderr
                        .contains("controlled [REDACTED] [REDACTED]")
                );
            }
            if matches!(mode, "reentrant" | "dropped") {
                assert!(completed.stderr.contains("log_unavailable=true"));
                assert!(!text.contains("[CRITICAL] [panic]"));
            } else {
                assert!(text.contains("[CRITICAL] [panic]"));
                if mode != "uncaught" {
                    assert!(text.contains("critical=1"));
                }
            }
            if mode == "thread" {
                assert!(text.contains("thread=panic-fixture-thread"));
            }
            assert!(
                std::fs::read_dir(case.join("logs/tmp/tooling"))
                    .unwrap()
                    .next()
                    .is_none()
            );
        }
        drop(log);
        // No Drop cleanup: failed comparisons preserve their exact diagnostic fixture.
        std::fs::remove_dir_all(root).unwrap();
    }
}
