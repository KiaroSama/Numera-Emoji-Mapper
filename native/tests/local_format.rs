//! Local target selection belongs to Rust; codec metadata is requested only for GIF motion.
use _native::{local_cli, media_adapter::MediaAdapter};
use std::path::PathBuf;

#[path = "support/bounded_media.rs"]
mod bounded_media;

#[test]
fn source_extension_and_magic_rules_choose_without_starting_a_codec() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let data = project
        .join("logs")
        .join(format!("local-format-{}", std::process::id()));
    std::fs::create_dir(&data).unwrap();
    let media = MediaAdapter {
        python: "must-not-start".into(),
        root: project,
        scratch: data.join("codec"),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for (name, bytes, expected) in [
            ("shape.JSON", &b"{}"[..], "animated"),
            ("clip.apng", &b"unused"[..], "video"),
            ("art.svg", &b"unused"[..], "static"),
            ("unknown.bin", &b"\x1a\x45\xdf\xa3"[..], "video"),
            ("vector.bin", &b"\x1f\x8b"[..], "animated"),
            ("opaque.bin", &b"unrecognized"[..], "static"),
        ] {
            let path = data.join(name);
            std::fs::write(&path, bytes).unwrap();
            assert_eq!(
                runtime
                    .block_on(local_cli::choose_format(&media, &path, "auto"))
                    .unwrap(),
                expected
            );
            assert_eq!(
                runtime
                    .block_on(local_cli::choose_format(&media, &path, "static"))
                    .unwrap(),
                "static"
            );
        }
        assert!(!data.join("codec").exists());
    }));
    drop(runtime);
    std::fs::remove_dir_all(data).unwrap();
    result.unwrap();
}

#[test]
fn original_gif_motion_cases_reach_the_native_codec_and_choose_the_same_format() {
    use _native::media_adapter::MediaCodec;
    use _native::media_native_adapter::NativeMediaAdapter;
    use pyo3::prelude::*;
    use pyo3::types::{PyDict, PyList};
    use serde_json::json;
    use std::time::{Duration, Instant};

    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let project = _native::tooling_environment::physical_root(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap(),
    )
    .unwrap();
    let parent =
        _native::tooling_environment::confined_directory(&project, "logs/tmp", true).unwrap();
    let data = parent.join(format!("native GIF format {}", std::process::id()));
    std::fs::create_dir(&data).unwrap();
    let data = _native::tooling_environment::physical_root(&data).unwrap();
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let images = py.import("PIL.Image")?;
        let first = images.call_method1("new", ("RGBA", (4, 4), "red"))?;
        let second = match images.call_method1("new", ("RGBA", (4, 4), "blue")) {
            Ok(image) => image,
            Err(error) => {
                first.call_method0("close")?;
                return Err(error);
            }
        };
        let saved = (|| -> PyResult<()> {
            first.call_method1("save", (data.join("still.gif"),))?;
            let options = PyDict::new(py);
            options.set_item("save_all", true)?;
            options.set_item("append_images", PyList::new(py, [&second])?)?;
            options.set_item("duration", 100)?;
            first.call_method("save", (data.join("moving.gif"),), Some(&options))?;
            Ok(())
        })();
        let first_closed = first.call_method0("close");
        let second_closed = second.call_method0("close");
        saved?;
        first_closed?;
        second_closed?;
        Ok(())
    })
    .unwrap();
    std::fs::write(data.join("bad.gif"), b"not a GIF").unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let media = bounded_media::BoundedCodec {
        native: NativeMediaAdapter {
            executable: PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji")),
            root: project,
            scratch: data.join("codecs"),
        },
        deadline,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for (name, animated, format) in [
        ("still.gif", false, "static"),
        ("moving.gif", true, "video"),
        ("bad.gif", false, "static"),
    ] {
        let path = data.join(name);
        let before = std::fs::read(&path).unwrap();
        assert_eq!(
            runtime
                .block_on(
                    media.call(
                        json!({"operation":"animation_info","source":path,"format":"static"})
                    )
                )
                .unwrap(),
            json!(animated),
            "{name}"
        );
        assert_eq!(
            runtime
                .block_on(local_cli::choose_format(&media, &path, "auto"))
                .unwrap(),
            format,
            "{name}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read_dir(&media.native.scratch).unwrap().count(), 0);
    }
    let missing = data.join("missing.gif");
    for requested in ["static", "video", "animated"] {
        assert_eq!(
            runtime
                .block_on(local_cli::choose_format(&media, &missing, requested))
                .unwrap(),
            requested
        );
    }
    assert_eq!(
        runtime
            .block_on(local_cli::choose_format(&media, &missing, "unsupported"))
            .unwrap_err(),
        "unsupported target format"
    );
    assert!(!missing.exists());
    assert!(
        Instant::now() < deadline,
        "GIF deadline elapsed; evidence retained"
    );
    assert!(!_native::process_owner::cleanup_pending(
        &media.native.scratch
    ));
    assert_eq!(std::fs::read_dir(&media.native.scratch).unwrap().count(), 0);
    drop(runtime);
    std::fs::remove_dir_all(data).expect("clean verified GIF format fixture");
}
