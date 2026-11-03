//! External-tool refusal/recovery responses; called only by the existing media fixture worker.
use std::io::Write;
use std::path::Path;

pub(super) fn run(args: &[String], directory: &Path, case: &str, tool: &str) {
    assert!(matches!(
        case,
        "fidelity_missing_vp9"
            | "fidelity_missing_vp8"
            | "fidelity_probe_recovery"
            | "fidelity_no_codec"
            | "fidelity_capability_recovery"
            | "fidelity_api_recovery"
    ));
    let capability = args == ["-v", "error", "-decoders"];
    let stage = if tool == "ffprobe" {
        "probe"
    } else if capability {
        "capability"
    } else {
        "decode"
    };
    let maximum = if case == "fidelity_probe_recovery" && tool == "ffprobe" {
        6
    } else {
        3
    };
    let index = (1..=maximum)
        .find(|index| !directory.join(format!("{stage}-{index}.pid")).exists())
        .expect("unexpected repeated external fidelity call");
    super::persist(
        &directory.join(format!("{stage}-{index}.pid")),
        std::process::id().to_string().as_bytes(),
    );
    super::persist(
        &directory.join(format!("{stage}-{index}.args")),
        args.join("\n").as_bytes(),
    );
    let opaque = directory.join("clip.webm");
    let translucent = directory.join("translucent.webm");
    assert_eq!(
        std::fs::read(&opaque).unwrap(),
        b"\x1a\x45\xdf\xa3 not a real video"
    );
    assert_eq!(
        std::fs::read(&translucent).unwrap(),
        b"\x1a\x45\xdf\xa3 undecoded translucent fixture"
    );
    if tool == "ffprobe" {
        assert_eq!(args.len(), 7);
        let source = Path::new(&args[6]);
        assert!(source == opaque || source == translucent);
        if (case == "fidelity_probe_recovery" && index == 1)
            || (case == "fidelity_api_recovery" && index == 3)
        {
            std::io::stderr()
                .lock()
                .write_all(b"ffprobe: transient failure\n")
                .unwrap();
            std::process::exit(1);
        }
        let codec = match case {
            "fidelity_no_codec" => "",
            "fidelity_missing_vp8" => "vp8",
            _ => "vp9",
        };
        write!(std::io::stdout().lock(),
            "{{\"streams\":[{{\"codec_type\":\"video\",\"codec_name\":\"{codec}\",\"width\":100,\"height\":100,\"avg_frame_rate\":\"10/1\"}}],\"format\":{{\"duration\":\"1.0\",\"format_name\":\"matroska,webm\"}}}}"
        ).unwrap();
    } else if capability {
        if matches!(
            case,
            "fidelity_capability_recovery" | "fidelity_api_recovery"
        ) && index == 1
        {
            std::io::stderr()
                .lock()
                .write_all(b"ffmpeg did not run\n")
                .unwrap();
            std::process::exit(1);
        }
        assert_ne!(case, "fidelity_no_codec");
        let response: &[u8] = if case.starts_with("fidelity_missing_") {
            b" V..... h264 fixture\n"
        } else {
            b" V..... libvpx-vp9 fixture\n"
        };
        std::io::stdout().lock().write_all(response).unwrap();
    } else {
        // A missing/unknown/failed decoder must never reach this branch or hash container bytes.
        assert_eq!(case, "fidelity_api_recovery");
        assert_eq!(index, 1);
        let input = args.iter().position(|arg| arg == "-i").unwrap();
        let selected = args
            .windows(2)
            .position(|pair| pair == ["-c:v", "libvpx-vp9"])
            .unwrap();
        assert!(selected < input);
        assert_eq!(args[input + 1], opaque.to_str().unwrap());
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-vf", "fps=10,scale=64:64,format=rgba"])
        );
        assert_eq!(args.last().unwrap(), "-");
        std::io::stdout()
            .lock()
            .write_all(&[20, 40, 60, 255].repeat(64 * 64))
            .unwrap();
    }
}
