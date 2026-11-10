//! Actual conversion grouping/cache/output ownership with native IPC and independent PNG bytes.
use _native::{
    config::Config, convert_cli, locks::Lease, logging::RunLog,
    media_native_adapter::NativeMediaAdapter,
};
use pyo3::prelude::*;
use serde_json::Value;
#[path = "support/retained_cli.rs"]
mod retained_cli;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs::{File, FileTimes};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

#[path = "support/bounded_media.rs"]
mod bounded_media;
#[path = "support/conversion_source_cases.rs"]
mod conversion_source_cases;
use bounded_media::BoundedCodec;

fn args(input: &Path, output: &Path, extra: &[&str]) -> Vec<String> {
    let mut args = vec![
        "--in".into(),
        input.to_str().unwrap().into(),
        "--out".into(),
        output.to_str().unwrap().into(),
    ];
    args.extend(extra.iter().map(|value| (*value).into()));
    args
}
fn pngs(output: &Path) -> Vec<String> {
    let mut names = std::fs::read_dir(output)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "png"))
        .map(|path| path.file_name().unwrap().to_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}
fn modified(path: &Path, seconds: u64) {
    File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(
            FileTimes::new().set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)),
        )
        .unwrap();
}
fn rgb_png(path: &Path) {
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let image = py
            .import("PIL.Image")?
            .call_method1("new", ("RGB", (100, 100), (12, 34, 56)))?;
        let saved = image.call_method1("save", (path.to_path_buf(), "PNG"));
        let closed = image.call_method0("close");
        saved?;
        closed?;
        Ok(())
    })
    .unwrap();
}
fn empty_codecs(output: &Path) {
    assert_eq!(
        std::fs::read_dir(output.join(".native-codec"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn native_conversion_repairs_reuses_falls_back_and_preserves_refused_outputs() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = _native::tooling_environment::physical_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
    )
    .unwrap();
    let parent = _native::tooling_environment::confined_directory(&root, "logs/tmp", true).unwrap();
    let scratch = parent.join(format!(
        "native conversion consumers {}",
        std::process::id()
    ));
    std::fs::create_dir(&scratch).unwrap();
    let scratch = _native::tooling_environment::physical_root(&scratch).unwrap();
    let config = Config::load_with_environment(
        scratch.clone(),
        &BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]),
    )
    .unwrap();
    let (original, provider) = _native::source_static_oracle::fixtures().unwrap();
    let converted: Value = std::env::var_os("NUMERA_STATIC_CONVERT_ORACLE")
        .map(PathBuf::from)
        .map(|path| serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap())
        .unwrap_or(original);
    _native::source_static_oracle::fixture_identity(&converted, &provider).unwrap();
    let case = converted["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "alpha-edge")
        .unwrap();
    let original_source = root.join(case["input"].as_str().unwrap());
    assert_eq!(
        _native::tooling_manifest::digest(&original_source).unwrap(),
        case["input_sha256"]
    );
    let original_bytes = std::fs::read(&original_source).unwrap();
    let log = RunLog::open(&scratch, "native-conversion-test", vec![]);
    let log_path = log.path.clone().expect("fixture file log required");
    let deadline = Instant::now() + Duration::from_secs(60);
    let calls = Cell::new(0);
    let make_media = |scratch| {
        calls.set(calls.get() + 1);
        BoundedCodec {
            native: NativeMediaAdapter {
                executable: PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji")),
                root: root.clone(),
                scratch,
            },
            deadline,
        }
    };
    let input = scratch.join("input with spaces");
    let output = scratch.join("output café");
    std::fs::create_dir(&input).unwrap();
    for name in ["art.png", "other.png"] {
        std::fs::write(input.join(name), &original_bytes).unwrap();
        modified(&input.join(name), 1_700_000_000);
    }
    std::fs::write(input.join("art.svg"), b"<svg broken").unwrap();
    modified(&input.join("art.svg"), 1_700_000_000);
    assert_eq!(
        convert_cli::run_with_codec(
            &config,
            &args(&input, &output, &["--limit", "-1"]),
            &log,
            make_media
        )
        .unwrap_err(),
        "--limit must be 0 or greater"
    );
    assert!(!output.exists());
    assert_eq!(calls.get(), 0);
    assert_eq!(
        convert_cli::run_with_codec(
            &config,
            &args(&input, &output, &["--limit", "1"]),
            &log,
            make_media
        )
        .unwrap(),
        0
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(pngs(&output), ["art.png"]);
    let target = output.join("art.png");
    let expected = std::fs::read(&target).unwrap();
    assert_eq!(expected.len() as u64, case["expected"]["bytes"]);
    assert_eq!(
        _native::tooling_manifest::digest(&target).unwrap(),
        case["expected"]["png_sha256"]
    );
    let metadata = _native::codec_static_identity::metadata(&target, true).unwrap();
    assert_eq!(metadata["size"], serde_json::json!([100, 100]));
    assert_eq!(metadata["mode"], "RGBA");
    assert_eq!(metadata["blank"], false);
    assert_eq!(
        std::fs::read(input.join("art.png")).unwrap(),
        original_bytes
    );
    empty_codecs(&output);

    let occupied = output.join("other.png");
    std::fs::create_dir(&occupied).unwrap();
    let keep = occupied.join("keep.bin");
    std::fs::write(&keep, b"previous durable output").unwrap();
    assert!(
        convert_cli::run_with_codec(&config, &args(&input, &output, &[]), &log, make_media)
            .is_err()
    );
    assert_eq!(std::fs::read(&keep).unwrap(), b"previous durable output");
    assert_eq!(std::fs::read(&target).unwrap(), expected);
    assert_eq!(
        std::fs::read(input.join("other.png")).unwrap(),
        original_bytes
    );
    assert_eq!(std::fs::read(output.join(".svg_cur")).unwrap(), b"other");
    empty_codecs(&output);

    let repair_input = scratch.join("repair input");
    let repair_output = scratch.join("repair output");
    std::fs::create_dir(&repair_input).unwrap();
    std::fs::create_dir(&repair_output).unwrap();
    let source = repair_input.join("art.png");
    let repaired = repair_output.join("art.png");
    std::fs::write(&source, &original_bytes).unwrap();
    modified(&source, 1_700_000_000);
    rgb_png(&repaired);
    modified(&repaired, 1_700_000_060);
    assert_eq!(
        _native::codec_static_identity::metadata(&repaired, true).unwrap()["mode"],
        "RGB"
    );
    assert_eq!(
        convert_cli::run_with_codec(
            &config,
            &args(&repair_input, &repair_output, &[]),
            &log,
            make_media
        )
        .unwrap(),
        0
    );
    assert_eq!(std::fs::read(&repaired).unwrap(), expected);
    let timestamp = repaired.metadata().unwrap().modified().unwrap();
    assert_eq!(
        convert_cli::run_with_codec(
            &config,
            &args(&repair_input, &repair_output, &[]),
            &log,
            make_media
        )
        .unwrap(),
        0
    );
    assert_eq!(std::fs::read(&repaired).unwrap(), expected);
    assert_eq!(repaired.metadata().unwrap().modified().unwrap(), timestamp);
    assert!(!repair_output.join(".svg_cur").exists());
    empty_codecs(&repair_output);

    let marker = repair_output.join(".svg_cur");
    std::fs::write(&marker, b"previous attempt").unwrap();
    let lease = Lease::acquire(&repair_output.join(".convert.lock"), "fixture UTC").unwrap();
    let before_calls = calls.get();
    let error = convert_cli::run_with_codec(
        &config,
        &args(&repair_input, &repair_output, &[]),
        &log,
        make_media,
    )
    .unwrap_err();
    assert!(error.starts_with("conversion output is already owned:"));
    assert_eq!(calls.get(), before_calls);
    assert_eq!(std::fs::read(&marker).unwrap(), b"previous attempt");
    assert_eq!(std::fs::read(&repaired).unwrap(), expected);
    assert_eq!(std::fs::read(&source).unwrap(), original_bytes);
    assert!(!repair_output.join(".svg_skip.txt").exists());
    drop(lease);

    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::remove_file(&marker).unwrap();
        rgb_png(&repaired);
        let invalid_rgb = std::fs::read(&repaired).unwrap();
        modified(&repaired, 1_700_000_060);
        let file = File::options()
            .read(true)
            .share_mode(1 | 2)
            .open(&repaired)
            .unwrap();
        assert!(
            convert_cli::run_with_codec(
                &config,
                &args(&repair_input, &repair_output, &[]),
                &log,
                make_media,
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&repaired).unwrap(), invalid_rgb);
        assert_eq!(std::fs::read(&source).unwrap(), original_bytes);
        assert_eq!(std::fs::read(&marker).unwrap(), b"art");
        empty_codecs(&repair_output);
        drop(file);
    }

    let logos = scratch.join("coins/logos");
    std::fs::create_dir_all(logos.join("svg")).unwrap();
    std::fs::create_dir(logos.join("png")).unwrap();
    for name in ["art", "quarantined"] {
        std::fs::write(logos.join(format!("svg/{name}.svg")), b"<svg broken").unwrap();
        std::fs::write(logos.join(format!("png/{name}.png")), &original_bytes).unwrap();
        modified(&logos.join(format!("png/{name}.png")), 1_700_000_000);
    }
    std::fs::write(logos.join(".svg_cur"), b"quarantined").unwrap();
    assert_eq!(
        convert_cli::run_with_codec(&config, &[], &log, make_media).unwrap(),
        0
    );
    let legacy_output = logos.join("emoji");
    assert_eq!(pngs(&legacy_output), ["art.png", "quarantined.png"]);
    for name in ["art", "quarantined"] {
        assert_eq!(
            std::fs::read(legacy_output.join(format!("{name}.png"))).unwrap(),
            expected
        );
        assert_eq!(
            std::fs::read(logos.join(format!("png/{name}.png"))).unwrap(),
            original_bytes
        );
        assert_eq!(
            std::fs::read(logos.join(format!("svg/{name}.svg"))).unwrap(),
            b"<svg broken"
        );
    }
    assert_eq!(
        std::fs::read(logos.join(".svg_skip.txt")).unwrap(),
        b"quarantined\n"
    );
    assert!(!logos.join(".svg_cur").exists());
    assert!(!legacy_output.join(".svg_skip.txt").exists());
    assert!(!legacy_output.join(".svg_cur").exists());
    empty_codecs(&legacy_output);
    assert_eq!(std::fs::read(original_source).unwrap(), original_bytes);
    conversion_source_cases::check(&scratch, &config, &log, make_media, deadline);
    assert!(
        Instant::now() < deadline,
        "native conversion fixture exceeded60s; evidence retained"
    );
    log.finish(0);
    drop(log);
    assert!(
        std::fs::read_to_string(&log_path)
            .unwrap()
            .contains("exit=0")
    );
    assert!(!_native::process_owner::cleanup_pending(&scratch));
    // All child envelopes were checked above; unresolved assertions retain this whole fixture.
    std::fs::remove_dir_all(&scratch).expect("clean verified native conversion fixture");
}
