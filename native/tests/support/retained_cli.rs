//! Transitional command fixture: the real retained codec closure, never a Python-free package claim.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn install(project: &Path, root: &Path) -> (PathBuf, BTreeMap<String, String>) {
    let project = _native::tooling_environment::physical_root(project).unwrap();
    let root = _native::tooling_environment::physical_root(root).unwrap();
    assert!(root.starts_with(project.join("logs/tmp")));
    assert_ne!(root, project.join("logs/tmp"));
    assert!(std::fs::read_dir(&root).unwrap().next().is_none());
    std::fs::create_dir(root.join("assets")).unwrap();
    std::fs::copy(
        project.join("assets/panel.html"),
        root.join("assets/panel.html"),
    )
    .unwrap();
    std::fs::copy(project.join("pyproject.toml"), root.join("pyproject.toml")).unwrap();
    let executable = root.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let package = root.join("emojikit");
    std::fs::create_dir(&package).unwrap();
    for name in [
        "__init__.py",
        "cli_env.py",
        "errors.py",
        "identity.py",
        "logsetup.py",
        "media.py",
        "media_bridge.py",
        "media_lottie.py",
        "packstate.py",
        "repaint.py",
        "video_decode.py",
    ] {
        std::fs::copy(project.join("emojikit").join(name), package.join(name)).unwrap();
    }
    let extension = project.join("native/target/debug").join(if cfg!(windows) {
        "_native.dll"
    } else if cfg!(target_os = "macos") {
        "lib_native.dylib"
    } else {
        "lib_native.so"
    });
    assert!(extension.is_file(), "CI must build the native extension");
    std::fs::copy(
        extension,
        package.join(if cfg!(windows) {
            "_native.pyd"
        } else {
            "_native.so"
        }),
    )
    .unwrap();
    let inherited = std::env::vars().collect::<BTreeMap<_, _>>();
    let mut environment = _native::tooling_test_runner::scrubbed_environment(&inherited);
    let configured = PathBuf::from(
        environment
            .get("PYO3_PYTHON")
            .expect("CI must select Python"),
    );
    let python = if configured.is_absolute() || configured.components().count() > 1 {
        _native::migration_bundle::resolved(&configured).unwrap()
    } else {
        _native::codec_process::executable(configured.to_str().unwrap().trim_end_matches(".exe"))
            .unwrap()
    };
    environment.insert("PYO3_PYTHON".into(), python.to_str().unwrap().into());
    // Safe-path suppresses CWD imports; admit only this copy, not an installed package.
    environment.insert("PYTHONPATH".into(), root.to_str().unwrap().into());
    environment.insert("PYTHONDONTWRITEBYTECODE".into(), "1".into());
    let environment =
        _native::tooling_environment::environment(&root, &environment, false).unwrap();
    (executable, environment)
}
