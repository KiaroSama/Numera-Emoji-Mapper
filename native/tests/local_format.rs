//! Local target selection belongs to Rust; codec metadata is requested only for GIF motion.
use _native::{local_cli, media_adapter::MediaAdapter};
use std::path::PathBuf;

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
