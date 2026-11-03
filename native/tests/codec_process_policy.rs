//! Source timeout policy and real process ownership around controlled external-tool stand-ins.
use _native::codec_process;
use serde_json::Value;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::time::Instant;
#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/codec_success.rs"]
mod codec_success;
#[path = "support/process_liveness.rs"]
mod process_liveness;
#[path = "support/remux_policy.rs"]
mod remux_policy;
#[path = "support/video_fidelity.rs"]
mod video_fidelity;

#[test]
fn timeout_defaults_unicode_integer_and_warnings_match_pinned_source() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/codec-timeout.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_hash"],
        "9e4c0a045f031bffd23fe5ff9e51a132363e9952a1cdde47d071bfc81234e887"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 14);
    for case in cases {
        let policy = codec_process::codec_budget(case["raw"].as_str()).unwrap();
        assert_eq!(
            policy.wall.as_secs(),
            case["seconds"].as_u64().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            policy.warning.as_deref().unwrap_or(""),
            case["stderr"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            policy.idle, policy.wall,
            "source has per-child wall bound; adapter owns outer idle"
        );
    }
    let policy = codec_process::codec_budget(Some("\u{1c}+۴_۲\u{1f}")).unwrap();
    assert_eq!(policy.wall.as_secs(), 42);
    assert_eq!(policy.idle, policy.wall);
    assert!(policy.warning.is_none());
    let policy = codec_process::codec_budget(Some("-۰۰۹")).unwrap();
    assert_eq!(policy.wall.as_secs(), 1);
    assert_eq!(
        policy.warning.as_deref(),
        Some("WARNING: EMOJI_FFMPEG_TIMEOUT=-9 below 1; using 1.\n")
    );
    let policy = codec_process::codec_budget(Some(&"0".repeat(4300))).unwrap();
    assert_eq!(policy.wall.as_secs(), 1);
    assert_eq!(
        policy.warning.as_deref(),
        Some("WARNING: EMOJI_FFMPEG_TIMEOUT=0 below 1; using 1.\n")
    );
    let policy = codec_process::codec_budget(Some(&"0".repeat(4301))).unwrap();
    assert_eq!(policy.wall.as_secs(), 300);
    assert_eq!(
        policy.warning.as_deref(),
        Some("WARNING: EMOJI_FFMPEG_TIMEOUT is not a whole number; using 300.\n")
    );
    assert_eq!(
        codec_process::codec_budget(Some("18446744073709551615"))
            .unwrap()
            .wall
            .as_secs(),
        u64::MAX
    );
    assert!(
        codec_process::codec_budget(Some("18446744073709551616"))
            .err()
            .unwrap()
            .contains("exceeds native duration range; refusing")
    );
}

#[test]
fn bounded_capture_reads_stdin_after_reaping_and_times_out_the_owned_tree() {
    if let Some(root) = std::env::var_os("NUMERA_CODEC_TIMEOUT_ROOT") {
        original_call_site_timeouts(Path::new(&root));
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let scratch = root
        .join("logs/tmp")
        .join(format!("codec process policy {}", std::process::id()));
    std::fs::create_dir_all(scratch.parent().unwrap()).unwrap();
    std::fs::create_dir(&scratch).unwrap();
    let input = scratch.join("input.txt");
    std::fs::write(&input, "source bytes é\n".as_bytes()).unwrap();
    let environment = std::env::vars_os().collect::<BTreeMap<OsString, OsString>>();
    let worker = std::path::PathBuf::from(
        std::env::var_os("NUMERA_MEDIA_FIXTURE_WORKER").expect("hosted fixture worker is required"),
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let normal = runtime
        .block_on(codec_process::capture_with_budget(
            &scratch,
            &worker,
            &["--capture-echo".into()],
            1024,
            Some(&input),
            &environment,
            codec_process::codec_budget(Some("1")).unwrap(),
        ))
        .unwrap();
    assert_eq!(normal.stdout, "source bytes é\n".as_bytes());
    assert_eq!(normal.stderr, b"fixture capture stderr\n");
    assert_eq!(
        std::fs::read(&input).unwrap(),
        "source bytes é\n".as_bytes()
    );
    let explicit_ready = scratch.join("explicit-timeout-ready");
    let started = Instant::now();
    let refused = runtime
        .block_on(codec_process::capture_with_budget(
            &scratch,
            &worker,
            &[
                "--capture-hold".into(),
                explicit_ready.to_string_lossy().into_owned(),
            ],
            1024,
            None,
            &environment,
            codec_process::Budget {
                wall: std::time::Duration::from_millis(500),
                idle: std::time::Duration::from_millis(500),
                warning: None,
            },
        ))
        .err()
        .expect("hung child must refuse");
    assert!(refused.contains("wall budget"), "{refused}");
    let explicit_pid = std::fs::read_to_string(&explicit_ready)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    assert!(explicit_pid > 0);
    process_liveness::assert_stopped(explicit_pid);
    assert!(
        started.elapsed().as_secs() < 15,
        "explicit half-second budget plus existing10s reap ceiling"
    );
    assert_eq!(
        std::fs::read_dir(scratch.join("logs/tmp")).unwrap().count(),
        0
    );
    let call_root = scratch.join("call sites");
    std::fs::create_dir(&call_root).unwrap();
    let tools = call_root.join("tools");
    std::fs::create_dir(&tools).unwrap();
    std::fs::create_dir(call_root.join("cases")).unwrap();
    for name in ["ffmpeg", "ffprobe"] {
        std::fs::copy(
            &worker,
            tools.join(if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.into()
            }),
        )
        .unwrap();
    }
    let original = std::env::vars().collect();
    let child_environment = _native::tooling_test_runner::scrubbed_environment(&original);
    let mut paths = vec![tools];
    if let Some(path) = child_environment.get("PATH") {
        paths.extend(std::env::split_paths(path));
    }
    let owner_directory = call_root.join("owner");
    std::fs::create_dir(&owner_directory).unwrap();
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "bounded_capture_reads_stdin_after_reaping_and_times_out_the_owned_tree",
            "--nocapture",
            "--test-threads=1",
        ])
        .current_dir(&call_root)
        .env_clear()
        .envs(child_environment)
        .env("PATH", std::env::join_paths(paths).unwrap())
        .env("NUMERA_CODEC_TIMEOUT_ROOT", &call_root)
        .env("EMOJI_FFMPEG_TIMEOUT", "1")
        .env("TEMP", &call_root)
        .env("TMP", &call_root)
        .env("TMPDIR", &call_root)
        .stdin(std::process::Stdio::null())
        .stdout(std::fs::File::create(call_root.join("stdout.log")).unwrap())
        .stderr(std::fs::File::create(call_root.join("stderr.log")).unwrap());
    let mut owner =
        _native::process_owner::OwnedProcess::spawn_session(&mut command, owner_directory).unwrap();
    drop(command);
    let status = runtime.block_on(owner.wait(
        std::time::Duration::from_secs(25),
        std::time::Duration::from_secs(12),
    ));
    owner.finish().unwrap();
    assert!(
        status.unwrap().success(),
        "{}",
        std::fs::read_to_string(call_root.join("stderr.log")).unwrap()
    );
    for name in CALL_SITES {
        assert_eq!(
            std::fs::read(call_root.join("cases").join(name).join("checked")).unwrap(),
            b"ok\n"
        );
    }
    assert!(!call_root.join("owner").exists());
    std::fs::remove_dir_all(scratch).unwrap();
}

const CALL_SITES: [&str; 44] = [
    "probe_video",
    "to_video_webm",
    "content_digest",
    "fingerprint",
    "video_frames",
    "first_frame",
    "publication_probe",
    "publication_probe_failed",
    "publication_probe_visible",
    "publication_probe_fade",
    "publication_probe_blank",
    "publication_probe_noise",
    "publication_probe_empty",
    "publication_probe_oversize",
    "publication_probe_metadata_failed",
    "publication_probe_metadata_invalid",
    "publication_probe_metadata_hang",
    "publication_probe_metadata_oversize",
    "publication_probe_decoder_failed",
    "publication_probe_decoder_missing",
    "publication_probe_decoder_oversize",
    "publication_probe_alpha",
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
    "fingerprint_once",
    "convert_alpha",
    "convert_static",
    "fidelity_missing_vp9",
    "fidelity_missing_vp8",
    "fidelity_probe_recovery",
    "fidelity_no_codec",
    "fidelity_capability_recovery",
    "fidelity_api_recovery",
    "preview_exit",
    "preview_wall",
];

fn original_call_site_timeouts(root: &Path) {
    use _native::codec_video::VideoDecoder;

    let root = _native::tooling_environment::physical_root(root).unwrap();
    let project = _native::tooling_environment::physical_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
    )
    .unwrap();
    assert!(root.starts_with(project.join("logs/tmp")));
    assert_eq!(std::env::var("EMOJI_FFMPEG_TIMEOUT").unwrap(), "1");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for name in CALL_SITES {
        let directory = root.join("cases").join(name);
        std::fs::create_dir(&directory).unwrap();
        let source = directory.join("clip.webm");
        let original = if name == "convert_static" {
            b"GIF89a not a real image".as_slice()
        } else {
            b"\x1a\x45\xdf\xa3 not a real video".as_slice()
        };
        std::fs::write(&source, original).unwrap();
        let output = directory.join("out.webm");
        let decoder = VideoDecoder::new(&directory).unwrap();
        let started = Instant::now();
        if video_fidelity::CASES.contains(&name) {
            video_fidelity::check(&directory, name, &runtime);
            assert!(started.elapsed().as_secs() < 15);
            continue;
        }
        if matches!(
            name,
            "fingerprint_once" | "convert_alpha" | "convert_static"
        ) {
            codec_success::check(&directory, name, &runtime);
            continue;
        }
        if remux_policy::CASES.contains(&name) {
            remux_policy::check(&directory, name, &runtime);
            assert!(started.elapsed().as_secs() < 15);
            continue;
        }
        let preview = name.starts_with("preview_");
        let result = if preview {
            let error = runtime
                .block_on(_native::codec_video_preview::preview_typed(
                    &decoder, &source, &output, 52, 15, true,
                ))
                .unwrap_err();
            let wall = name == "preview_wall";
            assert_eq!(
                error.kind,
                if wall {
                    "MediaError"
                } else {
                    "CalledProcessError"
                }
            );
            assert_eq!(error.handled, wall);
            if wall {
                assert_eq!(
                    error.message,
                    if cfg!(windows) {
                        "ffmpeg.exe timed out after 1s"
                    } else {
                        "ffmpeg timed out after 1s"
                    }
                );
                let reply = _native::codec_error::CodecError::worker_reply(Err(error)).unwrap();
                assert_eq!(reply.exit, 1);
                assert_eq!(reply.envelope["ok"], false);
            } else {
                assert!(error.message.ends_with("returned non-zero exit status 7."));
                assert!(_native::codec_error::CodecError::worker_reply(Err(error)).is_err());
            }
            Ok(())
        } else if name.starts_with("publication_probe") {
            publication_admission(&directory, &source, name, &runtime);
            Ok(())
        } else {
            runtime.block_on(async {
                match name {
                    "probe_video" => decoder.probe(&source).await.map(|_| ()),
                    "to_video_webm" => {
                        _native::codec_video_encode::convert(&decoder, &source, &output, 262144)
                            .await
                            .map(|_| ())
                    }
                    "content_digest" => decoder.content_key(&source).await.map(|_| ()),
                    "fingerprint" => decoder.fingerprint(&source).await.map(|_| ()),
                    "video_frames" => decoder.frames_rgba(&source, 10).await.map(|_| ()),
                    "first_frame" => decoder.perceptual_hash(&source).await.map(|_| ()),
                    _ => unreachable!(),
                }
            })
        };
        if name.starts_with("publication_probe") || preview {
            result.expect("settled command outcome must match its source error policy");
        } else {
            let error =
                result.expect_err("hanging tool must fail, never return a byte-hash fallback");
            assert!(error.contains("wall budget"), "{name}: {error}");
        }
        assert!(
            started.elapsed().as_secs() < 15,
            "{name} exceeded the one-second budget plus reap ceiling"
        );
        let marker = std::fs::read_to_string(directory.join("hanging-tool")).unwrap();
        let lines = marker.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0],
            if name == "probe_video" || name.starts_with("publication_probe_metadata_") {
                "ffprobe"
            } else {
                "ffmpeg"
            }
        );
        let pid = lines[1].parse::<u32>().unwrap();
        assert!(pid > 0);
        process_liveness::assert_stopped(pid);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert!(!output.exists());
        assert_eq!(
            std::fs::read_dir(directory.join("logs/tmp"))
                .unwrap()
                .count(),
            0
        );
        if name == "publication_probe" {
            let blocked = directory.join("logs/tmp");
            std::fs::remove_dir(&blocked).unwrap();
            std::fs::write(&blocked, b"owned admission refusal").unwrap();
            let refusal = runtime.block_on(_native::publication_media::usable(
                &directory, &source, "video",
            ));
            assert!(
                refusal.is_err(),
                "capture admission failure must never become usable=true"
            );
            assert_eq!(std::fs::read(&blocked).unwrap(), b"owned admission refusal");
            std::fs::remove_file(&blocked).unwrap();
            std::fs::create_dir(&blocked).unwrap();
        }
        let mut members = std::fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        members.sort();
        let mut expected = vec!["clip.webm", "hanging-tool", "logs"];
        if name.starts_with("publication_probe") || preview {
            expected.push("probe-arguments");
            if !preview {
                expected.push("publisher");
            }
            let probe = std::fs::read_to_string(directory.join("probe-arguments")).unwrap();
            assert_eq!(
                probe.lines().collect::<Vec<_>>(),
                [
                    "-v",
                    "error",
                    "-show_entries",
                    "stream=index,codec_type,codec_name,width,height,avg_frame_rate:format=duration,format_name",
                    "-of",
                    "json",
                    source.to_str().unwrap()
                ]
            );
            let capability =
                name.starts_with("publication_probe_decoder_") || name == "publication_probe_alpha";
            if capability {
                expected.push("decoder-arguments");
                assert_eq!(
                    std::fs::read(directory.join("decoder-arguments")).unwrap(),
                    b"-v\nerror\n-decoders"
                );
            } else {
                assert!(!directory.join("decoder-arguments").exists());
            }
            if !name.starts_with("publication_probe_metadata_")
                && !name.starts_with("publication_probe_decoder_")
            {
                expected.push("decode-arguments");
                assert!(directory.join("decode-arguments").is_file());
            } else {
                assert!(!directory.join("decode-arguments").exists());
            }
        }
        expected.sort();
        assert_eq!(
            members,
            expected.into_iter().map(OsString::from).collect::<Vec<_>>()
        );
        std::fs::write(directory.join("checked"), b"ok\n").unwrap();
    }
}

fn publication_admission(
    root: &Path,
    source: &Path,
    case: &str,
    runtime: &tokio::runtime::Runtime,
) {
    use _native::{
        atomic,
        catalog::Catalog,
        locks::Lease,
        media_adapter::MediaAdapter,
        publication::Publisher,
        publish_collection::{Options, publish},
        telegram::Telegram,
    };
    use bot_api::{Reply, Server, TOKEN};
    use serde_json::json;

    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let data = root.join("publisher");
    let path = data.join("publish_fixture.json");
    let key = "v:publication-fixture";
    let original = std::fs::read(source).unwrap();
    let mut cat = Catalog::open(&data.join("catalog.db"), "fixture UTC", root).unwrap();
    assert!(
        cat.insert(
            &json!({"content_key":key,"fmt":"video","file_path":source,
        "emojis":["😀"],"keywords":["publication probe"]}),
            "fixture UTC"
        )
        .unwrap()
    );
    let rows = cat.all(None).unwrap();
    let initial = json!({"base":"fixture","sets":[],"sent":[],"sent_full":[],
        "skipped":[],"in_flight":null});
    atomic::write_json(&path, &initial).unwrap();
    let before = std::fs::read(&path).unwrap();
    let blank = matches!(
        case,
        "publication_probe_blank" | "publication_probe_noise" | "publication_probe_alpha"
    );
    let oversized = matches!(
        case,
        "publication_probe_oversize"
            | "publication_probe_metadata_oversize"
            | "publication_probe_decoder_oversize"
    );
    let mut replies = Vec::new();
    if !blank && !oversized {
        let input = json!({"sticker":"attach://file0","format":"video",
            "emoji_list":["😀"],"keywords":["publication probe"]});
        let mut reply = Reply::new(
            "createNewStickerSet",
            &[],
            json!({"ok":false,"description":"Bad Request: controlled publication refusal"}),
        );
        reply.fields = BTreeMap::from([
            ("user_id".into(), "111111111".into()),
            ("name".into(), "fixture1_by_YourEmojiBot".into()),
            ("title".into(), "Fixture 1".into()),
            ("sticker_type".into(), "custom_emoji".into()),
            ("stickers".into(), json!([input]).to_string()),
        ]);
        reply.files.insert("file0".into(), original.clone());
        reply.observation = Some((path.clone(), "/in_flight/format".into(), json!("video")));
        replies.push(reply);
    }
    let mut server = Server::new(replies);
    let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
    let media = MediaAdapter {
        // Any accidental generic usable/Python decode is a hard failure in this consumer tracer.
        python: root.join("unavailable-python"),
        root: root.into(),
        scratch: root.join("unexpected-python-codec"),
    };
    let options = Options {
        base: "fixture".into(),
        title: "Fixture".into(),
        bot: "YourEmojiBot".into(),
        user_id: 111111111,
        fmt: "mixed".into(),
        per_set: 200,
        new_set: false,
        into_pack: None,
        repaint: false,
        default_emoji: "😀".into(),
        logo: None,
    };
    let mut state = initial.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        publish(
            &mut Publisher {
                tg: &tg,
                cat: &mut cat,
                media: &media,
                data: data.clone(),
                runtime,
                config: None,
            },
            &options,
            &[key.into()],
            &mut state,
        )
    }));
    let observed = server.finish();
    let result = match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    };
    let observed = observed.unwrap();
    assert_eq!(observed.len(), usize::from(!blank && !oversized));
    let mut expected = initial.clone();
    if oversized {
        assert!(result.unwrap_err().contains("output limit"));
        assert_eq!(std::fs::read(&path).unwrap(), before);
    } else {
        assert_eq!(
            result.unwrap(),
            json!({"uploaded":0,"failed":u64::from(!blank)})
        );
        if blank {
            expected["skipped"] = json!([key]);
        } else {
            let intent = &observed[0].observation.as_ref().unwrap()["in_flight"];
            assert_eq!(intent["key"], key);
            assert_eq!(intent["operation"], "create");
            assert_eq!(intent["format"], "video");
        }
    }
    assert_eq!(state, expected);
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap(),
        expected
    );
    assert_eq!(cat.all(None).unwrap(), rows);
    assert_eq!(
        cat.dispatch(json!({"operation":"published","base":"fixture","content_key":key}))
            .unwrap(),
        false
    );
    assert!(!media.scratch.exists());
    assert!(!data.join("manifests").exists());
    assert_eq!(std::fs::read(source).unwrap(), original);
    drop(cat);
    let lease = Lease::acquire(&data.join("publish_fixture.lock"), "after publication").unwrap();
    drop(lease);
    assert!(!_native::process_owner::cleanup_pending(root));
}
