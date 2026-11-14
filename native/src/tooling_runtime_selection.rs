//! Venv configuration and explicit executable admission; no interpreter starts during selection.
use crate::tooling_environment;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Selection {
    pub prefix: PathBuf,
    pub executable: PathBuf,
    pub shared_sites: bool,
    pub version: [u32; 3],
}

pub fn selection(root: &Path) -> Result<Selection, String> {
    let root = tooling_environment::physical_root(root)?;
    let prefix = tooling_environment::confined_directory(&root, ".venv", false)?;
    let executable = prefix.join(if cfg!(windows) {
        "Scripts/python.exe"
    } else {
        "bin/python"
    });
    read(&prefix, executable)
}

pub(super) fn explicit(
    root: &Path,
    requested: &Path,
    inherited: &BTreeMap<String, String>,
) -> Result<Selection, String> {
    let root = tooling_environment::physical_root(root)?;
    let name = requested
        .to_str()
        .filter(|name| !name.is_empty())
        .ok_or("check --python requires a UTF-8 executable path or name")?;
    if name.starts_with('-')
        || name.chars().any(char::is_control)
        || !requested.is_absolute()
            && requested.components().any(|part| {
                matches!(
                    part,
                    std::path::Component::Prefix(_) | std::path::Component::RootDir
                )
            })
    {
        return Err("check --python requires an executable path or name".into());
    }
    let executable = if requested.is_absolute() || requested.components().count() > 1 {
        if requested.is_absolute() {
            requested.into()
        } else {
            root.join(requested)
        }
    } else {
        let filename = if cfg!(windows) && !name.to_ascii_lowercase().ends_with(".exe") {
            format!("{name}.exe")
        } else {
            name.to_owned()
        };
        super::environment(inherited)
            .get("PATH")
            .into_iter()
            .flat_map(std::env::split_paths)
            .filter(|directory| directory.is_absolute())
            .map(|directory| directory.join(&filename))
            .find(|path| path.is_file())
            .ok_or("check --python interpreter is missing or unreadable; no checks started")?
    };
    let directory = executable
        .parent()
        .ok_or("check interpreter has no directory")?;
    let prefix = directory
        .parent()
        .ok_or("check interpreter has no environment directory")?;
    if !directory
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            if cfg!(windows) {
                name.eq_ignore_ascii_case("Scripts")
            } else {
                name == "bin"
            }
        })
        || executable.components().any(|part| {
            part.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case("WindowsApps")
        })
    {
        return Err(
            "check --python requires a compatible venv interpreter; no checks started".into(),
        );
    }
    // Canonicalize only the parent: resolving the final POSIX python link loses venv discovery.
    let directory = tooling_environment::physical_root(directory)?;
    let prefix = tooling_environment::physical_root(prefix)?;
    if directory.parent() != Some(prefix.as_path()) {
        return Err("check interpreter directory binding changed; no checks started".into());
    }
    let executable = directory.join(
        executable
            .file_name()
            .ok_or("check interpreter name missing")?,
    );
    read(&prefix, executable)
}

fn read(prefix: &Path, executable: PathBuf) -> Result<Selection, String> {
    tooling_environment::reject_links(
        executable
            .parent()
            .ok_or("package interpreter directory missing")?,
        "package interpreter directory cannot follow links",
    )?;
    if !prefix.is_dir() {
        return Err(
            "runtime-check requires a real package .venv; no environment is created".into(),
        );
    }
    let config = prefix.join("pyvenv.cfg");
    tooling_environment::reject_links(&config, "package pyvenv.cfg cannot follow links")?;
    let metadata = std::fs::symlink_metadata(&config)
        .map_err(|_| "package pyvenv.cfg is missing or unreadable")?;
    if !metadata.is_file() || metadata.len() > 65536 {
        return Err("package pyvenv.cfg must be a regular file under64KiB".into());
    }
    let file =
        std::fs::File::open(&config).map_err(|_| "package pyvenv.cfg is missing or unreadable")?;
    if !file
        .metadata()
        .map_err(|_| "cannot inspect open package pyvenv.cfg")?
        .is_file()
    {
        return Err("open package pyvenv.cfg must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read package pyvenv.cfg")?;
    if bytes.len() > 65536 {
        return Err("package pyvenv.cfg grew beyond64KiB".into());
    }
    let source = std::str::from_utf8(&bytes).map_err(|_| "package pyvenv.cfg must be UTF-8")?;
    let mut fields = BTreeMap::new();
    for line in source.lines() {
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_ascii_lowercase();
            if fields.insert(key, value.trim()).is_some() {
                return Err("package pyvenv.cfg contains duplicate settings".into());
            }
        }
    }
    let home = fields
        .get("home")
        .filter(|value| !value.is_empty())
        .ok_or("package pyvenv.cfg requires home")?;
    if !Path::new(home).is_absolute() || !Path::new(home).is_dir() {
        return Err(
            "package pyvenv.cfg home must name an existing absolute runtime directory".into(),
        );
    }
    let version = fields
        .get("version")
        .ok_or("package pyvenv.cfg requires version")?;
    let version: [u32; 3] = version
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "package pyvenv.cfg version must contain three integers")?
        .try_into()
        .map_err(|_| "package pyvenv.cfg version must contain three integers")?;
    if version[..2] != [3, 11] {
        return Err("package pyvenv.cfg requires Python3.11".into());
    }
    let shared_sites = match fields
        .get("include-system-site-packages")
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("true") => true,
        Some("false") => false,
        _ => {
            return Err("package pyvenv.cfg requires explicit include-system-site-packages".into());
        }
    };
    if cfg!(windows) {
        tooling_environment::reject_links(
            &executable,
            "Windows package interpreter cannot follow links",
        )?;
    }
    let metadata = std::fs::metadata(&executable)
        .map_err(|_| "package .venv interpreter is missing or unreadable")?;
    if !metadata.is_file() {
        return Err("package .venv interpreter must be a regular executable".into());
    }
    executable
        .to_str()
        .ok_or("package interpreter path must be UTF-8")?;
    Ok(Selection {
        prefix: prefix.into(),
        executable,
        shared_sites,
        version,
    })
}

pub(crate) fn sysconfig_paths(
    selected: &Selection,
    source: &str,
) -> Result<BTreeMap<String, PathBuf>, String> {
    if !source
        .lines()
        .any(|line| line == "Python version: \"3.11\"")
    {
        return Err("selected package interpreter did not report Python3.11".into());
    }
    let (paths, variables) = source
        .split_once("Paths: ")
        .and_then(|(_, tail)| tail.split_once("Variables: "))
        .ok_or("selected package interpreter returned no sysconfig paths")?;
    let versions = variables
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("py_version = \"")?
                .strip_suffix('"')
        })
        .collect::<Vec<_>>();
    let expected = selected.version.map(|part| part.to_string()).join(".");
    if versions != [expected.as_str()] {
        return Err("selected package interpreter version does not match pyvenv.cfg; use a CPython3.11 venv matching the linked Python version; no source checks started".into());
    }
    let mut result = BTreeMap::new();
    for name in ["data", "scripts", "platlib"] {
        let prefix = format!("{name} = \"");
        let values = paths
            .lines()
            .filter_map(|line| line.trim().strip_prefix(&prefix)?.strip_suffix('"'))
            .collect::<Vec<_>>();
        if values.len() != 1 || !Path::new(values[0]).is_absolute() {
            return Err(format!(
                "selected package interpreter returned ambiguous {name} path"
            ));
        }
        let physical = crate::migration_bundle::resolved(Path::new(values[0]))?;
        if !physical.starts_with(&selected.prefix) || name == "data" && physical != selected.prefix
        {
            return Err("selected package interpreter did not use the package .venv".into());
        }
        result.insert(name.to_owned(), physical);
    }
    Ok(result)
}
