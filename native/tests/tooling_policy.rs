//! Independent source captures exercise the public package/environment seam, not an oracle clone.
use _native::{tooling_environment as environment, tooling_manifest as manifest};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;
fn captures() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/native/tooling-source-contracts-v2.json");
    let value: Value = serde_json::from_slice(
        &std::fs::read(path)
            .expect("primary must capture pinned source fixture before running target tests"),
    )
    .unwrap();
    assert_eq!(
        value["baselineRevision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(value["sourceHashes"].as_object().unwrap().len(), 2);
    assert_eq!(
        value["sourceHashes"]["scripts/local_environment.py"],
        "b568feffc0c75e27b7f08cb69aaf546be4ce544427234379316b8f708b619766"
    );
    assert_eq!(
        value["sourceHashes"]["scripts/prebuilt_manifest.py"],
        "7db57eb04bbc3640a411533a3ceffc755c79b25b2312744953fde714a0c30d0c"
    );
    value
}
fn directories(root: &Path, directory: &Path, found: &mut Vec<String>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.push(
                path.strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .replace('\\', "/"),
            );
            directories(root, &path, found);
        }
    }
}

fn normalized(root: &Path, value: &Path) -> String {
    value
        .to_str()
        .unwrap()
        .replace(root.to_str().unwrap(), "$ROOT")
        .replace('\\', "/")
}

#[test]
fn source_runtime_and_build_environments_keep_input_and_physical_paths() {
    let fixture = captures();
    let cases = fixture["environment"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    for case in cases {
        let root = Root::new("tooling policy");
        let original = case["original"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
            .collect::<BTreeMap<_, _>>();
        let actual =
            environment::environment(&root.0, &original, case["build"].as_bool().unwrap()).unwrap();
        let actual = actual
            .into_iter()
            .map(|(key, value)| {
                (
                    key,
                    Value::String(
                        value
                            .replace(root.0.to_str().unwrap(), "$ROOT")
                            .replace('\\', "/"),
                    ),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        assert_eq!(Value::Object(actual), case["expected"], "{}", case["name"]);
        let mut created = Vec::new();
        directories(&root.0, &root.0, &mut created);
        created.sort();
        assert_eq!(
            serde_json::to_value(created).unwrap(),
            case["directories"],
            "{}",
            case["name"]
        );
        assert_eq!(
            serde_json::to_value(original).unwrap(),
            case["original_after"]
        );
    }
    let root = Root::new("tooling policy");
    for case in fixture["confined"].as_array().unwrap() {
        let actual =
            environment::confined_directory(&root.0, case["relative"].as_str().unwrap(), false);
        match case["expected"].get("error").and_then(Value::as_str) {
            Some(expected) => assert_eq!(actual.unwrap_err(), expected),
            None => assert_eq!(
                normalized(&root.0, &actual.unwrap()),
                case["expected"]["path"]
            ),
        }
    }
}

#[test]
fn schema_one_verification_matches_source_platform_hash_and_refusal_cases() {
    let fixture = captures();
    let cases = fixture["manifest"].as_array().unwrap();
    assert_eq!(
        cases.len(),
        24,
        "no empty or reduced source fixture accepted"
    );
    assert_eq!(
        cases
            .iter()
            .filter(|case| case["expected"].get("manifest").is_some())
            .count(),
        4,
        "ordinary valid source cases must not be counterfeit failures"
    );
    for case in cases {
        let root = Root::new("tooling policy");
        for (name, text) in case["payload"].as_object().unwrap() {
            let path = root.0.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text.as_str().unwrap().as_bytes()).unwrap();
        }
        if let Some(name) = case["missing"].as_str() {
            std::fs::remove_file(root.0.join(name)).unwrap();
        }
        if let Some(name) = case["changed"].as_str() {
            std::fs::write(root.0.join(name), b"changed").unwrap();
        }
        std::fs::write(
            root.0.join("package.json"),
            serde_json::to_vec(&case["manifest"]).unwrap(),
        )
        .unwrap();
        let host = manifest::Host {
            system: case["host"]["system"].as_str().unwrap().into(),
            machine: case["host"]["machine"].as_str().unwrap().into(),
            python: (
                case["host"]["python"][0].as_u64().unwrap() as u32,
                case["host"]["python"][1].as_u64().unwrap() as u32,
            ),
            libc: case["host"]["libc"].as_str().unwrap().into(),
            libc_version: case["host"]["libc_version"].as_str().unwrap().into(),
            macos: case["host"]["macos"].as_str().unwrap().into(),
        };
        let actual = manifest::verify(&root.0, &host);
        if let Some(expected) = case["expected"]["error"].as_str() {
            assert_eq!(actual.unwrap_err(), expected, "{}", case["name"]);
        } else {
            assert_eq!(
                actual.unwrap(),
                case["expected"]["manifest"],
                "{}",
                case["name"]
            );
        }
    }
}

#[test]
fn linked_owned_storage_is_refused_before_outside_write() {
    let root = Root::new("tooling policy");
    let outside = root.0.join("outside");
    let package = root.0.join("package");
    std::fs::create_dir(&outside).unwrap();
    std::fs::create_dir(&package).unwrap();
    let linked = package.join(".venv");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &linked).unwrap();
    #[cfg(windows)]
    {
        let log = _native::logging::RunLog::open(&root.0, "junction-test", vec![]);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let environment =
            _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
        let cmd = _native::codec_process::executable("cmd").unwrap();
        let result = runtime
            .block_on(_native::tooling_process::capture(
                &root.0,
                &[
                    cmd.into_os_string(),
                    "/d".into(),
                    "/c".into(),
                    "mklink".into(),
                    "/J".into(),
                    linked.clone().into_os_string(),
                    outside.clone().into_os_string(),
                ],
                &environment,
                std::time::Duration::from_secs(5),
                std::time::Duration::from_secs(3),
                &log,
            ))
            .unwrap();
        assert_eq!(result.code, 0, "{}{}", result.stdout, result.stderr);
    }
    assert!(
        environment::confined_directory(&package, ".venv", false)
            .unwrap_err()
            .contains("Linked project-local storage")
    );
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(&package).unwrap().count(), 1);
    #[cfg(windows)]
    std::fs::remove_dir(linked).unwrap();
    #[cfg(unix)]
    std::fs::remove_file(linked).unwrap();
}
