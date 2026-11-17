//! Rust-only fault fixture; never included in a product executable.
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

fn hold() -> ! {
    loop {
        std::thread::park_timeout(Duration::from_secs(60));
    }
}
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args[0] == "leaf" {
        std::fs::write(&args[1], std::process::id().to_string()).unwrap();
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
        std::fs::write(ready, child.id().to_string()).unwrap();
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
        std::fs::write(ready, std::process::id().to_string()).unwrap();
        hold();
    }
}
