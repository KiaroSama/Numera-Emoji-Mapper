//! Independent source captures exercise the public package/environment seam, not an oracle clone.
use crate::tooling_fixture;
use numera_runtime::tooling_environment as environment;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
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
        let mut expected = case["expected"].as_object().unwrap().clone();
        // Interpreter-only caches/flags disappear; physical native tool caches
        // and every ordinary caller variable remain part of the frozen matrix.
        for key in [
            "PIP_CACHE_DIR",
            "PYTHONPYCACHEPREFIX",
            "PYTHONUTF8",
            "PIP_DISABLE_PIP_VERSION_CHECK",
        ] {
            expected.remove(key);
        }
        if case["build"].as_bool().unwrap() {
            expected.insert(
                "CARGO_TARGET_DIR".into(),
                Value::String("$ROOT/runtime/target".into()),
            );
        }
        assert_eq!(
            Value::Object(actual),
            Value::Object(expected),
            "{}",
            case["name"]
        );
        let mut created = Vec::new();
        directories(&root.0, &root.0, &mut created);
        created.sort();
        let mut expected_dirs = case["directories"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .filter(|value| ![".cache/pip", ".cache/pycache"].contains(value))
            .map(|value| match value {
                "native" => "runtime",
                "native/target" => "runtime/target",
                other => other,
            })
            .collect::<Vec<_>>();
        expected_dirs.sort();
        assert_eq!(
            serde_json::to_value(created).unwrap(),
            serde_json::to_value(expected_dirs).unwrap(),
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
    junction::create(&outside, &linked).unwrap();
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
