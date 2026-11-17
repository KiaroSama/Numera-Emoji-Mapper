//! Test-only transparent process observer. It delegates to the actual FFmpeg codec.
use std::io::Write;
fn main() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let record = args
        .iter()
        .map(|s| s.to_string_lossy())
        .collect::<Vec<_>>()
        .join("\u{1f}");
    let mut log = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(std::env::var_os("NUMERA_MEDIA_FFMPEG_LOG").unwrap())
        .unwrap();
    writeln!(log, "{record}").unwrap();
    drop(log);
    let status = std::process::Command::new(std::env::var_os("NUMERA_MEDIA_REAL_FFMPEG").unwrap())
        .args(args)
        .status()
        .unwrap();
    std::process::exit(status.code().unwrap_or(1));
}
