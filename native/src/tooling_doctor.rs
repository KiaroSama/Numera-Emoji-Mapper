//! Non-interactive launcher doctor; optional features warn, required runtime evidence fails closed.
use crate::{
    config::Config, logging::RunLog, tooling_environment, tooling_process, tooling_runtime,
};
use pyo3::prelude::*;
use serde_json::{Value, json};
use std::io::Read;
use std::path::Path;
use std::time::Duration;

pub struct Report {
    pub code: i32,
    pub messages: Vec<String>,
}

pub fn unset_keys(template: &str, mut present: impl FnMut(&str) -> bool) -> Vec<String> {
    template
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with('#') {
                return None;
            }
            let (key, _) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty()
                || !key
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            {
                return None;
            }
            (!present(key)).then(|| key.to_owned())
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub fn assess(runtime: &Value, video: bool, unset: &[String]) -> Result<Report, String> {
    if runtime["implementation"] != "cpython"
        || runtime["version"][0] != 3
        || runtime["version"][1] != 11
        || runtime["safe_path"] != true
        || runtime["user_site"] != false
        || runtime["dont_write_bytecode"] != true
        || runtime["pillow_roundtrip"] != true
        || runtime["extension"]["api_version"] != 1
        || runtime["extension"]["greedy_callable"] != true
        || runtime["extension"]["near_callable"] != true
        || runtime["extension"]["package_imported"] != false
        || runtime["requests"]["imported"] != true
    {
        return Err("required native runtime or codec evidence is incomplete".into());
    }
    let dependencies = runtime["dependencies"]
        .as_array()
        .ok_or("missing codec observations")?;
    for name in ["Pillow", "resvg-py", "rlottie-python"] {
        if dependencies
            .iter()
            .filter(|dependency| dependency["distribution"] == name)
            .count()
            != 1
        {
            return Err("required codec observations are incomplete or duplicated".into());
        }
    }
    if !runtime["numpy"]["imported"].is_boolean() {
        return Err("optional numpy observation is unknown".into());
    }
    let mut messages = unset
        .iter()
        .map(|key| format!("[!] unset: {key}"))
        .collect::<Vec<_>>();
    if !video {
        messages.push("[!] ffmpeg/ffprobe not found: video emoji disabled.".into());
    }
    if runtime["numpy"]["imported"] == false {
        messages.push("[!] numpy not installed or unusable: coin remapping is unavailable.".into());
    }
    messages.push("[OK] All required runtime dependencies import correctly.".into());
    messages.push("[OK] Environment check complete.".into());
    Ok(Report { code: 0, messages })
}

/// Called only inside the existing fresh selected-runtime child; no application package import.
pub(crate) fn observe(py: Python<'_>, root: &Path, runtime: &Value) -> Result<Value, String> {
    let mut extensions = Vec::new();
    for suffix in runtime["extension_suffixes"]
        .as_array()
        .ok_or("missing native extension suffixes")?
    {
        let suffix = suffix.as_str().ok_or("invalid native extension suffix")?;
        let path = root.join("emojikit").join(format!("_native{suffix}"));
        tooling_environment::reject_links(&path, "doctor extension cannot follow links")?;
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && metadata.len() > 0 => extensions.push(path),
            Ok(_) => return Err("doctor native extension must be a nonempty regular file".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("cannot inspect installed native extension".into()),
        }
    }
    extensions.sort();
    extensions.dedup();
    if extensions.len() != 1 {
        return Err("doctor requires exactly one installed native extension".into());
    }
    let extension = crate::tooling_extension::inspect(py, root, &extensions[0])?;
    let dependency = |module: &str| -> Result<Value, String> {
        let library = match py.import(module) {
            Ok(library) => library,
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyImportError>(py) => {
                return Ok(json!({"imported":false}));
            }
            Err(_) => return Err("doctor dependency import failed unexpectedly".into()),
        };
        let origin: String = library
            .getattr("__file__")
            .and_then(|value| value.extract())
            .map_err(|_| "cannot inspect doctor dependency origin")?;
        let origin = crate::migration_bundle::resolved(Path::new(&origin))?;
        let prefix = crate::migration_bundle::resolved(Path::new(
            runtime["prefix"]
                .as_str()
                .ok_or("missing selected prefix")?,
        ))?;
        let base = crate::migration_bundle::resolved(Path::new(
            runtime["base_prefix"]
                .as_str()
                .ok_or("missing runtime base prefix")?,
        ))?;
        if !origin.is_file()
            || !(origin.starts_with(&prefix)
                || runtime["shared_sites"] == true && origin.starts_with(&base))
        {
            return Err("doctor dependency was not imported from an admitted runtime site".into());
        }
        Ok(json!({"imported":true,"origin":origin}))
    };
    Ok(
        json!({"extension":extension,"requests":dependency("requests")?,"numpy":dependency("numpy")?}),
    )
}

fn template(root: &Path) -> Result<String, String> {
    let path = root.join(".env.example");
    tooling_environment::reject_links(&path, "doctor template cannot follow links")?;
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| "doctor requires .env.example")?;
    if !metadata.is_file() || metadata.len() > 65536 {
        return Err("doctor template must be a regular file under64KiB".into());
    }
    let file = std::fs::File::open(path).map_err(|_| "cannot read doctor template")?;
    if !file
        .metadata()
        .map_err(|_| "cannot inspect open doctor template")?
        .is_file()
    {
        return Err("open doctor template must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read doctor template")?;
    if bytes.len() > 65536 {
        return Err("doctor template grew beyond64KiB".into());
    }
    String::from_utf8(bytes).map_err(|_| "doctor template must be UTF-8".into())
}

pub fn run(root: &Path, args: &[String]) -> i32 {
    if args == ["--help"] || args == ["-h"] {
        println!(
            "Usage: numera-emoji doctor\nNon-interactive installed-runtime check. Never installs, builds or contacts Telegram."
        );
        return 0;
    }
    let config = match Config::load(root.into()) {
        Ok(config) => config,
        Err(_) => {
            eprintln!("ERROR: cannot read doctor configuration");
            return 1;
        }
    };
    let log = std::sync::Arc::new(RunLog::open(root, "doctor", config.secret_values()));
    log.install_panic_hook();
    let result = (|| {
        if !args.is_empty() {
            return Err("doctor accepts no arguments".into());
        }
        let root = tooling_environment::physical_root(root)?;
        let binary = root.join(if cfg!(windows) {
            "native/runtime/numera-emoji.exe"
        } else {
            "native/runtime/numera-emoji"
        });
        tooling_environment::reject_links(
            &binary,
            "doctor installed executable cannot follow links",
        )?;
        if !binary.is_file() {
            return Err(
                "Installed native executable missing; install a verified prebuilt package.".into(),
            );
        }
        let original = tooling_runtime::environment(&std::env::vars().collect());
        let environment = tooling_environment::environment(&root, &original, false)?;
        let owner = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "cannot initialize doctor child owner")?;
        let report = owner.block_on(async {
            let startup = tooling_process::capture(
                &root,
                &[binary.clone().into_os_string(), "--help".into()],
                &environment,
                Duration::from_secs(10),
                Duration::from_secs(5),
                &log,
            )
            .await?;
            if startup.code != 0
                || !startup
                    .stdout
                    .contains("Numera Emoji Mapper native backend")
            {
                return Err("Installed native executable failed its startup probe.".into());
            }
            tooling_runtime::inspect_executable(
                &root,
                &binary,
                &["--doctor".into()],
                &original,
                &log,
            )
            .await
        })?;
        let missing = unset_keys(&template(&root)?, |key| !config.value(key).is_empty());
        let video = ["ffmpeg", "ffprobe"]
            .iter()
            .all(|name| crate::codec_process::executable(name).is_ok());
        assess(&report, video, &missing)
    })();
    let code = match result {
        Ok(report) => {
            for message in report.messages {
                if message.starts_with("[!]") {
                    log.event("WARNING", "doctor", &message);
                }
                println!("{message}");
            }
            report.code
        }
        Err(error) => {
            log.event("ERROR", "doctor", &error);
            1
        }
    };
    log.finish(code);
    code
}
