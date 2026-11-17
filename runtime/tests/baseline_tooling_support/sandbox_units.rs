//! Private copy fault at the actual production Session I/O boundary.
#![allow(non_snake_case)]
mod TheCloneSharesNothingWithTheSource {
    // Original: tests.test_panel_sandbox.TheCloneSharesNothingWithTheSource.test_a_failed_preview_copy_does_not_refuse_the_clone
    #[test]
    fn test_a_failed_preview_copy_does_not_refuse_the_clone() {
        use crate::tooling_sandbox::{Session, copy_media};
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("logs/tmp")
            .join(format!("sandbox-preview-fault-{}", std::process::id()));
        std::fs::create_dir_all(root.join("source/media")).unwrap();
        std::fs::create_dir(root.join("source/preview")).unwrap();
        let image = root.join("source/media/a.png");
        std::fs::write(&image, b"PNG-fixture").unwrap();
        std::fs::write(root.join("source/preview/a.webp"), b"RIFF-preview").unwrap();
        let mut cat =
            crate::catalog::Catalog::open(&root.join("source/catalog.db"), "fixture", &root)
                .unwrap();
        assert!(
            cat.insert(
                &serde_json::json!({"content_key":"s:a","fmt":"static","file_path":image}),
                "fixture"
            )
            .unwrap()
        );
        drop(cat);
        let session = Session::create_with(
            &root,
            &root.join("source"),
            &root.join("panel-sandbox-dest"),
            |source, target| {
                if source
                    .parent()
                    .and_then(std::path::Path::file_name)
                    .is_some_and(|name| name == "preview")
                {
                    Err("disk full".into())
                } else {
                    copy_media(source, target)
                }
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(session.count, 1);
        assert!(session.directory.join("catalog.db").is_file());
        assert_eq!(
            session.warnings,
            ["Preview cache not copied completely; previews render on demand"]
        );
        session.finish().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
