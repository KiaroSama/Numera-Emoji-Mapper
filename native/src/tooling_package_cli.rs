//! Public source-schema1 package tools; physical owning root, read-only Git provenance, no installation.
use crate::{
    config::Config, logging::RunLog, tooling_environment, tooling_manifest, tooling_package,
};
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

const PACKAGE_USAGE: &str = "usage: numera-emoji package-native [-h] --executable EXECUTABLE --wheel WHEEL --dependencies DEPENDENCIES [--out OUT]";

fn argument_error(message: &str) -> Arguments {
    eprintln!(
        "{PACKAGE_USAGE}\nnumera-emoji package-native: error: {}",
        crate::logging::redact(message, &[])
    );
    Arguments::Exit(2)
}
fn negative_path(value: &str) -> bool {
    let Some(number) = value.strip_prefix('-') else {
        return false;
    };
    let digits = |text: &str| !text.is_empty() && text.chars().all(crate::emoji_ids::decimal);
    digits(number)
        || number.split_once('.').is_some_and(|(whole, fraction)| {
            (whole.is_empty() || digits(whole)) && digits(fraction)
        })
}

/// Preserve the original path parser's prefixes, attached values, last-value
/// wins and help/error exits without starting the Python runtime.
pub fn arguments(root: &Path, args: &[String]) -> Result<Arguments, String> {
    const FLAGS: [&str; 5] = [
        "--help",
        "--executable",
        "--wheel",
        "--dependencies",
        "--out",
    ];
    let mut values = BTreeMap::new();
    let mut unknown = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let raw = &args[index];
        index += 1;
        if raw == "--" {
            unknown.extend(args[index..].iter().cloned());
            break;
        }
        let (name, attached) = raw
            .split_once('=')
            .map_or((raw.as_str(), None), |(name, value)| (name, Some(value)));
        let matches = FLAGS
            .iter()
            .copied()
            .filter(|flag| {
                name == *flag
                    || (name.len() > 2 && name.starts_with("--") && flag.starts_with(name))
            })
            .collect::<Vec<_>>();
        let flag = if name == "-h" {
            "--help"
        } else if matches.len() == 1 {
            matches[0]
        } else {
            unknown.push(raw.clone());
            continue;
        };
        if flag == "--help" {
            if attached.is_some() {
                return Ok(argument_error("argument --help: ignored explicit argument"));
            }
            println!(
                "{PACKAGE_USAGE}\n\nBuild a public source-schema1 archive; no installation or source build is performed.\n\noptions:\n  -h, --help            show this help message and exit\n  --executable EXECUTABLE\n  --wheel WHEEL\n  --dependencies DEPENDENCIES\n  --out OUT"
            );
            return Ok(Arguments::Exit(0));
        }
        let value = if let Some(value) = attached {
            value
        } else {
            let Some(value) = args.get(index).filter(|value| {
                !value.starts_with('-') || value.as_str() == "-" || negative_path(value)
            }) else {
                return Ok(argument_error(&format!(
                    "argument {flag}: expected one argument"
                )));
            };
            index += 1;
            value
        };
        values.insert(
            flag,
            PathBuf::from(if value.is_empty() { "." } else { value }),
        );
    }
    let required = ["--executable", "--wheel", "--dependencies"];
    let missing = required
        .iter()
        .filter(|flag| !values.contains_key(**flag))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Ok(argument_error(&format!(
            "the following arguments are required: {}",
            missing.join(", ")
        )));
    }
    if !unknown.is_empty() {
        return Ok(argument_error(&format!(
            "unrecognized arguments: {}",
            unknown.join(" ")
        )));
    }
    Ok(Arguments::Package(PackageOptions {
        executable: values.remove("--executable").expect("required argument"),
        wheel: values.remove("--wheel").expect("required argument"),
        dependencies: values.remove("--dependencies").expect("required argument"),
        output: values.remove("--out").unwrap_or_else(|| root.join("dist")),
    }))
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
