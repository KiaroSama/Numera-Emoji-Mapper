//! Whole historical imports use the admitted Git corpus and the existing bounded child owner.
use crate::source_git_inventory::Source;
use crate::{logging::RunLog, tooling_environment, tooling_process};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const ASSETS: &[(&str, &[u8], &str)] = &[
    (
        "assets/panel.html",
        include_bytes!("../../assets/panel.html"),
        "591cd8887e85bdc4fdd0fb85bf477c32153a7527b84d1ce45613c316cfa9a151",
    ),
    (
        "assets/panel-grid.js",
        include_bytes!("../../assets/panel-grid.js"),
        "9d736a6d624e47211ccd6e3a477fcc2a0ace86231e48914d203cc15699d98f58",
    ),
    (
        "assets/panel-motion.js",
        include_bytes!("../../assets/panel-motion.js"),
        "45736ed81a1b393f3bb27f8524b2d411ab3b43d3db0851e732cfae2cc3bc7cac",
    ),
    (
        "assets/panel-save.js",
        include_bytes!("../../assets/panel-save.js"),
        "a768b90ea70d777f1767d0cd94e097ec8c9c19d50563b60fa8aa7613c6c430cd",
    ),
    (
        "assets/panel-drag.js",
        include_bytes!("../../assets/panel-drag.js"),
        "77fbeb4a113a03d664c1a212cc84e81d536176a08968cdd209010a4696c143cd",
    ),
    (
        "assets/panel-actions.js",
        include_bytes!("../../assets/panel-actions.js"),
        "91f016455e68b6cf7af2df9ea37ec75395f6367d624f192bbb016b45855287b8",
    ),
    (
        "assets/panel-holding.js",
        include_bytes!("../../assets/panel-holding.js"),
        "04557214314e2a1b694ccac068a6b635b3ff0020e87a9beb68a6bdedaae4642f",
    ),
    (
        "assets/panel-draft.js",
        include_bytes!("../../assets/panel-draft.js"),
        "8a7fd7f4afdcea1bdc8ecf03f739c5cbb0c90eef9f32c087431a087a8be24b2b",
    ),
    (
        "assets/logo-128.png",
        include_bytes!("../../assets/logo-128.png"),
        "1cbdcacaa254f2361fc8c404a9edbfbb528f19a3f7467987e44c7a2730340d83",
    ),
];
const CASES: [&str; 3] = [
    "tests.test_coin_providers.VerifyLogosModule.test_module_imports_cleanly",
    "tests.test_native_consumer_boundary.NativeConsumerBoundary.test_all_references_import_with_retired_production_modules_blocked",
    "tests.test_native_consumer_boundary.NativeConsumerBoundary.test_sandbox_imports_without_the_python_panel_application",
];

fn identities() -> Result<BTreeMap<String, String>, String> {
    let document: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-baseline.json"
    ))
    .map_err(|_| "invalid pinned import inventory")?;
    if document["baseline_revision"] != BASELINE || document["source_files"] != 277 {
        return Err("whole import source identity drift".into());
    }
    let mut result = BTreeMap::new();
    for row in document["files"]
        .as_array()
        .ok_or("missing import inventory")?
    {
        let path = row["source"].as_str().ok_or("missing import source path")?;
        crate::source_test_inventory::source_path(path)?;
        let hash = row["sha256"].as_str().ok_or("missing import source hash")?;
        if hash.len() != 64 || result.insert(path.into(), hash.into()).is_some() {
            return Err("whole import inventory repeats or lacks an identity".into());
        }
    }
    if result.len() != 277 {
        return Err("whole import inventory requires all277sources".into());
    }
    Ok(result)
}

pub fn source_identity(sources: &[Source]) -> Result<(), String> {
    let mut expected = identities()?;
    let mut total = 0;
    for source in sources {
        total += source.text.len();
        if source.baseline_revision != BASELINE
            || source.text.len() > 2 * 1024 * 1024
            || total > 32 * 1024 * 1024
            || expected.remove(&source.path).as_deref() != Some(source.sha256.as_str())
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != source.sha256
        {
            return Err("whole import source identity drift; refusing execution".into());
        }
    }
    if !expected.is_empty() {
        return Err("whole import source inventory is incomplete".into());
    }
    Ok(())
}

fn workspace(root: &Path, stage: &Path) -> Result<PathBuf, String> {
    let root = tooling_environment::physical_root(root)?;
    let stage = tooling_environment::physical_root(stage)?;
    if stage.parent() != Some(root.join("logs/tmp").as_path())
        || !stage
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("original import replay "))
    {
        return Err("whole import stage must be a direct owned project scratch directory".into());
    }
    Ok(stage)
}

pub fn stage(root: &Path, directory: &Path, sources: &[Source]) -> Result<(), String> {
    source_identity(sources)?;
    let directory = workspace(root, directory)?;
    if std::fs::read_dir(&directory)
        .map_err(|_| "cannot inspect import stage")?
        .next()
        .is_some()
    {
        return Err("whole import stage must be empty".into());
    }
    for source in sources {
        let path = directory.join(&source.path);
        std::fs::create_dir_all(path.parent().ok_or("import source has no parent")?)
            .map_err(|_| "cannot create import source parent")?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| "cannot stage original import source")?;
        file.write_all(source.text.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "cannot persist original import source")?;
    }
    std::fs::create_dir(directory.join("assets"))
        .map_err(|_| "cannot create original import assets")?;
    for &(path, bytes, hash) in ASSETS {
        if crate::codec_raster::digest_hex(bytes) != hash {
            return Err("original import companion asset identity drift".into());
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(path))
            .map_err(|_| "cannot stage import companion asset")?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "cannot persist import companion asset")?;
    }
    validate(root, &directory)
}

pub fn validate(root: &Path, directory: &Path) -> Result<(), String> {
    validate_extra(root, directory, &BTreeMap::new())
}

pub(crate) fn validate_extra(
    root: &Path,
    directory: &Path,
    extra: &BTreeMap<String, String>,
) -> Result<(), String> {
    let directory = workspace(root, directory)?;
    let mut expected = identities()?;
    for &(path, bytes, hash) in ASSETS {
        if crate::codec_raster::digest_hex(bytes) != hash
            || expected.insert(path.into(), hash.into()).is_some()
        {
            return Err("original import companion asset identity drift".into());
        }
    }
    for (path, hash) in extra {
        if expected.insert(path.clone(), hash.clone()).is_some() {
            return Err("import runtime artifact shadows source".into());
        }
    }
    let mut actual = BTreeSet::new();
    let mut pending = vec![(directory.clone(), 0)];
    let mut count = 0;
    let mut total = 0;
    while let Some((parent, depth)) = pending.pop() {
        if depth > 16 || crate::process_owner::cleanup_pending(&parent) {
            return Err("whole import stage custody is unresolved".into());
        }
        for entry in std::fs::read_dir(parent).map_err(|_| "cannot enumerate import stage")? {
            count += 1;
            if count > 512 {
                return Err("whole import stage exceeds inventory bound".into());
            }
            let path = entry
                .map_err(|_| "cannot inspect import stage member")?
                .path();
            tooling_environment::reject_links(&path, "whole import stage cannot follow links")?;
            let metadata =
                std::fs::symlink_metadata(&path).map_err(|_| "cannot inspect import source")?;
            if metadata.is_dir() {
                let relative = path
                    .strip_prefix(&directory)
                    .map_err(|_| "import path escaped")?
                    .to_str()
                    .ok_or("import path is not UTF-8")?
                    .replace('\\', "/");
                if !expected
                    .keys()
                    .any(|key| key.starts_with(&format!("{relative}/")))
                {
                    return Err("unexpected whole import directory; evidence retained".into());
                }
                pending.push((path, depth + 1));
                continue;
            }
            if !metadata.is_file()
                || metadata.len()
                    > if extra.is_empty() {
                        2 * 1024 * 1024
                    } else {
                        256 * 1024 * 1024
                    }
            {
                return Err("whole import source must be regular and under2MiB".into());
            }
            let relative = path
                .strip_prefix(&directory)
                .map_err(|_| "import path escaped")?
                .to_str()
                .ok_or("import path is not UTF-8")?
                .replace('\\', "/");
            let hash = expected
                .get(&relative)
                .ok_or("unexpected whole import file; evidence retained")?;
            if extra.contains_key(&relative) {
                if crate::tooling_manifest::digest(&path)? != *hash || !actual.insert(relative) {
                    return Err("staged import runtime artifact identity drift".into());
                }
                continue;
            }
            let file = std::fs::File::open(&path).map_err(|_| "cannot open import source")?;
            let opened = file
                .metadata()
                .map_err(|_| "cannot inspect opened import source")?;
            if !opened.is_file() || opened.len() > 2 * 1024 * 1024 {
                return Err("opened import source is not a bounded regular file".into());
            }
            let mut bytes = Vec::new();
            file.take(2 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "cannot read import source")?;
            total += bytes.len();
            if bytes.len() > 2 * 1024 * 1024
                || total > 32 * 1024 * 1024
                || crate::codec_raster::digest_hex(&bytes) != *hash
                || !actual.insert(relative)
            {
                return Err("staged whole import source identity drift".into());
            }
        }
    }
    if actual != expected.keys().cloned().collect() {
        return Err("staged whole import source inventory is incomplete".into());
    }
    Ok(())
}

fn interpreter(inherited: &BTreeMap<String, String>) -> Result<PathBuf, String> {
    let requested = if let Some(value) = inherited.get("PYO3_PYTHON") {
        PathBuf::from(value)
    } else {
        pyo3::Python::initialize();
        pyo3::Python::attach(|py| {
            use pyo3::prelude::*;
            crate::source_oracle_runtime::require_runtime(py)?;
            py.import("sys")
                .and_then(|sys| sys.getattr("executable"))
                .and_then(|value| value.extract::<PathBuf>())
                .map_err(|_| "cannot inspect original source interpreter".to_owned())
        })?
    };
    let name = requested
        .to_str()
        .filter(|name| !name.is_empty())
        .ok_or("original source interpreter must be UTF-8")?;
    if name.starts_with('-') || name.chars().any(char::is_control) {
        return Err("invalid original source interpreter".into());
    }
    let executable = if requested.is_absolute() {
        requested
    } else if requested.components().count() == 1 {
        let filename = if cfg!(windows) && !name.to_ascii_lowercase().ends_with(".exe") {
            format!("{name}.exe")
        } else {
            name.to_owned()
        };
        inherited
            .get("PATH")
            .into_iter()
            .flat_map(std::env::split_paths)
            .filter(|directory| directory.is_absolute())
            .map(|directory| directory.join(&filename))
            .find(|path| path.is_file())
            .ok_or("original source interpreter is missing from borrowed PATH")?
    } else {
        return Err("original source interpreter must be absolute or a PATH command".into());
    };
    tooling_environment::physical_root(
        executable
            .parent()
            .ok_or("source interpreter has no parent")?,
    )?;
    #[cfg(windows)]
    tooling_environment::reject_links(
        &executable,
        "source interpreter cannot follow Windows links",
    )?;
    if !executable.is_file() {
        return Err("source interpreter must be regular".into());
    }
    Ok(executable)
}

pub fn real_video_fixture(root: &Path, selected: &Path) -> Result<PathBuf, String> {
    let root = tooling_environment::physical_root(root)?;
    tooling_environment::reject_links(selected, "real video fixture cannot follow links")?;
    if !selected.is_absolute() {
        return Err("real video fixture must be absolute".into());
    }
    let path = crate::migration_bundle::resolved(selected)?;
    let metadata =
        std::fs::symlink_metadata(&path).map_err(|_| "cannot inspect real video fixture")?;
    if !path.starts_with(&root)
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > 2 * 1024 * 1024
    {
        return Err(
            "real video fixture must be a nonempty regular project-contained file under2MiB".into(),
        );
    }
    Ok(path)
}

pub async fn observe_real_video(
    root: &Path,
    directory: &Path,
    clip: &Path,
    files: &BTreeMap<String, String>,
) -> Result<Value, String> {
    let names = [
        "pyproject.toml",
        if cfg!(windows) {
            "native/target/debug/numera-emoji.exe"
        } else {
            "native/target/debug/numera-emoji"
        },
        if cfg!(windows) {
            "emojikit/_native.pyd"
        } else {
            "emojikit/_native.so"
        },
    ];
    if files.len() != names.len()
        || names.iter().any(|name| !files.contains_key(*name))
        || files.get("pyproject.toml").map(String::as_str)
            != Some("b3474a001d7ea21ff0fb309fe0cfa5222e37f5e062b0137ed5e6ce22dbb995d5")
    {
        return Err("real-video replay runtime inventory drift".into());
    }
    let clip = real_video_fixture(root, clip)?;
    let before =
        crate::tooling_manifest::digest(&clip).map_err(|_| "cannot hash real-video input")?;
    let result = observe_mode(root, directory, Mode::RealVideo { files, clip: &clip }).await;
    if crate::tooling_manifest::digest(&clip).map_err(|_| "cannot recheck real-video input")?
        != before
    {
        return Err("original real-video input changed; evidence retained".into());
    }
    result
}

pub(crate) enum Mode<'a> {
    Imports,
    RealVideo {
        files: &'a BTreeMap<String, String>,
        clip: &'a Path,
    },
}
impl Mode<'_> {
    fn cases(&self) -> &[&str] {
        match self {
            Self::Imports => &CASES,
            Self::RealVideo { .. } => &[
                "tests.test_native_migration_cli.NativeMigrationCLI.test_real_operator_video_copy_migrates_and_source_can_restore_bundle",
            ],
        }
    }
}

pub async fn observe_owned(root: &Path, directory: &Path) -> Result<Value, String> {
    observe_mode(root, directory, Mode::Imports).await
}

pub(crate) async fn observe_mode(
    root: &Path,
    directory: &Path,
    mode: Mode<'_>,
) -> Result<Value, String> {
    match &mode {
        Mode::Imports => validate(root, directory)?,
        Mode::RealVideo { files, .. } => validate_extra(root, directory, files)?,
    }
    if !cfg!(any(windows, target_os = "linux")) {
        return Err("whole imports require verified descendant custody; no child started".into());
    }
    let inherited = crate::tooling_test_runner::scrubbed_environment(
        &std::env::vars().collect::<BTreeMap<_, _>>(),
    );
    let executable = interpreter(&inherited)?;
    let log = RunLog::open(root, "source-import-replay", vec![]);
    let result = observe_interpreter(root, directory, executable, &inherited, &log, &mode).await;
    if let Err(error) = &result {
        log.event("ERROR", "source-import", error);
    }
    log.finish(i32::from(result.is_err()));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_codec_log_retention_preserves_bytes_and_refuses_unknown_members() {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let parent = tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
        let root = parent.join(format!("import log retention {}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("logs")).unwrap();
        let input = root.join("codec-logs");
        std::fs::create_dir(&input).unwrap();
        let name = "media_bridge_2026-11-02_00-00-00_UTC_abcdef12.log";
        std::fs::write(input.join(name), b"synthetic owned codec diagnostic").unwrap();
        let unknown = input.join("caller.txt");
        std::fs::write(&unknown, b"caller bytes").unwrap();
        assert!(retain_codec_logs(&root, &input).is_err());
        assert_eq!(std::fs::read(&unknown).unwrap(), b"caller bytes");
        assert_eq!(std::fs::read_dir(root.join("logs")).unwrap().count(), 0);
        std::fs::remove_file(unknown).unwrap();
        retain_codec_logs(&root, &input).unwrap();
        assert!(!input.exists());
        let destinations = std::fs::read_dir(root.join("logs"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(destinations.len(), 1);
        assert_eq!(
            std::fs::read(destinations[0].join("codec-logs").join(name)).unwrap(),
            b"synthetic owned codec diagnostic"
        );
        // No child or real operational log belongs to this policy fixture.
        std::fs::remove_dir_all(root).unwrap();
    }
}

fn retain_codec_logs(root: &Path, directory: &Path) -> Result<(), String> {
    tooling_environment::reject_links(directory, "private codec logs cannot follow links")?;
    let mut count = 0;
    for entry in std::fs::read_dir(directory).map_err(|_| "cannot inventory private codec logs")? {
        count += 1;
        if count > 64 {
            return Err("private codec log inventory exceeded; retained".into());
        }
        let path = entry
            .map_err(|_| "cannot inspect private codec log")?
            .path();
        tooling_environment::reject_links(&path, "private codec logs cannot follow links")?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("invalid private codec log name")?;
        let suffix = name
            .strip_prefix("media_bridge_")
            .and_then(|name| name.strip_suffix(".log"))
            .ok_or("unexpected private codec evidence; retained")?;
        let metadata =
            std::fs::symlink_metadata(&path).map_err(|_| "cannot inspect private codec log")?;
        if suffix.len() < 32
            || suffix.len() > 64
            || !suffix
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || matches!(byte, b'_' | b'-' | b'U' | b'T'))
            || !metadata.is_file()
            || metadata.len() > 1024 * 1024
        {
            return Err("unsupported private codec evidence; retained".into());
        }
    }
    if count == 0 {
        return std::fs::remove_dir(directory)
            .map_err(|_| "cannot remove empty codec log directory".into());
    }
    let parent = tooling_environment::confined_directory(root, "logs", false)?;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let destination = loop {
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = parent.join(format!(
            "original-video-diagnostics-{}-{id}",
            std::process::id()
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => break path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err("cannot retain private codec diagnostics".into()),
        }
    };
    // Retain the entire inspected directory, not stdout, private file paths or digests.
    std::fs::rename(directory, destination.join("codec-logs"))
        .map_err(|_| "cannot retain private codec diagnostics; stage retained".into())
}

async fn observe_interpreter(
    root: &Path,
    directory: &Path,
    executable: PathBuf,
    inherited: &BTreeMap<String, String>,
    log: &RunLog,
    mode: &Mode<'_>,
) -> Result<Value, String> {
    let environment = crate::tooling_runtime::environment(inherited);
    let mut environment = tooling_environment::environment(root, &environment, false)?;
    let version = tooling_process::capture(
        root,
        &[
            executable.clone().into_os_string(),
            "-I".into(),
            "-B".into(),
            "-X".into(),
            "utf8".into(),
            "--version".into(),
        ],
        &environment,
        Duration::from_secs(5),
        Duration::from_secs(3),
        &log,
    )
    .await?;
    if version.code != 0 || version.stdout.trim() != "Python 3.11.9" || !version.stderr.is_empty() {
        return Err(
            "whole original imports require exactly CPython3.11.9; no assertions started".into(),
        );
    }
    environment.extend(crate::tooling_test_runner::scrubbed_environment(
        &environment,
    ));
    environment.insert("PYTHONDONTWRITEBYTECODE".into(), "1".into());
    // Safe-path excludes CWD; only the byte-admitted tree may supply original packages.
    environment.insert(
        "PYTHONPATH".into(),
        directory
            .to_str()
            .ok_or("import stage is not UTF-8")?
            .into(),
    );
    if let Mode::RealVideo { clip, .. } = mode {
        environment.insert(
            "NUMERA_REAL_VIDEO_FIXTURE".into(),
            clip.to_str().ok_or("real video path must be UTF-8")?.into(),
        );
    }
    let cases = mode.cases();
    let (wall, idle) = match mode {
        Mode::Imports => (15, 8),
        Mode::RealVideo { .. } => (60, 20),
    };
    let mut argv = vec![
        executable.into_os_string(),
        "-B".into(),
        "-s".into(),
        "-X".into(),
        "utf8".into(),
        "-m".into(),
        "unittest".into(),
        "-v".into(),
    ];
    argv.extend(cases.iter().map(|case| std::ffi::OsString::from(*case)));
    #[cfg(any(windows, target_os = "linux"))]
    let result = tooling_process::capture_descendants(
        directory,
        &argv,
        &environment,
        Duration::from_secs(wall),
        Duration::from_secs(idle),
        &log,
    )
    .await?;
    #[cfg(not(any(windows, target_os = "linux")))]
    let result = tooling_process::capture(
        directory,
        &argv,
        &environment,
        Duration::from_secs(wall),
        Duration::from_secs(idle),
        &log,
    )
    .await?;
    let lines = result.stderr.lines().collect::<Vec<_>>();
    if result.code != 0
        || !result.stdout.is_empty()
        || lines.last() != Some(&"OK")
        || !lines.iter().any(|line| {
            line.starts_with(&format!(
                "Ran {} test{} in ",
                cases.len(),
                if cases.len() == 1 { "" } else { "s" }
            ))
        })
        || lines
            .iter()
            .filter(|line| line.ends_with(" ... ok"))
            .count()
            != cases.len()
    {
        log.event(
            "ERROR",
            "source-import",
            &match mode {
                Mode::Imports => crate::logging::redact(&result.stderr, &[]),
                Mode::RealVideo { .. } => {
                    "original private-video assertion failed; private details omitted".into()
                }
            },
        );
        return Err("whole original import assertions failed; staged source retained".into());
    }
    for case in cases {
        let (module_class, method) = case
            .rsplit_once('.')
            .ok_or("invalid import case identity")?;
        if !lines.contains(&format!("{method} ({module_class}.{method}) ... ok").as_str()) {
            return Err("whole import assertion output identity drift".into());
        }
    }
    let scratch = directory.join("logs");
    // Capture has finished the complete descendant tree before any retained diagnostics move.
    for path in [scratch.join("tmp/tooling"), scratch.join("tmp")] {
        if path.exists() {
            std::fs::remove_dir(&path).map_err(|_| "whole import child left scratch; retained")?;
        }
    }
    if matches!(mode, Mode::RealVideo { .. }) {
        retain_codec_logs(root, &scratch)?;
    } else if scratch.exists() {
        std::fs::remove_dir(&scratch).map_err(|_| "whole import child left scratch; retained")?;
    }
    match mode {
        Mode::Imports => validate(root, directory)?,
        Mode::RealVideo { files, .. } => validate_extra(root, directory, files)?,
    }
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9",
        "tests_run":cases.len(),"cases":cases,"source_files":277,"errors":0,"failures":0,
        "skipped":0,"ports":"unchanged original whole modules and selected assertions; external selected CPython, shared descendant owner; not native-only package acceptance"}),
    )
}
