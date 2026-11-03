//! Stdlib-only hostile codec fixture, built by hosted CI; never a production worker.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
mod converter_watchdog;
mod media_video_fidelity;
mod media_video_remux;

fn hold() -> ! {
    loop {
        std::thread::park();
    }
}
fn string_field<'a>(text: &'a str, field: &str) -> &'a str {
    // This fixture accepts only compact serde envelopes and an enumerated ASCII tag.
    let prefix = format!("\"{field}\":\"");
    let value = text
        .split_once(&prefix)
        .expect("fixture field")
        .1
        .split_once('"')
        .unwrap()
        .0;
    assert!(
        value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    );
    value
}
fn persist(path: &Path, bytes: &[u8]) {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
}
fn marker(control: &Path, descendant: u32) {
    persist(
        &control.join("ready"),
        format!("{}\n{descendant}\n", std::process::id()).as_bytes(),
    );
}
fn timeout_tool(args: &[String], root: &Path) {
    let executable = std::env::current_exe().unwrap();
    let tool = executable.file_stem().unwrap().to_str().unwrap();
    assert!(matches!(tool, "ffmpeg" | "ffprobe"));
    let root = root.canonicalize().unwrap();
    let directory = std::env::current_dir().unwrap().canonicalize().unwrap();
    assert_eq!(directory.parent().unwrap(), root.join("cases"));
    let case = directory.file_name().unwrap().to_str().unwrap();
    if case.starts_with("fidelity_") {
        media_video_fidelity::run(args, &directory, case, tool);
        return;
    }
    if case.starts_with("remux_") {
        media_video_remux::run(args, &directory, case, tool);
        return;
    }
    assert!(matches!(
        case,
        "probe_video"
            | "to_video_webm"
            | "content_digest"
            | "fingerprint"
            | "video_frames"
            | "first_frame"
            | "publication_probe"
            | "publication_probe_failed"
            | "publication_probe_visible"
            | "publication_probe_fade"
            | "publication_probe_blank"
            | "publication_probe_noise"
            | "publication_probe_empty"
            | "publication_probe_oversize"
            | "publication_probe_metadata_failed"
            | "publication_probe_metadata_invalid"
            | "publication_probe_metadata_hang"
            | "publication_probe_metadata_oversize"
            | "publication_probe_decoder_failed"
            | "publication_probe_decoder_missing"
            | "publication_probe_decoder_oversize"
            | "publication_probe_alpha"
            | "remux_long"
            | "remux_short"
            | "fingerprint_once"
            | "convert_alpha"
            | "convert_static"
            | "preview_exit"
            | "preview_wall"
    ));
    let source = directory.join("clip.webm");
    let preview = case.starts_with("preview_");
    if matches!(
        case,
        "fingerprint_once" | "convert_alpha" | "convert_static"
    ) {
        let capability = args == ["-v", "error", "-decoders"];
        let stage = if tool == "ffprobe" {
            "probe"
        } else if capability {
            "capability"
        } else {
            "decode"
        };
        persist(
            &directory.join(format!("{stage}-pid")),
            std::process::id().to_string().as_bytes(),
        );
        persist(
            &directory.join(format!("{stage}-args")),
            args.join("\n").as_bytes(),
        );
        assert_eq!(
            std::fs::read(&source).unwrap(),
            if case == "convert_static" {
                b"GIF89a not a real image".as_slice()
            } else {
                b"\x1a\x45\xdf\xa3 not a real video".as_slice()
            }
        );
        if tool == "ffprobe" {
            assert_ne!(case, "convert_static");
            let codec = if case == "convert_alpha" {
                "vp9"
            } else {
                "h264"
            };
            write!(std::io::stdout().lock(),
                "{{\"streams\":[{{\"codec_type\":\"video\",\"codec_name\":\"{codec}\",\"width\":100,\"height\":100,\"avg_frame_rate\":\"30/1\"}}],\"format\":{{\"duration\":\"1.0\",\"format_name\":\"matroska,webm\"}}}}"
            ).unwrap();
        } else if capability {
            assert_eq!(case, "convert_alpha");
            std::io::stdout()
                .lock()
                .write_all(b" V..... libvpx-vp9 fixture\n")
                .unwrap();
        } else {
            let input = args.iter().position(|arg| arg == "-i").unwrap();
            assert_eq!(args[input + 1], source.to_str().unwrap());
            if case == "convert_alpha" {
                let selected = args
                    .windows(2)
                    .position(|pair| pair == ["-c:v", "libvpx-vp9"])
                    .unwrap();
                assert!(selected < input);
            } else {
                assert!(!args[..input].iter().any(|arg| arg == "-c:v"));
            }
            if case == "fingerprint_once" {
                assert!(
                    args.windows(2)
                        .any(|pair| pair == ["-vf", "fps=10,scale=64:64,format=rgba"])
                );
                assert_eq!(args.last().unwrap(), "-");
                std::io::stdout()
                    .lock()
                    .write_all(&[20, 40, 60, 255].repeat(64 * 64))
                    .unwrap();
            } else {
                let output = Path::new(args.last().unwrap());
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
                std::fs::write(output, b"encoded fixture bytes").unwrap();
            }
        }
        return;
    }
    let capability = tool == "ffmpeg" && args == ["-v", "error", "-decoders"];
    assert!(
        capability
            || args.iter().any(|arg| {
                Path::new(arg)
                    .canonicalize()
                    .is_ok_and(|path| path == source)
            })
    );
    assert_eq!(
        std::fs::read(&source).unwrap(),
        b"\x1a\x45\xdf\xa3 not a real video"
    );
    let alpha = case.starts_with("publication_probe_decoder_") || case == "publication_probe_alpha";
    let stage = if tool == "ffprobe" {
        "probe-arguments"
    } else if capability {
        "decoder-arguments"
    } else {
        "decode-arguments"
    };
    if case.starts_with("publication_probe") || preview {
        persist(&directory.join(stage), args.join("\n").as_bytes());
    }
    let terminal = || {
        persist(
            &directory.join("hanging-tool"),
            format!("{tool}\n{}\n", std::process::id()).as_bytes(),
        )
    };
    if tool == "ffprobe" && case != "probe_video" {
        if case.starts_with("publication_probe_metadata_") {
            terminal();
            match case {
                "publication_probe_metadata_failed" => std::process::exit(1),
                "publication_probe_metadata_invalid" => {
                    std::io::stdout().lock().write_all(b"{").unwrap();
                    return;
                }
                "publication_probe_metadata_oversize" => {
                    std::io::stdout()
                        .lock()
                        .write_all(&vec![0; 1024 * 1024 + 1])
                        .unwrap();
                    std::process::exit(1);
                }
                "publication_probe_metadata_hang" => hold(),
                _ => unreachable!(),
            }
        }
        // Most cases bypass capability probing to reach their actual decode/encode child.
        let codec = if alpha { "vp9" } else { "h264" };
        write!(std::io::stdout().lock(),
            "{{\"streams\":[{{\"codec_type\":\"video\",\"codec_name\":\"{codec}\",\"width\":100,\"height\":100,\"avg_frame_rate\":\"30/1\"}}],\"format\":{{\"duration\":\"1.0\",\"format_name\":\"matroska,webm\"}}}}"
        ).unwrap();
        return;
    }
    if capability {
        assert!(alpha);
        if case != "publication_probe_alpha" {
            terminal();
        }
        match case {
            "publication_probe_decoder_failed" => std::process::exit(1),
            "publication_probe_decoder_oversize" => {
                std::io::stdout()
                    .lock()
                    .write_all(&vec![0; 1024 * 1024 + 1])
                    .unwrap();
                std::process::exit(1);
            }
            "publication_probe_decoder_missing" => {
                std::io::stdout()
                    .lock()
                    .write_all(b" V..... h264 fixture\n")
                    .unwrap();
            }
            "publication_probe_alpha" => {
                std::io::stdout()
                    .lock()
                    .write_all(b" V..... libvpx-vp9 fixture\n")
                    .unwrap();
            }
            _ => unreachable!(),
        }
        return;
    }
    terminal();
    if preview {
        assert_eq!(tool, "ffmpeg");
        assert_eq!(
            args,
            [
                "-nostdin",
                "-v",
                "error",
                "-i",
                source.to_str().unwrap(),
                "-frames:v",
                "1",
                "-an",
                "-threads",
                "1",
                "-vf",
                "scale=52:52,format=rgba",
                "-f",
                "rawvideo",
                "-"
            ]
        );
        if case == "preview_exit" {
            std::process::exit(7);
        }
    }
    if case.starts_with("publication_probe") {
        assert_eq!(tool, "ffmpeg");
        for pair in [
            ["-t", "3.0"],
            ["-vf", "fps=4,scale=64:64,format=rgba"],
            ["-f", "rawvideo"],
        ] {
            assert!(args.windows(2).any(|args| args == pair));
        }
        assert!(args.iter().any(|arg| arg == "-nostdin"));
        if case == "publication_probe_alpha" {
            let selected = args
                .windows(2)
                .position(|pair| pair == ["-c:v", "libvpx-vp9"])
                .unwrap();
            let input = args.iter().position(|arg| arg == "-i").unwrap();
            assert!(selected < input, "alpha decoder must apply to the input");
        } else {
            assert!(!args.iter().any(|arg| arg == "-c:v"));
        }
        if case == "publication_probe_failed" {
            std::process::exit(1);
        }
        if case == "publication_probe_oversize" {
            std::io::stdout()
                .lock()
                .write_all(&vec![0; 16 * 1024 * 1024 + 1])
                .unwrap();
            std::process::exit(1);
        }
        if case != "publication_probe" {
            let counts: &[usize] = match case {
                "publication_probe_visible" => &[900, 900],
                "publication_probe_fade" => &[0, 0, 500, 900],
                "publication_probe_blank" | "publication_probe_alpha" => &[0, 0, 0, 0],
                "publication_probe_noise" => &[3, 3, 3],
                "publication_probe_empty" => &[],
                _ => unreachable!(),
            };
            let mut stdout = std::io::stdout().lock();
            for &count in counts {
                let mut frame = vec![0; 64 * 64 * 4];
                for pixel in frame.chunks_exact_mut(4).take(count) {
                    pixel[3] = 255;
                }
                stdout.write_all(&frame).unwrap();
            }
            return;
        }
    }
    hold();
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some("make-emoji-pngs")
        && let Ok(variant) = std::env::var("NUMERA_CONVERT_FIXTURE")
    {
        converter_watchdog::run(&args, &variant);
        return;
    }
    if let Some(root) = std::env::var_os("NUMERA_CODEC_TIMEOUT_ROOT") {
        timeout_tool(&args, Path::new(&root));
        return;
    }
    if args.first().map(String::as_str) == Some("--capture-hold") {
        assert_eq!(args.len(), 2);
        let ready = Path::new(&args[1]);
        assert!(ready.is_absolute());
        assert_eq!(
            ready.parent().unwrap().canonicalize().unwrap(),
            std::env::current_dir().unwrap().canonicalize().unwrap()
        );
        persist(ready, std::process::id().to_string().as_bytes());
        hold();
    }
    if std::env::current_exe().unwrap().file_stem().unwrap() == "ffprobe" {
        let workspace = PathBuf::from(std::env::var_os("TEMP").unwrap());
        let envelope = workspace
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        // Real worker -> VideoDecoder -> capture must keep this nested owner inside its envelope.
        persist(
            &envelope.join("nested-codec-ready"),
            format!("{}\n", std::process::id()).as_bytes(),
        );
        hold();
    }
    if args == ["--foreground-input"]
        || args == ["emoji-bot"] && std::env::var("NUMERA_FOREGROUND_INPUT").as_deref() == Ok("1")
    {
        use std::io::Read;
        let stdin = std::io::stdin();
        #[cfg(windows)]
        let handle = {
            use std::os::windows::io::AsHandle;
            stdin.as_handle().try_clone_to_owned().unwrap()
        };
        #[cfg(unix)]
        let handle = {
            use std::os::fd::AsFd;
            stdin.as_fd().try_clone_to_owned().unwrap()
        };
        let mut answer = Vec::new();
        // Read only the child's line; the following menu choice belongs to its parent.
        for byte in std::fs::File::from(handle).bytes().take(128) {
            let byte = byte.unwrap();
            if byte == b'\n' {
                break;
            }
            answer.push(byte);
        }
        assert_eq!(
            String::from_utf8(answer).unwrap().trim_end(),
            "child café 😀"
        );
        println!("foreground input accepted");
        return;
    }
    if args == ["--capture-echo"] {
        std::io::copy(&mut std::io::stdin().lock(), &mut std::io::stdout().lock()).unwrap();
        eprintln!("fixture capture stderr");
        return;
    }
    if args.first().map(String::as_str) == Some("--descendant") {
        persist(
            Path::new(&args[1]),
            std::process::id().to_string().as_bytes(),
        );
        hold();
    }
    assert_eq!(args.len(), 3);
    assert_eq!(args[0], "codec-worker");
    let request = PathBuf::from(&args[1]);
    let response = PathBuf::from(&args[2]);
    let text = std::fs::read_to_string(&request).unwrap();
    let mode = string_field(&text, "fixture_mode");
    let id = string_field(&text, "fixture_id");
    let control = std::env::current_dir()
        .unwrap()
        .join("logs/tmp")
        .join(format!("media-fixture-{id}"));
    assert!(control.is_dir());
    let workspace = request.parent().unwrap();
    match mode {
        "environment" => {
            assert_eq!(std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").unwrap(),"1");
            assert_eq!(std::env::var("PYTHONNOUSERSITE").unwrap(),"1");
            assert_eq!(std::env::var("PYTHONSAFEPATH").unwrap(),"1");
            assert_eq!(std::env::var("EMOJI_FFMPEG_TIMEOUT").unwrap(),"7");
            for name in ["GENERAL_BOT_TOKEN","UNRELATED_PRIVATE_VALUE","PYTHONPATH"] {
                assert!(std::env::var_os(name).is_none(),"unexpected inherited {name}");
            }
            for name in ["TEMP","TMP","TMPDIR"] {
                assert_eq!(PathBuf::from(std::env::var_os(name).unwrap()),workspace);
            }
            marker(&control,0);
            persist(&response,br#"{"ok":true,"result":{"scrubbed":true}}"#);
        }
        "idle" | "cancel" | "wall" => { marker(&control,0); hold(); }
        "descendant" => {
            let ready = control.join("descendant-ready");
            let mut command = Command::new(std::env::current_exe().unwrap());
            command.arg("--descendant").arg(&ready).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
            #[cfg(windows)] {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000); // CREATE_NO_WINDOW; inherited owner job remains intact.
            }
            let child = command.spawn().unwrap();
            let deadline = Instant::now()+Duration::from_secs(5);
            let expected_pid = child.id().to_string();
            while std::fs::read_to_string(&ready).ok().as_deref() != Some(expected_pid.as_str()) {
                assert!(Instant::now()<deadline,"descendant readiness timeout");
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(std::fs::read_to_string(ready).unwrap(),child.id().to_string());
            marker(&control,child.id());
            hold();
        }
        "oversize" => persist(&response,&vec![b'x';65537]),
        "missing-result" => persist(&response,br#"{"ok":true}"#),
        "false-success" => persist(&response,br#"{"ok":false,"error":"refused fixture"}"#),
        "nonobject" => persist(&response,b"[]"),
        "invalid-json" => persist(&response,b"not json"),
        "error-secret" => persist(&response,br#"{"ok":false,"error":"fixture 123456789:abcdefghijklmnopqrstuvwxyz0123456789 refused"}"#),
        "response-directory" => std::fs::create_dir(response).unwrap(),
        "output-missing" => persist(&response,br#"{"ok":true,"result":{"bytes":7}}"#),
        "output-directory" => {
            std::fs::create_dir(workspace.join("destination.webp")).unwrap();
            persist(&response,br#"{"ok":true,"result":{"bytes":7}}"#);
        }
        "cleanup-refusal" => {
            persist(&workspace.join("destination.webp"),b"fixture");
            persist(&response,br#"{"ok":true,"result":{"bytes":7}}"#);
            #[cfg(unix)] {
                use std::os::unix::fs::PermissionsExt;
                let blocked=workspace.join("cleanup-blocked");
                std::fs::create_dir(&blocked).unwrap();
                persist(&blocked.join("keep"),b"cleanup fault fixture");
                persist(&control.join("blocked-path"),blocked.to_str().unwrap().as_bytes());
                std::fs::set_permissions(blocked,std::fs::Permissions::from_mode(0)).unwrap();
            }
        }
        "response-link" => {
            let preserved = control.join("preserved.json");
            persist(&preserved,br#"{"ok":true,"result":true}"#);
            #[cfg(unix)] std::os::unix::fs::symlink(preserved,response).unwrap();
            #[cfg(windows)] std::os::windows::fs::symlink_file(preserved,response).unwrap();
        }
        _ => panic!("unknown fixture mode"),
    }
}
