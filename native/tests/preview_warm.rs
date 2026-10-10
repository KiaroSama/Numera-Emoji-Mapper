//! Warm-up uses the request cache, skips static grid cards and continues after bad media.
use _native::{media_adapter::MediaAdapter, panel_preview::Previews};
use serde_json::json;
use std::path::PathBuf;

#[test]
fn held_static_warms_compact_only_and_reuses_after_source_disappears() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let data = project
        .join("logs")
        .join(format!("preview-warm-{}", std::process::id()));
    std::fs::create_dir(&data).unwrap();
    let source = data.join("art.png");
    std::fs::copy(project.join("assets/numera-emoji-mapper-logo.png"), &source).unwrap();
    let python = std::env::var_os("PYO3_PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            project.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        });
    let animated = project.join("tests/fixtures/lottie/red_circle_512.tgs");
    let previews = Previews::new(
        MediaAdapter {
            python,
            root: project,
            scratch: data.join("codec"),
        },
        &data,
        1,
    );
    let view = json!({"view":[
        {"key":"l:logo","fmt":"animated","isLogo":true},
        {"key":"a:bad","fmt":"animated"},
        {"key":"a:moving","fmt":"animated","included":true},
        {"key":"s:shown","fmt":"static","included":true},
        {"key":"s:held","fmt":"static","included":false}],
        "by_key":{"l:logo":source,"a:bad":data.join("missing.tgs"),"a:moving":animated,"s:shown":source,"s:held":source}});
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let first = runtime.block_on(previews.warm(&view, 30));
        assert_eq!(first, json!({"rendered":5,"cached":0,"failed":4}));
        assert_eq!(std::fs::read_dir(data.join("preview")).unwrap().count(), 5);
        let bytes = runtime
            .block_on(previews.bytes("s:held", &source, 10, true, 72))
            .unwrap();
        assert!(bytes.starts_with(b"RIFF") && bytes[8..12] == *b"WEBP");
        std::fs::remove_file(&source).unwrap();
        assert_eq!(
            runtime.block_on(previews.warm(&view, 30)),
            json!({"rendered":0,"cached":5,"failed":4})
        );
        assert_eq!(
            runtime
                .block_on(previews.bytes("s:held", &source, 10, true, 72))
                .unwrap(),
            bytes
        );
    }));
    drop(runtime);
    std::fs::remove_dir_all(data).unwrap();
    outcome.unwrap();
}
