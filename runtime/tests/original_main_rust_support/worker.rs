//! Rust-only fault fixture; never included in a product executable.
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

fn hold() -> ! {
    loop {
        std::thread::park_timeout(Duration::from_secs(60));
    }
}
fn publish_ready(path: &Path, pid: u32) {
    // Keep ready.txt and ready.leaf staging paths distinct, and publish only
    // after the writer closes so an existence-based cancellation cannot cut it.
    let mut pending = path.as_os_str().to_owned();
    pending.push(".tmp");
    std::fs::write(&pending, pid.to_string()).unwrap();
    std::fs::rename(pending, path).unwrap();
}
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args[0] == "leaf" {
        publish_ready(Path::new(&args[1]), std::process::id());
        hold();
    }
    if args[0] == "tree" {
        let ready = Path::new(&args[1]);
        let leaf_ready = ready.with_extension("leaf");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("leaf")
            .arg(&leaf_ready)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !leaf_ready.exists() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        publish_ready(ready, child.id());
        child.wait().unwrap();
        return;
    }
    assert_eq!(args[0], "codec-worker");
    assert_eq!(args.len(), 3);
    if std::env::current_exe().unwrap().file_stem().unwrap() == "config-observer" {
        let timeout = std::env::var("EMOJI_FFMPEG_TIMEOUT").unwrap();
        let dotenv = std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").unwrap();
        assert!(std::env::var_os("GENERAL_BOT_TOKEN").is_none());
        let response = format!(
            "{{\"ok\":true,\"result\":{{\"timeout\":\"{timeout}\",\"dotenv\":\"{dotenv}\",\"credential\":null}}}}"
        );
        std::fs::write(&args[2], response).unwrap();
    } else {
        let ready = std::env::current_dir().unwrap().join("ready");
        publish_ready(&ready, std::process::id());
        hold();
    }
}
