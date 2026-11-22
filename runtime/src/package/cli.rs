use std::path::PathBuf;

pub fn run(root: &std::path::Path, command: &str, args: &[String]) -> i32 {
    if args == ["--help"] || args == ["-h"] {
        match command {
            "package-native" => println!(
                "Usage: numera-emoji package-native [--executable FILE] [--libraries DIR] [--out DIR] [--revision SHA] [--target PLATFORM]"
            ),
            "install-prebuilt" => println!(
                "Usage: numera-emoji install-prebuilt [--package-root DIR] --destination DIR"
            ),
            "verify-binary" => println!(
                "Usage: numera-emoji verify-binary --executable FILE --target PLATFORM [--logo FILE]"
            ),
            _ => println!("Usage: numera-emoji package-verify [--package-root DIR]"),
        }
        return 0;
    }
    let result = (|| -> Result<(), String> {
        let mut values = std::collections::BTreeMap::new();
        let allowed: &[&str] = match command {
            "package-native" => &[
                "--executable",
                "--libraries",
                "--out",
                "--revision",
                "--target",
            ],
            "install-prebuilt" => &["--package-root", "--destination"],
            "verify-binary" => &["--executable", "--target", "--logo"],
            _ => &["--package-root"],
        };
        let mut index = 0;
        while index < args.len() {
            let (name, attached) = args[index]
                .split_once('=')
                .map_or((args[index].as_str(), None), |(name, value)| {
                    (name, Some(value))
                });
            if !allowed.contains(&name) {
                return Err(format!("unknown {command} option: {name}"));
            }
            index += 1;
            let value = match attached {
                Some(value) => value,
                None => {
                    let value = args
                        .get(index)
                        .filter(|value| !value.starts_with("--"))
                        .ok_or_else(|| format!("{name} requires a value"))?;
                    index += 1;
                    value
                }
            };
            if value.is_empty() || values.insert(name, value).is_some() {
                return Err(format!("empty or duplicate {command} option: {name}"));
            }
        }
        let resolve = |value: &str| {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        };
        let package_root = values
            .get("--package-root")
            .map(|value| resolve(value))
            .unwrap_or_else(|| root.to_path_buf());
        match command {
            "package-native" => {
                let executable = match values.get("--executable") {
                    Some(value) => resolve(value),
                    None => std::env::current_exe().map_err(|error| error.to_string())?,
                };
                let libraries = values
                    .get("--libraries")
                    .map(|value| resolve(value))
                    .or_else(|| root.join("lib").is_dir().then(|| root.join("lib")));
                let output = values
                    .get("--out")
                    .map(|value| resolve(value))
                    .unwrap_or_else(|| root.join("dist"));
                let system = match std::env::consts::OS {
                    "windows" => "windows",
                    "linux" => "linux",
                    _ => return Err("unsupported package operating system".into()),
                };
                let arch = match std::env::consts::ARCH {
                    "x86_64" => "x64",
                    "aarch64" => "arm64",
                    _ => return Err("unsupported package CPU architecture".into()),
                };
                let default_target = format!("{system}-{arch}");
                let target = values.get("--target").copied().unwrap_or(&default_target);
                let revision = values.get("--revision").copied().unwrap_or("unknown");
                let archive = crate::package::package(
                    root,
                    &executable,
                    libraries.as_deref(),
                    &output,
                    revision,
                    target,
                )?;
                println!("{}", archive.display());
            }
            "install-prebuilt" => {
                let destination = values
                    .get("--destination")
                    .ok_or("--destination is required")?;
                let installed = crate::package::install(&package_root, &resolve(destination))?;
                println!("Installed standalone application: {}", installed.display());
            }
            "verify-binary" => {
                let owned = values
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                    .collect();
                crate::package::run_verify_binary_cli(&owned, &resolve)?;
                println!("[OK] Built binary architecture and embedded logo verified.");
            }
            _ => {
                crate::package::verify(&package_root)?;
                println!("[OK] Package contents match their manifest.");
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("ERROR: {}", crate::logging::redact(&error, &[]));
            2
        }
    }
}
