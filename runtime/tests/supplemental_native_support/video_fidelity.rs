// Retained helper from native/tests/support/video_fidelity.rs; source SHA256 95d8d72e577142e8723ad1546846a9bd1859ffee34039a5a158c4cceadeeb31a.
//! Original fidelity refusals/recovery at public decoder and catalog boundaries.
use _native::{catalog::Catalog, codec_video::VideoDecoder};
use serde_json::json;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::Path;

pub const CASES: [&str; 6] = [
    "fidelity_missing_vp9",
    "fidelity_missing_vp8",
    "fidelity_probe_recovery",
    "fidelity_no_codec",
    "fidelity_capability_recovery",
    "fidelity_api_recovery",
];

fn command_failure(directory: &Path, stage: &str, index: usize, tool: &str) -> String {
    let args = std::fs::read_to_string(directory.join(format!("{stage}-{index}.args"))).unwrap();
    super::super::fixture::command_failure(tool, args.lines(), 1)
}

pub fn check(directory: &Path, case: &str, runtime: &tokio::runtime::Runtime) {
    assert!(CASES.contains(&case));
    let opaque = directory.join("clip.webm");
    let translucent = directory.join("translucent.webm");
    let before = std::fs::read(&opaque).unwrap();
    let faint = b"\x1a\x45\xdf\xa3 undecoded translucent fixture";
    std::fs::write(&translucent, faint).unwrap();
    let decoder = VideoDecoder::new(directory).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    runtime.block_on(async {
        match case {
            "fidelity_missing_vp9" | "fidelity_missing_vp8" => {
                let mut keys = Vec::new();
                for path in [&opaque, &translucent] {
                    let result = decoder.fingerprint_typed(path).await;
                    let error = result.as_ref().unwrap_err();
                    let (codec, name) = if case == "fidelity_missing_vp9" {
                        ("vp9", "libvpx-vp9")
                    } else {
                        ("vp8", "libvpx")
                    };
                    assert_eq!(error.kind, "UndecodableVideo");
                    assert!(error.handled);
                    assert_eq!(error.message, format!(
                        "{}: {codec} keeps alpha in a separate layer and this ffmpeg has no {name} decoder, so a decode here cannot be trusted to carry transparency",
                        path.file_name().unwrap().to_str().unwrap()
                    ));
                    keys.push(result.as_ref().ok().cloned());
                    let reply = _native::codec_error::CodecError::worker_reply(result).unwrap();
                    assert_eq!(reply.exit, 1);
                    assert_eq!(reply.envelope["ok"], false);
                }
                assert_eq!(keys, [None, None], "a refusal must not invent a byte hash");
                let db = directory.join("r01.db");
                // Match the original pre-insert boundary: no key means no Catalog is even opened.
                let result: Result<(), String> = async {
                    let key = decoder.content_key(&opaque).await?;
                    let mut catalog = Catalog::open(&db, "fixture UTC", directory)?;
                    catalog.insert(
                        &json!({"content_key":key,"fmt":"video","file_path":opaque}),
                        "fixture UTC",
                    )?;
                    Ok(())
                }
                .await;
                assert!(
                    result
                        .unwrap_err()
                        .contains("alpha fidelity cannot be established")
                );
                assert!(!db.exists());
                assert!(!directory.join("r01.db-wal").exists());
                assert!(!directory.join("r01.db-shm").exists());
            }
            "fidelity_no_codec" => {
                let error = decoder.fingerprint_typed(&opaque).await.unwrap_err();
                assert_eq!(error.kind, "UndecodableVideo");
                assert_eq!(error.message, "clip.webm: the container named no video codec, so alpha fidelity cannot be established");
                assert!(error.handled);
            }
            "fidelity_probe_recovery" | "fidelity_capability_recovery" => {
                // Reuse this exact instance/file/tool: a fresh decoder would hide a poisoned cache.
                if case == "fidelity_probe_recovery" {
                    // Native probe failures must escape before metadata/capability/decode.
                    let error = tokio::time::timeout(deadline.saturating_duration_since(std::time::Instant::now()),
                        _native::codec_video_validate::validate_typed(&decoder, &opaque)).await.unwrap().unwrap_err();
                    assert_eq!(error.kind, "CalledProcessError");
                    assert_eq!(error.message, command_failure(directory, "probe", 1, "ffprobe"));
                    assert!(!error.handled);
                    assert!(_native::codec_error::CodecError::worker_reply(Err(error)).is_err());
                    assert!(!directory.join("capability-1.pid").exists());
                    assert!(!directory.join("decode-1.pid").exists());
                    // The actual tool removes its owned source after returning metadata.
                    // This exercises real Rust fs::metadata failure after a successful probe.
                    // Python Path.stat callback OSError/TypeError/KeyboardInterrupt/SystemExit
                    // apparatus disappears; no native callback exceptions are fabricated.
                    std::fs::write(directory.join("native-stat-remove"), b"remove after probe").unwrap();
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    let error = tokio::time::timeout(remaining,
                        _native::codec_video_validate::validate_typed(&decoder, &opaque)).await.unwrap().unwrap_err();
                    assert_eq!(error.kind, "OSError");
                    assert!(error.handled);
                    assert_eq!(error.message, std::fs::metadata(&opaque).unwrap_err().to_string());
                    assert!(!directory.join("capability-1.pid").exists());
                    assert!(!directory.join("decode-1.pid").exists());
                    std::fs::write(&opaque, &before).unwrap();
                    assert_eq!(std::fs::metadata(&opaque).unwrap().len(), before.len() as u64);

                } else {
                    let error = decoder.fingerprint_typed(&opaque).await.unwrap_err();
                    assert_eq!(error.kind, "UndecodableVideo");
                    assert!(error.handled);
                    assert_eq!(error.message, format!(
                        "could not ask ffmpeg which decoders it has, so alpha fidelity cannot be established: {}",
                        command_failure(directory, "capability", 1, "ffmpeg")
                    ));
                }
                for _ in 0..2 {
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    assert!(!remaining.is_zero());
                    assert_eq!(
                        tokio::time::timeout(remaining, decoder.decoder_args(&opaque)).await.unwrap().unwrap(),
                        ["-c:v", "libvpx-vp9"]
                    );
                }
            }
            "fidelity_api_recovery" => {
                let error = decoder.fingerprint_typed(&opaque).await.unwrap_err();
                assert_eq!(error.kind, "UndecodableVideo");
                assert!(error.handled);
                assert_eq!(error.message, format!(
                    "could not ask ffmpeg which decoders it has, so alpha fidelity cannot be established: {}",
                    command_failure(directory, "capability", 1, "ffmpeg")
                ));
                assert_eq!(_native::codec_error::CodecError::worker_reply(Err(error)).unwrap().exit, 1);
                let fingerprint = decoder.fingerprint_typed(&opaque).await.unwrap();
                // Same independently fixed RGBA sample/key as existing codec_success, not a target golden.
                assert_eq!(
                    fingerprint["content_key"],
                    "v:769f9a9a414a35bdb6e3ee62419e9c1e"
                );
                assert_eq!(fingerprint["phash"], 0);
                assert_eq!(
                    decoder.content_key(&opaque).await.unwrap(),
                    fingerprint["content_key"]
                );
                assert_eq!(
                    json!(decoder.perceptual_hash(&opaque).await.unwrap()),
                    fingerprint["phash"]
                );
                let tool = _native::codec_process::executable("ffmpeg").unwrap();
                let expected_tools = directory.parent().unwrap().parent().unwrap().join("tools");
                assert_eq!(tool.parent(), Some(expected_tools.as_path()));
                let tool_bytes = std::fs::read(&tool).unwrap();
                let previous = tool.metadata().unwrap().modified().unwrap();
                let changed = std::time::SystemTime::UNIX_EPOCH
                    + std::time::Duration::from_secs(1_700_000_000);
                assert_ne!(previous, changed);
                {
                    let file = std::fs::File::options().write(true).open(&tool).unwrap();
                    file.set_modified(changed).unwrap();
                }
                assert_eq!(tool.metadata().unwrap().modified().unwrap(), changed);
                let result = _native::codec_video_validate::is_blank_typed(&decoder, &opaque).await;
                {
                    let file = std::fs::File::options().write(true).open(&tool).unwrap();
                    file.set_modified(previous).unwrap();
                }
                assert_eq!(tool.metadata().unwrap().modified().unwrap(), previous);
                assert_eq!(std::fs::read(&tool).unwrap(), tool_bytes);
                let error = result.unwrap_err();
                assert_eq!(error.kind, "UndecodableVideo");
                assert_eq!(error.message, format!(
                    "clip.webm: the codec could not be read, so the decoder that preserves its alpha cannot be chosen: {}",
                    command_failure(directory, "probe", 3, "ffprobe")
                ));
                assert!(error.handled);
            }
            _ => unreachable!(),
        }
    });
    assert_eq!(std::fs::read(&opaque).unwrap(), before);
    assert_eq!(std::fs::read(&translucent).unwrap(), faint);
    let (probes, capabilities, decodes) = match case {
        "fidelity_missing_vp9" | "fidelity_missing_vp8" => (3, 1, 0),
        "fidelity_probe_recovery" => (3, 1, 0),
        "fidelity_no_codec" => (1, 0, 0),
        "fidelity_capability_recovery" => (2, 2, 0),
        "fidelity_api_recovery" => (3, 2, 1),
        _ => unreachable!(),
    };
    let mut expected = ["clip.webm", "translucent.webm", "logs"]
        .map(OsString::from)
        .into_iter()
        .collect::<BTreeSet<_>>();
    for (stage, count) in [
        ("probe", probes),
        ("capability", capabilities),
        ("decode", decodes),
    ] {
        for index in 1..=count {
            let prefix = format!("{stage}-{index}");
            let pid = std::fs::read_to_string(directory.join(format!("{prefix}.pid")))
                .unwrap()
                .parse()
                .unwrap();
            super::process_liveness::assert_stopped(pid);
            for suffix in ["pid", "args"] {
                expected.insert(format!("{prefix}.{suffix}").into());
            }
            let args = std::fs::read_to_string(directory.join(format!("{prefix}.args"))).unwrap();
            let args = args.lines().collect::<Vec<_>>();
            match stage {
                "probe" => {
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
                    let path = if case.starts_with("fidelity_missing_") && index == 2 {
                        &translucent
                    } else {
                        &opaque
                    };
                    assert_eq!(args[6], path.to_str().unwrap());
                    assert_eq!(args.len(), 7);
                }
                "capability" => assert_eq!(args, ["-v", "error", "-decoders"]),
                "decode" => {
                    let input = args.iter().position(|arg| *arg == "-i").unwrap();
                    let codec = args
                        .windows(2)
                        .position(|pair| pair == ["-c:v", "libvpx-vp9"])
                        .unwrap();
                    assert!(codec < input);
                    assert_eq!(args[input + 1], opaque.to_str().unwrap());
                    assert!(
                        args.windows(2)
                            .any(|pair| pair == ["-vf", "fps=10,scale=64:64,format=rgba"])
                    );
                }
                _ => unreachable!(),
            }
        }
    }
    assert_eq!(
        std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<BTreeSet<_>>(),
        expected
    );
    assert_eq!(
        std::fs::read_dir(directory.join("logs/tmp"))
            .unwrap()
            .count(),
        0
    );
    assert!(!_native::process_owner::cleanup_pending(directory));
    std::fs::write(directory.join("checked"), b"ok\n").unwrap();
}
