use super::baseline_tooling_support::*;
use numera_runtime as runtime;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq)]
pub struct DatabaseState {
    journal: String,
    schema: Vec<(String, String, Option<String>)>,
    data: std::collections::BTreeMap<String, Vec<Vec<String>>>,
}
pub fn database_state(path: &Path) -> DatabaseState {
    let db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let journal = db
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    let schema: Vec<(String, String, Option<String>)> = db
        .prepare("SELECT type,name,sql FROM sqlite_schema ORDER BY type,name")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut data = std::collections::BTreeMap::new();
    for (_, name, _) in schema.iter().filter(|(kind, _, _)| kind == "table") {
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let columns = statement.column_count();
        let mut values = statement
            .query_map([], |row| {
                (0..columns)
                    .map(|index| row.get_ref(index).map(|value| format!("{value:?}")))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        values.sort();
        data.insert(name.clone(), values);
    }
    DatabaseState {
        journal,
        schema,
        data,
    }
}
pub fn set_media_path(source: &Path, stored: &str) {
    rusqlite::Connection::open(source.join("catalog.db"))
        .unwrap()
        .execute("UPDATE items SET file_path=?", [stored])
        .unwrap();
}
#[cfg(unix)]
pub fn directory_link(source: &Path, target: &Path) {
    std::os::unix::fs::symlink(source, target).unwrap();
}
#[cfg(windows)]
pub fn directory_link(source: &Path, target: &Path) {
    // The source case uses a junction, which requires no developer-mode or
    // symlink privilege. Create the real reparse point with the ready Rust crate.
    junction::create(source, target).unwrap();
}
#[cfg(unix)]
pub fn remove_directory_link(path: &Path) {
    std::fs::remove_file(path).unwrap();
}
#[cfg(windows)]
pub fn remove_directory_link(path: &Path) {
    std::fs::remove_dir(path).unwrap();
}

pub fn package_fixture(
    root: &Root,
    executable: &Path,
    libraries: Option<&Path>,
) -> std::path::PathBuf {
    for member in ["LICENSE", "README.md", ".env.example"] {
        root.file(member, &std::fs::read(project().join(member)).unwrap());
    }
    for member in runtime::package::RUNTIME_ASSETS {
        root.file(member, &std::fs::read(project().join(member)).unwrap());
    }
    let archive = runtime::package::package(
        &root.0,
        executable,
        libraries,
        &root.0.join("dist"),
        "original-install-fixture",
        &runtime::package::platform().unwrap(),
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(archive).unwrap()).unwrap();
    let prefix = archive
        .by_index(0)
        .unwrap()
        .name()
        .unwrap()
        .split('/')
        .next()
        .unwrap()
        .to_owned();
    let downloads = root.0.join("downloads");
    std::fs::create_dir(&downloads).unwrap();
    archive.extract(&downloads).unwrap();
    downloads.join(prefix)
}

pub fn execute_binary(
    root: &Path,
    binary: &Path,
    args: &[&str],
    input: Option<&Path>,
) -> runtime::tooling_process::Completed {
    let mut environment =
        runtime::tooling_environment::environment(root, &machine(), false).unwrap();
    environment.insert(
        "PYO3_PYTHON".into(),
        root.join("must-not-exist/python.exe")
            .to_string_lossy()
            .into_owned(),
    );
    environment.insert(
        "PYTHONHOME".into(),
        root.join("must-not-exist/python-home")
            .to_string_lossy()
            .into_owned(),
    );
    let mut argv = vec![binary.as_os_str().to_os_string()];
    argv.extend(args.iter().map(std::ffi::OsString::from));
    let log = runtime::logging::RunLog::open(root, "native-install-fixture", vec![]);
    let result = executor()
        .block_on(async {
            if let Some(input) = input {
                runtime::tooling_process::capture_with_input(
                    root,
                    &argv,
                    &environment,
                    input,
                    Duration::from_secs(30),
                    Duration::from_secs(15),
                    &log,
                )
                .await
            } else {
                runtime::tooling_process::capture(
                    root,
                    &argv,
                    &environment,
                    Duration::from_secs(30),
                    Duration::from_secs(15),
                    &log,
                )
                .await
            }
        })
        .unwrap();
    log.finish(result.code);
    result
}

pub fn direct_test_child(
    root: &Root,
    args: &[&str],
    environment: &std::collections::BTreeMap<String, String>,
) -> runtime::tooling_process::Completed {
    let mut argv = vec![std::env::current_exe().unwrap().into_os_string()];
    argv.extend(args.iter().map(std::ffi::OsString::from));
    let log = runtime::logging::RunLog::open(&root.0, "direct-test-admission-probe", vec![]);
    executor()
        .block_on(runtime::tooling_process::capture(
            &root.0,
            &argv,
            environment,
            Duration::from_secs(15),
            Duration::from_secs(10),
            &log,
        ))
        .unwrap()
}
pub fn metadata_for(manifest: &str) -> serde_json::Value {
    let root = Root::new();
    let mut environment = machine();
    if let Ok(path) = std::env::var("PATH") {
        environment.insert("PATH".into(), path);
    }
    for (key, value) in [
        ("CARGO_HOME", option_env!("CARGO_HOME")),
        ("RUSTUP_HOME", option_env!("RUSTUP_HOME")),
    ] {
        if let Some(value) = value {
            environment.insert(key.into(), value.into());
        }
    }
    environment.insert("RUSTUP_AUTO_INSTALL".into(), "0".into());
    let argv = vec![
        env!("CARGO").into(),
        "metadata".into(),
        "--manifest-path".into(),
        project().join(manifest).into_os_string(),
        "--locked".into(),
        "--offline".into(),
        "--no-deps".into(),
        "--format-version=1".into(),
    ];
    let log = runtime::logging::RunLog::open(&root.0, "actual-guard-root-inventory", vec![]);
    let result = executor()
        .block_on(runtime::tooling_process::capture(
            &root.0,
            &argv,
            &environment,
            Duration::from_secs(30),
            Duration::from_secs(15),
            &log,
        ))
        .unwrap();
    assert_eq!(result.code, 0, "{}", result.stderr);
    serde_json::from_str(&result.stdout).unwrap()
}

pub struct Construction {
    owner: runtime::process_owner::OwnedProcess,
    ready: std::path::PathBuf,
}
impl Construction {
    pub fn spawn(root: &Root, _source: &Path) -> Self {
        let workspace = runtime::tooling_process::workspace(&root.0).unwrap();
        std::fs::create_dir(workspace.join("owner")).unwrap();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        let mut environment = machine();
        if let Ok(marker) = std::env::var("NUMERA_RUNTIME_TEST_ADMISSION") {
            environment.insert("NUMERA_RUNTIME_TEST_ADMISSION".into(), marker);
        }
        command
            .args(["construction_child_fixture", "--exact", "--test-threads=1"])
            .env_clear()
            .envs(environment)
            .env("NUMERA_CONSTRUCTION_FIXTURE", &root.0)
            .stdin(std::process::Stdio::null())
            .stdout(std::fs::File::create(workspace.join("stdout")).unwrap())
            .stderr(std::fs::File::create(workspace.join("stderr")).unwrap());
        Self {
            owner: runtime::process_owner::OwnedProcess::spawn_session(
                &mut command,
                workspace.join("owner"),
            )
            .unwrap(),
            ready: root.0.join("construction-ready"),
        }
    }
    pub fn ready(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.ready.is_file() {
            assert!(
                Instant::now() < deadline,
                "construction helper did not acquire its clone"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(std::fs::read(&self.ready).unwrap(), b"READY\n");
    }
    pub fn stop(&mut self) {
        self.owner.finish().unwrap();
    }
}
