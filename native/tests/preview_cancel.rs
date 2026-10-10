//! Canceling warm-up reaps an active codec before its cache staging is removed.
use _native::{media_adapter::MediaAdapter, panel_preview::Previews};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn cancelled_warmup_does_not_leave_codec_or_cache_staging() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let data = project
        .join("logs")
        .join(format!("preview-cancel-{}", std::process::id()));
    std::fs::create_dir_all(data.join("emojikit")).unwrap();
    std::fs::write(data.join("emojikit/__init__.py"), b"").unwrap();
    std::fs::write(
        data.join("emojikit/media.py"),
        b"NATIVE_CODEC_GROUP_VERSION = 1\n",
    )
    .unwrap();
    std::fs::write(data.join("emojikit/media_bridge.py"),
        b"import pathlib,threading\npathlib.Path('ready').write_text('ready',encoding='utf-8')\nthreading.Event().wait()\n").unwrap();
    let source = data.join("art.tgs");
    std::fs::write(
        &source,
        b"blocking process fixture, not codec decode evidence",
    )
    .unwrap();
    let python = std::env::var_os("PYO3_PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            project.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        });
    let previews = Previews::new(
        MediaAdapter {
            python,
            root: data.clone(),
            scratch: data.join("codec"),
        },
        &data,
        1,
    );
    let view = json!({"view":[{"key":"a:fixture","fmt":"animated"}],"by_key":{"a:fixture":source}});
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let reached = runtime.block_on(async {
        let warming = previews.warm(&view, 15);
        tokio::pin!(warming);
        let ready = async {
            while !data.join("ready").is_file() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        tokio::select! {
            _ = tokio::time::timeout(Duration::from_secs(5), ready) => data.join("ready").is_file(),
            _ = &mut warming => false,
        }
    });
    drop(runtime);
    let children = std::fs::read_dir(data.join("codec"))
        .map(|entries| entries.count())
        .unwrap_or(0);
    let staged = std::fs::read_dir(data.join("preview"))
        .map(|entries| entries.count())
        .unwrap_or(0);
    std::fs::remove_dir_all(data).unwrap();
    assert!(reached, "warm-up never reached active codec cancellation");
    assert_eq!(children, 0, "warm-up codec ownership was not cleaned");
    assert_eq!(staged, 0, "warm-up left a cache staging file");
}
