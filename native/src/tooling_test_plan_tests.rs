//! Prebuild selection and exact public input snapshots; no compiler or suite execution.
use super::*;
mod fixture {
    use crate as _native;
    include!("../tests/support/tooling_fixture.rs");
}
#[test]
fn selectors_are_exact_unions_not_globs_or_new_cargo_modes() {
    let args = [
        "--baseline",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "--output",
        "logs/tmp/plan.json",
        "--lib",
        "--test",
        "case",
        "--bin",
        "numera-build",
    ]
    .map(str::to_owned);
    let options = arguments(&args).unwrap();
    assert!(options.lib);
    assert_eq!(options.tests, ["case"]);
    assert_eq!(options.bins, ["numera-build"]);
    for extra in [
        vec!["--test", "*"],
        vec!["--features", "x"],
        vec!["--doc"],
        vec!["--lib", "--lib"],
        vec!["--test", "case", "--test", "case"],
    ] {
        let mut args = args[..4].to_vec();
        args.extend(extra.into_iter().map(str::to_owned));
        assert!(arguments(&args).is_err());
    }
}
#[test]
fn git_stage_inventory_filters_protected_before_admitting_public_member_roles() {
    let oid = "1".repeat(40);
    let bytes = format!(
        "100644 {oid} 0\tprivate/data.json\0100644 {oid} 0\tassets/panel.ts\0100644 {oid} 0\tcoins/keywords.csv\0100755 {oid} 0\tcoins/run_convert.ps1\0100644 {oid} 0\tdocs/GUIDE.md\0"
    );
    let rows = index(&bytes).unwrap();
    assert_eq!(rows.len(), 4);
    assert!(!rows.contains_key("private/data.json"));
    assert_eq!(role("assets/panel.ts", false).unwrap(), "support");
    assert_eq!(role("coins/run_convert.ps1", false).unwrap(), "source");
    assert!(role("assets/unknown.svg", false).is_err());
    for entry in [
        format!("120000 {oid} 0\ttests/link.py\0"),
        format!("160000 {oid} 0\tnative/src/submodule\0"),
        format!("100644 {oid} 1\ttests/a.py\0"),
        format!("100644 {oid} 0\ttests/a.py"),
    ] {
        assert!(index(&entry).is_err());
    }
    let duplicate = format!("100644 {oid} 0\ttests/a.py\0100644 {oid} 0\ttests/a.py\0");
    assert!(index(&duplicate).is_err());
    let templates = format!(
        "100644 {oid} 0\t.env.example\0100644 {oid} 0\t.env\0100644 {oid} 0\t.env.local\0100644 {oid} 0\tprivate/.env.example\0100644 {oid} 0\t.env.example.key\0"
    );
    assert_eq!(
        index(&templates)
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [".env.example"]
    );
    assert_eq!(role(".env.example", false).unwrap(), "config");
    assert!(PATHS.contains(&".env.example"));
}
fn metadata(root: &Path) -> Value {
    json!({"version":1,"resolve":null,"workspace_root":root.join("native"),"workspace_members":["opaque-package-id"],"packages":[
        {"id":"other","manifest_path":root.join("other/Cargo.toml"),"targets":[]},
        {"id":"opaque-package-id","manifest_path":root.join("native/Cargo.toml"),"targets":[
            {"name":"_native","kind":["cdylib","rlib"],"src_path":root.join("native/src/lib.rs"),"test":true},
            {"name":"numera-emoji","kind":["bin"],"src_path":root.join("native/src/main.rs"),"test":true},
            {"name":"cases","kind":["test"],"src_path":root.join("native/tests/cases.rs"),"test":false}]}]})
}
#[test]
fn actual_metadata_package_and_explicit_target_selection_preserve_cargo_identity() {
    let root = fixture::Root::new("prebuild metadata");
    for name in [
        "native/Cargo.toml",
        "other/Cargo.toml",
        "native/src/lib.rs",
        "native/src/main.rs",
        "native/tests/cases.rs",
    ] {
        let path = root.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"abc").unwrap();
    }
    let options = Options {
        baseline: "a".repeat(40),
        output: "logs/tmp/plan.json".into(),
        lib: false,
        bins: vec![],
        tests: vec![],
    };
    let (package, targets) = select(&root.0, &metadata(&root.0), &options, &[]).unwrap();
    assert_eq!(package, "opaque-package-id");
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[1]["mode"], "noncoverage");
    let explicit = Options {
        tests: vec!["cases".into()],
        ..options
    };
    assert_eq!(
        select(&root.0, &metadata(&root.0), &explicit, &[])
            .unwrap()
            .1[0]["name"],
        "cases"
    );
    let mut wrong = metadata(&root.0);
    wrong["workspace_members"] = json!(["other"]);
    assert!(select(&root.0, &wrong, &explicit, &[]).is_err());
    let unknown = Options {
        tests: vec!["absent".into()],
        ..explicit
    };
    assert!(select(&root.0, &metadata(&root.0), &unknown, &[]).is_err());
    for (name, source, hash, inactive) in [
        (
            "sandbox_lease_windows",
            "native/tests/sandbox_lease_windows.rs",
            "8d109d5487a650b109e82165f5b41750b9fcccf0b09ab5d34d04e845ea2ff5db",
            cfg!(all(
                target_os = "linux",
                target_arch = "x86_64",
                target_env = "gnu",
                target_pointer_width = "64"
            )),
        ),
        (
            "tooling_test_cohort",
            "native/tests/tooling_test_cohort.rs",
            "26019ab49ab33348875191ddfe233e4e2b127e4c4781ad0670e05712682887c7",
            cfg!(all(windows, target_arch = "x86_64", target_env = "msvc")),
        ),
    ] {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        assert_eq!(
            crate::codec_raster::digest_hex(&std::fs::read(project.join(source)).unwrap()),
            hash,
            "platform fixture must match its reviewed source bytes on every host"
        );
        std::fs::write(
            root.0.join(source),
            b"synthetic source, not an emptiness proof",
        )
        .unwrap();
        let mut actual = metadata(&root.0);
        actual["packages"][1]["targets"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":name,"kind":["test"],"src_path":root.0.join(source),"test":true}));
        let files = [json!({"path":source,"role":"target","sha256":hash})];
        let explicit = Options {
            baseline: "a".repeat(40),
            output: "logs/tmp/plan.json".into(),
            lib: false,
            bins: vec![],
            tests: vec![name.into()],
        };
        let (_, selected) = select(&root.0, &actual, &explicit, &files).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(
            selected[0]["mode"],
            if inactive { "noncoverage" } else { "observe" }
        );
        assert_eq!(selected[0].get("reason").is_some(), inactive);
        let mut drift = files;
        drift[0]["sha256"] = json!("0".repeat(64));
        let changed = select(&root.0, &actual, &explicit, &drift).unwrap().1;
        assert_eq!(changed[0]["mode"], "observe");
        assert!(changed[0].get("reason").is_none());
    }
    root.clean().unwrap();
}
#[test]
fn public_worktree_bytes_use_literal_digest_and_fail_on_protected_or_missing_source() {
    let root = fixture::Root::new("prebuild closure");
    let files = [
        ".env.example",
        "tests/original.py",
        "native/src/lib.rs",
        "native/data/python311-decimal.json",
        "native/data/python311-nonprintable.json",
        "tests/fixtures/golden.json",
        "native/Cargo.toml",
        "native/Cargo.lock",
        ".cargo/tests.toml",
    ];
    let oid = "1".repeat(40);
    let mut inventory = String::new();
    for name in files {
        let path = root.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"abc").unwrap();
        inventory.push_str(&format!("100644 {oid} 0\t{name}\0"));
    }
    let targets = vec![json!({"src_path":"native/src/lib.rs"})];
    let rows = closure(&root.0, &index(&inventory).unwrap(), &targets).unwrap();
    assert!(
        rows.iter().all(|row| row["sha256"]
            == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
    assert!(
        rows.iter()
            .any(|row| row["path"] == ".env.example" && row["role"] == "config")
    );
    assert!(PATHS.contains(&"native/data"));
    assert_eq!(
        rows.iter()
            .filter(|row| row["path"].as_str().unwrap().starts_with("native/data/"))
            .map(|row| (row["path"].as_str().unwrap(), row["role"].as_str().unwrap()))
            .collect::<Vec<_>>(),
        [
            ("native/data/python311-decimal.json", "support"),
            ("native/data/python311-nonprintable.json", "support")
        ]
    );
    let without = inventory.replace(&format!("100644 {oid} 0\ttests/original.py\0"), "");
    assert!(closure(&root.0, &index(&without).unwrap(), &targets).is_err());
    std::fs::write(root.0.join("tests/original.py"), b"drift").unwrap();
    assert_ne!(
        closure(&root.0, &index(&inventory).unwrap(), &targets).unwrap(),
        rows
    );
    root.clean().unwrap();
}
