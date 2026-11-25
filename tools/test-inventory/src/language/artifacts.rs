use std::path::{Component, Path, PathBuf};

pub(super) fn safe(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !Path::new(path).is_absolute()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
pub(super) fn private(path: &str) -> bool {
    let normalized = path.to_ascii_lowercase();
    let path = normalized.as_str();
    path == ".env"
        || path.starts_with(".env.") && path != ".env.example"
        || [
            ".git/",
            ".ai/",
            ".agents/",
            ".codex/",
            ".aws/",
            "private/",
            ".backups/",
            "logs/",
            ".cache/",
        ]
        .iter()
        .any(|prefix| path.starts_with(prefix))
        || path
            .split('/')
            .any(|s| matches!(s, "node_modules" | "target" | "__pycache__" | ".venv"))
}
pub(crate) fn linked(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
pub(super) fn physical_file(root: &Path, path: &str) -> Result<PathBuf, String> {
    if !safe(path) {
        return Err("candidate path is not repository-relative".into());
    }
    let mut current = root.to_path_buf();
    for part in Path::new(path).components() {
        current.push(part.as_os_str());
        let metadata = std::fs::symlink_metadata(&current)
            .map_err(|_| "candidate is missing or unreadable")?;
        if linked(&metadata) {
            return Err("candidate or an ancestor is a link/reparse point".into());
        }
    }
    if !current.is_file() {
        return Err("candidate is not a regular file".into());
    }
    Ok(current)
}
pub(super) fn forbidden_suffix(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [
        ".py", ".pyw", ".pyc", ".pyo", ".ps1", ".psm1", ".psd1", ".sh", ".bash", ".zsh", ".fish",
        ".bat", ".cmd", ".c", ".cc", ".cpp", ".cxx", ".h", ".hpp", ".rb", ".pl", ".php", ".lua",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
}
pub(super) fn legacy_metadata(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    matches!(
        name.as_str(),
        "pyproject.toml"
            | "ruff.toml"
            | "pytest.ini"
            | "tox.ini"
            | "setup.cfg"
            | "pipfile"
            | "pipfile.lock"
            | "uv.lock"
    ) || name.starts_with("requirements") && name.ends_with(".txt")
}
pub(super) fn legacy_program(value: &str) -> bool {
    let name = value
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(value)
        .trim_matches(['\'', '"'])
        .to_ascii_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    matches!(
        name,
        "python"
            | "python2"
            | "python3"
            | "pythonw"
            | "py"
            | "pypy"
            | "pypy3"
            | "pwsh"
            | "powershell"
            | "bash"
            | "sh"
            | "zsh"
            | "fish"
            | "cmd"
            | "cscript"
            | "wscript"
    ) || [
        ".py", ".pyw", ".ps1", ".psm1", ".sh", ".bash", ".cmd", ".bat",
    ]
    .iter()
    .any(|suffix| name.ends_with(suffix))
        || name.starts_with("python3.")
        || name.starts_with("python2.")
}
/// Recognize source statements, not historical filenames, ordinary prose or JSON IDs.
pub(super) fn script_line(line: &str) -> bool {
    let line = line.trim().trim_start_matches('\u{feff}');
    if line.starts_with("#!") && !line.starts_with("#![") {
        return line
            .split_whitespace()
            .any(|v| legacy_program(v) || v == "-S")
            && !line.contains("cargo");
    }
    let lower = line.to_ascii_lowercase();
    if (line.starts_with("def ") || line.starts_with("async def ") || line.starts_with("class "))
        && line.ends_with(':')
    {
        return true;
    }
    if line.starts_with("from ") && line.contains(" import ") {
        return true;
    }
    if let Some(import) = line.strip_prefix("import ")
        && !import.is_empty()
        && import
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_., ".contains(c))
    {
        return true;
    }
    line.starts_with("if __name__")
        || line.starts_with("print(")
        || lower.starts_with("param(")
        || lower.starts_with("[cmdletbinding(")
        || lower.starts_with("$erroractionpreference")
        || lower.starts_with("set-strictmode ")
        || lower.starts_with("write-host ")
        || lower.starts_with("invoke-expression ")
        || lower.starts_with("set -e")
        || lower.starts_with("set -u")
        || lower.starts_with("@echo off")
}
pub(super) fn script_text(text: &str, markdown: bool) -> bool {
    let mut fence = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if markdown && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
            let marker = trimmed.chars().next().unwrap();
            let length = trimmed.chars().take_while(|c| *c == marker).count();
            match fence {
                Some((m, n)) if m == marker && length >= n => fence = None,
                None => fence = Some((marker, length)),
                _ => {}
            }
            continue;
        }
        if fence.is_none() && script_line(line) {
            return true;
        }
    }
    false
}
pub(super) fn json_has_program(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(value) => {
            script_text(value, false)
                || value.lines().map(str::trim).any(|line| {
                    let lower = line.to_ascii_lowercase();
                    [
                        "exec(",
                        "eval(",
                        "__import__(",
                        "subprocess.run(",
                        "os.system(",
                        "printf ",
                        "export ",
                        "function ",
                    ]
                    .iter()
                    .any(|prefix| lower.starts_with(prefix))
                        || line.starts_with('$') && (line.contains('=') || line.starts_with("$("))
                })
        }
        serde_json::Value::Array(values) => values.iter().any(json_has_program),
        serde_json::Value::Object(values) => values.values().any(json_has_program),
        _ => false,
    }
}
fn declaration_file(path: &str) -> bool {
    path == "assets/panel-globals.d.ts"
}
pub(super) fn browser(path: &str) -> bool {
    path.starts_with("assets/")
        && [".js", ".html", ".css"]
            .iter()
            .any(|suffix| path.ends_with(suffix))
        || declaration_file(path)
}
pub(super) fn worker(path: &str) -> bool {
    (path.starts_with("worker/src/") || path.starts_with("worker/test/")) && path.ends_with(".ts")
        || path.starts_with("worker/migrations/") && path.ends_with(".sql")
}
pub(super) fn markdown(path: &str) -> bool {
    path.ends_with(".md") || path == "runtime/licenses/notices.md.hbs"
}
pub(super) fn license(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name == "LICENSE"
        || name.starts_with("LICENSE.")
        || name == "NOTICE"
        || name.starts_with("NOTICE.")
}
pub(super) fn config(path: &str) -> bool {
    matches!(
        path,
        ".editorconfig"
            | ".env.example"
            | ".gitattributes"
            | ".gitignore"
            | ".graphifyignore"
            | "Cargo.lock"
            | "runtime/Cargo.lock"
            | "tools/ci/Cargo.lock"
            | "tools/test-inventory/Cargo.lock"
            | "rust-toolchain.toml"
    ) || path.ends_with("/.gitignore")
        || path.ends_with("/Cargo.toml")
        || path == "runtime/about.toml"
        || path == "worker/wrangler.toml"
        || path == "tests/runtime-tests.toml"
        || path.starts_with(".cargo/") && path.ends_with(".toml")
        || path.starts_with(".github/") && (path.ends_with(".yml") || path.ends_with(".yaml"))
}
pub(super) fn media(path: &str, bytes: &[u8]) -> bool {
    match Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
    {
        "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "ico" => {
            bytes.len() >= 6
                && bytes.starts_with(b"\x00\x00\x01\x00")
                && u16::from_le_bytes([bytes[4], bytes[5]]) >= 1
        }
        "jpg" | "jpeg" => bytes.starts_with(b"\xff\xd8\xff"),
        "gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
        "webm" => bytes.starts_with(b"\x1a\x45\xdf\xa3"),
        "tgs" => bytes.starts_with(b"\x1f\x8b"),
        "svg" => std::str::from_utf8(bytes).is_ok_and(|s| {
            let s = s.trim_start();
            (s.starts_with("<svg") || s.starts_with("<?xml"))
                && !s.to_ascii_lowercase().contains("<script")
        }),
        _ => false,
    }
}
pub(super) fn digest_receipt(text: &str) -> bool {
    !text.trim().is_empty()
        && text.lines().all(|line| {
            let Some((hash, path)) = line.split_once("  ") else {
                return false;
            };
            hash.len() == 64
                && hash.bytes().all(|b| b.is_ascii_hexdigit())
                && safe(path)
                && !path.chars().any(char::is_control)
        })
}
pub(super) fn package_scripts(path: &str, value: &serde_json::Value) -> Result<(), String> {
    if !matches!(path, "assets/package.json" | "worker/package.json") {
        return Ok(());
    }
    if let Some(scripts) = value.get("scripts") {
        let scripts = scripts
            .as_object()
            .ok_or("Node scripts must be a declarative command map")?;
        for script in scripts.values() {
            let command = script.as_str().ok_or("Node script command must be text")?;
            let executable = command.split_whitespace().next().unwrap_or("");
            let permitted = if path.starts_with("assets/") {
                ["playwright", "tsc", "cargo"].contains(&executable)
            } else {
                ["tsc", "vitest", "wrangler", "cargo"].contains(&executable)
            };
            if !permitted || command.chars().any(|c| ";&|`$\n\r".contains(c)) {
                return Err("Node configuration may call ready browser/Worker tools, not embed a project shell/interpreter launcher".into());
            }
        }
    }
    Ok(())
}
pub(super) fn manifest_binding(text: &str) -> bool {
    fn bindings(value: &toml_span::Value<'_>) -> bool {
        let Some(table) = value.as_table() else {
            return false;
        };
        table.iter().any(|(key, value)| {
            if matches!(
                key.name.as_ref(),
                "dependencies" | "dev-dependencies" | "build-dependencies"
            ) {
                value.as_table().is_some_and(|dependencies| {
                    dependencies.iter().any(|(name, definition)| {
                        let package = definition
                            .as_table()
                            .and_then(|table| table.get("package"))
                            .and_then(|value| value.as_str());
                        ["pyo3", "cpython", "rustpython", "rustpython-vm", "maturin"]
                            .iter()
                            .any(|binding| name.name == *binding || package == Some(binding))
                    })
                })
            } else {
                bindings(value)
            }
        })
    }
    toml_span::parse(text)
        .map(|value| bindings(&value))
        .unwrap_or(true)
}
pub(super) fn ci_program(text: &str) -> bool {
    let mut block = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        let trimmed = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        if trimmed.starts_with("shell:")
            && trimmed
                .split_whitespace()
                .nth(1)
                .is_some_and(|s| s.trim_matches(['\'', '"']).starts_with("python"))
        {
            return true;
        }
        if let Some(run) = trimmed.strip_prefix("run:") {
            if matches!(run.trim(), "|" | ">" | "|-" | ">-") {
                block = Some(indent);
                continue;
            }
            if interpreter_command(run.trim_matches([' ', '\'', '"'])) {
                return true;
            }
        } else if let Some(parent) = block {
            if !trimmed.is_empty() && indent <= parent {
                block = None;
            } else if interpreter_command(trimmed)
                || script_line(trimmed)
                || trimmed.starts_with("function ")
                || trimmed.contains("() {")
            {
                return true;
            }
        }
    }
    false
}
fn interpreter_command(line: &str) -> bool {
    let mut start = true;
    for word in line.split_whitespace() {
        if start && legacy_program(word.trim_matches(['\'', '"', '&'])) {
            return true;
        }
        start =
            matches!(word, "then" | "do" | ";" | "&&" | "||" | "exec" | "&") || word.ends_with(';');
    }
    false
}
