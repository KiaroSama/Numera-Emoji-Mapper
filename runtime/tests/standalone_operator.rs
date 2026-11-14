//! Installed collection/panel workflows with no interpreter, source package or external network.
use numera_runtime::{logging::RunLog, package, process_owner::OwnedProcess, tooling_process};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);

fn copy_assets(source: &Path, destination: &Path) {
    std::fs::create_dir(destination).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            copy_assets(&entry.path(), &target);
        } else {
            assert!(kind.is_file(), "asset fixture must not follow links");
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn installed_package(project: &Path, scratch: &Path) -> PathBuf {
    let source = scratch.join("package source");
    std::fs::create_dir(&source).unwrap();
    for name in ["LICENSE", "README.md", ".env.example", "run.sh", "run.ps1"] {
        std::fs::copy(project.join(name), source.join(name)).unwrap();
    }
    copy_assets(&project.join("assets"), &source.join("assets"));
    let archive = package::package(
        &source,
        Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
        None,
        &source.join("dist"),
        "standalone-operator-fixture",
        &package::platform().unwrap(),
    )
    .unwrap();
    let downloads = scratch.join("downloads");
    std::fs::create_dir(&downloads).unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive).unwrap()).unwrap();
    let prefix = zip
        .by_index(0)
        .unwrap()
        .name()
        .split('/')
        .next()
        .unwrap()
        .to_owned();
    zip.extract(&downloads).unwrap();
    drop(zip);
    let installed = package::install(&downloads.join(prefix), &scratch.join("installed")).unwrap();
    std::fs::remove_dir_all(source).unwrap();
    std::fs::remove_dir_all(downloads).unwrap();
    installed
}

fn panel_roundtrip(
    runtime: &tokio::runtime::Runtime,
    root: &Path,
    binary: &Path,
    environment: &BTreeMap<String, String>,
    key: &str,
) {
    let reservation = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let envelope = root.join("logs/tmp/panel-smoke");
    std::fs::create_dir_all(&envelope).unwrap();
    let ownership = envelope.join("owner");
    std::fs::create_dir(&ownership).unwrap();
    let mut command = Command::new(binary);
    command
        .args(["panel", "--port", &port.to_string(), "--no-open"])
        .current_dir(root)
        .env_clear()
        .envs(environment)
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            std::fs::File::create(envelope.join("stdout")).unwrap(),
        ))
        .stderr(Stdio::from(
            std::fs::File::create(envelope.join("stderr")).unwrap(),
        ));
    let mut owner = OwnedProcess::spawn_session(&mut command, ownership).unwrap();
    drop(command);
    let address = format!("http://127.0.0.1:{port}");
    runtime.block_on(async {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            if client
                .get(format!("{address}/api/ping"))
                .send()
                .await
                .is_ok_and(|response| response.status() == 200)
            {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "installed panel did not start: {}",
                std::fs::read_to_string(envelope.join("stderr")).unwrap()
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let response = client.get(format!("{address}/")).send().await.unwrap();
        let status = response.status();
        let page = response.text().await.unwrap();
        assert_eq!(status, 200, "{page}");
        assert!(page.contains(key));
        assert!(!page.contains("__ASSET_VER__"));
        let token = page
            .split_once("const TOKEN = \"")
            .unwrap()
            .1
            .split('"')
            .next()
            .unwrap();
        assert!(!token.is_empty());
        for name in [
            "panel-grid.js",
            "panel-motion.js",
            "panel-save.js",
            "panel-drag.js",
            "panel-actions.js",
            "panel-holding.js",
            "panel-draft.js",
            "logo-128.png",
        ] {
            let response = client
                .get(format!("{address}/static/{name}"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200, "missing installed asset: {name}");
            assert_eq!(
                response.bytes().await.unwrap().as_ref(),
                std::fs::read(root.join("assets").join(name)).unwrap()
            );
        }
        let motion = std::fs::read_to_string(root.join("assets/panel-motion.js")).unwrap();
        assert!(motion.contains("const ANIM_BUDGET = 24;"));
        let response = client
            .get(format!("{address}/preview/{key}?size=52&fps=15&still=1"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "native panel preview failed");
        let preview =
            numera_runtime::native_raster::decode(&response.bytes().await.unwrap()).unwrap();
        assert!(preview.rgba.width() <= 52 && preview.rgba.height() <= 52);
        let excluded = json!({"known":[key], "excluded":[key]});
        let denied = client
            .post(format!("{address}/api/save"))
            .json(&excluded)
            .send()
            .await
            .unwrap();
        assert_eq!(denied.status(), 403);
        let response = client
            .post(format!("{address}/api/save"))
            .header("x-panel-token", token)
            .header("origin", &address)
            .json(&excluded)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["excluded"], 1);
        let db = rusqlite::Connection::open(root.join("collection/catalog.db")).unwrap();
        let included: i64 = db
            .query_row(
                "SELECT included FROM items WHERE content_key=?",
                [key],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(included, 0);
        drop(db);
        let response = client
            .post(format!("{address}/api/save"))
            .header("x-panel-token", token)
            .header("origin", &address)
            .json(&json!({"known":[key],"excluded":[],"packs":[[key,2]]}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["included"], 1);
    });
    owner.finish().unwrap();
    drop(owner);
    let db = rusqlite::Connection::open(root.join("collection/catalog.db")).unwrap();
    let included: i64 = db
        .query_row(
            "SELECT included FROM items WHERE content_key=?",
            [key],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(included, 1);
    let plan: Value =
        serde_json::from_slice(&std::fs::read(root.join("collection/pack_plan.json")).unwrap())
            .unwrap();
    assert_eq!(plan["version"], 1);
    assert_eq!(plan["targets"], json!([[key, 2]]));
    assert!(!numera_runtime::process_owner::cleanup_pending(&envelope));
}

#[test]
fn installed_native_application_ingests_and_saves_panel_without_a_language_runtime() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let base =
        numera_runtime::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
    let scratch = base.join(format!(
        "standalone operator café {}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&scratch).unwrap();
    let binary = installed_package(project, &scratch);
    let root = binary.parent().unwrap().canonicalize().unwrap();
    let empty_path = scratch.join("empty-path");
    std::fs::create_dir(&empty_path).unwrap();
    let mut environment = BTreeMap::from([
        ("PATH".into(), empty_path.to_str().unwrap().into()),
        ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
        // Deliberately unusable interpreter overrides must have no bearing on native operation.
        ("PYTHONHOME".into(), "unavailable-interpreter".into()),
        ("PYO3_PYTHON".into(), "unavailable-interpreter".into()),
    ]);
    if cfg!(windows) {
        for key in ["SystemRoot", "WINDIR", "SYSTEMDRIVE"] {
            if let Ok(value) = std::env::var(key) {
                environment.insert(key.into(), value);
            }
        }
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let log = RunLog::open(&root, "standalone-operator-test", vec![]);
    let call = |args: &[OsString]| {
        let mut argv = vec![binary.clone().into_os_string()];
        argv.extend_from_slice(args);
        runtime
            .block_on(tooling_process::capture(
                &root,
                &argv,
                &environment,
                Duration::from_secs(30),
                Duration::from_secs(10),
                &log,
            ))
            .unwrap()
    };
    let help = call(&["--help".into()]);
    assert_eq!(help.code, 0, "{}{}", help.stdout, help.stderr);
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(project.join("tests/fixtures/native/static-codec.json")).unwrap(),
    )
    .unwrap();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "opaque")
        .unwrap_or(&fixture["cases"][0]);
    let source = scratch.join("opaque.png");
    std::fs::copy(project.join(case["input"].as_str().unwrap()), &source).unwrap();
    let collected = call(&[
        "add-media".into(),
        source.into_os_string(),
        "--keywords".into(),
        "native".into(),
    ]);
    assert_eq!(
        collected.code, 0,
        "{}{}",
        collected.stdout, collected.stderr
    );
    let db = rusqlite::Connection::open(root.join("collection/catalog.db")).unwrap();
    let (key, stored): (String, String) = db
        .query_row("SELECT content_key, file_path FROM items", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    // add-media converts to a fitted 100x100 canvas before ingesting. This key was
    // established independently with historical Pillow 12.3.0 fit_100 followed by
    // its 64x64 LANCZOS fingerprint, not from the unfitted 13x9 source fixture.
    assert_eq!(key, "s:86b15344e713c7bb73c6289547bb0e11");
    assert_ne!(key, case["expected"]["content_key"].as_str().unwrap());
    let converted = numera_runtime::paths::resolve(&root.join("collection"), &stored, &root);
    let raster = numera_runtime::native_raster::open(&converted).unwrap();
    assert_eq!(raster.rgba.dimensions(), (100, 100));
    let conversions: Value = serde_json::from_slice(
        &std::fs::read(project.join("tests/fixtures/native/static-convert.json")).unwrap(),
    )
    .unwrap();
    let expected = conversions["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "opaque")
        .unwrap();
    assert_eq!(
        numera_runtime::native_raster::digest_hex(raster.rgba.as_raw()),
        expected["expected"]["rgba_sha256"].as_str().unwrap()
    );
    drop(db);
    let doctor = call(&["doctor".into()]);
    assert_eq!(doctor.code, 0, "{}{}", doctor.stdout, doctor.stderr);
    assert!(doctor.stdout.contains("Native PNG and WebP codecs passed"));
    panel_roundtrip(&runtime, &root, &binary, &environment, &key);
    for name in [
        ".venv",
        "emojikit",
        "pyproject.toml",
        "wheels",
        ".cache/pip",
        ".cache/pycache",
    ] {
        assert!(!root.join(name).exists(), "native operation created {name}");
    }
    assert!(!numera_runtime::process_owner::cleanup_pending(&root));
    log.finish(0);
    drop(log);
    std::fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn production_environment_drops_interpreter_selection_without_creating_its_caches() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let base =
        numera_runtime::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
    let root: PathBuf = base.join(format!(
        "standalone environment {}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let original = [
        "PYTHONHOME",
        "PYTHONPATH",
        "PYO3_PYTHON",
        "PIP_CACHE_DIR",
        "VIRTUAL_ENV",
        "__PYVENV_LAUNCHER__",
    ]
    .into_iter()
    .map(|key| (key.into(), "unavailable".into()))
    .chain([("PATH".into(), "original path".into())])
    .collect();
    let selected =
        numera_runtime::tooling_environment::environment(&root, &original, false).unwrap();
    assert_eq!(selected["PATH"], "original path");
    assert!(!selected.keys().any(|key| key.starts_with("PYTHON")
        || key.starts_with("PYO3")
        || key.starts_with("PIP_")
        || key == "VIRTUAL_ENV"
        || key == "__PYVENV_LAUNCHER__"));
    assert!(!root.join(".venv").exists());
    assert!(!root.join(".cache/pip").exists());
    assert!(!root.join(".cache/pycache").exists());
    std::fs::remove_dir_all(root).unwrap();
}
