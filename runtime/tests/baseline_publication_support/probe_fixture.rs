//! Interpreter-free ffmpeg/ffprobe fixture. Only an isolated test child puts this on PATH.
use std::io::Write;
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let stem = std::path::Path::new(&args[0])
        .file_stem()
        .unwrap()
        .to_string_lossy();
    let calls = std::env::var_os("NUMERA_PUBLICATION_PROBE_CALLS").unwrap();
    writeln!(
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(calls)
            .unwrap(),
        "{}|{}|{}",
        std::process::id(),
        stem,
        args[1..].join(" ")
    )
    .unwrap();
    if stem == "ffprobe" {
        println!(
            r#"{{"streams":[{{"codec_type":"video","codec_name":"h264","width":64,"height":64,"duration":"1","avg_frame_rate":"4/1","pix_fmt":"rgba"}}],"format":{{"duration":"1","format_name":"matroska,webm"}}}}"#
        );
    } else if std::env::var("NUMERA_PUBLICATION_PROBE_HANG").as_deref() == Ok("1") {
        loop {
            std::thread::park_timeout(std::time::Duration::from_secs(60));
        }
    } else {
        let raw = std::fs::read(std::env::var_os("NUMERA_PUBLICATION_PROBE_RAW").unwrap()).unwrap();
        std::io::stdout().lock().write_all(&raw).unwrap();
    }
}
