use quote::ToTokens;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::{Meta, spanned::Spanned, visit::Visit};

pub fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
#[derive(Clone, Debug, Serialize)]
pub struct Function {
    pub file: String,
    pub target: String,
    pub function: String,
    pub source_line: usize,
    pub body_sha256: String,
    pub linux: bool,
    pub windows: bool,
    pub ignore_linux: Option<String>,
    pub ignore_windows: Option<String>,
    pub assertions: usize,
    pub calls: BTreeSet<String>,
    pub original_claims: BTreeSet<String>,
    pub nonempty: bool,
}
#[derive(Default, Debug, Serialize)]
pub struct Inventory {
    pub ci_context: TestContext,
    pub crate_roots: Vec<(String, String)>,
    pub functions: Vec<Function>,
    pub source_sha256: BTreeMap<String, String>,
    pub errors: Vec<String>,
}
/// The unfiltered CI command uses --release with Cargo's default features.
#[derive(Clone, Debug, Serialize)]
pub struct TestContext {
    pub profile: &'static str,
    pub debug_assertions: bool,
    pub default_features: BTreeSet<String>,
}
impl Default for TestContext {
    fn default() -> Self {
        Self {
            profile: "release",
            debug_assertions: false,
            default_features: BTreeSet::new(),
        }
    }
}
fn default_feature_closure(features: &BTreeMap<String, Vec<String>>) -> BTreeSet<String> {
    let mut active = BTreeSet::new();
    let mut pending = vec!["default".to_owned()];
    while let Some(feature) = pending.pop() {
        if !features.contains_key(&feature) || !active.insert(feature.clone()) {
            continue;
        }
        for enabled in &features[&feature] {
            if let Some((dependency, _)) = enabled.split_once('/') {
                if !dependency.ends_with('?') {
                    pending.push(dependency.to_owned());
                }
            } else if !enabled.starts_with("dep:") {
                pending.push(enabled.clone());
            }
        }
    }
    active
}
fn ci_context(root: &Path) -> Result<TestContext, String> {
    let text =
        std::fs::read_to_string(root.join("runtime/Cargo.toml")).map_err(|e| e.to_string())?;
    let value =
        toml_span::parse(&text).map_err(|_| "cannot parse runtime Cargo context".to_owned())?;
    let table = value
        .as_table()
        .ok_or("runtime Cargo context is not a table")?;
    let mut features = BTreeMap::new();
    if let Some(values) = table.get("features").and_then(|v| v.as_table()) {
        for (key, value) in values {
            let entries = value
                .as_array()
                .ok_or("Cargo feature is not an array")?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or("Cargo feature member is not text")
                })
                .collect::<Result<Vec<_>, _>>()?;
            features.insert(key.name.to_string(), entries);
        }
    }
    let mut result = TestContext {
        default_features: default_feature_closure(&features),
        ..TestContext::default()
    };
    if let Some(release) = table
        .get("profile")
        .and_then(|v| v.as_table())
        .and_then(|v| v.get("release"))
        .and_then(|v| v.as_table())
        && let Some(debug) = release.get("debug-assertions")
    {
        result.debug_assertions = debug
            .as_bool()
            .ok_or("release debug-assertions must be boolean")?;
    }
    Ok(result)
}
#[derive(Clone, Copy)]
pub(crate) enum Platform {
    Linux,
    Windows,
    Macos,
}
#[derive(Default)]
struct Facts {
    assertions: usize,
    calls: BTreeSet<String>,
}
impl<'a> Visit<'a> for Facts {
    fn visit_macro(&mut self, node: &'a syn::Macro) {
        if node.path.segments.last().is_some_and(|s| {
            ["assert", "assert_eq", "assert_ne", "panic"].contains(&s.ident.to_string().as_str())
        }) {
            self.assertions += 1;
        }
        syn::visit::visit_macro(self, node);
    }
    fn visit_expr_call(&mut self, node: &'a syn::ExprCall) {
        self.calls.insert(node.func.to_token_stream().to_string());
        syn::visit::visit_expr_call(self, node);
    }
    fn visit_expr_method_call(&mut self, node: &'a syn::ExprMethodCall) {
        self.calls.insert(format!(".{}", node.method));
        syn::visit::visit_expr_method_call(self, node);
    }
}
fn values(list: &syn::MetaList) -> Vec<Meta> {
    list.parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
        .map(|p| p.into_iter().collect())
        .unwrap_or_default()
}
fn string(meta: &Meta) -> Option<String> {
    let Meta::NameValue(value) = meta else {
        return None;
    };
    let syn::Expr::Lit(value) = &value.value else {
        return None;
    };
    let syn::Lit::Str(value) = &value.lit else {
        return None;
    };
    Some(value.value())
}
// Unknown custom predicates stay unknown through negation and cannot prove reachability.
fn predicate(meta: &Meta, platform: Platform, context: &TestContext) -> Option<bool> {
    match meta {
        Meta::Path(p) => match p.to_token_stream().to_string().as_str() {
            "test" => Some(true),
            "debug_assertions" => Some(context.debug_assertions),
            "unix" => Some(!matches!(platform, Platform::Windows)),
            "windows" => Some(matches!(platform, Platform::Windows)),
            "doc" | "doctest" | "miri" => Some(false),
            _ => None,
        },
        Meta::NameValue(p) => {
            let name = string(meta)?;
            let expected = match p.path.to_token_stream().to_string().as_str() {
                "target_os" => match platform {
                    Platform::Linux => "linux",
                    Platform::Windows => "windows",
                    Platform::Macos => "macos",
                },
                "target_arch" => "x86_64",
                "target_family" => {
                    if matches!(platform, Platform::Windows) {
                        "windows"
                    } else {
                        "unix"
                    }
                }
                "target_env" => match platform {
                    Platform::Linux => "gnu",
                    Platform::Windows => "msvc",
                    Platform::Macos => "",
                },
                "target_vendor" => match platform {
                    Platform::Linux => "unknown",
                    Platform::Windows => "pc",
                    Platform::Macos => "apple",
                },
                "target_pointer_width" => "64",
                "target_endian" => "little",
                "target_abi" => "",
                "feature" => return Some(context.default_features.contains(&name)),
                _ => return None,
            };
            Some(name == expected)
        }
        Meta::List(list) => {
            let args = values(list);
            let evaluated = args
                .iter()
                .map(|value| predicate(value, platform, context))
                .collect::<Vec<_>>();
            match list.path.to_token_stream().to_string().as_str() {
                "all" if evaluated.contains(&Some(false)) => Some(false),
                "all" if evaluated.iter().all(|value| *value == Some(true)) => Some(true),
                "any" if evaluated.contains(&Some(true)) => Some(true),
                "any" if evaluated.iter().all(|value| *value == Some(false)) => Some(false),
                "not" if evaluated.len() == 1 => evaluated[0].map(|value| !value),
                _ => None,
            }
        }
    }
}
fn effective(
    attrs: &[syn::Attribute],
    platform: Platform,
    context: &TestContext,
) -> Option<Vec<Meta>> {
    fn add(
        meta: &Meta,
        platform: Platform,
        context: &TestContext,
        result: &mut Vec<Meta>,
    ) -> Option<()> {
        if meta.path().is_ident("cfg_attr") {
            let Meta::List(list) = meta else {
                return None;
            };
            let args = values(list);
            if predicate(args.first()?, platform, context)? {
                for value in args.iter().skip(1) {
                    add(value, platform, context, result)?;
                }
            }
        } else {
            result.push(meta.clone());
        }
        Some(())
    }
    let mut result = Vec::new();
    for attr in attrs {
        add(&attr.meta, platform, context, &mut result)?;
    }
    Some(result)
}
fn test_meta(meta: &Meta) -> bool {
    meta.path().is_ident("test")
        || meta
            .path()
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>()
            == ["tokio", "test"]
}
pub(crate) fn test_attribute(attribute: &syn::Attribute) -> bool {
    fn potential(meta: &Meta) -> bool {
        test_meta(meta)
            || matches!(meta,Meta::List(list) if list.path.is_ident("cfg_attr") && values(list).iter().skip(1).any(potential))
    }
    potential(&attribute.meta)
}
pub(crate) fn test_enabled(
    attrs: &[syn::Attribute],
    platform: Platform,
    context: &TestContext,
) -> bool {
    effective(attrs, platform, context).is_some_and(|attrs| attrs.iter().any(test_meta))
}
pub(crate) fn admitted(
    attrs: &[syn::Attribute],
    platform: Platform,
    context: &TestContext,
) -> bool {
    effective(attrs, platform, context).is_some_and(|attrs| {
        attrs
            .iter()
            .filter(|m| m.path().is_ident("cfg"))
            .all(|m| match m {
                Meta::List(list) => list
                    .parse_args::<Meta>()
                    .is_ok_and(|p| predicate(&p, platform, context) == Some(true)),
                _ => false,
            })
    })
}
fn ignored(attrs: &[syn::Attribute], platform: Platform, context: &TestContext) -> Option<String> {
    effective(attrs, platform, context)?
        .iter()
        .find(|m| m.path().is_ident("ignore"))
        .map(|m| string(m).unwrap_or_default())
}

fn claims(lines: &[&str], start: usize, end: usize) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    for line in &lines[start..end] {
        let line = line.trim_start();
        if !line.starts_with("//") {
            continue;
        }
        // Only adjacent line comments are considered; strings and raw fixture bodies never enter here.
        for token in line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.')) {
            let token = token.trim_matches('.');
            let normalized = if token.starts_with("tests.test_") {
                token.to_owned()
            } else if token.starts_with("test_") && token.matches('.').count() >= 2 {
                format!("tests.{token}")
            } else {
                continue;
            };
            if normalized.split('.').count() == 4 {
                result.insert(normalized);
            }
        }
    }
    result
}
struct Parser {
    root: PathBuf,
    seen: BTreeSet<(PathBuf, String, String)>,
    output: Inventory,
}
impl Parser {
    fn file(
        &mut self,
        path: PathBuf,
        ns: Vec<String>,
        target: &str,
        mask: (bool, bool),
        directory_owned: bool,
    ) {
        let canonical = match path.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                self.output.errors.push(format!(
                    "cannot read wired Rust module {}: {error}",
                    path.display()
                ));
                return;
            }
        };
        if !canonical.starts_with(&self.root) {
            self.output.errors.push(format!(
                "wired module escapes project root: {}",
                canonical.display()
            ));
            return;
        }
        if !self
            .seen
            .insert((canonical.clone(), ns.join("::"), target.to_owned()))
        {
            return;
        }
        let text = match std::fs::read_to_string(&canonical) {
            Ok(v) => v,
            Err(error) => {
                self.output.errors.push(error.to_string());
                return;
            }
        };
        let relative = canonical
            .strip_prefix(&self.root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        self.output
            .source_sha256
            .insert(relative, digest(text.as_bytes()));
        match syn::parse_file(&text) {
            Ok(file) => {
                let mask = (
                    mask.0 && admitted(&file.attrs, Platform::Linux, &self.output.ci_context),
                    mask.1 && admitted(&file.attrs, Platform::Windows, &self.output.ci_context),
                );
                let parent = canonical.parent().unwrap();
                let directory =
                    if directory_owned || canonical.file_stem().is_some_and(|n| n == "mod") {
                        parent.to_path_buf()
                    } else {
                        parent.join(canonical.file_stem().unwrap())
                    };
                self.items(
                    &file.items,
                    &canonical,
                    (&directory, parent),
                    &text,
                    ns,
                    (target, mask),
                )
            }
            Err(error) => self
                .output
                .errors
                .push(format!("{}: {error}", canonical.display())),
        }
    }
    fn items(
        &mut self,
        items: &[syn::Item],
        path: &Path,
        directories: (&Path, &Path),
        text: &str,
        ns: Vec<String>,
        context: (&str, (bool, bool)),
    ) {
        let (target, mask) = context;
        let (module_directory, explicit_directory) = directories;
        let lines = text.lines().collect::<Vec<_>>();
        for item in items {
            match item {
                syn::Item::Fn(f) if f.attrs.iter().any(test_attribute) => {
                    let linux = mask.0
                        && admitted(&f.attrs, Platform::Linux, &self.output.ci_context)
                        && test_enabled(&f.attrs, Platform::Linux, &self.output.ci_context);
                    let windows = mask.1
                        && admitted(&f.attrs, Platform::Windows, &self.output.ci_context)
                        && test_enabled(&f.attrs, Platform::Windows, &self.output.ci_context);
                    if linux || windows {
                        let mut identity = ns.clone();
                        identity.push(f.sig.ident.to_string());
                        let start = f.span().start().line.saturating_sub(1);
                        let mut facts = Facts::default();
                        facts.visit_block(&f.block);
                        self.output.functions.push(Function {
                            file: path
                                .strip_prefix(&self.root)
                                .unwrap()
                                .to_string_lossy()
                                .replace('\\', "/"),
                            target: target.into(),
                            function: identity.join("::"),
                            source_line: f.span().start().line,
                            body_sha256: digest(f.block.to_token_stream().to_string().as_bytes()),
                            linux,
                            windows,
                            ignore_linux: ignored(
                                &f.attrs,
                                Platform::Linux,
                                &self.output.ci_context,
                            ),
                            ignore_windows: ignored(
                                &f.attrs,
                                Platform::Windows,
                                &self.output.ci_context,
                            ),
                            assertions: facts.assertions,
                            calls: facts.calls,
                            original_claims: {
                                let mut begin = start;
                                while begin > 0 {
                                    let previous = lines[begin - 1].trim();
                                    if previous.is_empty() || previous.starts_with("//") {
                                        begin -= 1;
                                    } else {
                                        break;
                                    }
                                }
                                claims(&lines, begin, start)
                            },
                            nonempty: !f.block.stmts.is_empty(),
                        });
                    }
                }
                syn::Item::Mod(module) => {
                    let mask = (
                        mask.0 && admitted(&module.attrs, Platform::Linux, &self.output.ci_context),
                        mask.1
                            && admitted(&module.attrs, Platform::Windows, &self.output.ci_context),
                    );
                    if mask.0 || mask.1 {
                        let mut child = ns.clone();
                        child.push(module.ident.to_string());
                        if let Some((_, items)) = &module.content {
                            self.items(
                                items,
                                path,
                                (
                                    &module_directory.join(module.ident.to_string()),
                                    &module_directory.join(module.ident.to_string()),
                                ),
                                text,
                                child,
                                (target, mask),
                            );
                        } else {
                            let explicit = module
                                .attrs
                                .iter()
                                .find(|a| a.path().is_ident("path"))
                                .and_then(|a| string(&a.meta));
                            let directory_owned = explicit.is_some();
                            let childpath = if let Some(explicit) = explicit {
                                explicit_directory.join(explicit)
                            } else {
                                let plain = module_directory.join(format!("{}.rs", module.ident));
                                if plain.exists() {
                                    plain
                                } else {
                                    module_directory
                                        .join(module.ident.to_string())
                                        .join("mod.rs")
                                }
                            };
                            self.file(childpath, child, target, mask, directory_owned);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
fn collect_context(
    root: &Path,
    roots: &[(PathBuf, String)],
    context: Result<TestContext, String>,
) -> Inventory {
    let mut parser = Parser {
        root: root.canonicalize().unwrap_or_else(|_| root.into()),
        seen: BTreeSet::new(),
        output: Inventory::default(),
    };
    match context {
        Ok(context) => parser.output.ci_context = context,
        Err(error) => parser.output.errors.push(error),
    }
    for (path, target) in roots {
        parser.output.crate_roots.push((
            path.strip_prefix(&parser.root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/"),
            target.clone(),
        ));
        parser.file(path.clone(), Vec::new(), target, (true, true), true);
    }
    parser
        .output
        .functions
        .sort_by(|a, b| (&a.target, &a.function, &a.file).cmp(&(&b.target, &b.function, &b.file)));
    parser.output
}
pub fn collect_targets(root: &Path, roots: &[(PathBuf, String)]) -> Inventory {
    collect_context(root, roots, ci_context(root))
}
pub struct CargoTargets {
    pub roots: Vec<(PathBuf, String)>,
    pub context: TestContext,
}
pub fn collect_cargo_targets(root: &Path, targets: &CargoTargets) -> Inventory {
    collect_context(root, &targets.roots, Ok(targets.context.clone()))
}

/// Fixture convenience API. Production CLI uses Cargo metadata's actual test targets.
pub fn collect(root: &Path) -> Inventory {
    let root = root.canonicalize().unwrap_or_else(|_| root.into());
    let mut roots = vec![(root.join("runtime/src/lib.rs"), "lib".into())];
    let mut extra_errors = Vec::new();
    let manifest = std::fs::read_to_string(root.join("runtime/Cargo.toml")).unwrap_or_default();
    if manifest
        .lines()
        .any(|l| l.replace(' ', "").starts_with("autotests=false"))
    {
        extra_errors.push("runtime Cargo disables automatic integration test discovery".into());
    }
    match std::fs::read_dir(root.join("runtime/tests")) {
        Ok(entries) => {
            for path in entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "rs"))
            {
                let target = path.file_stem().unwrap().to_string_lossy().into_owned();
                roots.push((path, target));
            }
        }
        Err(error) => extra_errors.push(error.to_string()),
    }
    roots.sort();
    let mut result = collect_targets(&root, &roots);
    result.errors.extend(extra_errors);
    result
}

pub fn cargo_targets(root: &Path) -> Result<CargoTargets, String> {
    let manifest = root
        .join("runtime/Cargo.toml")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let output = std::process::Command::new(
        std::env::var_os("CARGO")
            .or_else(|| option_env!("CARGO").map(Into::into))
            .unwrap_or_else(|| "cargo".into()),
    )
    .args([
        "metadata",
        "--locked",
        "--offline",
        "--no-deps",
        "--format-version=1",
        "--manifest-path",
    ])
    .arg(&manifest)
    .output()
    .map_err(|e| format!("cannot inspect actual Cargo test target wiring: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "actual Cargo target inventory failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let package = metadata["packages"]
        .as_array()
        .and_then(|packages| {
            packages.iter().find(|p| {
                p["manifest_path"].as_str().is_some_and(|p| {
                    Path::new(p).canonicalize().ok().as_deref() == Some(manifest.as_path())
                })
            })
        })
        .ok_or("runtime package missing from Cargo metadata")?;
    let mut context = ci_context(root)?;
    context.default_features = metadata_features(package)?;
    Ok(CargoTargets {
        roots: roots_from_metadata(package)?,
        context,
    })
}
fn metadata_features(package: &serde_json::Value) -> Result<BTreeSet<String>, String> {
    let features: BTreeMap<String, Vec<String>> = package
        .get("features")
        .map(|features| serde_json::from_value(features.clone()))
        .transpose()
        .map_err(|_| "invalid Cargo feature definitions")?
        .unwrap_or_default();
    Ok(default_feature_closure(&features))
}
fn roots_from_metadata(package: &serde_json::Value) -> Result<Vec<(PathBuf, String)>, String> {
    let mut roots = Vec::new();
    let active = metadata_features(package)?;
    for target in package["targets"]
        .as_array()
        .ok_or("Cargo package has no targets")?
    {
        if target["test"] != true {
            continue;
        }
        if let Some(required) = target.get("required-features") {
            let required = required
                .as_array()
                .ok_or("Cargo required-features is not an array")?;
            let mut enabled = true;
            for feature in required {
                enabled &= active.contains(
                    feature
                        .as_str()
                        .ok_or("Cargo required-feature is not text")?,
                );
            }
            if !enabled {
                continue;
            }
        }
        let kinds = target["kind"]
            .as_array()
            .ok_or("Cargo target has no kind")?;
        let kind = if kinds.iter().any(|k| k == "lib" || k == "rlib") {
            "lib"
        } else if kinds.iter().any(|k| k == "test") {
            "test"
        } else if kinds.iter().any(|k| k == "bin") {
            "bin"
        } else {
            continue;
        };
        let name = target["name"].as_str().ok_or("Cargo target has no name")?;
        let name = if kind == "lib" {
            "lib".into()
        } else if kind == "bin" {
            format!("bin:{name}")
        } else {
            name.into()
        };
        let path = PathBuf::from(
            target["src_path"]
                .as_str()
                .ok_or("Cargo target has no source path")?,
        );
        roots.push((path, name));
    }
    roots.sort();
    Ok(roots)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cargo_metadata_honors_actual_custom_paths_names_and_disabled_test_targets() {
        let roots=roots_from_metadata(&serde_json::json!({"targets":[
            {"name":"runtime","kind":["lib"],"test":true,"src_path":"/fixture/custom/library.rs"},
            {"name":"actual-name","kind":["test"],"test":true,"src_path":"/fixture/custom/check.rs"},
            {"name":"dead","kind":["test"],"test":false,"src_path":"/fixture/dead.rs"},
            {"name":"app","kind":["bin"],"test":true,"src_path":"/fixture/main.rs"}
        ]})).unwrap();
        assert_eq!(roots.len(), 3);
        assert!(roots.contains(&(
            PathBuf::from("/fixture/custom/check.rs"),
            "actual-name".into()
        )));
        assert!(roots.contains(&(PathBuf::from("/fixture/custom/library.rs"), "lib".into())));
    }
}
