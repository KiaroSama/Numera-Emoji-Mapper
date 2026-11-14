//! Original creator recipes at the public native conversion seam, sharing its artifact and deadline.
use super::bounded_media::BoundedCodec;
use _native::{config::Config, convert_cli, logging::RunLog, media_adapter::MediaCodec};
use pyo3::prelude::*;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const OLD: u64 = 1_700_000_000;
const RED: (u8, u8, u8, u8) = (240, 20, 20, 255);
const BLUE: (u8, u8, u8, u8) = (20, 20, 240, 255);
const CLEAR: (u8, u8, u8, u8) = (0, 0, 0, 0);
use crate::retained_cli;

const EMPTY_SVG: &str =
    "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"64\"></svg>";
const BLUE_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"64\"><rect width=\"64\" height=\"64\" fill=\"#0000ff\"/></svg>";

fn image(path: &Path, size: (u32, u32), color: (u8, u8, u8, u8), jpeg: bool) {
    Python::attach(|py| -> PyResult<()> {
        let module = py.import("PIL.Image")?;
        assert_eq!(
            module.getattr("__version__")?.extract::<String>()?,
            "12.3.0"
        );
        let image = if jpeg {
            module.call_method1("new", ("RGB", size, (color.0, color.1, color.2)))?
        } else {
            module.call_method1("new", ("RGBA", size, color))?
        };
        let saved = image.call_method1("save", (path,));
        let closed = image.call_method0("close");
        saved?;
        closed?;
        Ok(())
    })
    .unwrap();
}

fn center(path: &Path) -> (u8, u8, u8, u8) {
    Python::attach(|py| -> PyResult<_> {
        let image = py.import("PIL.Image")?.call_method1("open", (path,))?;
        let inspected = image
            .call_method1("getpixel", ((50, 50),))
            .and_then(|pixel| pixel.extract::<(u8, u8, u8, u8)>());
        let closed = image.call_method0("close");
        let pixel = inspected?;
        closed?;
        Ok(pixel)
    })
    .unwrap()
}

struct Observed<'a> {
    inner: BoundedCodec,
    marker: &'a Path,
    seen: &'a Mutex<Vec<String>>,
}
impl MediaCodec for Observed<'_> {
    fn with_scratch(&self, scratch: PathBuf) -> Self {
        Self {
            inner: self.inner.with_scratch(scratch),
            marker: self.marker,
            seen: self.seen,
        }
    }
    async fn call(&self, request: Value) -> Result<Value, String> {
        if request["operation"] == "convert" {
            let name = std::fs::read_to_string(self.marker).map_err(|e| e.to_string())?;
            self.seen
                .lock()
                .map_err(|_| "conversion observation poisoned")?
                .push(name);
        }
        self.inner.call(request).await
    }
}

pub fn check(
    root: &Path,
    config: &Config,
    log: &RunLog,
    make_media: impl Fn(PathBuf) -> BoundedCodec,
    deadline: std::time::Instant,
) {
    Python::initialize();
    let parent = root.join("original creator cases");
    std::fs::create_dir(&parent).unwrap();
    for (name, size, color, accepted) in [
        ("blank-helper", (50, 50), CLEAR, false),
        ("solid-helper", (40, 70), (200, 30, 30, 255), true),
        ("blank-metadata", (100, 100), CLEAR, false),
        ("opaque-metadata", (100, 100), (10, 120, 200, 255), true),
    ] {
        let source = parent.join(format!("{name}.png"));
        image(&source, size, color, false);
        let before = std::fs::read(&source).unwrap();
        if name.ends_with("metadata") {
            assert_eq!(
                _native::codec_static_identity::metadata(&source, true).unwrap()["blank"],
                !accepted
            );
        } else {
            let output = parent.join(format!("{name}-out.png"));
            let result = _native::codec_image::to_static_png_typed(&source, &output);
            if accepted {
                assert_eq!(result.unwrap()["size"], serde_json::json!([100, 100]));
                assert_eq!(
                    _native::codec_static_identity::metadata(&output, true).unwrap()["size"],
                    serde_json::json!([100, 100])
                );
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.kind, "MediaError");
                assert!(error.handled);
                assert!(!output.exists());
            }
        }
        assert_eq!(std::fs::read(&source).unwrap(), before);
    }
    for case in [
        "reuse",
        "newer",
        "corrupt",
        "wrong-size",
        "blank",
        "priority",
        "blank-fallback",
        "blank-sources",
        "all-failed",
        "partial",
        "quarantine",
    ] {
        let directory = parent.join(case);
        let input = directory.join("in");
        let output = directory.join("out");
        std::fs::create_dir_all(&input).unwrap();
        let marker = output.join(".svg_cur");
        let seen = Mutex::new(Vec::new());
        let run = || {
            convert_cli::run_with_codec(
                config,
                &super::args(&input, &output, &[]),
                log,
                |scratch| Observed {
                    inner: make_media(scratch),
                    marker: &marker,
                    seen: &seen,
                },
            )
        };
        if case == "priority" {
            image(&input.join("foo.png"), (40, 70), RED, false);
            image(&input.join("foo.jpg"), (40, 70), RED, true);
            std::fs::write(input.join("foo.svg"), BLUE_SVG.as_bytes()).unwrap();
        } else if matches!(case, "blank-fallback" | "blank-sources") {
            image(
                &input.join("foo.png"),
                (40, 70),
                if case == "blank-fallback" { RED } else { CLEAR },
                false,
            );
            std::fs::write(input.join("foo.svg"), EMPTY_SVG.as_bytes()).unwrap();
        } else {
            let name = if case == "partial" {
                "good.png"
            } else {
                "a.png"
            };
            image(
                &input.join(name),
                (40, 70),
                if case == "all-failed" { CLEAR } else { RED },
                false,
            );
            if matches!(case, "all-failed" | "partial") {
                image(&input.join("bad.png"), (40, 70), CLEAR, false);
            }
        }
        let initial = std::fs::read_dir(&input)
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                (path.clone(), std::fs::read(path).unwrap())
            })
            .collect::<Vec<_>>();
        let target = output.join("a.png");
        if matches!(case, "corrupt" | "wrong-size" | "blank" | "quarantine") {
            std::fs::create_dir(&output).unwrap();
            match case {
                "corrupt" => std::fs::write(&target, b"not a png at all").unwrap(),
                "wrong-size" => image(&target, (64, 64), BLUE, false),
                "blank" => image(&target, (100, 100), CLEAR, false),
                "quarantine" => std::fs::write(&marker, b"a").unwrap(),
                _ => unreachable!(),
            }
        }
        assert_eq!(
            run().unwrap(),
            match case {
                "all-failed" | "blank-sources" => 4,
                "partial" => 3,
                _ => 0,
            },
            "{case}"
        );
        if case == "quarantine" {
            assert!(!target.exists());
            assert_eq!(
                std::fs::read_to_string(output.join(".svg_skip.txt")).unwrap(),
                "a\n"
            );
            assert!(seen.lock().unwrap().is_empty());
        } else if matches!(case, "all-failed" | "blank-sources") {
            assert!(super::pngs(&output).is_empty());
            if case == "blank-sources" {
                assert_eq!(*seen.lock().unwrap(), ["foo", "foo"]);
            }
        } else {
            let name = match case {
                "priority" | "blank-fallback" => "foo.png",
                "partial" => "good.png",
                _ => "a.png",
            };
            assert_eq!(super::pngs(&output), [name]);
            let info = _native::codec_static_identity::metadata(&output.join(name), true).unwrap();
            assert_eq!(info["size"], serde_json::json!([100, 100]), "{case}");
            assert_eq!(info["mode"], "RGBA", "{case}");
            assert_eq!(info["blank"], false, "{case}");
            let pixel = center(&output.join(name));
            if case == "priority" {
                assert!(pixel.2 > pixel.0);
            } else {
                assert!(pixel.0 > pixel.2);
            }
        }
        assert!(!marker.exists(), "{case} left its heartbeat");
        if case == "priority" {
            assert_eq!(*seen.lock().unwrap(), ["foo"]);
        } else if case == "blank-fallback" {
            assert_eq!(*seen.lock().unwrap(), ["foo", "foo"]);
        } else if case == "reuse" || case == "newer" {
            assert_eq!(*seen.lock().unwrap(), ["a"]);
            let source = input.join("a.png");
            let before = std::fs::read(&target).unwrap();
            super::modified(&target, OLD);
            if case == "reuse" {
                super::modified(&source, OLD - 60);
                assert_eq!(run().unwrap(), 0);
                assert_eq!(
                    target.metadata().unwrap().modified().unwrap(),
                    std::time::UNIX_EPOCH + std::time::Duration::from_secs(OLD)
                );
                assert_eq!(std::fs::read(&target).unwrap(), before);
                assert_eq!(*seen.lock().unwrap(), ["a"]);
            } else {
                image(&source, (40, 70), BLUE, false);
                super::modified(&source, OLD + 60);
                let edited = std::fs::read(&source).unwrap();
                assert_eq!(run().unwrap(), 0);
                let pixel = center(&target);
                assert!(pixel.2 > pixel.0);
                assert_ne!(std::fs::read(&target).unwrap(), before);
                assert_eq!(std::fs::read(&source).unwrap(), edited);
                assert_eq!(*seen.lock().unwrap(), ["a", "a"]);
            }
        }
        for (path, bytes) in initial {
            if case != "newer" {
                assert_eq!(std::fs::read(path).unwrap(), bytes, "{case}");
            }
        }
        assert!(!marker.exists());
        if case == "quarantine" {
            assert!(!output.join(".native-codec").exists());
        } else {
            super::empty_codecs(&output);
        }
        assert!(!_native::process_owner::cleanup_pending(&directory));
    }
    keyword_and_quarantine_commands(root, &parent, log, deadline);
}

fn keyword_and_quarantine_commands(
    root: &Path,
    parent: &Path,
    log: &RunLog,
    deadline: std::time::Instant,
) {
    use _native::tooling_process;
    use std::time::Duration;
    let command_root = parent.join("command root");
    std::fs::create_dir(&command_root).unwrap();
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let (executable, mut environment) = retained_cli::install(project, &command_root);
    let coin = command_root.join("coins/logos/emoji");
    let corpus = command_root.join("corpus");
    let other = command_root.join("other");
    for directory in [&coin, &corpus, &other] {
        std::fs::create_dir_all(directory).unwrap();
        image(&directory.join("a.png"), (100, 100), RED, false);
    }
    let csv = command_root.join("coins/keywords.csv");
    let csv_bytes = b"ticker,name,format,file,keywords\na,Alpha,png,logos/png/a.png,\"a, Alpha\"\n";
    std::fs::write(&csv, csv_bytes).unwrap();
    environment.insert("FAKE_TOKEN".into(), "fixture".into());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for (source, configured, keywords) in [
        (&coin, "", 1),
        (&corpus, corpus.to_str().unwrap(), 1),
        (&other, "", 0),
    ] {
        environment.insert("COIN_EMOJI_DIR".into(), configured.into());
        let before = std::fs::read(source.join("a.png")).unwrap();
        let argv = vec![
            executable.as_os_str().to_owned(),
            "build-pack".into(),
            "--base".into(),
            "cryptoemoji".into(),
            "--title".into(),
            "Fixture".into(),
            "--user-id".into(),
            "111111111".into(),
            "--source-dir".into(),
            source.as_os_str().to_owned(),
            "--token-env".into(),
            "FAKE_TOKEN".into(),
            "--dry-run".into(),
        ];
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "conversion deadline exhausted before dry-run"
        );
        let result = runtime
            .block_on(tooling_process::capture(
                &command_root,
                &argv,
                &environment,
                remaining.min(Duration::from_secs(10)),
                remaining.min(Duration::from_secs(5)),
                log,
            ))
            .unwrap();
        assert_eq!(result.code, 0, "{} {}", result.stdout, result.stderr);
        assert!(
            result.stdout.contains(&format!("keywords={keywords}")),
            "{}",
            result.stdout
        );
        assert_eq!(std::fs::read(source.join("a.png")).unwrap(), before);
        assert!(!command_root.join("state_cryptoemoji.json").exists());
    }
    let invalid_output = command_root.join("invalid output");
    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
    assert!(!remaining.is_zero());
    let refused = runtime
        .block_on(tooling_process::capture(
            &command_root,
            &[
                executable.as_os_str().to_owned(),
                "make-emoji-pngs".into(),
                "--in".into(),
                other.as_os_str().to_owned(),
                "--out".into(),
                invalid_output.as_os_str().to_owned(),
                "--limit".into(),
                "-1".into(),
            ],
            &environment,
            remaining.min(Duration::from_secs(10)),
            remaining.min(Duration::from_secs(5)),
            log,
        ))
        .unwrap();
    assert_eq!(refused.code, 2, "{} {}", refused.stdout, refused.stderr);
    assert!(refused.stderr.contains("--limit"));
    assert!(refused.stdout.is_empty());
    assert!(!invalid_output.exists());
    for (case, expected, made, failed) in [
        ("fallback", 0, 1, 0),
        ("empty", 4, 0, 1),
        ("partial", 3, 1, 1),
    ] {
        let directory = command_root.join(format!("legacy {case}"));
        std::fs::create_dir(&directory).unwrap();
        let (relocated, environment) = retained_cli::install(project, &directory);
        let logos = directory.join("coins/logos");
        std::fs::create_dir_all(logos.join("svg")).unwrap();
        std::fs::create_dir(logos.join("png")).unwrap();
        let stem = if case == "fallback" { "btc" } else { "bad" };
        std::fs::write(logos.join(format!("svg/{stem}.svg")), EMPTY_SVG.as_bytes()).unwrap();
        if case != "empty" {
            image(
                &logos.join(format!("png/{stem}.png")),
                (40, 70),
                if case == "fallback" { RED } else { CLEAR },
                false,
            );
        }
        if case == "partial" {
            image(&logos.join("png/good.png"), (40, 70), RED, false);
        }
        // Each legacy root uses the same built artifact and retained codec closure, not another build.
        let originals = [
            logos.join(format!("svg/{stem}.svg")),
            logos.join(format!("png/{stem}.png")),
            logos.join("png/good.png"),
        ]
        .into_iter()
        .filter(|path| path.exists())
        .map(|path| (path.clone(), std::fs::read(path).unwrap()))
        .collect::<Vec<_>>();
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(!remaining.is_zero());
        let result = runtime
            .block_on(tooling_process::capture(
                &directory,
                &[relocated.into_os_string(), "make-emoji-pngs".into()],
                &environment,
                remaining.min(Duration::from_secs(10)),
                remaining.min(Duration::from_secs(5)),
                log,
            ))
            .unwrap();
        assert_eq!(
            result.code, expected,
            "{case}: {} {}",
            result.stdout, result.stderr
        );
        assert_eq!(
            result.stdout,
            format!(
                "DONE: made {made} this run (svg=0, png={made}, failed={failed}); total emoji PNGs: {made}.\n"
            )
        );
        let output = logos.join("emoji");
        assert_eq!(
            super::pngs(&output),
            match case {
                "fallback" => vec!["btc.png"],
                "partial" => vec!["good.png"],
                _ => vec![],
            }
        );
        if made > 0 {
            let file = output.join(if case == "fallback" {
                "btc.png"
            } else {
                "good.png"
            });
            let info = _native::codec_static_identity::metadata(&file, true).unwrap();
            assert_eq!(info["size"], serde_json::json!([100, 100]));
            assert_eq!(info["blank"], false);
            let pixel = center(&file);
            assert!(pixel.0 > pixel.2);
        }
        for (path, bytes) in originals {
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
        assert!(!output.join(".native-codec").exists());
        let codec_storage = directory.join("logs/tmp/native-convert");
        assert!(codec_storage.is_dir());
        assert_eq!(std::fs::read_dir(&codec_storage).unwrap().count(), 0);
        assert!(!logos.join(".svg_cur").exists());
        assert!(
            std::fs::read_dir(directory.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none()
        );
        assert!(!_native::process_owner::cleanup_pending(&directory));
    }
    let input = parent.join("quarantine/in");
    let output = parent.join("quarantine/out");
    let argv = vec![
        executable.into_os_string(),
        "make-emoji-pngs".into(),
        "--in".into(),
        input.into_os_string(),
        "--out".into(),
        output.clone().into_os_string(),
    ];
    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
    assert!(!remaining.is_zero());
    let result = runtime
        .block_on(tooling_process::capture(
            &command_root,
            &argv,
            &environment,
            remaining.min(Duration::from_secs(10)),
            remaining.min(Duration::from_secs(5)),
            log,
        ))
        .unwrap();
    assert_eq!(result.code, 0, "{} {}", result.stdout, result.stderr);
    assert!(result.stdout.contains("REVIEW") && result.stdout.contains("a"));
    assert!(!output.join("a.png").exists());
    assert_eq!(std::fs::read(output.join(".svg_skip.txt")).unwrap(), b"a\n");
    assert_eq!(std::fs::read(csv).unwrap(), csv_bytes);
    assert!(!output.join(".native-codec").exists());
    let codec_storage = command_root.join("logs/tmp/native-convert");
    assert!(codec_storage.is_dir());
    assert_eq!(std::fs::read_dir(&codec_storage).unwrap().count(), 0);
    assert_eq!(
        std::fs::read_dir(command_root.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    assert!(!_native::process_owner::cleanup_pending(root));
    runtime.shutdown_timeout(Duration::from_secs(1));
}
