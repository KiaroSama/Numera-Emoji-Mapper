//! Fault injector and transparent delegate for actual FFmpeg/FFprobe test commands.
use std::{io::Write, path::Path, time::Duration};
fn main() {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.first().is_some_and(|s| s == "--hang-grandchild") {
        std::thread::sleep(Duration::from_secs(60));
        return;
    }
    let probe = std::env::current_exe().unwrap().file_stem().unwrap() == "ffprobe";
    let mode = std::fs::read_to_string(std::env::var_os("NUMERA_VIDEO_MODE").unwrap()).unwrap();
    let capability = arguments.iter().any(|s| s == "-decoders");
    let count_path = std::env::var_os("NUMERA_VIDEO_COUNT").unwrap();
    let count = std::fs::read_to_string(&count_path)
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    if probe || capability {
        std::fs::write(&count_path, (count + 1).to_string()).unwrap();
    }
    match (mode.trim(), probe, capability) {
        ("missing-decoder", false, true) => {
            println!(" V..... synthetic-no-alpha-codec");
            return;
        }
        ("flaky-probe", true, _) if count == 0 => std::process::exit(1),
        ("no-codec", true, _) => {
            println!(
                "{{\"streams\":[{{\"codec_type\":\"video\",\"codec_name\":\"\",\"width\":100,\"height\":100,\"avg_frame_rate\":\"10/1\"}}],\"format\":{{\"duration\":\"1\",\"format_name\":\"matroska,webm\"}}}}"
            );
            return;
        }
        ("capability-error", false, true) => std::process::exit(1),
        ("hang", _, _) => {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("--hang-grandchild")
                .spawn()
                .unwrap();
            let path = std::env::var_os("NUMERA_VIDEO_PIDS").unwrap();
            let mut pids = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(Path::new(&path))
                .unwrap();
            writeln!(pids, "{} {}", std::process::id(), child.id()).unwrap();
            drop(pids);
            let status = child.wait().unwrap();
            std::process::exit(status.code().unwrap_or(1));
        }
        _ => {}
    }
    let real = std::env::var_os(if probe {
        "NUMERA_VIDEO_REAL_FFPROBE"
    } else {
        "NUMERA_VIDEO_REAL_FFMPEG"
    })
    .unwrap();
    let status = std::process::Command::new(real)
        .args(arguments)
        .status()
        .unwrap();
    std::process::exit(status.code().unwrap_or(1));
}
