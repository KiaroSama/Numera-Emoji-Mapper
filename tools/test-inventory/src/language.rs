//! Repository language boundaries; this is distinct from original-case wiring/execution.
use crate::ast::digest;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use syn::visit::Visit;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vendor {
    pub root: String,
    pub upstream: String,
    pub revision: String,
    pub provenance: String,
    pub notice: String,
    pub files: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema: u32,
    pub vendor: Vendor,
    pub frozen_data: BTreeMap<String, String>,
}
#[derive(Clone, Debug)]
pub struct Candidate {
    pub path: String,
    pub executable: bool,
    pub git_link: bool,
}
#[derive(Debug, Serialize)]
pub struct Finding {
    pub path: String,
    pub rule: &'static str,
    pub message: String,
}
#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub schema: u32,
    pub checked_files: usize,
    pub categories: BTreeMap<String, usize>,
    pub findings: Vec<Finding>,
    pub proves_behavioral_retirement: bool,
}
impl Report {
    fn reject(&mut self, path: &str, rule: &'static str, message: impl Into<String>) {
        self.findings.push(Finding {
            path: path.into(),
            rule,
            message: message.into(),
        });
    }
    fn accept(&mut self, category: &str) {
        *self.categories.entry(category.into()).or_default() += 1;
    }
}
fn safe(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !Path::new(path).is_absolute()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
fn private(path: &str) -> bool {
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
fn physical_file(root: &Path, path: &str) -> Result<PathBuf, String> {
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
fn forbidden_suffix(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [
        ".py", ".pyw", ".pyc", ".pyo", ".ps1", ".psm1", ".psd1", ".sh", ".bash", ".zsh", ".fish",
        ".bat", ".cmd", ".c", ".cc", ".cpp", ".cxx", ".h", ".hpp", ".rb", ".pl", ".php", ".lua",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
}
fn legacy_metadata(path: &str) -> bool {
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
fn legacy_program(value: &str) -> bool {
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
fn script_line(line: &str) -> bool {
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
fn script_text(text: &str, markdown: bool) -> bool {
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
fn json_has_program(value: &serde_json::Value) -> bool {
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
fn browser(path: &str) -> bool {
    path.starts_with("assets/")
        && [".js", ".html", ".css"]
            .iter()
            .any(|suffix| path.ends_with(suffix))
        || declaration_file(path)
}
fn worker(path: &str) -> bool {
    (path.starts_with("worker/src/") || path.starts_with("worker/test/")) && path.ends_with(".ts")
        || path.starts_with("worker/migrations/") && path.ends_with(".sql")
}
fn markdown(path: &str) -> bool {
    path.ends_with(".md") || path == "runtime/licenses/notices.md.hbs"
}
fn license(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name == "LICENSE"
        || name.starts_with("LICENSE.")
        || name == "NOTICE"
        || name.starts_with("NOTICE.")
}
fn config(path: &str) -> bool {
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
        || path.starts_with(".cargo/") && path.ends_with(".toml")
        || path.starts_with(".github/") && (path.ends_with(".yml") || path.ends_with(".yaml"))
}
fn media(path: &str, bytes: &[u8]) -> bool {
    match Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
    {
        "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
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
fn digest_receipt(text: &str) -> bool {
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
fn package_scripts(path: &str, value: &serde_json::Value) -> Result<(), String> {
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
fn manifest_binding(text: &str) -> bool {
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
fn ci_program(text: &str) -> bool {
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

struct RustCode<'a> {
    bindings: Vec<BTreeMap<String, &'a syn::Expr>>,
    aliases: Vec<BTreeMap<String, String>>,
    violations: BTreeSet<&'static str>,
}
impl<'a> RustCode<'a> {
    fn push_scope(&mut self, items: impl IntoIterator<Item = &'a syn::Item>) {
        self.bindings.push(BTreeMap::new());
        self.aliases.push(BTreeMap::new());
        for item in items {
            match item {
                syn::Item::Const(value) => {
                    self.bindings
                        .last_mut()
                        .unwrap()
                        .insert(value.ident.to_string(), &value.expr);
                }
                syn::Item::Static(value)
                    if matches!(value.mutability, syn::StaticMutability::None) =>
                {
                    self.bindings
                        .last_mut()
                        .unwrap()
                        .insert(value.ident.to_string(), &value.expr);
                }
                syn::Item::Type(alias) => {
                    if let syn::Type::Path(path) = &*alias.ty {
                        self.aliases.last_mut().unwrap().insert(
                            alias.ident.to_string(),
                            path.path
                                .segments
                                .iter()
                                .map(|s| s.ident.to_string())
                                .collect::<Vec<_>>()
                                .join("::"),
                        );
                    }
                }
                syn::Item::Use(import) => self.import(&import.tree, String::new()),
                _ => {}
            }
        }
    }
    fn pop_scope(&mut self) {
        self.bindings.pop();
        self.aliases.pop();
    }
    fn command_type(&self, name: &str, depth: usize) -> bool {
        if depth > 12 {
            return false;
        }
        if name == "Command" || name.ends_with("process::Command") {
            return true;
        }
        self.aliases
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .is_some_and(|alias| self.command_type(alias, depth + 1))
    }
    fn values(&self, expr: &'a syn::Expr, depth: usize) -> Vec<String> {
        if depth > 12 {
            return Vec::new();
        }
        match expr {
            syn::Expr::Lit(e) => match &e.lit {
                syn::Lit::Str(s) => vec![s.value()],
                _ => Vec::new(),
            },
            syn::Expr::Reference(e) => self.values(&e.expr, depth + 1),
            syn::Expr::Paren(e) => self.values(&e.expr, depth + 1),
            syn::Expr::Group(e) => self.values(&e.expr, depth + 1),
            syn::Expr::Path(e) => {
                if let Some(name) = e.path.get_ident() {
                    if let Some(value) = self
                        .bindings
                        .iter()
                        .rev()
                        .find_map(|scope| scope.get(&name.to_string()))
                    {
                        return self.values(value, depth + 1);
                    }
                    return vec![name.to_string()];
                }
                Vec::new()
            }
            syn::Expr::Field(e) => match &e.member {
                syn::Member::Named(n) => vec![n.to_string()],
                _ => Vec::new(),
            },
            syn::Expr::Call(e) => e
                .args
                .iter()
                .flat_map(|e| self.values(e, depth + 1))
                .collect(),
            syn::Expr::MethodCall(e) => self
                .values(&e.receiver, depth + 1)
                .into_iter()
                .chain(e.args.iter().flat_map(|e| self.values(e, depth + 1)))
                .collect(),
            syn::Expr::Index(e) => self.first(&e.expr, depth + 1),
            _ => Vec::new(),
        }
    }
    fn first(&self, expr: &'a syn::Expr, depth: usize) -> Vec<String> {
        if depth > 12 {
            return Vec::new();
        }
        match expr {
            syn::Expr::Reference(e) => self.first(&e.expr, depth + 1),
            syn::Expr::Array(e) => e
                .elems
                .first()
                .map(|e| self.values(e, depth + 1))
                .unwrap_or_default(),
            syn::Expr::Path(e) => e
                .path
                .get_ident()
                .and_then(|n| {
                    self.bindings
                        .iter()
                        .rev()
                        .find_map(|s| s.get(&n.to_string()))
                })
                .map(|e| self.first(e, depth + 1))
                .unwrap_or_default(),
            syn::Expr::Macro(e) if e.mac.path.is_ident("vec") => {
                let parse = e.mac.parse_body_with(
                    syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated,
                );
                // Macro-owned nodes cannot borrow the original tree; only static strings are needed.
                parse
                    .ok()
                    .and_then(|values| {
                        values.first().and_then(|e| {
                            if let syn::Expr::Lit(e) = e
                                && let syn::Lit::Str(s) = &e.lit
                            {
                                return Some(vec![s.value()]);
                            }
                            None
                        })
                    })
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }
    fn check_program(&mut self, values: Vec<String>) {
        if values.iter().any(|v| {
            legacy_program(v)
                || matches!(
                    v.as_str(),
                    "PYO3_PYTHON" | "PYTHON_EXECUTABLE" | "POWERSHELL_EXE"
                )
        }) {
            self.violations
                .insert("actual Rust command hands execution to a retired language interpreter");
        }
    }
    fn import(&mut self, tree: &syn::UseTree, prefix: String) {
        let first = match tree {
            syn::UseTree::Path(p) => Some(&p.ident),
            syn::UseTree::Name(p) => Some(&p.ident),
            syn::UseTree::Rename(p) => Some(&p.ident),
            _ => None,
        };
        if prefix.is_empty()
            && first.is_some_and(|n| {
                matches!(
                    n.to_string().as_str(),
                    "pyo3" | "cpython" | "rustpython" | "rustpython_vm"
                )
            })
        {
            self.violations
                .insert("first-party Rust retains a Python interpreter binding");
        }
        match tree {
            syn::UseTree::Path(p) => self.import(&p.tree, format!("{prefix}{}::", p.ident)),
            syn::UseTree::Name(p) => {
                self.aliases
                    .last_mut()
                    .unwrap()
                    .insert(p.ident.to_string(), format!("{prefix}{}", p.ident));
            }
            syn::UseTree::Rename(p) => {
                self.aliases
                    .last_mut()
                    .unwrap()
                    .insert(p.rename.to_string(), format!("{prefix}{}", p.ident));
            }
            syn::UseTree::Group(p) => {
                for item in &p.items {
                    self.import(item, prefix.clone());
                }
            }
            syn::UseTree::Glob(_) => {}
        }
    }
}
impl<'a> Visit<'a> for RustCode<'a> {
    fn visit_file(&mut self, file: &'a syn::File) {
        self.push_scope(&file.items);
        syn::visit::visit_file(self, file);
        self.pop_scope();
    }
    fn visit_item_mod(&mut self, module: &'a syn::ItemMod) {
        if let Some((_, items)) = &module.content {
            self.push_scope(items);
            syn::visit::visit_item_mod(self, module);
            self.pop_scope();
        } else {
            syn::visit::visit_item_mod(self, module);
        }
    }
    fn visit_block(&mut self, block: &'a syn::Block) {
        self.push_scope(block.stmts.iter().filter_map(|stmt| {
            if let syn::Stmt::Item(item) = stmt {
                Some(item)
            } else {
                None
            }
        }));
        syn::visit::visit_block(self, block);
        self.pop_scope();
    }
    fn visit_local(&mut self, node: &'a syn::Local) {
        if let syn::Pat::Ident(name) = &node.pat
            && name.mutability.is_none()
            && let Some(initial) = &node.init
            && let Some(scope) = self.bindings.last_mut()
        {
            scope.insert(name.ident.to_string(), &initial.expr);
        }
        syn::visit::visit_local(self, node);
    }
    fn visit_item_use(&mut self, node: &'a syn::ItemUse) {
        self.import(&node.tree, String::new());
        syn::visit::visit_item_use(self, node);
    }
    fn visit_path(&mut self, path: &'a syn::Path) {
        if path.segments.first().is_some_and(|s| {
            matches!(
                s.ident.to_string().as_str(),
                "pyo3" | "cpython" | "rustpython" | "rustpython_vm"
            )
        }) {
            self.violations
                .insert("first-party Rust retains a Python interpreter binding");
        }
        syn::visit::visit_path(self, path);
    }
    fn visit_expr_call(&mut self, node: &'a syn::ExprCall) {
        if let syn::Expr::Path(path) = &*node.func {
            let names = path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>();
            let joined = names.join("::");
            let constructor = names.last().is_some_and(|s| s == "new")
                && names.len() > 1
                && self.command_type(&names[..names.len() - 1].join("::"), 0);
            if constructor && let Some(program) = node.args.first() {
                self.check_program(self.values(program, 0));
            }
            if joined.ends_with("tooling_process::capture")
                || joined.ends_with("tooling_process::capture_with_input")
            {
                if let Some(argv) = node.args.iter().nth(1) {
                    self.check_program(self.first(argv, 0));
                }
            } else if (joined.ends_with("codec_process::capture")
                || joined.ends_with("codec_process::capture_with_budget"))
                && let Some(program) = node.args.iter().nth(1)
            {
                self.check_program(self.values(program, 0));
            }
        }
        syn::visit::visit_expr_call(self, node);
    }
}
fn rust_violations(text: &str) -> Result<BTreeSet<&'static str>, String> {
    let file =
        syn::parse_file(text).map_err(|e| format!("cannot parse first-party Rust source: {e}"))?;
    let mut visitor = RustCode {
        bindings: Vec::new(),
        aliases: Vec::new(),
        violations: BTreeSet::new(),
    };
    visitor.visit_file(&file);
    Ok(visitor.violations)
}

pub fn git_candidates(root: &Path) -> Result<Vec<Candidate>, String> {
    fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
        let output = std::process::Command::new("git")
            .arg("--no-optional-locks")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .map_err(|_| "cannot invoke Git to enumerate candidate files")?;
        if !output.status.success() {
            return Err(
                "language policy requires a Git worktree; Git candidate enumeration failed".into(),
            );
        }
        Ok(output.stdout)
    }
    let mut paths = BTreeMap::new();
    for row in git(root, &["ls-files", "--cached", "--stage", "-z"])?
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
    {
        let text = std::str::from_utf8(row).map_err(|_| "candidate filename is not UTF-8")?;
        let (meta, path) = text.split_once('\t').ok_or("invalid Git index record")?;
        if root.join(path).symlink_metadata().is_ok() {
            paths.insert(
                path.to_owned(),
                (meta.starts_with("100755 "), meta.starts_with("120000 ")),
            );
        }
    }
    for row in git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
    {
        let path = std::str::from_utf8(row).map_err(|_| "candidate filename is not UTF-8")?;
        paths.entry(path.into()).or_insert((false, false));
    }
    Ok(paths
        .into_iter()
        .map(|(path, (executable, git_link))| Candidate {
            path,
            executable,
            git_link,
        })
        .collect())
}
fn vendor_receipts(root: &Path, policy: &Policy, paths: &BTreeSet<&str>, report: &mut Report) {
    let vendor = &policy.vendor;
    if policy.schema != 1
        || vendor.root != "runtime/vendor/rlottie"
        || vendor.upstream != "https://github.com/laggykiller/rlottie"
        || vendor.revision != "47b71a7e9072ac67b2e18c408d3668fed278fb93"
    {
        report.reject(
            "tools/test-inventory/language-policy.json",
            "policy",
            "unknown policy schema or unreviewed native vendor identity",
        );
    }
    for path in vendor.files.keys() {
        if !safe(path)
            || !path.starts_with(&(vendor.root.clone() + "/"))
            || !paths.contains(path.as_str())
        {
            report.reject(
                path,
                "vendor-receipt",
                "pinned third-party member is absent or outside its declared subset",
            );
        }
    }
    for required in [
        "COPYING",
        "NUMERA-SOURCE.md",
        "licenses/COPYING.MIT",
        "licenses/COPYING.MPL",
        "licenses/COPYING.SKIA",
        "licenses/COPYING.FTL",
        "licenses/COPYING.PIX",
        "licenses/COPYING.RPD",
        "licenses/COPYING.STB",
    ] {
        let path = format!("{}/{required}", vendor.root);
        if !vendor.files.contains_key(&path) {
            report.reject(
                &path,
                "vendor-license",
                "third-party license/provenance is not pinned",
            );
        }
    }
    for (path, needles) in [
        (
            &vendor.provenance,
            vec![vendor.upstream.as_str(), vendor.revision.as_str()],
        ),
        (&vendor.notice, vec!["rlottie", vendor.revision.as_str()]),
    ] {
        match physical_file(root,path).and_then(|p|std::fs::read_to_string(p).map_err(|_|"cannot read third-party notice".into())) {
            Ok(text) if needles.iter().all(|needle|text.contains(needle))=>{},
            _=>report.reject(path,"vendor-notice","native dependency requires its real upstream provenance and redistribution notices"),
        }
    }
}

pub fn check(root: &Path, policy: &Policy, candidates: &[Candidate]) -> Report {
    let mut report = Report {
        schema: 1,
        ..Report::default()
    };
    let mut seen = BTreeSet::new();
    let paths = candidates
        .iter()
        .map(|c| c.path.as_str())
        .collect::<BTreeSet<_>>();
    vendor_receipts(root, policy, &paths, &mut report);
    for candidate in candidates {
        let path = &candidate.path;
        if !seen.insert(path) {
            report.reject(path, "candidate", "duplicate candidate path");
            continue;
        }
        report.checked_files += 1;
        if private(path) {
            report.reject(
                path,
                "private-candidate",
                "ignored/private/build material must not be published; content was not read",
            );
            continue;
        }
        if candidate.git_link {
            report.reject(path,"physical-file","Git symlink entries are not regular source/data, including on checkouts that materialize their text");
            continue;
        }
        let physical = match physical_file(root, path) {
            Ok(p) => p,
            Err(e) => {
                report.reject(path, "physical-file", e);
                continue;
            }
        };
        let bytes = match std::fs::read(&physical) {
            Ok(v) => v,
            Err(_) => {
                report.reject(path, "unreadable", "cannot read candidate source/data");
                continue;
            }
        };
        if path.starts_with(&(policy.vendor.root.clone() + "/")) {
            if policy
                .vendor
                .files
                .get(path)
                .is_some_and(|expected| digest(&bytes) == *expected)
            {
                report.accept("pinned-third-party-native");
            } else {
                report.reject(path,"vendor-receipt","unlisted or modified vendor member; a directory is not a first-party code exemption");
            }
            continue;
        }
        if forbidden_suffix(path) || legacy_metadata(path) {
            report.reject(path,"first-party-language","retired interpreter/native application source or launcher metadata is not permitted");
            continue;
        }
        let text = std::str::from_utf8(&bytes).ok();
        if text.is_some_and(|s| {
            s.lines()
                .next()
                .is_some_and(|l| l.starts_with("#!") && !l.starts_with("#![") && script_line(l))
        }) {
            report.reject(
                path,
                "interpreter-shebang",
                "retired interpreter shebang is not data or Rust source",
            );
            continue;
        }
        if path.ends_with(".rs") || path.ends_with(".rs.in") {
            match text
                .ok_or("Rust source is not UTF-8".into())
                .and_then(rust_violations)
            {
                Ok(violations) if violations.is_empty() => report.accept("first-party-rust"),
                Ok(violations) => {
                    for issue in violations {
                        report.reject(path, "rust-interpreter-handoff", issue);
                    }
                }
                Err(error) => report.reject(path, "rust-source", error),
            }
            continue;
        }
        if candidate.executable {
            report.reject(
                path,
                "executable-artifact",
                "first-party non-Rust source/data must not be a committed executable launcher",
            );
            continue;
        }
        if browser(path) || worker(path) {
            if let Some(text) = text.filter(|s| !script_text(s, false)) {
                if !text.is_empty() {
                    report.accept(if browser(path) {
                        "browser-source"
                    } else {
                        "worker-source"
                    });
                } else {
                    report.reject(path, "empty-source", "approved-language source is empty");
                }
            } else {
                report.reject(
                    path,
                    "disguised-source",
                    "approved browser/Worker path contains another language's script",
                );
            }
            continue;
        }
        if markdown(path) || license(path) {
            if text.is_some_and(|s| !script_text(s, true)) {
                report.accept("documentation-or-license-template");
            } else {
                report.reject(path,"disguised-document","document contains an unfenced executable script rather than declarative prose/examples");
            }
            continue;
        }
        if let Some(expected) = policy.frozen_data.get(path) {
            if digest(&bytes) == *expected {
                report.accept("frozen-data-fixture");
            } else {
                report.reject(
                    path,
                    "frozen-data",
                    "approved data exception changed; review its independent fixture provenance",
                );
            }
            continue;
        }
        if path.ends_with(".json") {
            match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(value) if !json_has_program(&value)=>match package_scripts(path,&value) {
                    Ok(())=>report.accept("declarative-json"),Err(e)=>report.reject(path,"json-launcher",e)
                },
                Ok(_)=>report.reject(path,"json-program","new JSON contains executable source; only exact reviewed frozen parser fixtures may retain source text"),
                Err(_)=>report.reject(path,"json-data","JSON is malformed and has no reviewed frozen-data receipt"),
            }
            continue;
        }
        if config(path) {
            if let Some(text) = text {
                let bad = if path.starts_with(".github/") {
                    ci_program(text)
                } else {
                    script_text(text, false)
                        || path.ends_with("Cargo.toml") && manifest_binding(text)
                };
                if !bad {
                    report.accept("declarative-config");
                } else {
                    report.reject(path,"configuration-launcher","declarative configuration contains a retired interpreter/binding or embedded project script");
                }
            } else {
                report.reject(path, "configuration-data", "configuration is not UTF-8");
            }
            continue;
        }
        if path.ends_with(".csv") && text.is_some_and(|s| !script_text(s, false)) {
            report.accept("csv-data");
            continue;
        }
        if path.ends_with(".sha256") && text.is_some_and(digest_receipt) {
            report.accept("source-digest-receipt");
            continue;
        }
        if media(path, &bytes) {
            report.accept("media-data");
            continue;
        }
        report.reject(path,"unclassified-artifact","unrecognized first-party source/data type; do not rename executable source into a data extension");
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    pub(super) struct Tree {
        pub(super) root: PathBuf,
        pub(super) policy: Policy,
    }
    impl Tree {
        pub(super) fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "numera-language-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&root).unwrap();
            let result = std::process::Command::new("git")
                .args(["init", "--quiet"])
                .arg(&root)
                .output()
                .unwrap();
            assert!(result.status.success());
            let mut tree = Self {
                root,
                policy: Policy {
                    schema: 1,
                    vendor: Vendor {
                        root: "runtime/vendor/rlottie".into(),
                        upstream: "https://github.com/laggykiller/rlottie".into(),
                        revision: "47b71a7e9072ac67b2e18c408d3668fed278fb93".into(),
                        provenance: "runtime/vendor/rlottie/NUMERA-SOURCE.md".into(),
                        notice: "THIRD_PARTY_NOTICES.md".into(),
                        files: BTreeMap::new(),
                    },
                    frozen_data: BTreeMap::new(),
                },
            };
            for name in [
                "COPYING",
                "NUMERA-SOURCE.md",
                "licenses/COPYING.MIT",
                "licenses/COPYING.MPL",
                "licenses/COPYING.SKIA",
                "licenses/COPYING.FTL",
                "licenses/COPYING.PIX",
                "licenses/COPYING.RPD",
                "licenses/COPYING.STB",
                "src/renderer.cpp",
            ] {
                let path = format!("runtime/vendor/rlottie/{name}");
                let bytes = if name == "NUMERA-SOURCE.md" {
                    b"https://github.com/laggykiller/rlottie\n47b71a7e9072ac67b2e18c408d3668fed278fb93\n".as_slice()
                } else {
                    b"// approved third-party fixture\n".as_slice()
                };
                tree.put(&path, bytes);
                tree.policy.vendor.files.insert(path, digest(bytes));
            }
            tree.put(
                "THIRD_PARTY_NOTICES.md",
                b"# Notices\nrlottie 47b71a7e9072ac67b2e18c408d3668fed278fb93\n",
            );
            tree
        }
        pub(super) fn put(&self, path: &str, bytes: &[u8]) {
            let file = self.root.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, bytes).unwrap();
        }
        pub(super) fn run(&self) -> Report {
            check(
                &self.root,
                &self.policy,
                &git_candidates(&self.root).unwrap(),
            )
        }
        fn rule(&self, name: &str) -> bool {
            self.run().findings.iter().any(|f| f.rule == name)
        }
    }
    impl Drop for Tree {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.root).unwrap();
        }
    }
    #[test]
    fn real_candidate_tree_accepts_rust_browser_worker_and_declarative_data() {
        let tree = Tree::new();
        for (file, text) in [
            ("runtime/src/main.rs", "fn main() {}"),
            ("tools/ci/src/main.rs", "fn main() {}"),
            ("assets/panel-grid.js", "export const x = 1;"),
            (
                "assets/tests/page.spec.js",
                "import { test } from '@playwright/test';",
            ),
            ("assets/panel.html", "<!doctype html><main></main>"),
            ("assets/panel.css", "main { color: red; }"),
            (
                "assets/panel-globals.d.ts",
                "interface Window { fixture: string; }",
            ),
            (
                "worker/src/index.ts",
                "export default { fetch(): Response { return new Response('ok'); } };",
            ),
            ("worker/test/input.test.ts", "const x: number = 1;"),
            (
                "worker/migrations/001.sql",
                "CREATE TABLE messages(id INTEGER);",
            ),
            ("runtime/data/python311-decimal.json", "[[48,57]]"),
            (
                "tests/fixtures/receipt.json",
                "{\"source\":\"tests/test_original.py\",\"sha256\":\"abc\"}",
            ),
            ("coins/keywords.csv", "ticker,keywords\nbtc,coin"),
            ("runtime/Cargo.toml", "[package]\nname='fixture'\n"),
            (
                "runtime/tests/fixture.rs.in",
                "#[test] fn actual_rust_fixture() { assert_eq!(1, 1); }",
            ),
            (
                "tests/fixtures/source.sha256",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  tests/original.py\n",
            ),
            (
                "assets/package.json",
                "{\"scripts\":{\"test\":\"playwright test\"}}",
            ),
            (
                "worker/package.json",
                "{\"scripts\":{\"test\":\"vitest run\"}}",
            ),
            (
                "runtime/licenses/notices.md.hbs",
                "# Notices\n{{#each crates}}\n{{package.name}}\n{{/each}}\n",
            ),
        ] {
            tree.put(file, text.as_bytes());
        }
        let report = tree.run();
        assert!(report.findings.is_empty(), "{:?}", report.findings);
        assert_eq!(report.categories["first-party-rust"], 3);
        assert!(!report.proves_behavioral_retirement);
    }
    #[test]
    fn legacy_extensions_are_rejected_even_inside_fixture_browser_or_worker_directories() {
        let tree = Tree::new();
        for file in [
            "runtime/main.py",
            "assets/test.ps1",
            "worker/scripts/deploy.sh",
            "tests/fixtures/example.py",
            "tools/check.cmd",
        ] {
            tree.put(file, b"# intentionally forbidden source\n");
        }
        let report = tree.run();
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|f| f.rule == "first-party-language")
                .count(),
            5
        );
    }
    #[test]
    fn renamed_python_powershell_and_shell_programs_are_not_treated_as_data() {
        let tree = Tree::new();
        for (file, text) in [
            (
                "tests/fixtures/oracle.json",
                "from pathlib import Path\ndef main():\n    pass\n",
            ),
            ("tools/launcher.txt", "param($Token)\nWrite-Host $Token"),
            ("runtime/payload.data", "#!/bin/sh\necho application\n"),
            (
                "docs/renamed.md",
                "# old app\nimport os\ndef main():\n    pass\n",
            ),
        ] {
            tree.put(file, text.as_bytes());
        }
        let report = tree.run();
        for file in [
            "tests/fixtures/oracle.json",
            "tools/launcher.txt",
            "runtime/payload.data",
            "docs/renamed.md",
        ] {
            assert!(report.findings.iter().any(|f| f.path == file), "{file}");
        }
        tree.put("runtime/template.rs.in", b"import os\nprint('hidden app')");
        tree.put(
            "tests/fixtures/payload.sha256",
            b"#!/bin/sh\necho hidden app\n",
        );
        assert!(tree.rule("rust-source"));
        assert!(tree.rule("interpreter-shebang"));
    }
    #[test]
    fn executable_json_payload_is_refused_but_exact_reviewed_parser_fixture_is_preserved() {
        let mut tree = Tree::new();
        for source in [
            r#"{"script":"$name = 'synthetic'; Write-Host $name"}"#,
            r#"{"script":"printf synthetic"}"#,
            r#"{"script":"exec('print(1)')"}"#,
        ] {
            tree.put("tests/fixtures/parser.json", source.as_bytes());
            assert!(tree.rule("json-program"), "{source}");
        }
        let source = br#"{"source":"import os\ndef fixture():\n    pass\n"}"#;
        tree.put("tests/fixtures/parser.json", source);
        assert!(tree.rule("json-program"));
        tree.policy
            .frozen_data
            .insert("tests/fixtures/parser.json".into(), digest(source));
        assert!(tree.run().findings.is_empty());
        tree.put(
            "tests/fixtures/parser.json",
            br#"{"source":"import subprocess\ndef application():\n    pass\n"}"#,
        );
        assert!(tree.rule("frozen-data"));
    }
    #[test]
    fn frozen_corrupt_media_and_nonfinite_json_remain_test_data_without_an_execution_exception() {
        let mut tree = Tree::new();
        for (path, bytes) in [
            ("tests/fixtures/nonfinite.json", b"{\"x\":NaN}".as_slice()),
            ("tests/fixtures/corrupt.tgs", b"not gzip".as_slice()),
        ] {
            tree.put(path, bytes);
            tree.policy.frozen_data.insert(path.into(), digest(bytes));
        }
        assert!(tree.run().findings.is_empty());
        tree.put("tests/fixtures/disguised.py", b"print('application')");
        tree.policy.frozen_data.insert(
            "tests/fixtures/disguised.py".into(),
            digest(b"print('application')"),
        );
        assert!(tree.rule("first-party-language"));
    }
    #[test]
    fn documentation_code_fences_do_not_become_active_script_oracle_files() {
        let tree = Tree::new();
        tree.put(
            "README.md",
            b"# Docs\n```python\nimport os\ndef example():\n    pass\n```\nPlain explanation.\n",
        );
        assert!(tree.run().findings.is_empty());
        tree.put(
            "docs/raw.md",
            b"# Looks like docs\nfrom pathlib import Path\n",
        );
        assert!(tree.rule("disguised-document"));
    }
    #[test]
    fn real_rust_interpreter_handoffs_and_bindings_fail_but_quoted_rejection_examples_do_not() {
        let tree = Tree::new();
        tree.put("runtime/src/good.rs",br##"fn fixture(){ let historical="tests/reference/catalog.py";let rejected=r#"use pyo3::Python; Command::new(\"python\")"#; assert!(!historical.is_empty() && !rejected.is_empty()); }"##);
        assert!(tree.run().findings.is_empty());
        tree.put("runtime/src/bad.rs",b"use std::process::Command as Process;fn main(){let executable=\"python3\";let _=Process::new(executable);}");
        assert!(tree.rule("rust-interpreter-handoff"));
        tree.put("runtime/src/bad.rs", b"use pyo3::prelude::*; fn main(){} ");
        assert!(tree.rule("rust-interpreter-handoff"));
        tree.put(
            "runtime/Cargo.toml",
            b"[target.'cfg(windows)'.dependencies]\nbridge={package='pyo3',version='1'}\n",
        );
        assert!(tree.rule("configuration-launcher"));
        tree.put(
            "runtime/Cargo.toml",
            b"[package.metadata]\nold_binding='pyo3'\n",
        );
        assert!(!tree.rule("configuration-launcher"));
    }
    #[test]
    fn indexed_argument_vectors_and_owned_capture_cannot_hide_an_interpreter() {
        let tree = Tree::new();
        tree.put("runtime/src/main.rs",b"fn main(){let argv=vec![\"powershell.exe\",\"-File\",\"payload.txt\"];let _=std::process::Command::new(&argv[0]);}");
        assert!(tree.rule("rust-interpreter-handoff"));
        tree.put("runtime/src/main.rs",b"fn main(){let argv=[\"python\",\"oracle.data\"];tooling_process::capture(root,&argv,env,wall,idle,log);}");
        assert!(tree.rule("rust-interpreter-handoff"));
    }
    #[test]
    fn real_const_static_and_command_type_aliases_cannot_hide_a_retired_interpreter() {
        let tree = Tree::new();
        for source in [
            "const EXEC:&str=\"python3\";fn main(){let _=std::process::Command::new(EXEC);}",
            "fn main(){let _=std::process::Command::new(EXEC);}static EXEC:&str=\"powershell.exe\";",
            "type Runner=std::process::Command;fn main(){let _=Runner::new(\"python3\");}",
            "use std::process::Command as Process;type Runner=Process;fn main(){const EXEC:&str=\"sh\";let _=Runner::new(EXEC);}",
            "const ARGV:[&str;2]=[\"python\",\"oracle.data\"];fn main(){tooling_process::capture(root,&ARGV,env,wall,idle,log);}",
        ] {
            tree.put("runtime/src/main.rs", source.as_bytes());
            assert!(tree.rule("rust-interpreter-handoff"), "{source}");
        }
        tree.put("runtime/src/main.rs",b"const EXEC:&str=\"historical python path\";type Runner=std::process::Command;fn main(){let EXEC=\"cargo\";let _=Runner::new(EXEC);}");
        assert!(tree.run().findings.is_empty());
    }
    #[test]
    fn vendor_is_an_exact_licensed_subset_not_a_directory_wide_language_bypass() {
        let tree = Tree::new();
        assert!(tree.run().findings.is_empty());
        tree.put(
            "runtime/vendor/rlottie/launcher.py",
            b"print('application')",
        );
        assert!(tree.rule("vendor-receipt"));
        std::fs::remove_file(tree.root.join("runtime/vendor/rlottie/launcher.py")).unwrap();
        tree.put(
            "runtime/vendor/rlottie/src/renderer.cpp",
            b"// changed source",
        );
        assert!(tree.rule("vendor-receipt"));
    }
    #[test]
    fn missing_native_license_or_real_redistribution_notice_is_rejected() {
        let tree = Tree::new();
        std::fs::remove_file(tree.root.join("runtime/vendor/rlottie/COPYING")).unwrap();
        assert!(tree.rule("vendor-receipt"));
        tree.put("THIRD_PARTY_NOTICES.md", b"# no vendor notice\n");
        assert!(tree.rule("vendor-notice"));
    }
    #[test]
    fn browser_and_worker_exceptions_do_not_allow_unrelated_first_party_javascript_or_typescript() {
        let tree = Tree::new();
        for path in [
            "tools/helper.js",
            "runtime/src/logic.ts",
            "worker/scripts/admin.ts",
            "assets/backend.ts",
        ] {
            tree.put(path, b"const x = 1;\n");
        }
        assert_eq!(
            tree.run()
                .findings
                .iter()
                .filter(|f| f.rule == "unclassified-artifact")
                .count(),
            4
        );
    }
    #[test]
    fn declarative_node_and_ci_configuration_cannot_launch_old_project_scripts() {
        let tree = Tree::new();
        tree.put(
            "assets/package.json",
            br#"{"scripts":{"test":"python tests/oracle.data"}}"#,
        );
        assert!(tree.rule("json-launcher"));
        tree.put(
            "assets/package.json",
            br#"{"scripts":{"test":"playwright test"}}"#,
        );
        tree.put(".github/workflows/ci.yml",b"name: CI\njobs:\n  check:\n    steps:\n      - run: |\n          python3 oracle.txt\n");
        assert!(tree.rule("configuration-launcher"));
        tree.put(".github/workflows/ci.yml",b"name: CI\njobs:\n  check:\n    steps:\n      - shell: pwsh\n        run: |\n          rustup default 1.100.0\n      - shell: bash\n        run: cargo test --locked\n");
        assert!(tree.run().findings.is_empty());
    }
    #[test]
    fn ignored_private_files_are_not_candidates_and_published_private_paths_are_not_read() {
        let tree = Tree::new();
        tree.put(".gitignore", b"private/\n.env\n");
        tree.put("private/secret.txt", b"not real credentials");
        tree.put(".env", b"not real credentials");
        assert!(tree.run().findings.is_empty());
        let report = check(
            &tree.root,
            &tree.policy,
            &[Candidate {
                path: ".env".into(),
                executable: false,
                git_link: false,
            }],
        );
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule == "private-candidate")
        );
        let report = check(
            &tree.root,
            &tree.policy,
            &[Candidate {
                path: "../outside.json".into(),
                executable: false,
                git_link: false,
            }],
        );
        assert!(report.findings.iter().any(|f| f.rule == "physical-file"));
    }
    #[test]
    #[cfg_attr(not(unix), ignore = "Unix symlink fixture permissions")]
    fn linked_candidate_cannot_escape_into_an_unread_private_tree() {
        #[cfg(unix)]
        {
            let tree = Tree::new();
            tree.put("outside-private.txt", b"not real credentials");
            std::os::unix::fs::symlink(
                tree.root.join("outside-private.txt"),
                tree.root.join("README.md"),
            )
            .unwrap();
            assert!(tree.rule("physical-file"));
        }
    }
}

#[cfg(test)]
mod index_controls {
    use super::*;
    #[test]
    fn git_link_and_executable_modes_cannot_disappear_on_a_regular_file_checkout() {
        let tree = super::tests::Tree::new();
        let root = &tree.root;
        assert!(tree.run().findings.is_empty());
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap()
        };
        git(&["init", "--quiet"]);
        std::fs::write(root.join("README.md"), b"outside target text").unwrap();
        let hash = git(&["hash-object", "-w", "--no-filters", "README.md"]);
        git(&[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("120000,{},README.md", hash.trim()),
        ]);
        let rows = git_candidates(root).unwrap();
        assert!(rows.iter().any(|c| c.path == "README.md" && c.git_link));
        let policy = &tree.policy;
        assert!(
            check(root, policy, &rows)
                .findings
                .iter()
                .any(|f| f.path == "README.md" && f.rule == "physical-file")
        );
        std::fs::write(root.join("payload.json"), b"{}").unwrap();
        let hash = git(&["hash-object", "-w", "--no-filters", "payload.json"]);
        git(&[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100755,{},payload.json", hash.trim()),
        ]);
        let rows = git_candidates(root).unwrap();
        assert!(
            rows.iter()
                .any(|c| c.path == "payload.json" && c.executable)
        );
        assert!(
            check(root, policy, &rows)
                .findings
                .iter()
                .any(|f| f.path == "payload.json" && f.rule == "executable-artifact")
        );
    }
}

pub fn default_policy() -> Result<Policy, String> {
    serde_json::from_str(include_str!("../language-policy.json")).map_err(|e| e.to_string())
}
