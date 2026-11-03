//! Additive selected-venv probe; production initialization stays unchanged until package acceptance.
#![allow(unsafe_code)]
use crate::{logging::RunLog, tooling_environment, tooling_process};
use pyo3::prelude::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

#[cfg(windows)]
type Wide = u16;
#[cfg(not(windows))]
type Wide = i32;
static PROGRAM: OnceLock<Vec<Wide>> = OnceLock::new();
const RUNTIME_KEYS: &[&str] = &[
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
];

#[path = "tooling_runtime_selection.rs"]
mod runtime_selection;
pub(crate) use runtime_selection::sysconfig_paths;
pub use runtime_selection::{Selection, selection};

pub(crate) fn selected(
    root: &Path,
    requested: Option<&Path>,
    inherited: &BTreeMap<String, String>,
) -> Result<Selection, String> {
    match requested {
        Some(path) => runtime_selection::explicit(root, path, inherited),
        None => selection(root),
    }
}

pub fn environment(original: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut selected = original
        .iter()
        .filter_map(|(key, value)| {
            let name = RUNTIME_KEYS.iter().find(|name| {
                if cfg!(windows) {
                    key.eq_ignore_ascii_case(name)
                } else {
                    key.as_str() == **name
                }
            })?;
            Some(((*name).to_string(), value.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    for key in [
        "NUMERA_EMOJI_MAPPER_NO_DOTENV",
        "PYTHONUTF8",
        "PYTHONNOUSERSITE",
        "PYTHONSAFEPATH",
        "PYTHONDONTWRITEBYTECODE",
    ] {
        selected.insert(key.into(), "1".into());
    }
    selected
}

fn initialize(selected: &Selection) -> Result<(), String> {
    // This function is reached only by the fresh, single-threaded private probe.
    // No runtime/extension caller can replace an already initialized interpreter.
    // SAFETY: preinitialization status query, with no borrowed Python object.
    if unsafe { pyo3::ffi::Py_IsInitialized() } != 0 {
        return Err(
            "selected runtime must be configured before the first Python attachment".into(),
        );
    }
    // SAFETY: Py_GetVersion is available before initialization and returns immutable static storage.
    let linked = unsafe { pyo3::ffi::Py_GetVersion() };
    if linked.is_null() {
        return Err("cannot inspect linked Python version".into());
    }
    // SAFETY: the Python C API promises this non-null pointer is a NUL-terminated static string.
    let linked = unsafe { std::ffi::CStr::from_ptr(linked) }
        .to_str()
        .map_err(|_| "linked Python version is not UTF-8")?;
    let linked = linked
        .split_whitespace()
        .next()
        .unwrap_or("")
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "linked Python version is not a supported stable release")?;
    if linked.as_slice() != selected.version {
        return Err("package pyvenv.cfg does not match the linked Python version".into());
    }
    #[cfg(windows)]
    let mut program = {
        use std::os::windows::ffi::OsStrExt;
        selected
            .executable
            .as_os_str()
            .encode_wide()
            .collect::<Vec<_>>()
    };
    #[cfg(not(windows))]
    let mut program = selected
        .executable
        .to_str()
        .ok_or("package interpreter path must be UTF-8")?
        .chars()
        .map(|value| value as i32)
        .collect::<Vec<_>>();
    if program.contains(&0) {
        return Err("package interpreter path contains NUL".into());
    }
    program.push(0);
    PROGRAM
        .set(program)
        .map_err(|_| "selected runtime program name is already configured")?;
    let program = PROGRAM
        .get()
        .ok_or("selected runtime program name is unavailable")?;
    // abi3-py311 excludes PyConfig. This backward-compatible stable API is the narrow
    // exception; static storage remains alive for the entire interpreter lifetime.
    #[allow(deprecated)]
    // SAFETY: before initialization, supported platform wchar_t, immutable static NUL-terminated storage.
    unsafe {
        pyo3::ffi::Py_SetProgramName(program.as_ptr());
    }
    Python::initialize();
    Ok(())
}

fn observe(selected: &Selection, codecs: bool) -> Result<Value, String> {
    Python::attach(|py| -> Result<Value, String> {
        let inspect = || -> PyResult<Value> {
            let sys = py.import("sys")?;
            let implementation: String =
                sys.getattr("implementation")?.getattr("name")?.extract()?;
            let version = sys.getattr("version_info")?;
            let version: Vec<u32> = (0..3)
                .map(|index| version.get_item(index)?.extract())
                .collect::<PyResult<_>>()?;
            let executable: String = sys.getattr("executable")?.extract()?;
            let prefix: String = sys.getattr("prefix")?.extract()?;
            let exec_prefix: String = sys.getattr("exec_prefix")?.extract()?;
            let base_prefix: String = sys.getattr("base_prefix")?.extract()?;
            let flags = sys.getattr("flags")?;
            let safe_path: bool = flags.getattr("safe_path")?.extract()?;
            let no_user_site: i32 = flags.getattr("no_user_site")?.extract()?;
            let dont_write_bytecode: bool = sys.getattr("dont_write_bytecode")?.extract()?;
            let user_site: Option<bool> =
                py.import("site")?.getattr("ENABLE_USER_SITE")?.extract()?;
            if implementation != "cpython"
                || version.as_slice() != selected.version
                || !safe_path
                || no_user_site != 1
                || !dont_write_bytecode
                || user_site != Some(false)
            {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "selected runtime requires CPython3.11, safe path and disabled user site",
                ));
            }
            let same = |path: &str| crate::migration_bundle::resolved(Path::new(path));
            let actual_prefix = same(&prefix).map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
            let actual_exec_prefix =
                same(&exec_prefix).map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
            if Path::new(&executable) != selected.executable
                || actual_prefix != selected.prefix
                || actual_exec_prefix != selected.prefix
            {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "embedded interpreter did not select the package .venv",
                ));
            }
            let base = same(&base_prefix).map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
            let paths = py.import("sysconfig")?.call_method0("get_paths")?;
            let scripts: String = paths.get_item("scripts")?.extract()?;
            let platlib: String = paths.get_item("platlib")?.extract()?;
            for path in [&scripts, &platlib] {
                let physical = same(path).map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
                if !Path::new(path).is_absolute() || !physical.starts_with(&selected.prefix) {
                    return Err(pyo3::exceptions::PyRuntimeError::new_err(
                        "selected runtime installation paths escaped the package .venv",
                    ));
                }
            }
            let extension_suffixes: Vec<String> = py
                .import("importlib.machinery")?
                .getattr("EXTENSION_SUFFIXES")?
                .extract()?;
            if extension_suffixes.is_empty() {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "selected runtime has no native extension suffixes",
                ));
            }
            let mut report = json!({"implementation":implementation,"version":version,
                "executable":executable,"prefix":prefix,"exec_prefix":exec_prefix,
                "base_prefix":base_prefix,"shared_sites":selected.shared_sites,
                "safe_path":safe_path,"user_site":user_site,"dont_write_bytecode":dont_write_bytecode,
                "scripts":scripts,"platlib":platlib,"extension_suffixes":extension_suffixes});
            if !codecs {
                return Ok(report);
            }
            let maturin_present = !py
                .import("importlib.util")?
                .call_method1("find_spec", ("maturin",))?
                .is_none();
            let metadata = py.import("importlib.metadata")?;
            let maturin_version: Option<String> = if maturin_present {
                Some(metadata.call_method1("version", ("maturin",))?.extract()?)
            } else {
                None
            };
            let mut dependencies = Vec::new();
            for (distribution, module, expected) in [
                ("Pillow", "PIL", "12.3.0"),
                ("resvg-py", "resvg_py", "0.5.0"),
                ("rlottie-python", "rlottie_python", "1.3.8"),
            ] {
                let actual: String = metadata
                    .call_method1("version", (distribution,))?
                    .extract()?;
                if actual != expected {
                    return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "selected runtime requires {distribution}=={expected}"
                    )));
                }
                let library = py.import(module)?;
                let origin: String = library.getattr("__file__")?.extract()?;
                let physical = same(&origin).map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
                if !physical.is_file()
                    || !(physical.starts_with(&selected.prefix)
                        || selected.shared_sites && physical.starts_with(&base))
                {
                    return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "{distribution} was not imported from an admitted runtime site"
                    )));
                }
                dependencies
                    .push(json!({"distribution":distribution,"version":actual,"origin":physical}));
            }
            py.import("resvg_py")?.getattr("svg_to_bytes")?;
            py.import("rlottie_python")?.getattr("LottieAnimation")?;
            let pixels = [7_u8, 11, 13, 0, 251, 3, 5, 255];
            let image = crate::codec_raster::OpenImage(py.import("PIL.Image")?.call_method1(
                "frombytes",
                ("RGBA", (2, 1), pyo3::types::PyBytes::new(py, &pixels)),
            )?);
            let roundtrip = crate::codec_raster::pixels(&image.0)? == pixels;
            image.close()?;
            if !roundtrip {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "selected Pillow RGBA roundtrip failed",
                ));
            }
            report["maturin_present"] = json!(maturin_present);
            report["maturin_version"] = json!(maturin_version);
            report["dependencies"] = json!(dependencies);
            report["pillow_roundtrip"] = json!(roundtrip);
            Ok(report)
        };
        inspect().map_err(|error| {
            format!(
                "selected runtime observation failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })
}

fn check_python(args: &[String]) -> Result<Option<&Path>, String> {
    match args {
        [] => Ok(None),
        [option] if matches!(option.as_str(), "--doctor" | "--checks") => Ok(None),
        [option, flag] if option == "--checks" && flag == "--no-browser" => Ok(None),
        [option, path] if option == "--extension" && !path.is_empty() => Ok(None),
        [mode, option, path] if mode == "--checks" && option == "--python" && !path.is_empty() => Ok(Some(Path::new(path))),
        [mode, flag, option, path] if mode == "--checks" && flag == "--no-browser" && option == "--python" && !path.is_empty() => Ok(Some(Path::new(path))),
        _ => Err("private runtime probe accepts only --extension FILE, --doctor or --checks [--no-browser] [--python FILE]".into()),
    }
}

/// Internal same-binary mode; never selects a different dynamically loaded Python library.
///
/// # Safety
/// Call only from the fresh native binary before any thread or Python attachment is started.
/// No other user of the Python C API may initialize or configure it concurrently.
pub unsafe fn probe(root: &Path, args: &[String]) -> i32 {
    let log = std::sync::Arc::new(RunLog::open(root, "runtime-probe", vec![]));
    log.install_panic_hook();
    let result = (|| {
        let requested = check_python(args)?;
        let doctor = args == ["--doctor"];
        let checks = args.first().map(String::as_str) == Some("--checks");
        let extension = match args {
            [option, path] if option == "--extension" => Some(Path::new(path)),
            _ => None,
        };
        for key in [
            "PYTHONHOME",
            "PYTHONPATH",
            "PYTHONSTARTUP",
            "__PYVENV_LAUNCHER__",
            "VIRTUAL_ENV",
        ] {
            if std::env::var_os(key).is_some() {
                return Err(
                    "private runtime probe refuses inherited Python overrides; use runtime-check"
                        .into(),
                );
            }
        }
        for key in [
            "PYTHONUTF8",
            "PYTHONNOUSERSITE",
            "PYTHONSAFEPATH",
            "PYTHONDONTWRITEBYTECODE",
            "NUMERA_EMOJI_MAPPER_NO_DOTENV",
        ] {
            if std::env::var_os(key).as_deref() != Some(std::ffi::OsStr::new("1")) {
                return Err(
                    "private runtime probe requires the scrubbed runtime-check environment".into(),
                );
            }
        }
        let selected = selected(root, requested, &std::env::vars().collect())?;
        initialize(&selected)?;
        let mut report = observe(&selected, !checks)?;
        if checks {
            report["check_prerequisites"] = Python::attach(|py| -> PyResult<Value> {
                // The original -c probe sees this CWD. Admit only the verified physical root,
                // never an inherited PYTHONPATH; find_spec must still import the dotted parent.
                let path = root.to_str().ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err("check root must be UTF-8")
                })?;
                py.import("sys")?
                    .getattr("path")?
                    .call_method1("insert", (0, path))?;
                let util = py.import("importlib.util")?;
                crate::tooling_check::prerequisites(
                    util.as_any(),
                    args.iter().any(|argument| argument == "--no-browser"),
                )
                .map(|present| json!(present))
            })
            .map_err(|error| {
                format!(
                    "check prerequisite discovery failed: {}",
                    crate::logging::redact(&error.to_string(), &[])
                )
            })?;
        }
        if let Some(path) = extension {
            report["extension"] =
                Python::attach(|py| crate::tooling_extension::inspect(py, root, path))?;
        }
        if doctor {
            let extras = Python::attach(|py| crate::tooling_doctor::observe(py, root, &report))?;
            for key in ["extension", "requests", "numpy"] {
                report[key] = extras[key].clone();
            }
        }
        println!(
            "{}",
            serde_json::to_string(&report).map_err(|_| "cannot encode runtime observation")?
        );
        Ok::<_, String>(())
    })();
    let code = match result {
        Ok(()) => 0,
        Err(error) => {
            log.event("ERROR", "runtime-probe", &error);
            1
        }
    };
    log.finish(code);
    code
}

pub(crate) async fn inspect(
    root: &Path,
    args: &[String],
    inherited: &BTreeMap<String, String>,
    log: &RunLog,
) -> Result<Value, String> {
    match args {
        [] => {}
        [option, path] if option == "--extension" && !path.is_empty() => {}
        _ => return Err("runtime-check accepts only --extension FILE".into()),
    }
    let executable =
        std::env::current_exe().map_err(|_| "cannot locate runtime probe executable")?;
    inspect_executable(root, &executable, args, inherited, log).await
}

pub(crate) async fn inspect_executable(
    root: &Path,
    executable: &Path,
    args: &[String],
    inherited: &BTreeMap<String, String>,
    log: &RunLog,
) -> Result<Value, String> {
    let selected = selected(root, check_python(args)?, inherited)?;
    let independent_paths = independent_selected(root, &selected, inherited, log).await?;
    let environment = tooling_environment::environment(root, &environment(inherited), false)?;
    let mut probe = vec![executable.as_os_str().to_owned(), "runtime-probe".into()];
    probe.extend(args.iter().map(std::ffi::OsString::from));
    let completed = tooling_process::capture(
        root,
        &probe,
        &environment,
        Duration::from_secs(10),
        Duration::from_secs(5),
        log,
    )
    .await?;
    if completed.code != 0 {
        return Err(format!(
            "selected runtime probe failed: {}",
            crate::logging::redact(&completed.stderr, &[])
        ));
    }
    let report: Value = serde_json::from_str(&completed.stdout)
        .map_err(|_| "runtime probe returned invalid JSON")?;
    for (name, field) in [
        ("data", "prefix"),
        ("scripts", "scripts"),
        ("platlib", "platlib"),
    ] {
        let observed = report[field]
            .as_str()
            .ok_or_else(|| format!("runtime probe returned no {field} path"))?;
        if crate::migration_bundle::resolved(Path::new(observed))? != independent_paths[name] {
            return Err(format!(
                "selected executable and embedded {name} paths differ"
            ));
        }
    }
    Ok(report)
}

/// Selected interpreter evidence before wheel installation; does not import codecs or application code.
pub(crate) async fn independent(
    root: &Path,
    inherited: &BTreeMap<String, String>,
    log: &RunLog,
) -> Result<BTreeMap<String, PathBuf>, String> {
    let selected = selection(root)?;
    independent_selected(root, &selected, inherited, log).await
}

async fn independent_selected(
    root: &Path,
    selected: &Selection,
    inherited: &BTreeMap<String, String>,
    log: &RunLog,
) -> Result<BTreeMap<String, PathBuf>, String> {
    let environment = tooling_environment::environment(root, &environment(inherited), false)?;
    let independent = tooling_process::capture(
        root,
        &[
            selected.executable.clone().into_os_string(),
            "-I".into(),
            "-B".into(),
            "-X".into(),
            "utf8".into(),
            "-m".into(),
            "sysconfig".into(),
        ],
        &environment,
        Duration::from_secs(10),
        Duration::from_secs(5),
        log,
    )
    .await?;
    if independent.code != 0 {
        return Err("selected package interpreter failed its independent stdlib probe".into());
    }
    sysconfig_paths(selected, &independent.stdout)
}

pub fn run(root: &Path, args: &[String]) -> i32 {
    if args == ["--help"] || args == ["-h"] {
        println!(
            "Usage: numera-emoji runtime-check [--extension FILE]\nInspect the selected package .venv, external codecs and optional project-local native extension in a fresh bounded native process. No environment creation, installation or activation."
        );
        return 0;
    }
    let log = std::sync::Arc::new(RunLog::open(root, "runtime-check", vec![]));
    log.install_panic_hook();
    let result = (|| {
        let mut original = BTreeMap::new();
        for key in RUNTIME_KEYS {
            if let Some(value) = std::env::var_os(key) {
                original.insert(
                    (*key).into(),
                    value
                        .into_string()
                        .map_err(|_| "runtime prerequisite must be UTF-8")?,
                );
            }
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "cannot initialize runtime probe owner")?;
        let report = runtime.block_on(inspect(root, args, &original, &log))?;
        println!(
            "{}",
            serde_json::to_string(&report).map_err(|_| "cannot encode runtime observation")?
        );
        Ok::<_, String>(())
    })();
    let code = match result {
        Ok(()) => 0,
        Err(error) => {
            log.event("ERROR", "runtime-check", &error);
            1
        }
    };
    log.finish(code);
    code
}
