//! Preserve explicit data-relative paths, legacy project-relative paths and external archives.
use std::path::{Component, Path, PathBuf};

fn absolute(path: &Path) -> std::io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

pub fn resolve(data_dir: &Path, stored: &str, project_root: &Path) -> PathBuf {
    if let Some(relative) = stored.strip_prefix("./") {
        return data_dir.join(relative);
    }
    let path = Path::new(stored);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    }
}

fn equal_component(a: Component<'_>, b: Component<'_>) -> bool {
    #[cfg(windows)]
    {
        a.as_os_str().to_string_lossy().to_lowercase()
            == b.as_os_str().to_string_lossy().to_lowercase()
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

pub fn store(data_dir: &Path, path: &Path) -> std::io::Result<String> {
    let path = absolute(path)?;
    let base = absolute(data_dir)?;
    let components = path.components().collect::<Vec<_>>();
    let prefix = base.components().collect::<Vec<_>>();
    if components.len() >= prefix.len()
        && prefix
            .iter()
            .zip(&components)
            .all(|(a, b)| equal_component(*a, *b))
    {
        let relative = components[prefix.len()..].iter().collect::<PathBuf>();
        let value = relative.to_str().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "media path is not UTF-8")
        })?;
        return Ok(format!(
            "./{}",
            if value.is_empty() {
                ".".into()
            } else {
                value.replace('\\', "/")
            }
        ));
    }
    path.to_str().map(str::to_owned).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "media path is not UTF-8")
    })
}
