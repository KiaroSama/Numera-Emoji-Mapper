//! Historical tooling recipes refuse forged source/input before runtime or workspace access.
use _native::{source_git_inventory::Source, source_tooling_oracle as oracle};
use serde_json::json;
use std::path::Path;

#[test]
fn tooling_recipe_pins_inputs_without_trusting_caller_observations() {
    let original = oracle::fixture().unwrap();
    oracle::fixture_identity(&original).unwrap();
    for malformed in [
        json!(null),
        json!([]),
        json!("bad"),
        json!({}),
        json!({"manifest":false}),
    ] {
        assert!(oracle::fixture_identity(&malformed).is_err());
    }
    let mut observations = original.clone();
    observations["manifest"][0]["expected"] = json!({"error":"forged output"});
    observations["environment"][0]["expected"] = json!({});
    observations["environment"][0]["original_after"] = json!({});
    observations["environment"][0]["directories"] = json!([]);
    observations["confined"][0]["expected"] = json!({"path":"forged"});
    oracle::fixture_identity(&observations).unwrap();
    for mutation in [
        "payload",
        "host",
        "name",
        "order",
        "environment",
        "confinement",
        "extra",
    ] {
        let mut changed = observations.clone();
        match mutation {
            "payload" => changed["manifest"][0]["payload"]["assets/panel.html"] = json!("changed"),
            "host" => changed["manifest"][0]["host"]["python"] = json!([3, 12]),
            "name" => changed["manifest"][0]["name"] = json!("renamed"),
            "order" => changed["manifest"].as_array_mut().unwrap().swap(0, 1),
            "environment" => changed["environment"][0]["original"]["KEEP"] = json!("changed"),
            "confinement" => changed["confined"][0]["relative"] = json!("../outside"),
            "extra" => changed["unreviewed"] = json!(true),
            _ => unreachable!(),
        }
        assert!(oracle::fixture_identity(&changed).is_err(), "{mutation}");
    }
}

#[test]
fn package_recipe_pins_inputs_without_trusting_the_recorded_manifest() {
    use _native::source_package_oracle as package;
    let fixture = package::fixture().unwrap();
    package::fixture_identity(&fixture).unwrap();
    let mut observations = fixture.clone();
    observations["expected"] = json!({"platform":"caller forged platform"});
    package::fixture_identity(&observations).unwrap();
    for mutation in ["input", "wheel", "revision", "extra"] {
        let mut changed = observations.clone();
        match mutation {
            "input" => changed["inputs"]["README.md"] = json!("changed"),
            "wheel" => changed["wheel_input"] = json!("build/other.whl"),
            "revision" => changed["revision"] = json!("different"),
            "extra" => changed["unreviewed"] = json!(true),
            _ => unreachable!(),
        }
        assert!(package::fixture_identity(&changed).is_err(), "{mutation}");
    }
    for malformed in [json!(null), json!([]), json!({})] {
        assert!(package::fixture_identity(&malformed).is_err());
    }
    let sources = [
        (
            "scripts/local_environment.py",
            "b568feffc0c75e27b7f08cb69aaf546be4ce544427234379316b8f708b619766",
        ),
        (
            "scripts/package_native.py",
            "cb504e8f0e22789a278129f893bb14cf1954483d9a3f620254e6ac5da7a326b0",
        ),
    ]
    .map(|(path, hash)| Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: hash.into(),
        text: "untrusted source".into(),
    });
    let root = Path::new("must-not-be-inspected");
    assert!(
        package::observe(root, root, &sources[0], &sources[1], &fixture)
            .unwrap_err()
            .contains("source identity drift")
    );
}

#[test]
fn sandbox_recipe_refuses_changed_inputs_and_sources_before_runtime() {
    use _native::source_sandbox_oracle as sandbox;
    let fixture = sandbox::fixture().unwrap();
    sandbox::fixture_identity(&fixture).unwrap();
    let mut outputs = fixture.clone();
    outputs["scrubbed"] = json!({"forged":"output"});
    for case in outputs["arguments"].as_array_mut().unwrap() {
        case["expected"] = json!([]);
        case["port"] = json!(1);
        case["all"] = json!(true);
        case["with_pack"] = json!([]);
        case["bot_username"] = json!("forged");
    }
    sandbox::fixture_identity(&outputs).unwrap();
    for mutation in ["template", "environment", "arguments", "order", "extra"] {
        let mut drift = outputs.clone();
        match mutation {
            "template" => drift["template"] = json!("OTHER=\n"),
            "environment" => drift["original"]["KEEP"] = json!("changed"),
            "arguments" => drift["arguments"][0]["input"] = json!(["--all"]),
            "order" => drift["arguments"].as_array_mut().unwrap().swap(0, 1),
            "extra" => drift["unreviewed"] = json!(true),
            _ => unreachable!(),
        }
        assert!(sandbox::fixture_identity(&drift).is_err(), "{mutation}");
    }
    for invalid in [
        json!(null),
        json!([]),
        json!({}),
        json!({"arguments":false}),
    ] {
        assert!(sandbox::fixture_identity(&invalid).is_err());
    }
    let source = Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: "scripts/panel_sandbox.py".into(),
        sha256: "a37b7609521a78ebca9a6d0a2d1dae7b70fccce2fa50af8d5a16f2d60dfef2ae".into(),
        text: "raise AssertionError('must not execute')".into(),
    };
    let root = Path::new("must-not-be-inspected");
    assert!(
        sandbox::observe(root, root, &source, &fixture)
            .unwrap_err()
            .contains("source identity drift")
    );
}

#[test]
fn collection_state_sources_refuse_before_runtime_or_workspace_access() {
    let sources = [
        (
            "tests/reference/collection_state.py",
            "b8bf2695954cbe61a3e3cb68c8da0c583aa80a1b645abba01eabe9a28776074a",
        ),
        (
            "tests/reference/packstate.py",
            "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
        ),
        (
            "tests/test_build_collection_state.py",
            "479e52e0e83c9e0e4f2edb4a29198d9360e3312eb6ce279f39d31090a8288469",
        ),
    ]
    .map(|(path, hash)| Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: hash.into(),
        text: "raise AssertionError('must not execute')".into(),
    });
    let root = Path::new("must-not-be-inspected");
    assert!(
        _native::source_collection_state_oracle::observe(
            root,
            root,
            &[&sources[0], &sources[1], &sources[2]]
        )
        .unwrap_err()
        .contains("source identity drift")
    );
    assert!(_native::source_collection_state_oracle::source_identity(&[]).is_err());
}

#[test]
fn tooling_forged_sources_refuse_before_python_or_scratch() {
    let (fixture, root) = (
        oracle::fixture().unwrap(),
        Path::new("must-not-be-inspected"),
    );
    let sources = [
        (
            "scripts/local_environment.py",
            "b568feffc0c75e27b7f08cb69aaf546be4ce544427234379316b8f708b619766",
        ),
        (
            "scripts/prebuilt_manifest.py",
            "7db57eb04bbc3640a411533a3ceffc755c79b25b2312744953fde714a0c30d0c",
        ),
    ]
    .map(|(path, hash)| Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: hash.into(),
        text: "raise AssertionError('untrusted source must never execute')".into(),
    });
    assert!(
        oracle::source_identity(&sources[0], &sources[1])
            .unwrap_err()
            .contains("source identity drift")
    );
    assert!(
        oracle::observe(root, root, &sources[0], &sources[1], &fixture)
            .unwrap_err()
            .contains("source identity drift")
    );
}
