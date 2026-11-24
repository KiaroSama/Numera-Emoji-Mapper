//! Resolve installed npm without passing developer arguments through a command shell.
use super::Step;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NpmCommand {
    Executable(PathBuf),
    Node { executable: PathBuf, cli: PathBuf },
}

impl NpmCommand {
    pub(super) fn discover() -> Option<Self> {
        let path = std::env::var_os("PATH")?;
        discover_in(
            &std::env::split_paths(&path).collect::<Vec<_>>(),
            cfg!(windows),
        )
    }

    pub(super) fn step(&self, label: &str, args: &[&str], wall: Duration) -> Step {
        let executable = match self {
            Self::Executable(path) => path,
            Self::Node { executable, .. } => executable,
        };
        let mut step = super::build_steps::step(label, executable, args, wall);
        if let Self::Node { cli, .. } = self {
            step.argv.insert(1, cli.as_os_str().into());
        }
        step
    }
}

fn installed_file(path: &Path) -> Option<PathBuf> {
    if !path.is_file() {
        return None;
    }
    let parent = path.parent()?;
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    // Preserve the invocation filename for native shims and Unix symlink dispatch.
    Some(parent.canonicalize().ok()?.join(path.file_name()?))
}

fn discover_in(directories: &[PathBuf], windows: bool) -> Option<NpmCommand> {
    for directory in directories {
        if let Some(executable) =
            installed_file(&directory.join(if windows { "npm.exe" } else { "npm" }))
        {
            return Some(NpmCommand::Executable(executable));
        }
        if !windows
            || !["npm.cmd", "npm.ps1"]
                .iter()
                .any(|name| directory.join(name).is_file())
        {
            continue;
        }
        let Some(cli) = installed_file(&directory.join("node_modules/npm/bin/npm-cli.js")) else {
            continue;
        };
        // Standard npm shims prefer their sibling Node, then the first Node on PATH.
        // Execute the CLI with that native runtime so every argument stays literal.
        let executable = installed_file(&directory.join("node.exe")).or_else(|| {
            directories
                .iter()
                .find_map(|directory| installed_file(&directory.join("node.exe")))
        });
        if let Some(executable) = executable {
            return Some(NpmCommand::Node { executable, cli });
        }
    }
    None
}

#[cfg(test)]
#[path = "../../tests/developer_npm_support/mod.rs"]
mod tests;
