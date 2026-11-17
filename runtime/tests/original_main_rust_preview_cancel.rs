// Original-main Rust test; never counted toward1516 Python IDs.
// Source native/tests/preview_cancel.rs @9c3cbb43305b; SHA256 03af5cb2e44ca41e226d4ebacf6d1489424083caeb8c5408b72661d046747459
extern crate numera_runtime as _native;
#[path = "original_main_rust_support/mod.rs"]
mod support;
// Canceling warm-up reaps an active codec before its cache staging is removed.
use _native::{media_adapter::MediaAdapter, panel_preview::Previews};
use serde_json::json;
use std::time::Duration;

#[test]
fn cancelled_warmup_does_not_leave_codec_or_cache_staging() {
    let scratch = support::Scratch::new("preview-cancel");
    let data = scratch.0.clone();
    let executable = scratch.worker("blocking-codec");
    let source = data.join("art.tgs");
    std::fs::write(
        &source,
        b"blocking process fixture, not codec decode evidence",
    )
    .unwrap();
    let previews = Previews::new(
        MediaAdapter {
            executable,
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
    if reached {
        support::terminated(
            std::fs::read_to_string(data.join("ready"))
                .unwrap()
                .parse()
                .unwrap(),
        );
    }
    assert!(reached, "warm-up never reached active codec cancellation");
    assert_eq!(children, 0, "warm-up codec ownership was not cleaned");
    assert_eq!(staged, 0, "warm-up left a cache staging file");
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
