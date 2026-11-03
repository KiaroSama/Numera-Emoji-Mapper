//! Native doctor consumes real selected-runtime evidence, never a successful import-name guess.
use _native::tooling_doctor;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn runtime() -> Value {
    json!({"implementation":"cpython","version":[3,11,9],"safe_path":true,
        "user_site":false,"dont_write_bytecode":true,"pillow_roundtrip":true,
        "dependencies":[{"distribution":"Pillow"},{"distribution":"resvg-py"},{"distribution":"rlottie-python"}],
        "extension":{"api_version":1,"greedy_callable":true,"near_callable":true,"package_imported":false},
        "requests":{"imported":true},"numpy":{"imported":true}})
}

#[test]
fn doctor_requires_runtime_and_native_api_evidence_but_optional_video_and_numpy_only_warn() {
    let evidence = runtime();
    let complete = tooling_doctor::assess(&evidence, true, &[]).unwrap();
    assert_eq!(complete.code, 0);
    assert!(
        complete
            .messages
            .iter()
            .any(|line| line.contains("Environment check complete"))
    );
    let mut partial = evidence.clone();
    partial["numpy"] = json!({"imported":false});
    let warning = tooling_doctor::assess(&partial, false, &["GENERAL_BOT_TOKEN".into()]).unwrap();
    assert_eq!(warning.code, 0);
    assert!(
        warning
            .messages
            .iter()
            .any(|line| line.contains("video emoji disabled"))
    );
    assert!(
        warning
            .messages
            .iter()
            .any(|line| line.contains("numpy not installed"))
    );
    assert!(
        warning
            .messages
            .iter()
            .any(|line| line.contains("unset: GENERAL_BOT_TOKEN"))
    );
    for (field, value) in [
        ("implementation", json!("pypy")),
        ("version", json!([3, 12, 0])),
        ("safe_path", json!(false)),
        ("pillow_roundtrip", json!(false)),
        ("extension", Value::Null),
        ("requests", json!({"imported":false})),
    ] {
        let mut changed = evidence.clone();
        changed[field] = value;
        assert!(
            tooling_doctor::assess(&changed, true, &[]).is_err(),
            "accepted {field}"
        );
    }
    for field in [
        "api_version",
        "greedy_callable",
        "near_callable",
        "package_imported",
    ] {
        let mut changed = evidence.clone();
        changed["extension"][field] = if field == "api_version" {
            json!(2)
        } else {
            json!(field == "package_imported")
        };
        assert!(
            tooling_doctor::assess(&changed, true, &[]).is_err(),
            "accepted {field}"
        );
    }
}

#[test]
fn doctor_warns_about_template_key_names_without_returning_or_mutating_values() {
    let template = "# COMMENT=ignored\nGENERAL_BOT_TOKEN=\nBRAND_LOGO_BOTS=\nGENERAL_BOT_TOKEN=\nOPTIONAL=example\n";
    let inherited: BTreeMap<String, String> = BTreeMap::from([
        (
            "GENERAL_BOT_TOKEN".into(),
            "synthetic-sensitive-value".into(),
        ),
        ("BRAND_LOGO_BOTS".into(), String::new()),
    ]);
    let before = inherited.clone();
    let missing = tooling_doctor::unset_keys(template, |key| {
        inherited
            .get(key)
            .is_some_and(|value| !value.trim().is_empty())
    });
    assert_eq!(missing, ["BRAND_LOGO_BOTS", "OPTIONAL"]);
    assert_eq!(inherited, before);
    let text = tooling_doctor::assess(&runtime(), false, &missing)
        .unwrap()
        .messages
        .join("\n");
    assert!(!text.contains("synthetic-sensitive-value"));
    assert!(!text.contains("example"));
}
