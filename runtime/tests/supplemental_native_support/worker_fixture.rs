//! Real, owned Rust fixture compilation shared by supplemental media regressions.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};
pub fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .into()
}
pub fn worker() -> PathBuf {
    if let Some(path) = std::env::var_os("NUMERA_MEDIA_FIXTURE_WORKER") {
        let path = PathBuf::from(path);
        numera_runtime::tooling_environment::reject_links(&path, "fixture worker must be physical")
            .unwrap();
        assert!(path.is_absolute() && path.is_file());
        return path;
    }
    static WORKER: OnceLock<PathBuf> = OnceLock::new();
    WORKER
        .get_or_init(|| {
            let root = project();
            let directory = root
                .join("logs/tmp")
                .join(format!("supplemental-codec-worker-{}", std::process::id()));
            std::fs::create_dir(&directory).unwrap();
            let output = directory.join(format!("media-worker{}", std::env::consts::EXE_SUFFIX));
            let source =
                root.join("runtime/tests/supplemental_native_support/fixtures/media_worker.rs");
            let compiler = option_env!("NUMERA_BASELINE_RUSTC").unwrap_or("rustc");
            let args = [
                compiler.into(),
                "--edition=2024".into(),
                "--crate-name=supplemental_media_fixture".into(),
                source.into_os_string(),
                "-o".into(),
                output.clone().into_os_string(),
            ];
            let environment = std::env::vars().collect::<BTreeMap<_, _>>();
            let log = numera_runtime::logging::RunLog::open(&directory, "fixture-compiler", vec![]);
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let captured = runtime
                .block_on(numera_runtime::tooling_process::capture(
                    &directory,
                    &args,
                    &environment,
                    Duration::from_secs(60),
                    Duration::from_secs(30),
                    &log,
                ))
                .unwrap();
            assert_eq!(captured.code, 0, "{}", captured.stderr);
            assert!(output.is_file());
            output
        })
        .clone()
}

pub fn test_environment() -> BTreeMap<String, String> {
    let mut environment = BTreeMap::new();
    for name in [
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "PATHEXT",
        "LANG",
        "LC_ALL",
        "LD_LIBRARY_PATH",
        "NUMERA_RUNTIME_TEST_ADMISSION",
    ] {
        if let Ok(value) = std::env::var(name) {
            environment.insert(name.to_owned(), value);
        }
    }
    environment.insert("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into());
    environment.insert("TELEGRAM_API_BASE".into(), "http://127.0.0.1:9".into());
    environment
}
