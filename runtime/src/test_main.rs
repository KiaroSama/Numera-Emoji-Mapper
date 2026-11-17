//! Developer-only runner for production tests; never included in release archives.
use numera_runtime::{logging, migration_bundle, process_owner, tooling_environment};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

#[cfg(target_os = "linux")]
#[path = "tooling_test_network_linux.rs"]
mod network;

#[cfg(all(windows, target_env = "msvc"))]
fn compiler_environment(environment: &mut BTreeMap<String, String>) -> Result<(), String> {
    let linker = Path::new(env!("NUMERA_BASELINE_MSVC_LINKER"));
    if !linker.is_absolute()
        || !linker.is_file()
        || !linker
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("link.exe"))
    {
        return Err("compiled MSVC linker is unavailable".into());
    }
    let libraries = env!("NUMERA_BASELINE_MSVC_LIB");
    let directories = std::env::split_paths(libraries).collect::<Vec<_>>();
    if directories.is_empty()
        || directories
            .iter()
            .any(|path| !path.is_absolute() || !path.is_dir())
    {
        return Err("compiled MSVC library directories are unavailable".into());
    }
    let original = environment
        .get("PATH")
        .ok_or("test compiler requires PATH")?;
    // Native x64 MSVC keeps its host DLLs beside this linker. Preserve the
    // already admitted PATH, including FFmpeg, after that one tool directory.
    let path = std::env::join_paths(
        std::iter::once(
            linker
                .parent()
                .ok_or("MSVC linker has no directory")?
                .to_path_buf(),
        )
        .chain(std::env::split_paths(original)),
    )
    .map_err(|_| "cannot construct isolated compiler PATH")?;
    let path = path
        .into_string()
        .map_err(|_| "isolated compiler PATH must be UTF-8")?;
    environment.insert("PATH".into(), path);
    environment.insert("LIB".into(), libraries.into());
    Ok(())
}

// Validate before preparing a workspace or starting a child. Keep every
// ordinary libtest argument as an OsString; only lower valid parallelism.
fn test_arguments(args: &[OsString]) -> Result<Vec<OsString>, String> {
    let mut result = args.to_vec();
    let mut present = false;
    let mut index = 0;
    while index < result.len() {
        let raw = result[index].clone();
        if raw == "--test-threads" {
            index += 1;
            let value = result
                .get_mut(index)
                .ok_or("--test-threads requires a value")?;
            value
                .to_str()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|count| *count > 0)
                .ok_or("--test-threads must be a positive integer")?;
            *value = "1".into();
            present = true;
        } else if let Some(value) = raw.to_string_lossy().strip_prefix("--test-threads=") {
            value
                .parse::<usize>()
                .ok()
                .filter(|count| *count > 0)
                .ok_or("--test-threads must be a positive integer")?;
            result[index] = "--test-threads=1".into();
            present = true;
        }
        index += 1;
    }
    if !present {
        result.push("--test-threads=1".into());
    }
    Ok(result)
}

fn execute(root: &Path, args: &[OsString]) -> Result<i32, String> {
    let physical_root = tooling_environment::physical_root(root)?;
    let root = physical_root.as_path();
    let executable = args
        .first()
        .ok_or("runtime test runner requires a test executable")?;
    tooling_environment::reject_links(
        Path::new(executable),
        "test executable cannot follow links",
    )?;
    // The shared resolver strips Windows' verbatim prefix after link admission;
    // both sides of the confinement check must use the same physical path form.
    let executable = migration_bundle::resolved(Path::new(executable))?;
    if !executable.starts_with(root) || !executable.is_file() {
        return Err("test executable must be a regular project-contained file".into());
    }
    let test_args = test_arguments(&args[1..])?;
    let mut environment = BTreeMap::new();
    for key in [
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "PATHEXT",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
        "DYLD_FALLBACK_LIBRARY_PATH",
        "LANG",
        "LC_ALL",
    ] {
        if let Ok(value) = std::env::var(key) {
            environment.insert(key.into(), value);
        }
    }
    #[cfg(all(windows, target_env = "msvc"))]
    compiler_environment(&mut environment)?;
    for (key, value) in [
        ("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1"),
        ("NUMERA_RUNTIME_TEST_ADMISSION", "isolated-v1"),
        ("TELEGRAM_API_BASE", "http://127.0.0.1:9"),
        ("BRAND_LOGO_BOTS", ""),
        ("COLLECTION_PACK_BASE", "YourBrand_Emoji_Packs"),
        ("COIN_PACK_BASE", "cryptoemoji"),
        ("COIN_PACK_TITLE", "@YourBrand Crypto Emoji"),
        ("RUST_BACKTRACE", "1"),
    ] {
        environment.insert(key.into(), value.into());
    }
    let environment = tooling_environment::environment(root, &environment, false)?;
    let parent = tooling_environment::confined_directory(root, "logs/tmp", true)?;
    let storage = parent.join(format!("runtime-tests-{}", std::process::id()));
    std::fs::create_dir(&storage).map_err(|e| e.to_string())?;
    let owner_root = storage.join("owner");
    std::fs::create_dir(&owner_root).map_err(|e| e.to_string())?;
    let stdout_path = storage.join("stdout");
    let stderr_path = storage.join("stderr");
    let create = |path| {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())
    };
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command =
            std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
        command.args([
            OsString::from("--test-net-bootstrap"),
            OsString::from("--"),
            executable.into_os_string(),
        ]);
        command
    };
    #[cfg(not(target_os = "linux"))]
    let mut command = std::process::Command::new(executable);
    command
        .args(test_args)
        .current_dir(root)
        .env_clear()
        .envs(environment)
        .stdin(Stdio::null())
        .stdout(Stdio::from(create(&stdout_path)?))
        .stderr(Stdio::from(create(&stderr_path)?));
    #[cfg(any(windows, target_os = "linux"))]
    let mut owner = process_owner::OwnedProcess::spawn_descendants(&mut command, owner_root)?;
    #[cfg(not(any(windows, target_os = "linux")))]
    let mut owner = process_owner::OwnedProcess::spawn(&mut command, owner_root)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let result = runtime.block_on(async {
        tokio::select! {
            result = owner.wait(Duration::from_secs(900), Duration::from_secs(180)) => result,
            error = async {
                loop {
                    for path in [&stdout_path, &stderr_path] {
                        if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 8 * 1024 * 1024 {
                            return Err("runtime test output exceeded capture budget".to_string());
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            } => error,
        }
    });
    owner.finish()?;
    for (path, stderr) in [(&stdout_path, false), (&stderr_path, true)] {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&bytes[..bytes.len().min(8 * 1024 * 1024)]);
        if stderr {
            eprint!("{text}");
        } else {
            print!("{text}");
        }
    }
    std::fs::remove_dir_all(storage).map_err(|e| e.to_string())?;
    Ok(result?.code().unwrap_or(1))
}

fn main() -> std::process::ExitCode {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("runtime project root");
    #[cfg(target_os = "linux")]
    if args
        .first()
        .is_some_and(|arg| arg == "--test-net-bootstrap")
    {
        // SAFETY: first dispatch of a fresh process, before threads, runtime or logging.
        let code = unsafe { network::run(root, &args[1..]) };
        return (code as u8).into();
    }
    let code = match execute(root, &args) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("ERROR: {}", logging::redact(&error, &[]));
            2
        }
    };
    #[cfg(windows)]
    std::process::exit(code);
    #[cfg(not(windows))]
    (code as u8).into()
}

#[cfg(test)]
#[path = "../tests/support/test_admission.rs"]
mod test_admission;

#[cfg(test)]
#[path = "supplemental_runner_argument_tests.rs"]
mod supplemental_arguments;
