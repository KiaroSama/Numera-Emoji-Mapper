//! Counted remux responses inside the existing external-tool fixture worker.
use std::io::Write;
use std::path::Path;

pub(super) fn run(args: &[String], directory: &Path, case: &str, tool: &str) {
    assert!(matches!(
        case,
        "remux_long"
            | "remux_short"
            | "remux_source_long"
            | "remux_second_exit"
            | "remux_command_exit"
            | "remux_equal_large"
            | "remux_large"
            | "remux_nan"
            | "remux_second_nan"
            | "remux_probe_json"
            | "remux_probe_null"
    ));
    let index = (1..=2)
        .find(|index| !directory.join(format!("{tool}-{index}.pid")).exists())
        .expect("unexpected repeated remux command");
    let stage = format!("{tool}-{index}");
    super::persist(
        &directory.join(format!("{stage}.pid")),
        std::process::id().to_string().as_bytes(),
    );
    super::persist(
        &directory.join(format!("{stage}.args")),
        args.join("\n").as_bytes(),
    );
    let source = directory.join("clip.webm");
    let before = std::fs::read(&source).unwrap();
    if case == "remux_equal_large" {
        assert_eq!(&before[..4], b"\x1a\x45\xdf\xa3");
        assert_eq!(before.len(), 262145);
        assert!(before[4..].iter().all(|byte| *byte == b'x'));
    } else {
        assert_eq!(before, b"\x1a\x45\xdf\xa3 not a real video");
    }
    if tool == "ffmpeg" {
        assert_eq!(index, 1);
        assert_eq!(args.len(), 9);
        assert_eq!(
            &args[..8],
            [
                "-nostdin",
                "-v",
                "error",
                "-y",
                "-i",
                source.to_str().unwrap(),
                "-c",
                "copy"
            ]
        );
        if case == "remux_command_exit" {
            std::io::stderr()
                .lock()
                .write_all(b"controlled remux refusal\n")
                .unwrap();
            std::process::exit(7);
        }
        let output = Path::new(&args[8]);
        assert_eq!(output.parent().unwrap(), directory);
        assert!(
            output
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with(".video-output-")
        );
        assert_eq!(std::fs::metadata(output).unwrap().len(), 0);
        let bytes = match case {
            "remux_equal_large" => before,
            "remux_large" => vec![b'y'; 262145],
            _ => b"remuxed fixture bytes".to_vec(),
        };
        std::fs::write(output, bytes).unwrap();
        return;
    }
    assert_eq!(tool, "ffprobe");
    assert_eq!(args.len(), 7);
    let output = Path::new(&args[6]);
    assert_eq!(output.parent().unwrap(), directory);
    assert_eq!(
        &args[..6],
        [
            "-v",
            "error",
            "-show_entries",
            "stream=index,codec_type,codec_name,width,height,avg_frame_rate:format=duration,format_name",
            "-of",
            "json"
        ]
    );
    let expected = match case {
        "remux_equal_large" => before,
        "remux_large" => vec![b'y'; 262145],
        _ => b"remuxed fixture bytes".to_vec(),
    };
    assert_eq!(std::fs::read(output).unwrap(), expected);
    if case == "remux_second_exit" && index == 2 {
        std::io::stderr()
            .lock()
            .write_all(b"controlled second probe refusal\n")
            .unwrap();
        std::process::exit(7);
    }
    if matches!(case, "remux_probe_json" | "remux_probe_null") {
        assert_eq!(index, 1);
        std::io::stdout()
            .lock()
            .write_all(if case == "remux_probe_json" {
                b"{".as_slice()
            } else {
                b"{\"format\":null}".as_slice()
            })
            .unwrap();
        return;
    }
    let long = matches!(
        case,
        "remux_long" | "remux_source_long" | "remux_second_exit" | "remux_second_nan"
    );
    assert!(index == 1 || long && case != "remux_long");
    let duration = if case == "remux_nan" || case == "remux_second_nan" && index == 2 {
        "NaN"
    } else if long && index == 1 {
        "3.9"
    } else {
        "2.5"
    };
    write!(std::io::stdout().lock(),
        "{{\"streams\":[{{\"codec_type\":\"video\",\"codec_name\":\"vp9\"}}],\"format\":{{\"duration\":\"{duration}\"}}}}"
    ).unwrap();
}
