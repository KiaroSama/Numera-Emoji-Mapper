//! Public source-schema1 package tools; physical owning root, read-only Git provenance, no installation.
use crate::{
    config::Config, logging::RunLog, tooling_environment, tooling_manifest, tooling_package,
};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub struct PackageOptions {
    pub executable: PathBuf,
    pub wheel: PathBuf,
    pub dependencies: PathBuf,
    pub output: PathBuf,
}

pub enum Arguments {
    Package(PackageOptions),
    Exit(u8),
}

/// Delegate the original CLI grammar to the unchanged external stdlib parser.
pub fn arguments(root: &Path, args: &[String]) -> Result<Arguments, String> {
    Python::initialize();
    Python::attach(|py| -> Result<Arguments, String> {
        let result = (|| -> PyResult<Arguments> {
            let kwargs = PyDict::new(py);
            kwargs.set_item("prog", "numera-emoji package-native")?;
            kwargs.set_item("description", "Build a public source-schema1 archive; no installation or source build is performed.")?;
            let parser = py
                .import("argparse")?
                .getattr("ArgumentParser")?
                .call((), Some(&kwargs))?;
            let path = py.import("pathlib")?.getattr("Path")?;
            for name in ["--executable", "--wheel", "--dependencies", "--out"] {
                let option = PyDict::new(py);
                option.set_item("type", &path)?;
                if name == "--out" {
                    option.set_item("default", path.call1((root.join("dist"),))?)?;
                } else {
                    option.set_item("required", true)?;
                }
                parser.call_method("add_argument", (name,), Some(&option))?;
            }
            match parser.call_method1("parse_args", (PyList::new(py, args)?,)) {
                Ok(parsed) => Ok(Arguments::Package(PackageOptions {
                    executable: parsed.getattr("executable")?.extract()?,
                    wheel: parsed.getattr("wheel")?.extract()?,
                    dependencies: parsed.getattr("dependencies")?.extract()?,
                    output: parsed.getattr("out")?.extract()?,
                })),
                Err(error) if error.is_instance_of::<pyo3::exceptions::PySystemExit>(py) => {
                    Ok(Arguments::Exit(error.value(py).getattr("code")?.extract()?))
                }
                Err(error) => Err(error),
            }
        })();
        // Embedded CPython is not finalized by native exit, so help/errors need an explicit flush.
        let sys = py.import("sys").map_err(|error| error.to_string())?;
        for stream in ["stdout", "stderr"] {
            sys.getattr(stream)
                .and_then(|stream| stream.call_method0("flush"))
                .map_err(|error| error.to_string())?;
        }
        result.map_err(|error| error.to_string())
    })
}

pub fn options(root: &Path, args: &[String]) -> Result<PackageOptions, String> {
    match arguments(root, args)? {
        Arguments::Package(options) => Ok(options),
        Arguments::Exit(code) => Err(format!("package argument parser exited with status {code}")),
    }
}

pub fn git_environment(inherited: &BTreeMap<OsString, OsString>) -> BTreeMap<OsString, OsString> {
    let mut environment = crate::source_git_inventory::git_environment(inherited);
    environment.retain(|key, _| {
        let upper = key.to_string_lossy().to_ascii_uppercase();
        !["TOKEN", "SECRET", "PASSWORD", "API_KEY"]
            .iter()
            .any(|hint| upper.contains(hint))
            && !matches!(
                upper.as_str(),
                "PACK_OWNER_USER_ID" | "PACK_LINKS_CHAT_ID" | "OWNER_ID"
            )
    });
    environment.insert("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into());
    environment
}

/// Reuses the source reader's owning-repository/environment policy; its new tests remain unexecuted.
pub async fn head(root: &Path) -> Result<String, String> {
    let root = tooling_environment::physical_root(root)?;
    let environment = git_environment(&std::env::vars_os().collect());
    let git = crate::codec_process::executable("git")?;
    let captured = crate::codec_process::capture_with_environment(
        &root,
        &git,
        &[
            "--no-replace-objects".into(),
            "rev-parse".into(),
            "--show-toplevel".into(),
        ],
        16 * 1024,
        None,
        &environment,
    )
    .await?;
    let location = std::str::from_utf8(&captured.stdout)
        .map_err(|_| "Git package root is not UTF-8")?
        .trim();
    if tooling_environment::physical_root(Path::new(location))? != root {
        return Err("Package root must be the owning Git repository".into());
    }
    let captured = crate::codec_process::capture_with_environment(
        &root,
        &git,
        &[
            "--no-replace-objects".into(),
            "rev-parse".into(),
            "HEAD".into(),
        ],
        128,
        None,
        &environment,
    )
    .await?;
    let revision = std::str::from_utf8(&captured.stdout)
        .map_err(|_| "Git HEAD is not ASCII")?
        .trim();
    if revision.len() != 40
        || !revision
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("Package provenance requires exact lowercase40hex Git HEAD".into());
    }
    Ok(revision.into())
}

pub fn run(config: &Config, command: &str, args: &[String], log: &RunLog) -> Result<u8, String> {
    if command == "package-verify" && (args == ["--help"] || args == ["-h"]) {
        println!(
            "Usage: numera-emoji package-verify\nVerify this extracted source-schema1 package's platform, Python3.11 prerequisite, file hashes and native wheel. No installation or activation is performed."
        );
        return Ok(0);
    }
    match command {
        "package-native" => {
            let parsed = match arguments(&config.root, args)? {
                Arguments::Package(options) => options,
                Arguments::Exit(code) => return Ok(code),
            };
            let root = tooling_environment::physical_root(&config.root)?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| "Cannot initialize package process monitor")?;
            let revision = runtime.block_on(head(&root))?;
            let target = tooling_manifest::embedded_host()?.platform_id()?;
            let archive = tooling_package::package(
                &root,
                &parsed.executable,
                &parsed.wheel,
                &parsed.dependencies,
                &parsed.output,
                &revision,
                &target,
            )?;
            println!("{}", archive.display());
            log.event(
                "INFO",
                "package",
                "Source-schema1 archive and checksum published; no installation performed",
            );
            Ok(0)
        }
        "package-verify" => {
            if !args.is_empty() {
                return Err(format!("unknown package verification option: {}", args[0]));
            }
            let root = tooling_environment::physical_root(&config.root)?;
            // This observes the embedded runtime, not a selected future venv.
            // Only package policy is certified; installer/launcher activation stays gated.
            let manifest = tooling_manifest::verify(&root, &tooling_manifest::embedded_host()?)?;
            println!(
                "Verified package {} ({})",
                manifest["version"].as_str().unwrap_or(""),
                manifest["platform"].as_str().unwrap_or("")
            );
            log.event(
                "INFO",
                "package",
                "Source-schema1 platform and file inventory verified; no activation performed",
            );
            Ok(0)
        }
        _ => Err("Unknown tooling package command".into()),
    }
}
