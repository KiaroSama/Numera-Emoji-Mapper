//! Codec children receive the validated non-secret timeout, never operator credentials.
use _native::media_adapter::MediaAdapter;
use serde_json::json;
use std::path::PathBuf;

#[test]
fn codec_timeout_uses_dotenv_then_environment_and_keeps_hermetic_children() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let root = project
        .join("logs")
        .join(format!("codec-config-{}", std::process::id()));
    std::fs::create_dir_all(root.join("emojikit")).unwrap();
    std::fs::write(root.join("emojikit/__init__.py"), b"").unwrap();
    std::fs::write(
        root.join("emojikit/media.py"),
        b"NATIVE_CODEC_GROUP_VERSION = 1\n",
    )
    .unwrap();
    std::fs::write(
        root.join(".env"),
        b"EMOJI_FFMPEG_TIMEOUT=1\nGENERAL_BOT_TOKEN=test-only-never-forwarded\n",
    )
    .unwrap();
    std::fs::write(root.join("emojikit/media_bridge.py"), b"import json,os,pathlib,sys\nr={'timeout':os.environ.get('EMOJI_FFMPEG_TIMEOUT'),'dotenv':os.environ.get('NUMERA_EMOJI_MAPPER_NO_DOTENV'),'credential':os.environ.get('GENERAL_BOT_TOKEN')}\npathlib.Path(sys.argv[2]).write_text(json.dumps({'ok':True,'result':r}),encoding='utf-8')\n").unwrap();
    let python = std::env::var_os("PYO3_PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            project.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        });
    let adapter = MediaAdapter {
        python,
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
    std::fs::remove_dir_all(root).unwrap();
    result.unwrap();
}
