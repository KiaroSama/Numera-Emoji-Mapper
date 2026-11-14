//! Source remux outcomes through the existing controlled tools and owned process cohort.
use _native::codec_video::VideoDecoder;
use _native::codec_video_encode::{reencode, reencode_typed};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::Path;

pub const CASES: [&str; 11] = [
    "remux_long",
    "remux_short",
    "remux_source_long",
    "remux_second_exit",
    "remux_command_exit",
    "remux_equal_large",
    "remux_large",
    "remux_nan",
    "remux_second_nan",
    "remux_probe_json",
    "remux_probe_null",
];

fn members(directory: &Path) -> BTreeSet<OsString> {
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect()
}

fn command_failure(directory: &Path, stage: &str, tool: &str) -> String {
    use pyo3::prelude::*;
    let args = std::fs::read_to_string(directory.join(format!("{stage}.args"))).unwrap();
    let executable = _native::codec_process::executable(tool).unwrap();
    Python::initialize();
    Python::attach(|py| {
        let command = pyo3::types::PyList::empty(py);
        command.append(executable.as_os_str()).unwrap();
        for arg in args.lines() {
            command.append(arg).unwrap();
        }
        py.import("subprocess")
            .unwrap()
            .getattr("CalledProcessError")
            .unwrap()
            .call1((7, command))
            .unwrap()
            .str()
            .unwrap()
            .to_string()
    })
}

pub fn check(directory: &Path, case: &str, runtime: &tokio::runtime::Runtime) {
    assert!(CASES.contains(&case));
    let source = directory.join("clip.webm");
    let before = if case == "remux_equal_large" {
        let mut bytes = vec![b'x'; 262145];
        bytes[..4].copy_from_slice(b"\x1a\x45\xdf\xa3");
        bytes
    } else {
        b"\x1a\x45\xdf\xa3 not a real video".to_vec()
    };
    std::fs::write(&source, &before).unwrap();
    let decoder = VideoDecoder::new(directory).unwrap();
    let (changed, warning) = if case == "remux_long" {
        (runtime.block_on(reencode(&decoder, &source)).unwrap(), None)
    } else {
        let outcome = runtime.block_on(reencode_typed(&decoder, &source)).unwrap();
        (outcome.rewritten, outcome.warning)
    };
    assert_eq!(
        changed,
        matches!(case, "remux_short" | "remux_nan"),
        "{case}: {warning:?}"
    );
    let expected_warning = match case {
        "remux_source_long" => Some("re-encode of clip.webm would declare 2.500s, over the 3.0s cap (the source under-declares its own length); keeping the original".to_owned()),
        "remux_second_nan" => Some("re-encode of clip.webm would declare nans, over the 3.0s cap (the source under-declares its own length); keeping the original".to_owned()),
        "remux_probe_json" => Some("re-encode skipped for clip.webm (video): Expecting property name enclosed in double quotes: line 1 column 2 (char 1)".to_owned()),
        "remux_probe_null" => Some("re-encode skipped for clip.webm (video): 'NoneType' object has no attribute 'get'".to_owned()),
        "remux_second_exit" => Some(format!(
            "re-encode skipped for clip.webm (video): {}",
            command_failure(directory, "ffprobe-2", "ffprobe")
        )),
        "remux_command_exit" => Some(format!(
            "re-encode skipped for clip.webm (video): {}",
            command_failure(directory, "ffmpeg-1", "ffmpeg")
        )),
        "remux_large" => Some("re-encode of clip.webm would be 262145 bytes, over the video cap of 262144; keeping the original".to_owned()),
        _ => None,
    };
    assert_eq!(warning, expected_warning, "{case}");
    assert_eq!(
        std::fs::read(&source).unwrap(),
        if changed {
            b"remuxed fixture bytes".to_vec()
        } else {
            before
        }
    );
    let probes = match case {
        "remux_source_long" | "remux_second_exit" | "remux_second_nan" => 2,
        "remux_command_exit" => 0,
        _ => 1,
    };
    let mut expected: BTreeSet<OsString> = ["clip.webm", "logs"]
        .map(OsString::from)
        .into_iter()
        .collect();
    let mut output = None;
    for (tool, count) in [("ffmpeg", 1), ("ffprobe", probes)] {
        for index in 1..=count {
            let stage = format!("{tool}-{index}");
            let pid = std::fs::read_to_string(directory.join(format!("{stage}.pid")))
                .unwrap()
                .parse::<u32>()
                .unwrap();
            assert!(pid > 0);
            super::process_liveness::assert_stopped(pid);
            let args = std::fs::read_to_string(directory.join(format!("{stage}.args"))).unwrap();
            let args = args.lines().collect::<Vec<_>>();
            if tool == "ffmpeg" {
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
                let staged = Path::new(args[8]);
                assert_eq!(staged.parent().unwrap(), directory);
                assert!(
                    staged
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .starts_with(".video-output-")
                );
                assert!(!staged.exists());
                output = Some(staged.to_path_buf());
            } else {
                assert_eq!(
                    args,
                    [
                        "-v",
                        "error",
                        "-show_entries",
                        "stream=index,codec_type,codec_name,width,height,avg_frame_rate:format=duration,format_name",
                        "-of",
                        "json",
                        output.as_ref().unwrap().to_str().unwrap()
                    ]
                );
            }
            expected.insert(format!("{stage}.pid").into());
            expected.insert(format!("{stage}.args").into());
        }
    }
    assert_eq!(
        members(directory),
        expected,
        "{case}: unexpected process or staged artifact"
    );
    let scratch = directory.join("logs/tmp");
    assert!(std::fs::read_dir(&scratch).unwrap().next().is_none());
    assert!(!_native::process_owner::cleanup_pending(directory));
    if matches!(case, "remux_long" | "remux_short") {
        // Reuse the existing admission fault: an unknown child lifetime cannot become false.
        std::fs::remove_dir(&scratch).unwrap();
        std::fs::write(&scratch, b"owned remux admission refusal").unwrap();
        let before = std::fs::read(&source).unwrap();
        if case == "remux_long" {
            assert!(runtime.block_on(reencode(&decoder, &source)).is_err());
        } else {
            let error = runtime
                .block_on(reencode_typed(&decoder, &source))
                .err()
                .unwrap();
            assert_eq!(error.kind, "UnverifiedCodecError");
            assert!(!error.handled);
        }
        assert_eq!(std::fs::read(&source).unwrap(), before);
        assert_eq!(
            std::fs::read(&scratch).unwrap(),
            b"owned remux admission refusal"
        );
        let retained = std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".video-output-")
            })
            .collect::<Vec<_>>();
        assert_eq!(
            retained.len(),
            1,
            "outer refusal must retain staged evidence"
        );
        let metadata = std::fs::symlink_metadata(&retained[0]).unwrap();
        assert!(metadata.is_file() && !metadata.file_type().is_symlink());
        assert_eq!(
            metadata.len(),
            0,
            "admission failed before a tool could write"
        );
        expected.insert(retained[0].file_name().unwrap().to_owned());
        assert_eq!(members(directory), expected);
        std::fs::remove_file(&retained[0]).unwrap();
        std::fs::remove_file(&scratch).unwrap();
        std::fs::create_dir(&scratch).unwrap();
    }
    std::fs::write(directory.join("checked"), b"ok\n").unwrap();
}
