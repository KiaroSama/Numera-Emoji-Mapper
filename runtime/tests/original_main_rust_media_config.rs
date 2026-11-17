// Original-main Rust test; never counted toward1516 Python IDs.
// Source native/tests/media_config.rs @9c3cbb43305b; SHA256 b89357be72a2ce619c4504258e8953ecb567ee6c575169b37279e938482c246b
extern crate numera_runtime as _native;
#[path = "original_main_rust_support/mod.rs"]
mod support;
// Codec children receive the validated non-secret timeout, never operator credentials.
use _native::media_adapter::MediaAdapter;
use serde_json::json;

#[test]
fn codec_timeout_uses_dotenv_then_environment_and_keeps_hermetic_children() {
    let scratch = support::Scratch::new("codec-config");
    let root = scratch.0.clone();
    std::fs::write(
        root.join(".env"),
        b"EMOJI_FFMPEG_TIMEOUT=1\nGENERAL_BOT_TOKEN=test-only-never-forwarded\n",
    )
    .unwrap();
    let adapter = MediaAdapter {
        executable: scratch.worker("config-observer"),
        root: root.clone(),
        scratch: root.join("codec"),
    };
    let original = [
        "NUMERA_EMOJI_MAPPER_NO_DOTENV",
        "EMOJI_FFMPEG_TIMEOUT",
        "GENERAL_BOT_TOKEN",
    ]
    .map(|key| (key, std::env::var_os(key)));
    // This integration binary has one test; environment changes never share another test thread.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        unsafe {
            std::env::remove_var("NUMERA_EMOJI_MAPPER_NO_DOTENV");
            std::env::remove_var("EMOJI_FFMPEG_TIMEOUT");
            std::env::set_var("GENERAL_BOT_TOKEN", "test-only-parent-token");
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert_eq!(
            runtime.block_on(adapter.call(json!({}))).unwrap(),
            json!({"timeout":"1","dotenv":"1","credential":null})
        );
        unsafe {
            std::env::set_var("EMOJI_FFMPEG_TIMEOUT", "2");
        }
        assert_eq!(
            runtime.block_on(adapter.call(json!({}))).unwrap()["timeout"],
            "2"
        );
        unsafe {
            std::env::set_var("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1");
            std::env::remove_var("EMOJI_FFMPEG_TIMEOUT");
        }
        assert_eq!(
            runtime.block_on(adapter.call(json!({}))).unwrap()["timeout"],
            "300"
        );
        assert_eq!(std::fs::read_dir(root.join("codec")).unwrap().count(), 0);
    }));
    for (key, value) in original {
        unsafe {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
    result.unwrap();
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
