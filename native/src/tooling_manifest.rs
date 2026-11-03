//! Verify source schema1 before any install; hashes are streamed and links never followed.
use crate::tooling_environment;
use pyo3::prelude::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Host {
    pub system: String,
    pub machine: String,
    pub python: (u32, u32),
    pub libc: String,
    pub libc_version: String,
    pub macos: String,
}

impl Host {
    pub fn platform_id(&self) -> Result<String, String> {
        let system = match self.system.as_str() {
            "Windows" => "windows",
            "Linux" => "linux",
            "Darwin" => "macos",
            _ => return Err("Unsupported package platform or architecture".into()),
        };
        let arch = match self.machine.to_lowercase().as_str() {
            "amd64" | "x86_64" => "x64",
            "arm64" | "aarch64" => "arm64",
            _ => return Err("Unsupported package platform or architecture".into()),
        };
        Ok(format!("{system}-{arch}"))
    }
}

/// Observation of the embedded runtime only; this is not selected-venv bootstrap proof.
pub fn embedded_host() -> Result<Host, String> {
    Python::initialize();
    Python::attach(|py| -> PyResult<Host> {
        let platform = py.import("platform")?;
        let version = py.import("sys")?.getattr("version_info")?;
        let libc = platform
            .call_method0("libc_ver")?
            .extract::<(String, String)>()?;
        let mac = platform
            .call_method0("mac_ver")?
            .get_item(0)?
            .extract::<String>()?;
        Ok(Host {
            system: platform.call_method0("system")?.extract()?,
            machine: platform.call_method0("machine")?.extract()?,
            python: (
                version.get_item(0)?.extract()?,
                version.get_item(1)?.extract()?,
            ),
            libc: libc.0,
            libc_version: libc.1,
            macos: mac,
        })
    })
    .map_err(|_| "Cannot inspect external Python runtime/platform".into())
}

pub fn project_version(root: &Path) -> Result<String, String> {
    let source = std::fs::read_to_string(root.join("pyproject.toml")).map_err(|e| e.to_string())?;
    Python::initialize();
    // TOML parsing remains an unchanged external stdlib API, not an embedded
    // first-party Python parser or a second guessed version configuration.
    Python::attach(|py| -> PyResult<String> {
        py.import("tomllib")?
            .call_method1("loads", (source,))?
            .get_item("project")?
            .get_item("version")?
            .extract()
    })
    .map_err(|_| "Invalid project version in pyproject.toml".into())
}

pub fn digest(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 64 * 1024];
    loop {
        let read = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hash.update(&bytes[..read]);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub fn member_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    if name.starts_with('/')
        || name.contains(['\\', ':'])
        || name.split('/').any(|part| part == "..")
    {
        return Err("Invalid package member path".into());
    }
    // PurePosixPath collapses repeated separators and '.' in schema1 source.
    let path = name
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .fold(root.to_path_buf(), |path, part| path.join(part));
    tooling_environment::reject_links(&path, &format!("Linked package member: {name}"))?;
    Ok(path)
}

pub fn verify(root: &Path, host: &Host) -> Result<Value, String> {
    let manifest_path = root.join("package.json");
    tooling_environment::reject_links(&manifest_path, "Linked package manifest is not supported")?;
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).map_err(|e| e.to_string())?)
            .map_err(|_| "Unsupported package manifest")?;
    let root = crate::migration_bundle::resolved(root)?;
    if !manifest.is_object()
        || !(manifest["schema"] == 1
            || manifest["schema"] == true
            || manifest["schema"].as_f64() == Some(1.0))
    {
        return Err("Unsupported package manifest".into());
    }
    let target = host
        .platform_id()
        .map_err(|_| "Download the package matching this operating system and CPU")?;
    if manifest["platform"] != target {
        return Err("Download the package matching this operating system and CPU".into());
    }
    if host.python != (3, 11) {
        return Err(
            "This offline package needs Python 3.11 (matching its bundled dependency wheels)"
                .into(),
        );
    }
    if target.starts_with("linux-") {
        let version = host
            .libc_version
            .split('.')
            .take(2)
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>();
        if host.libc != "glibc"
            || !version.is_ok_and(|parts| parts.as_slice() >= [2, 39].as_slice())
        {
            return Err(
                "This Linux package needs glibc 2.39+ (Ubuntu 24.04+); Alpine is not supported"
                    .into(),
            );
        }
    }
    if target.starts_with("macos-")
        && host
            .macos
            .split('.')
            .next()
            .and_then(|v| v.parse::<u32>().ok())
            .is_none_or(|v| v < 15)
    {
        return Err("This macOS package needs macOS 15+".into());
    }
    let files = manifest["files"]
        .as_object()
        .filter(|files| !files.is_empty())
        .ok_or("Package manifest has no file inventory")?;
    for (name, expected) in files {
        let path = member_path(&root, name)?;
        if !path.is_file()
            || expected
                .as_str()
                .is_none_or(|expected| digest(&path).as_deref() != Ok(expected))
        {
            return Err(format!("Missing or changed package member: {name}"));
        }
    }
    let version = project_version(&root)?;
    let wheel = manifest["wheel"]
        .as_str()
        .ok_or("Package version or wheel is inconsistent")?;
    if manifest["version"] != version || wheel.is_empty() || wheel.contains(['/', '\\']) {
        return Err("Package version or wheel is inconsistent".into());
    }
    if !files.contains_key(&format!("wheels/{wheel}"))
        || !wheel.starts_with(&format!("numera_emoji_core-{version}-"))
        || !wheel.ends_with(".whl")
    {
        return Err("Matching native wheel is absent".into());
    }
    let executable = if target.starts_with("windows-") {
        "native/runtime/numera-emoji.exe"
    } else {
        "native/runtime/numera-emoji"
    };
    if [
        executable,
        "assets/panel.html",
        "requirements.txt",
        "scripts/launch_prebuilt.py",
    ]
    .iter()
    .any(|name| !files.contains_key(*name))
    {
        return Err("Package is incomplete".into());
    }
    Ok(manifest)
}
