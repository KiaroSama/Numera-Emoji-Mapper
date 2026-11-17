//! Actual Rust CLI tails; no source-language oracle or fake command dispatcher.
use crate::tooling_fixture;
use numera_runtime::{logging::RunLog, tooling_environment, tooling_process};
use std::{collections::BTreeMap, ffi::OsString, path::Path, time::Duration};
use tooling_fixture::Root;

fn invoke(
    root: &Path,
    executable: &Path,
    args: &[&str],
    extra: &[(&str, &str)],
) -> tooling_process::Completed {
    let mut environment = BTreeMap::new();
    for key in [
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "PATHEXT",
        "LD_LIBRARY_PATH",
        "LANG",
        "LC_ALL",
        "NUMERA_RUNTIME_TEST_ADMISSION",
    ] {
        if let Ok(value) = std::env::var(key) {
            environment.insert(key.into(), value);
        }
    }
    environment.insert("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into());
    environment.extend(
        extra
            .iter()
            .map(|(key, value)| ((*key).into(), (*value).into())),
    );
    let environment = tooling_environment::environment(root, &environment, false).unwrap();
    let mut argv = vec![executable.as_os_str().to_owned()];
    argv.extend(args.iter().map(OsString::from));
    let log = RunLog::open(root, "supplemental-command", vec![]);
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(tooling_process::capture(
            root,
            &argv,
            &environment,
            Duration::from_secs(30),
            Duration::from_secs(10),
            &log,
        ))
        .unwrap();
    log.finish(result.code);
    result
}

// native/tests/tooling_doctor.rs::doctor_warns_about_template_key_names_without_returning_or_mutating_values
#[test]
fn doctor_warns_about_template_key_names_without_returning_or_mutating_values() {
    let root = Root::new("supplemental doctor café");
    std::fs::create_dir(root.0.join("assets")).unwrap();
    std::fs::write(root.0.join("assets/panel.html"), b"fixture").unwrap();
    let template = b"# COMMENT=ignored\nGENERAL_BOT_TOKEN=\nBRAND_LOGO_BOTS=\nGENERAL_BOT_TOKEN=\nOPTIONAL=example\n";
    std::fs::write(root.0.join(".env.example"), template).unwrap();
    std::fs::write(root.0.join(".env"), b"GENERAL_BOT_TOKEN=\xff").unwrap();
    let executable = root
        .0
        .join(format!("numera-emoji{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let before = std::env::vars_os().collect::<BTreeMap<_, _>>();
    let result = invoke(
        &root.0,
        &executable,
        &["doctor"],
        &[
            ("GENERAL_BOT_TOKEN", "synthetic-sensitive-value"),
            ("BRAND_LOGO_BOTS", ""),
            ("PATH", ""),
        ],
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    let text = format!("{}{}", result.stdout, result.stderr);
    assert!(text.contains("unset: BRAND_LOGO_BOTS"));
    assert!(text.contains("unset: OPTIONAL"));
    assert!(!text.contains("unset: GENERAL_BOT_TOKEN"));
    assert!(!text.contains("synthetic-sensitive-value"));
    assert!(!text.contains("example"));
    assert_eq!(
        std::fs::read(root.0.join(".env.example")).unwrap(),
        template
    );
    assert_eq!(std::env::vars_os().collect::<BTreeMap<_, _>>(), before);
    root.clean().unwrap();
}

// native/tests/tooling_convert_cli.rs::copied_public_cli_help_usage_and_missing_prerequisite_bypass_operator_config
// The separate prebuilt-runtime prerequisite disappeared in the standalone binary;
// actual public help/usage must still avoid dotenv and conversion output writes.
#[test]
fn copied_public_converter_help_and_usage_bypass_operator_configuration() {
    let root = Root::new("supplemental converter public CLI");
    std::fs::create_dir(root.0.join("assets")).unwrap();
    std::fs::write(root.0.join("assets/panel.html"), b"fixture").unwrap();
    std::fs::write(root.0.join(".env"), b"GENERAL_BOT_TOKEN=\xff").unwrap();
    let executable = root
        .0
        .join(format!("numera-emoji{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    for (args, code, text) in [
        (vec!["native-convert-owner", "--help"], 0, "--stdout FILE"),
        (vec!["native-convert-owner"], 2, "--in is required"),
    ] {
        let actual = invoke(&root.0, &executable, &args, &[("PATH", "")]);
        assert_eq!(actual.code, code, "{}", actual.stderr);
        assert!(format!("{}{}", actual.stdout, actual.stderr).contains(text));
        for absent in ["collection", "output", "out.txt", "err.txt"] {
            assert!(!root.0.join(absent).exists());
        }
    }
    assert_eq!(
        std::fs::read(root.0.join(".env")).unwrap(),
        b"GENERAL_BOT_TOKEN=\xff"
    );
    root.clean().unwrap();
}

// native/tests/tooling_similarity_cli.rs::actual_benchmark_command_replays_read_only_catalog_without_operator_configuration
// Python speedup/oracle claims are retired; the actual Rust report and read-only input are retained.
#[test]
fn actual_benchmark_command_replays_read_only_catalog_without_operator_configuration() {
    let root = Root::new("supplemental benchmark café");
    std::fs::create_dir(root.0.join("runtime")).unwrap();
    std::fs::write(
        root.0.join("runtime/Cargo.toml"),
        b"[package]\nname='fixture'\nversion='0.0.0'\n",
    )
    .unwrap();
    std::fs::write(root.0.join(".env"), b"GENERAL_BOT_TOKEN=\xff").unwrap();
    let executable = root
        .0
        .join(format!("numera-dev{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(env!("CARGO_BIN_EXE_numera-dev"), &executable).unwrap();
    let catalog = root.0.join("catalog café # literal%20.db");
    let db = rusqlite::Connection::open(&catalog).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT, position INTEGER, phash INTEGER); INSERT INTO items VALUES ('z',2,7),('b',1,-1),('a',1,0);").unwrap();
    db.close().unwrap();
    let before = std::fs::read(&catalog).unwrap();
    for (args, expected, text) in [
        (vec!["--help"], 0, "--catalog"),
        (
            vec!["benchmark-similarity", "--unknown"],
            2,
            "requires a value",
        ),
        (
            vec![
                "benchmark-similarity",
                "--catalog",
                catalog.to_str().unwrap(),
            ],
            0,
            "Rust similarity: hashes=3 samples=5",
        ),
        (
            vec!["benchmark-similarity", "--catalog", "missing.db"],
            1,
            "benchmark catalog must be an existing regular file",
        ),
    ] {
        let result = invoke(&root.0, &executable, &args, &[("PATH", "")]);
        assert_eq!(result.code, expected, "{}{}", result.stdout, result.stderr);
        assert!(format!("{}{}", result.stdout, result.stderr).contains(text));
        assert_eq!(std::fs::read(&catalog).unwrap(), before);
        assert!(!root.0.join("missing.db").exists());
    }
    let logs = std::fs::read_dir(root.0.join("logs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "log"))
        .map(|path| std::fs::read_to_string(path).unwrap())
        .collect::<Vec<_>>();
    let success = logs
        .iter()
        .find(|text| text.contains("Rust similarity: hashes=3"))
        .unwrap();
    assert!(success.find("Rust similarity: hashes=3").unwrap() < success.rfind("exit=0").unwrap());
    let failure = logs
        .iter()
        .find(|text| text.contains("benchmark catalog must be"))
        .unwrap();
    assert!(failure.find("benchmark catalog must be").unwrap() < failure.rfind("exit=1").unwrap());
    assert!(!root.0.join("collection").exists());
    root.clean().unwrap();
}
