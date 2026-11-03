//! Original tracked-file secret policy; diagnostics contain keys and paths, never values.
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

const MAX_BYTES: u64 = 2 * 1024 * 1024;

pub fn scan(
    root: &Path,
    configuration: &str,
    keys: &[String],
    tracked: &[String],
) -> Result<Vec<(String, String)>, String> {
    let root = crate::tooling_environment::physical_root(root)?;
    let mut secrets = Vec::<(String, String)>::new();
    for line in crate::emoji_ids::lines(configuration) {
        let line = line.trim_matches(crate::emoji_ids::whitespace);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim_matches(crate::emoji_ids::whitespace);
        let value = value
            .trim_matches(crate::emoji_ids::whitespace)
            .trim_matches('"')
            .trim_matches('\'');
        if value.chars().count() < 12 || !keys.iter().any(|part| key.contains(part)) {
            continue;
        }
        if let Some((_, previous)) = secrets.iter_mut().find(|(name, _)| name == key) {
            *previous = value.into();
        } else {
            secrets.push((key.into(), value.into()));
        }
    }
    let mut leaks = Vec::new();
    for name in tracked {
        if name.is_empty()
            || name.contains(['\\', ':', '\0'])
            || name
                .split('/')
                .any(|part| part.is_empty() || matches!(part, "." | ".."))
        {
            return Err("tracked secret scan requires confined relative paths".into());
        }
        let path = root.join(name);
        crate::tooling_environment::reject_links(&path, "tracked secret scan cannot follow links")?;
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > MAX_BYTES {
            continue;
        }
        let Ok(file) = std::fs::File::open(&path) else {
            continue;
        };
        let mut bytes = Vec::new();
        if file.take(MAX_BYTES + 1).read_to_end(&mut bytes).is_err()
            || bytes.len() as u64 > MAX_BYTES
        {
            continue;
        }
        // Replacement decoding is part of this scanner's pinned source contract only.
        let text = String::from_utf8_lossy(&bytes);
        for (key, value) in &secrets {
            if text.contains(value.as_str()) {
                leaks.push((key.clone(), name.clone()));
            }
        }
    }
    Ok(leaks)
}

/// Enumerate the owning index with the existing bounded process/environment policy.
pub async fn tracked(root: &Path) -> Result<Vec<String>, String> {
    let root = crate::tooling_environment::physical_root(root)?;
    let mut environment = BTreeMap::new();
    for name in [
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "PATHEXT",
        "LANG",
        "LC_ALL",
    ] {
        if let Some(value) = std::env::var_os(name) {
            environment.insert(name.into(), value);
        }
    }
    let environment = crate::source_git_inventory::git_environment(&environment);
    let git = crate::codec_process::executable("git")?;
    let arguments = ["--no-replace-objects", "rev-parse", "--show-toplevel"].map(String::from);
    let location = crate::codec_process::capture_with_environment(
        &root,
        &git,
        &arguments,
        16 * 1024,
        None,
        &environment,
    )
    .await?;
    let location = std::str::from_utf8(&location.stdout)
        .map_err(|_| "tracked secret scan Git root is not UTF-8")?
        .trim();
    if crate::tooling_environment::physical_root(Path::new(location))? != root {
        return Err("tracked secret scan requires the owning Git repository".into());
    }
    let arguments = ["--no-replace-objects", "ls-files", "-z"].map(String::from);
    let captured = crate::codec_process::capture_with_environment(
        &root,
        &git,
        &arguments,
        2 * 1024 * 1024,
        None,
        &environment,
    )
    .await?;
    let text = std::str::from_utf8(&captured.stdout)
        .map_err(|_| "tracked secret scan filenames are not UTF-8")?;
    if !text.is_empty() && !text.ends_with('\0') {
        return Err("tracked secret scan Git output is incomplete".into());
    }
    Ok(text
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(String::from)
        .collect())
}
