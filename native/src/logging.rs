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
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    for secret in secrets {
        if !secret.is_empty() {
            out = out.replace(secret, "[REDACTED]")
        }
    }
    let mut result = String::new();
    let chars = out.chars().collect::<Vec<_>>();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        let mut j = i;
        while j < chars.len() && chars[j].is_ascii_digit() {
            j += 1
        }
        if j - i >= 6 && chars.get(j) == Some(&':') {
            let mut end = j + 1;
            while end < chars.len() && token_char(chars[end]) {
                end += 1
            }
            if end - j > 20 {
                result.push_str("[REDACTED]");
                i = end;
                continue;
            }
        }
        result.push(chars[start]);
        i += 1;
    }
    result
}
pub struct RunLog {
    file: Mutex<Option<File>>,
    secrets: Vec<String>,
    pub path: Option<PathBuf>,
    started: Instant,
}
impl RunLog {
    pub fn open(root: &Path, name: &str, secrets: Vec<String>) -> Self {
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
            file: Mutex::new(file),
            secrets,
            path,
            started: Instant::now(),
        };
        log.event("INFO", "runtime", "native backend started");
        log
    }
    pub fn event(&self, level: &str, component: &str, message: &str) {
        let text = format!(
            "[{}] [{level}] [{component}] {}\n",
            utc(),
            redact(message, &self.secrets)
        );
        if let Ok(mut guard) = self.file.lock()
            && let Some(file) = guard.as_mut()
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
    pub fn finish(&self, code: i32) {
        self.event(
            if code == 0 { "INFO" } else { "ERROR" },
            "runtime",
            &format!(
                "exit={code} duration_ms={}",
                self.started.elapsed().as_millis()
            ),
        );
    }
}
