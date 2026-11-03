//! Original completed/pending/skip bookkeeping and file progress through the actual command.
use super::{Commands, FIRST, UTC, download, execute, listing, me, member, mutation, notification};
use crate::bot_api::Server;
use _native::catalog::Catalog;
use serde_json::{Value, json};
use std::path::PathBuf;

pub(super) fn run(
    commands: &Commands<'_>,
    images: &[PathBuf; 3],
    bytes: &[Vec<u8>; 3],
    keys: &[String; 3],
) {
    for variant in ["normal", "all-failed", "partial", "permanent"] {
        let data = commands
            .root
            .join(format!("publication-progress-{variant}"));
        let path = data.join("publish_fixture.json");
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
        for index in 0..2 {
            assert!(
                cat.insert(
                    &json!({"content_key":keys[index],"fmt":"static",
                "file_path":images[index],"emojis":["😀"],"keywords":["public | artwork"]}),
                    UTC
                )
                .unwrap()
            );
        }
        drop(cat);
        let first = member("progress-first", "progress-first-unique", "1");
        let second = member("progress-second", "progress-second-unique", "2");
        let mut replies = vec![me()];
        let mut live = Vec::new();
        for index in 0..2 {
            if !live.is_empty() {
                replies.push(listing(FIRST, &live));
            }
            let mut attempt = mutation(
                &path,
                FIRST,
                &keys[index],
                (&bytes[index], false),
                live.is_empty(),
            );
            if variant == "all-failed" || (index == 1 && variant != "normal") {
                attempt.payload = json!({"ok":false,"description":if variant == "permanent" {
                    "Bad Request: wrong file type"
                } else { "Bad Request: temporary fixture refusal" }});
                replies.push(attempt);
            } else {
                replies.push(attempt);
                live.push(if index == 0 {
                    first.clone()
                } else {
                    second.clone()
                });
                replies.push(listing(FIRST, &live));
                download(
                    &mut replies,
                    live.last().unwrap()["file_id"].as_str().unwrap(),
                    &bytes[index],
                );
            }
        }
        if !live.is_empty() {
            replies.push(listing(FIRST, &live));
        }
        if variant == "normal" {
            replies.push(notification(FIRST, false, &path));
        }
        let first_len = replies.len();
        // The second command is a real resume: definite failures remain pending,
        // permanent skips do not, and already committed artwork never uploads again.
        replies.push(me());
        if !live.is_empty() {
            replies.push(listing(FIRST, &live));
        }
        if variant == "all-failed" || variant == "partial" {
            for index in usize::from(variant == "partial")..2 {
                if !live.is_empty() {
                    replies.push(listing(FIRST, &live));
                }
                replies.push(mutation(
                    &path,
                    FIRST,
                    &keys[index],
                    (&bytes[index], false),
                    live.is_empty(),
                ));
                live.push(if index == 0 {
                    first.clone()
                } else {
                    second.clone()
                });
                replies.push(listing(FIRST, &live));
                download(
                    &mut replies,
                    live.last().unwrap()["file_id"].as_str().unwrap(),
                    &bytes[index],
                );
            }
        }
        replies.push(listing(FIRST, &live));
        if variant != "normal" {
            replies.push(notification(FIRST, false, &path));
        }
        let expected = replies
            .iter()
            .map(|reply| (reply.method.clone(), reply.fields.clone()))
            .collect::<Vec<_>>();
        let mut server = Server::new(replies);
        println!("native collection progress command: {variant}");
        let before_logs = std::fs::read_dir(commands.root.join("logs"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<std::collections::BTreeSet<_>>();
        let result = execute(commands, &data, &[], &server, None);
        let logs = std::fs::read_dir(commands.root.join("logs"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                !before_logs.contains(path)
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| {
                            name.starts_with("build-collection_") && name.ends_with(".log")
                        })
            })
            .map(|path| std::fs::read_to_string(path).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(logs.len(), 1, "one command owns one log");
        let log = &logs[0];
        assert!(log.contains("exit="));
        assert_eq!(
            result.code,
            match variant {
                "all-failed" => 4,
                "partial" => 3,
                _ => 0,
            },
            "{variant}: {} {}",
            result.stdout,
            result.stderr
        );
        let summary = match variant {
            "all-failed" => "DONE: 0 uploaded, 2 failed.",
            "partial" => "DONE: 1 uploaded, 1 failed.",
            _ => "\nDONE.\n",
        };
        assert!(
            result.stdout.contains(summary),
            "{variant}: {}",
            result.stdout
        );
        let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(saved["in_flight"].is_null());
        assert_eq!(
            saved["sent"],
            if variant == "normal" {
                json!([FIRST])
            } else {
                json!([])
            }
        );
        assert_eq!(
            saved["skipped"],
            if variant == "permanent" {
                json!([keys[1]])
            } else {
                json!([])
            }
        );
        if variant == "all-failed" {
            assert_eq!(saved["sets"], json!([]));
        } else {
            assert_eq!(
                saved["sets"][0]["keys"],
                if variant == "normal" {
                    json!(keys[..2])
                } else {
                    json!([keys[0]])
                }
            );
            assert_eq!(
                saved["sets"][0]["live"],
                if variant == "normal" { 2 } else { 1 }
            );
        }
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
        for (index, key) in keys[..2].iter().enumerate() {
            let landed = variant != "all-failed" && (index == 0 || variant == "normal");
            assert_eq!(
                cat.dispatch(json!({"operation":"published","base":"fixture","content_key":key}))
                    .unwrap(),
                landed
            );
            assert_eq!(
                cat.dispatch(json!({"operation":"custom_id","base":"fixture","content_key":key}))
                    .unwrap(),
                if landed {
                    json!((index + 1).to_string())
                } else {
                    Value::Null
                }
            );
        }
        drop(cat);
        let repeated = execute(commands, &data, &[], &server, None);
        assert_eq!(
            repeated.code, 0,
            "{variant} resume: {} {}",
            repeated.stdout, repeated.stderr
        );
        assert!(repeated.stdout.contains("\nDONE.\n"));
        let requests = server.finish().unwrap();
        assert_eq!(requests.len(), expected.len());
        for (actual, (method, form)) in requests.iter().zip(expected) {
            assert_eq!(actual.method, method, "{variant}");
            assert_eq!(actual.form, form, "{variant}");
        }
        assert_eq!(
            requests[..first_len]
                .iter()
                .filter(|r| r.method == "sendMessage")
                .count(),
            usize::from(variant == "normal")
        );
        assert_eq!(
            requests[first_len..]
                .iter()
                .filter(|r| r.method == "sendMessage")
                .count(),
            usize::from(variant != "normal")
        );
        let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let count = if variant == "permanent" { 1 } else { 2 };
        assert_eq!(saved["sets"].as_array().unwrap().len(), 1);
        assert_eq!(saved["sets"][0]["keys"], json!(keys[..count]));
        assert_eq!(saved["sets"][0]["live"], count);
        assert_eq!(saved["sets"][0]["logo"], false);
        assert_eq!(saved["sent"], json!([FIRST]));
        assert!(saved["in_flight"].is_null());
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
        for (index, key) in keys[..2].iter().enumerate() {
            assert_eq!(
                cat.dispatch(json!({"operation":"published","base":"fixture","content_key":key}))
                    .unwrap(),
                index < count
            );
            assert_eq!(
                cat.dispatch(json!({"operation":"custom_id","base":"fixture","content_key":key}))
                    .unwrap(),
                if index < count {
                    json!((index + 1).to_string())
                } else {
                    Value::Null
                }
            );
        }
        drop(cat);
        for request in requests.iter().filter(|request| {
            ["createNewStickerSet", "addStickerToSet"].contains(&request.method.as_str())
        }) {
            let observed = request
                .observation
                .as_ref()
                .expect("mutation intent was observed before response");
            let intent = &observed["in_flight"];
            let creating = request.method == "createNewStickerSet";
            assert_eq!(intent["set_name"], FIRST);
            assert_eq!(intent["set_index"], 1);
            assert_eq!(intent["operation"], if creating { "create" } else { "add" });
            assert_eq!(
                intent["expected_before"],
                if creating { Value::Null } else { json!(1) }
            );
            assert_eq!(intent["format"], "static");
            assert!(!intent["started_utc"].as_str().unwrap().is_empty());
        }
        if variant == "normal" {
            let uploads = log
                .lines()
                .filter(|line| line.contains("] uploaded "))
                .collect::<Vec<_>>();
            assert_eq!(uploads.len(), 2, "{log}");
            for (index, line) in uploads.iter().enumerate() {
                assert!(line.contains(&keys[index]) && line.contains(FIRST));
                assert!(line.contains(&format!("{}/2 this run", index + 1)));
                assert!(line.contains(&format!("#{}", index + 1)));
            }
        }
        assert!(!_native::process_owner::cleanup_pending(commands.root));
    }
    blank_media_case(commands, images, bytes, keys, false);
    blank_media_case(commands, images, bytes, keys, true);
    real_video_visibility(commands);
}

fn real_video_visibility(commands: &Commands<'_>) {
    use crate::bounded_media::BoundedCodec;
    use _native::{
        codec_process, media_adapter::MediaCodec, media_native_adapter::NativeMediaAdapter,
    };
    use std::time::{Duration, Instant};

    let baseline: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/source-baseline.json"
    ))
    .unwrap();
    for (source, hash) in [
        (
            "tests/_video_fixtures.py",
            "3b5175f1206e0fe685c53e42ff00bbcc7cbd8a0d36ec74196a8cb342bcc70caf",
        ),
        (
            "tests/test_native_video_visibility.py",
            "91496af63a4b489d2e5debf469050a9888cb2c87ab628defefaea21cdacf1619",
        ),
    ] {
        assert!(
            baseline["files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["source"] == source && row["sha256"] == hash)
        );
    }
    let data = commands.root.join("publication-real-video-visibility");
    std::fs::create_dir(&data).unwrap();
    let ffmpeg = codec_process::executable("ffmpeg").unwrap();
    codec_process::executable("ffprobe").unwrap();
    let environment = commands
        .environment
        .iter()
        .map(|(key, value)| {
            (
                std::ffi::OsString::from(key),
                std::ffi::OsString::from(value),
            )
        })
        .collect();
    let clips = ["blank", "fade"].map(|name| data.join(format!("{name}.webm")));
    for (index, output) in clips.iter().enumerate() {
        let raw = data.join(format!(
            "{}.rgba",
            if index == 0 { "blank" } else { "fade" }
        ));
        let mut frames = vec![0_u8; 100 * 100 * 4 * 30];
        if index == 1 {
            for pixel in frames[100 * 100 * 4 * 15..].chunks_exact_mut(4) {
                pixel.copy_from_slice(&[220, 20, 20, 255]);
            }
        }
        std::fs::write(&raw, &frames).unwrap();
        // Exact original _video_fixtures.encode recipe; CPU libvpx preserves its alpha/lossless contract.
        let args = [
            "-y",
            "-v",
            "error",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            "100x100",
            "-framerate",
            "30",
            "-i",
            raw.to_str().unwrap(),
            "-c:v",
            "libvpx-vp9",
            "-threads",
            "1",
            "-cpu-used",
            "8",
            "-lossless",
            "1",
            "-pix_fmt",
            "yuva420p",
            "-auto-alt-ref",
            "0",
            output.to_str().unwrap(),
        ]
        .map(String::from);
        println!("native real video visibility encode: {index}");
        commands
            .runtime
            .block_on(codec_process::capture_with_budget(
                commands.root,
                &ffmpeg,
                &args,
                4096,
                None,
                &environment,
                codec_process::Budget {
                    wall: Duration::from_secs(5),
                    idle: Duration::from_secs(3),
                    warning: None,
                },
            ))
            .unwrap();
        assert_eq!(std::fs::read(&raw).unwrap(), frames);
        assert!(output.is_file() && output.metadata().unwrap().len() > 0);
        std::fs::remove_file(raw).unwrap();
    }
    let originals = clips.each_ref().map(|path| std::fs::read(path).unwrap());
    let media = BoundedCodec {
        native: NativeMediaAdapter {
            executable: commands.executable.into(),
            root: commands.root.into(),
            scratch: data.join("admission-codec"),
        },
        deadline: Instant::now() + Duration::from_secs(60),
    };
    for (source, expected) in clips.iter().zip([false, true]) {
        assert_eq!(
            commands
                .runtime
                .block_on(media.call(json!({
                    "operation":"usable","source":source,"format":"video"
                })))
                .unwrap(),
            expected
        );
    }
    let fingerprint = commands
        .runtime
        .block_on(media.call(json!({
            "operation":"fingerprint","source":clips[0],"format":"video"
        })))
        .unwrap();
    let key = fingerprint["content_key"].as_str().unwrap();
    let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
    assert!(
        cat.insert(
            &json!({"content_key":key,"fmt":"video","file_path":clips[0],
        "phash":fingerprint["phash"]}),
            UTC
        )
        .unwrap()
    );
    let rows = cat.all(None).unwrap();
    drop(cat);
    let mut server = Server::new(vec![me()]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        execute(commands, &data, &[], &server, None)
    }));
    let requests = server.finish();
    let result = match result {
        Ok(value) => value,
        Err(panic) => std::panic::resume_unwind(panic),
    };
    let requests = requests.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "getMe");
    assert_eq!(result.code, 0, "{} {}", result.stdout, result.stderr);
    assert!(result.stdout.contains("\nDONE.\n"));
    let state: Value =
        serde_json::from_slice(&std::fs::read(data.join("publish_fixture.json")).unwrap()).unwrap();
    assert_eq!(state["skipped"], json!([key]));
    assert_eq!(state["sets"], json!([]));
    assert_eq!(state["sent"], json!([]));
    assert!(state["in_flight"].is_null());
    assert!(!data.join("manifests").exists());
    let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
    assert_eq!(cat.all(None).unwrap(), rows);
    assert_eq!(
        cat.dispatch(json!({"operation":"published_keys"})).unwrap(),
        json!([])
    );
    drop(cat);
    for (path, bytes) in clips.iter().zip(&originals) {
        assert_eq!(&std::fs::read(path).unwrap(), bytes);
    }
    assert!(
        std::fs::read_dir(&media.native.scratch)
            .unwrap()
            .next()
            .is_none()
    );
    assert!(!data.join("tmp/native-codec").exists());
    let lease = _native::locks::Lease::acquire(
        &data.join("publish_fixture.lock"),
        "after video visibility",
    )
    .unwrap();
    drop(lease);
    assert!(!_native::process_owner::cleanup_pending(commands.root));
}

fn blank_media_case(
    commands: &Commands<'_>,
    images: &[PathBuf; 3],
    bytes: &[Vec<u8>; 3],
    keys: &[String; 3],
    unreadable: bool,
) {
    use pyo3::prelude::*;
    let data = commands.root.join(if unreadable {
        "publication-unreadable-static"
    } else {
        "publication-blank-mixed"
    });
    let path = data.join("publish_fixture.json");
    let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
    for index in 0..2 {
        assert!(
            cat.insert(
                &json!({"content_key":keys[index],"fmt":"static",
            "file_path":images[index],"emojis":["😀"],"keywords":["public | artwork"]}),
                UTC
            )
            .unwrap()
        );
    }
    let blank_keys = [
        "s:blank0000000000000000000000000",
        "s:sparse000000000000000000000000",
    ];
    let blank_files = [data.join("blank.png"), data.join("sparse.png")];
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let module = py.import("PIL.Image")?;
        assert_eq!(
            module.getattr("__version__")?.extract::<String>()?,
            "12.3.0"
        );
        for (index, file) in blank_files.iter().enumerate() {
            let image = module.call_method1("new", ("RGBA", (100, 100), (0, 0, 0, 0)))?;
            let saved = (|| -> PyResult<()> {
                if index == 1 {
                    image.call_method1("putpixel", ((5, 5), (255, 255, 255, 255)))?;
                }
                image.call_method1("save", (file, "PNG"))?;
                Ok(())
            })();
            let closed = image.call_method0("close");
            saved?;
            closed?;
        }
        Ok(())
    })
    .unwrap();
    let originals = blank_files
        .each_ref()
        .map(|file| std::fs::read(file).unwrap());
    for (key, file) in blank_keys.iter().zip(&blank_files) {
        assert!(
            cat.insert(
                &json!({"content_key":key,"fmt":"static","file_path":file,
            "emojis":["😀"],"keywords":["blank"]}),
                UTC
            )
            .unwrap()
        );
    }
    let corrupt_key = "s:unreadable-publication-fixture";
    let corrupt = data.join("unreadable.png");
    let corrupt_bytes = b"not an image; the upload must decide";
    if unreadable {
        std::fs::write(&corrupt, corrupt_bytes).unwrap();
        assert!(
            cat.insert(
                &json!({"content_key":corrupt_key,"fmt":"static",
            "file_path":corrupt,"emojis":["😀"],"keywords":["public | artwork"]}),
                UTC
            )
            .unwrap()
        );
    }
    let rows = cat.all(None).unwrap();
    drop(cat);
    let first = member("blank-first", "blank-first-unique", "1");
    let second = member("blank-second", "blank-second-unique", "2");
    let mut replies = vec![
        me(),
        mutation(&path, FIRST, &keys[0], (&bytes[0], false), true),
        listing(FIRST, std::slice::from_ref(&first)),
    ];
    download(&mut replies, "blank-first", &bytes[0]);
    replies.push(listing(FIRST, std::slice::from_ref(&first)));
    replies.push(mutation(&path, FIRST, &keys[1], (&bytes[1], false), false));
    replies.push(listing(FIRST, &[first.clone(), second.clone()]));
    download(&mut replies, "blank-second", &bytes[1]);
    if unreadable {
        replies.push(listing(FIRST, &[first.clone(), second.clone()]));
        let mut attempt = mutation(&path, FIRST, corrupt_key, (corrupt_bytes, false), false);
        attempt.payload =
            json!({"ok":false,"description":"Bad Request: controlled unreadable refusal"});
        replies.push(attempt);
    }
    replies.push(listing(FIRST, &[first, second]));
    let expected = replies
        .iter()
        .map(|reply| (reply.method.clone(), reply.fields.clone()))
        .collect::<Vec<_>>();
    let mut server = Server::new(replies);
    println!("native collection blank media command: mixed family unreadable={unreadable}");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        execute(commands, &data, &[], &server, None)
    }));
    let requests = server.finish();
    let result = match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    };
    let requests = requests.unwrap();
    assert_eq!(
        result.code,
        if unreadable { 3 } else { 0 },
        "{} {}",
        result.stdout,
        result.stderr
    );
    if unreadable {
        assert!(result.stdout.contains("DONE: 2 uploaded, 1 failed."));
    } else {
        assert!(result.stdout.contains("\nDONE.\n"));
        assert!(!result.stdout.contains("failed"));
    }
    assert_eq!(requests.len(), expected.len());
    for (request, (method, fields)) in requests.iter().zip(expected) {
        assert_eq!(request.method, method);
        assert_eq!(request.form, fields);
        assert_ne!(request.method, "sendMessage");
    }
    let state: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(state["in_flight"].is_null());
    assert_eq!(state["sets"].as_array().unwrap().len(), 1);
    assert_eq!(state["sets"][0]["fmt"], "mixed");
    assert_eq!(state["sets"][0]["keys"], json!(keys[..2]));
    assert_eq!(state["sets"][0]["live"], 2);
    assert_eq!(state["skipped"], json!(blank_keys));
    assert_eq!(state["sent"], json!([]));
    let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
    for (index, key) in keys[..2].iter().enumerate() {
        assert_eq!(
            cat.dispatch(json!({"operation":"custom_id","base":"fixture",
            "content_key":key}))
                .unwrap(),
            (index + 1).to_string()
        );
    }
    for (index, key) in blank_keys.iter().enumerate() {
        assert_eq!(cat.get(key).unwrap(), Some(rows[index + 2].clone()));
        assert_eq!(
            cat.dispatch(json!({"operation":"published","base":"fixture",
            "content_key":key}))
                .unwrap(),
            false
        );
        assert_eq!(
            std::fs::read(&blank_files[index]).unwrap(),
            originals[index]
        );
    }
    if unreadable {
        assert_eq!(cat.get(corrupt_key).unwrap(), rows.last().cloned());
        assert_eq!(
            cat.dispatch(json!({"operation":"published","base":"fixture",
            "content_key":corrupt_key}))
                .unwrap(),
            false
        );
        assert_eq!(std::fs::read(&corrupt).unwrap(), corrupt_bytes);
        assert!(
            !state["skipped"]
                .as_array()
                .unwrap()
                .contains(&json!(corrupt_key))
        );
    }
    drop(cat);
    assert_eq!(
        std::fs::read_dir(data.join("tmp/native-codec"))
            .unwrap()
            .count(),
        0
    );
    let lease =
        _native::locks::Lease::acquire(&data.join("publish_fixture.lock"), "after blank admission")
            .unwrap();
    drop(lease);
    assert!(!_native::process_owner::cleanup_pending(commands.root));
}
