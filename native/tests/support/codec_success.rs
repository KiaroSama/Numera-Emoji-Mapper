//! Successful controlled-tool observations in the existing isolated codec process cohort.
use _native::codec_video::VideoDecoder;
use serde_json::json;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::Path;

pub fn check(directory: &Path, name: &str, runtime: &tokio::runtime::Runtime) {
    assert!(matches!(
        name,
        "fingerprint_once" | "convert_alpha" | "convert_static"
    ));
    let source = directory.join("clip.webm");
    let before = std::fs::read(&source).unwrap();
    let output = directory.join("out.webm");
    let decoder = VideoDecoder::new(directory).unwrap();
    runtime.block_on(async {
        if name == "fingerprint_once" {
            let fingerprint = decoder.fingerprint_typed(&source).await.unwrap();
            assert_eq!(
                fingerprint["content_key"],
                "v:769f9a9a414a35bdb6e3ee62419e9c1e"
            );
            assert_eq!(fingerprint["phash"], 0);
            assert_eq!(
                decoder.content_key(&source).await.unwrap(),
                fingerprint["content_key"]
            );
            assert_eq!(decoder.perceptual_hash(&source).await.unwrap(), Some(0));
        } else {
            let converted =
                _native::codec_video_encode::convert_typed(&decoder, &source, &output, 262144)
                    .await
                    .unwrap();
            assert_eq!(converted, json!({"bytes":21}));
            assert_eq!(std::fs::read(&output).unwrap(), b"encoded fixture bytes");
        }
    });
    let stages: &[&str] = match name {
        "fingerprint_once" => &["probe", "decode"],
        "convert_alpha" => &["probe", "capability", "decode"],
        _ => &["decode"],
    };
    let mut expected = ["clip.webm", "logs"]
        .map(OsString::from)
        .into_iter()
        .collect::<BTreeSet<_>>();
    if name != "fingerprint_once" {
        expected.insert("out.webm".into());
    }
    for stage in stages {
        let pid = std::fs::read_to_string(directory.join(format!("{stage}-pid")))
            .unwrap()
            .parse()
            .unwrap();
        super::process_liveness::assert_stopped(pid);
        for suffix in ["pid", "args"] {
            expected.insert(format!("{stage}-{suffix}").into());
        }
    }
    assert_eq!(
        std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<BTreeSet<_>>(),
        expected
    );
    let args = std::fs::read_to_string(directory.join("decode-args")).unwrap();
    let args = args.lines().collect::<Vec<_>>();
    let input = args.iter().position(|arg| *arg == "-i").unwrap();
    assert_eq!(
        args[..input]
            .windows(2)
            .any(|pair| pair == ["-c:v", "libvpx-vp9"]),
        name == "convert_alpha"
    );
    assert_eq!(std::fs::read(&source).unwrap(), before);
    assert!(
        std::fs::read_dir(directory.join("logs/tmp"))
            .unwrap()
            .next()
            .is_none()
    );
    assert!(!_native::process_owner::cleanup_pending(directory));
    std::fs::write(directory.join("checked"), b"ok\n").unwrap();
}
