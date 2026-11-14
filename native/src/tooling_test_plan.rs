//! Pre-retirement public checkout PLAN snapshots; neither metadata nor hashes authenticate CI.
use crate::tooling_test_observation::manifest::{SMALL_LIMIT, read_file, relative};
use crate::tooling_test_observation::paths::{member_path, public_path};
use crate::{logging::RunLog, tooling_environment, tooling_process};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;
const PATHS: &[&str] = &[
    "native/src",
    "native/data",
    "native/tests",
    "runtime/src",
    "runtime/tests",
    "runtime/build.rs",
    "runtime/Cargo.toml",
    "runtime/Cargo.lock",
    "runtime/vendor/rlottie/src",
    "runtime/vendor/rlottie/inc",
    "runtime/vendor/rlottie/cmake",
    "runtime/vendor/rlottie/CMakeLists.txt",
    "runtime/vendor/rlottie/rlottie.expmap",
    "runtime/vendor/rlottie/rlottie.pc.in",
    "runtime/vendor/rlottie/COPYING",
    "runtime/vendor/rlottie/NUMERA-SOURCE.md",
    "runtime/vendor/rlottie/licenses",
    "tests",
    "emojikit",
    "coins",
    "scripts",
    "assets",
    "docs",
    "run.ps1",
    "run.sh",
    "README.md",
    "CHANGELOG.md",
    "LICENSE",
    "THIRD_PARTY_NOTICES.md",
    ".github/CONTRIBUTING.md",
    ".github/workflows",
    ".github/actions",
    ".github/dependabot.yml",
    "native/Cargo.toml",
    "native/Cargo.lock",
    ".cargo/tests.toml",
    ".cargo/runtime-tests.toml",
    ".env.example",
    "pyproject.toml",
    "ruff.toml",
    "requirements.txt",
    "requirements-coins.txt",
    "requirements-dev.txt",
    "requirements-lint.txt",
];
fn fail(message: &str) -> String {
    format!(
        "test-plan refused: {message}; correct public inputs/target selection and regenerate before Cargo; no coverage or CI acceptance inferred"
    )
}
fn text(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_control)
}
fn hex(value: &str, n: usize) -> bool {
    value.len() == n
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
struct Options {
    baseline: String,
    output: PathBuf,
    lib: bool,
    bins: Vec<String>,
    tests: Vec<String>,
}
fn arguments(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        baseline: String::new(),
        output: PathBuf::new(),
        lib: false,
        bins: vec![],
        tests: vec![],
    };
    let mut index = 0;
    let mut seen = BTreeSet::new();
    while index < args.len() {
        let key = args[index].as_str();
        index += 1;
        if key == "--lib" {
            if options.lib {
                return Err(fail("duplicate --lib"));
            }
            options.lib = true;
            continue;
        }
        if !matches!(key, "--baseline" | "--output" | "--bin" | "--test") {
            return Err(fail(
                "unsupported option; use --baseline REV --output PATH and exact --lib/--bin/--test selectors",
            ));
        }
        let value = args
            .get(index)
            .filter(|s| text(s) && !s.starts_with('-'))
            .ok_or_else(|| fail("option requires a control-free value"))?;
        index += 1;
        if !seen.insert((key, value))
            || (matches!(key, "--baseline" | "--output")
                && seen.iter().filter(|(k, _)| *k == key).count() > 1)
        {
            return Err(fail("duplicate option/target"));
        }
        match key {
            "--baseline" => options.baseline = value.clone(),
            "--output" => options.output = PathBuf::from(value.as_str()),
            _ => {
                if value
                    .chars()
                    .any(|c| matches!(c, '*' | '?' | '[' | ']' | '/' | '\\' | ':'))
                {
                    return Err(fail("target names must be exact, not globs/paths"));
                }
                if key == "--bin" {
                    options.bins.push(value.clone());
                } else {
                    options.tests.push(value.clone());
                }
            }
        }
    }
    if !hex(&options.baseline, 40)
        || options.output.as_os_str().is_empty()
        || options.output.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err(fail(
            "--baseline requires lowercase40hex and --output requires a new JSON path",
        ));
    }
    Ok(options)
}
fn select(
    root: &Path,
    metadata: &Value,
    options: &Options,
    files: &[Value],
) -> Result<(String, Vec<Value>), String> {
    if metadata["version"] != 1 || metadata.get("resolve").is_none_or(|v| !v.is_null()) {
        return Err(fail("expected Cargo metadata format1 --no-deps"));
    }
    let workspace = metadata["workspace_root"]
        .as_str()
        .ok_or_else(|| fail("metadata lacks workspace root"))?;
    if !Path::new(workspace).is_absolute()
        || tooling_environment::physical_root(Path::new(workspace))? != root.join("native")
    {
        return Err(fail("metadata workspace root is not native"));
    }
    let packages = metadata["packages"]
        .as_array()
        .filter(|a| a.len() <= 4096)
        .ok_or_else(|| fail("metadata requires bounded packages"))?;
    let mut selected = None;
    for package in packages {
        let path = package["manifest_path"]
            .as_str()
            .ok_or_else(|| fail("package lacks manifest_path"))?;
        if !Path::new(path).is_absolute() {
            return Err(fail("Cargo package paths must be absolute"));
        }
        if crate::tooling_environment::physical_root(
            Path::new(path)
                .parent()
                .ok_or_else(|| fail("package path has no parent"))?,
        )? == root.join("native")
            && selected.replace(package).is_some()
        {
            return Err(fail("multiple metadata packages bind native manifest"));
        }
    }
    let package = selected.ok_or_else(|| fail("metadata has no owning native package"))?;
    if relative(
        root,
        Path::new(package["manifest_path"].as_str().unwrap_or("")),
    )? != "native/Cargo.toml"
    {
        return Err(fail("metadata does not bind native/Cargo.toml"));
    }
    let id = package["id"]
        .as_str()
        .filter(|s| text(s))
        .ok_or_else(|| fail("metadata package ID missing"))?;
    if !metadata["workspace_members"]
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v == id))
    {
        return Err(fail("native package ID is not an actual workspace member"));
    }
    let all = package["targets"]
        .as_array()
        .filter(|a| a.len() <= 4096)
        .ok_or_else(|| fail("metadata targets missing or excessive"))?;
    let default = !options.lib && options.bins.is_empty() && options.tests.is_empty();
    let mut targets = Vec::new();
    let mut matched = BTreeSet::new();
    for target in all {
        let name = target["name"]
            .as_str()
            .filter(|s| text(s))
            .ok_or_else(|| fail("metadata target name missing"))?;
        let kind = target["kind"]
            .as_array()
            .filter(|a| !a.is_empty())
            .ok_or_else(|| fail("metadata target kind missing"))?;
        if !target["test"].is_boolean() {
            return Err(fail("metadata target test flag must be boolean"));
        }
        let bin = kind == &[json!("bin")];
        let integration = kind == &[json!("test")];
        let lib = kind
            .iter()
            .all(|v| matches!(v.as_str(), Some("lib" | "rlib" | "cdylib")));
        let wanted = if default {
            target["test"] == true && (lib || bin || integration)
        } else {
            (options.lib && lib)
                || (bin && options.bins.iter().any(|v| v == name))
                || (integration && options.tests.iter().any(|v| v == name))
        };
        if !wanted {
            continue;
        }
        if target
            .get("required-features")
            .is_some_and(|v| v.as_array().is_none_or(|a| !a.is_empty()))
            || target.get("harness") == Some(&json!(false))
        {
            return Err(fail(
                "feature-gated/custom harness targets are outside this finite PLAN",
            ));
        }
        let path = target["src_path"]
            .as_str()
            .ok_or_else(|| fail("target source path missing"))?;
        if !Path::new(path).is_absolute() {
            return Err(fail("target source path must be absolute"));
        }
        let source = relative(root, Path::new(path))?;
        member_path("target", &source)?;
        if !matched.insert((
            name.to_owned(),
            serde_json::to_string(kind).map_err(|_| fail("cannot serialize target kind"))?,
        )) {
            return Err(fail("duplicate Cargo target identity"));
        }
        let kinds = kind
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| fail("target kind must be text"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let reason = crate::tooling_test_cargo::noncoverage_reason(name, &kinds, &source, files);
        let mut row = json!({"name":name,"kind":kind,"src_path":source,"mode":if reason.is_some(){"noncoverage"}else{"observe"}});
        if let Some(reason) = reason {
            row["reason"] = json!(reason);
        }
        targets.push(row);
    }
    for (kind, names) in [("bin", &options.bins), ("test", &options.tests)] {
        if names.iter().any(|name| {
            !targets
                .iter()
                .any(|t| t["name"] == *name && t["kind"] == json!([kind]))
        }) {
            return Err(fail(
                "requested exact target not found in actual Cargo metadata",
            ));
        }
    }
    if targets.is_empty()
        || (options.lib
            && !targets.iter().any(|t| {
                t["kind"].as_array().is_some_and(|k| {
                    k.iter()
                        .all(|v| matches!(v.as_str(), Some("lib" | "rlib" | "cdylib")))
                })
            }))
    {
        return Err(fail("selection has no matching targets/library"));
    }
    Ok((id.into(), targets))
}
fn index(value: &str) -> Result<BTreeMap<String, String>, String> {
    if value.is_empty() || !value.ends_with('\0') {
        return Err(fail("Git stage inventory is empty or lacks NUL terminator"));
    }
    let mut files = BTreeMap::new();
    for record in value.split_terminator('\0') {
        let (header, name) = record
            .split_once('\t')
            .ok_or_else(|| fail("malformed Git stage record"))?;
        if !text(name)
            || name.contains(['\\', ':'])
            || name
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
        {
            return Err(fail("invalid Git inventory path"));
        }
        if !public_path(name) {
            continue;
        }
        let parts = header.split(' ').collect::<Vec<_>>();
        if parts.len() != 3
            || !matches!(parts[0], "100644" | "100755")
            || !hex(parts[1], 40)
            || parts[2] != "0"
        {
            return Err(fail("only unconflicted regular tracked files are admitted"));
        }
        if files.insert(name.into(), header.into()).is_some() || files.len() > 4096 {
            return Err(fail("duplicate or excessive public Git members"));
        }
    }
    Ok(files)
}
fn role(name: &str, entry: bool) -> Result<&'static str, String> {
    if entry {
        member_path("target", name)?;
        return Ok("target");
    }
    for role in ["source", "fixture", "config", "support", "target"] {
        if member_path(role, name).is_ok() {
            return Ok(role);
        }
    }
    Err(fail(
        "a relevant public input is outside the admitted closure policy; approve its exact role/path before PLAN generation",
    ))
}
fn closure(
    root: &Path,
    inventory: &BTreeMap<String, String>,
    targets: &[Value],
) -> Result<Vec<Value>, String> {
    let entries = targets
        .iter()
        .map(|t| {
            t["src_path"]
                .as_str()
                .ok_or_else(|| fail("selected target has no source"))
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if entries.iter().any(|p| !inventory.contains_key(*p)) {
        return Err(fail(
            "selected Cargo source is not tracked in the admitted Git inventory",
        ));
    }
    let mut files = Vec::new();
    let mut total = 0_u64;
    for name in inventory.keys() {
        let role = role(name, entries.contains(name.as_str()))
            .map_err(|error| format!("{error}: {name}"))?;
        let (hash, _, size) = read_file(&root.join(name), SMALL_LIMIT, false)?;
        total += size;
        if total > 64 * 1024 * 1024 {
            return Err(fail("public closure exceeds64MiB"));
        }
        files.push(json!({"role":role,"path":name,"sha256":hash}));
    }
    if ["source", "fixture"]
        .iter()
        .any(|r| !files.iter().any(|f| f["role"] == *r))
        || (!targets.is_empty() && !files.iter().any(|f| f["role"] == "target"))
        || [
            "native/Cargo.toml",
            "native/Cargo.lock",
            ".cargo/tests.toml",
        ]
        .iter()
        .any(|p| !inventory.contains_key(*p))
    {
        return Err(fail(
            "pre-retirement PLAN requires retained source, target, fixture and Cargo manifest/lock/runner config; retired-source schemas require separate approval",
        ));
    }
    Ok(files)
}
async fn command(
    root: &Path,
    program: &Path,
    args: &[&str],
    environment: &BTreeMap<String, String>,
    log: &RunLog,
) -> Result<String, String> {
    let argv = std::iter::once(program.as_os_str().to_owned())
        .chain(args.iter().map(|arg| OsString::from(*arg)))
        .collect::<Vec<_>>();
    let done = tooling_process::capture(
        root,
        &argv,
        environment,
        Duration::from_secs(30),
        Duration::from_secs(10),
        log,
    )
    .await?;
    if done.code != 0 {
        return Err(fail(
            "metadata/Git prerequisite command failed; inspect private operational diagnostics",
        ));
    }
    Ok(done.stdout)
}
/// One metadata call, no build/test scheduling. Caller must run original Cargo only after this snapshot.
pub(crate) async fn produce(root: &Path, args: &[String], log: &RunLog) -> Result<PathBuf, String> {
    let options = arguments(args)?;
    let root = tooling_environment::physical_root(root)?;
    let output = if options.output.is_absolute() {
        options.output.clone()
    } else {
        root.join(&options.output)
    };
    tooling_environment::reject_links(&output, "PLAN output cannot follow links")?;
    let parent = output
        .parent()
        .ok_or_else(|| fail("output needs a parent directory"))?;
    let rel = parent
        .strip_prefix(&root)
        .map_err(|_| fail("output escapes project"))?
        .to_str()
        .ok_or_else(|| fail("output must be UTF-8"))?
        .replace('\\', "/");
    if !(rel == "logs/tmp" || rel.starts_with("logs/tmp/"))
        || !output.extension().is_some_and(|x| x == "json")
        || output.exists()
    {
        return Err(fail("--output must be a new JSON file under logs/tmp"));
    }
    tooling_environment::confined_directory(&root, &rel, true)?;
    let original = std::env::vars_os().collect();
    let mut inherited = BTreeMap::new();
    for (key, value) in crate::tooling_package_cli::git_environment(&original) {
        let key = key
            .into_string()
            .map_err(|_| fail("prerequisite environment key is not UTF-8"))?;
        if [
            "PATH",
            "SYSTEMROOT",
            "WINDIR",
            "PATHEXT",
            "HOME",
            "USERPROFILE",
            "CARGO_HOME",
            "RUSTUP_HOME",
            "RUSTUP_TOOLCHAIN",
            "LD_LIBRARY_PATH",
            "DYLD_LIBRARY_PATH",
            "GIT_NO_LAZY_FETCH",
            "GIT_OPTIONAL_LOCKS",
            "GIT_TERMINAL_PROMPT",
        ]
        .iter()
        .any(|name| key.eq_ignore_ascii_case(name))
        {
            inherited.insert(
                key,
                value
                    .into_string()
                    .map_err(|_| fail("prerequisite environment value is not UTF-8"))?,
            );
        }
    }
    let environment = tooling_environment::environment(&root, &inherited, false)?;
    let git = crate::codec_process::executable("git")?;
    let cargo = crate::codec_process::executable("cargo")?;
    let head = crate::tooling_package_cli::head(&root).await?;
    let mut git_args = vec![
        "--no-replace-objects",
        "--literal-pathspecs",
        "ls-files",
        "--cached",
        "--stage",
        "-z",
        "--full-name",
        "--",
    ];
    git_args.extend(PATHS);
    let listed = command(&root, &git, &git_args, &environment, log).await?;
    let inventory = index(&listed)?;
    let before = closure(&root, &inventory, &[])?;
    let metadata = command(
        &root,
        &cargo,
        &[
            "metadata",
            "--no-deps",
            "--format-version=1",
            "--manifest-path",
            "native/Cargo.toml",
            "--locked",
            "--offline",
        ],
        &environment,
        log,
    )
    .await?;
    let value: Value =
        serde_json::from_str(&metadata).map_err(|_| fail("Cargo metadata is not JSON"))?;
    let (package, targets) = select(&root, &value, &options, &before)?;
    let files = closure(&root, &inventory, &targets)?;
    let previous = before
        .iter()
        .map(|f| (f["path"].clone(), f["sha256"].clone()))
        .collect::<Vec<_>>();
    let current = files
        .iter()
        .map(|f| (f["path"].clone(), f["sha256"].clone()))
        .collect::<Vec<_>>();
    if previous != current
        || command(&root, &git, &git_args, &environment, log).await? != listed
        || crate::tooling_package_cli::head(&root).await? != head
    {
        return Err(fail(
            "public input bytes/index/HEAD changed while preparing PLAN",
        ));
    }
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|_| fail("cannot allocate PLAN staging identity"))?;
    let id = random
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let stage = root.join("logs/tmp").join(format!("test-plan-{id}"));
    std::fs::create_dir(&stage).map_err(|_| fail("cannot create exclusive PLAN stage"))?;
    eprintln!("test plan inputs retained at {}", stage.display());
    let candidate = stage.join("plan.json");
    let plan = serde_json::to_vec(&json!({"schema":1,"target_revision":head,"baseline_revision":options.baseline,"package_id":package,"files":files,"targets":targets})).map_err(|_|fail("cannot serialize prebuild PLAN"))?;
    if plan.len() as u64 > SMALL_LIMIT {
        return Err(fail("prebuild PLAN exceeds8MiB"));
    }
    crate::atomic::write_bytes(&candidate, &plan)
        .map_err(|_| fail("cannot persist prebuild PLAN"))?;
    let receipt = json!({"schema":1,"trust":"untrusted","metadata_sha256":crate::codec_raster::digest_hex(metadata.as_bytes()),"index_sha256":crate::codec_raster::digest_hex(listed.as_bytes()),"limits":"pre-retirement public worktree snapshot; baseline membership, complete runtime closure and CI provenance not established"});
    crate::atomic::write_json(&stage.join("inputs.json"), &receipt)
        .map_err(|_| fail("cannot preserve prebuild input binding"))?;
    crate::atomic::publish_new(&candidate, &output)
        .map_err(|_| fail("PLAN publication failed; do not overwrite an earlier plan"))?;
    Ok(output)
}
#[cfg(test)]
#[path = "tooling_test_plan_tests.rs"]
mod tests;
