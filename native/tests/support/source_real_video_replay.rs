//! Local-by-design original migration assertion; no fixture means no real-data acceptance.
use _native::source_git_inventory::Source;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn check(project: &Path, runtime: &tokio::runtime::Runtime, sources: &[Source]) {
    let Some(selected) = std::env::var_os("NUMERA_REAL_VIDEO_FIXTURE") else {
        eprintln!(
            "local-by-design: NUMERA_REAL_VIDEO_FIXTURE not selected; original real-video assertion not run, no acceptance claim"
        );
        return;
    };
    let selected = _native::source_import_replay::real_video_fixture(project, Path::new(&selected))
        .expect("invalid real-video fixture; private path omitted");
    let before =
        _native::tooling_manifest::digest(&selected).expect("cannot hash real-video fixture");
    let parent =
        _native::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
    let directory = parent.join(format!(
        "original import replay real video {}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).unwrap();
    _native::source_import_replay::stage(project, &directory, sources).unwrap();
    let binary_name = if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    };
    let extension = if cfg!(windows) {
        "_native.dll"
    } else {
        "lib_native.so"
    };
    let mut files = BTreeMap::new();
    for (input, relative) in [
        (
            PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji")),
            format!("native/target/debug/{binary_name}"),
        ),
        (
            project.join("native/target/debug").join(extension),
            format!(
                "emojikit/{}",
                if cfg!(windows) {
                    "_native.pyd"
                } else {
                    "_native.so"
                }
            ),
        ),
        (project.join("pyproject.toml"), "pyproject.toml".into()),
    ] {
        _native::tooling_environment::reject_links(
            &input,
            "real-video runtime artifact cannot follow links",
        )
        .unwrap();
        let metadata =
            std::fs::symlink_metadata(&input).expect("real-video runtime artifact required");
        assert!(metadata.is_file() && metadata.len() <= 256 * 1024 * 1024);
        let hash = _native::tooling_manifest::digest(&input).unwrap();
        if relative == "pyproject.toml" {
            assert_eq!(
                hash,
                "b3474a001d7ea21ff0fb309fe0cfa5222e37f5e062b0137ed5e6ce22dbb995d5"
            );
        }
        let output = directory.join(&relative);
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        assert!(!output.exists());
        std::fs::copy(&input, &output).unwrap();
        assert_eq!(_native::tooling_manifest::digest(&output).unwrap(), hash);
        assert!(files.insert(relative, hash).is_none());
    }
    let actual = runtime.block_on(_native::source_import_replay::observe_real_video(
        project, &directory, &selected, &files,
    )).expect("original real-video assertion failed; staged diagnostics retained, private details omitted");
    assert_eq!(actual["tests_run"], 1);
    assert_eq!(actual["skipped"], 0);
    assert_eq!(actual["source_runtime"], "CPython3.11.9");
    assert!(
        _native::tooling_manifest::digest(&selected).unwrap() == before,
        "original real-video input changed; private digest omitted"
    );
    // The original method and outer descendant owner have released every copied input/child.
    std::fs::remove_dir_all(directory).unwrap();
}
