//! A declared source list cannot become retirement approval through deletion or fabricated targets.
use serde_json::{Value, json};
const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
fn inventory() -> Value {
    json!({"baseline_revision":BASELINE,"discovery_complete":true,"files":[{
        "source":"tests/test_source.py","sha256":"a".repeat(64),"test_cases":["Parent.test_one","Child.test_one"]
    }]})
}
fn entry(name: &str) -> Value {
    json!({"source_file":"tests/test_source.py","source_case":name,"source_hash":"a".repeat(64),"status":"parity",
        "target_file":"native/tests/source_test_inventory.rs","target_case":"source_inventory_keeps_methods_assertions_imports_decorators_and_cli_arguments",
        "fixture":"tests/fixtures/native/source-inventory.json","fixture_hash":"b".repeat(64),
        "evidence":{"baseline_revision":BASELINE,"target_revision":"c".repeat(40),"result":"verified","platform":"fixture-platform","command":"fixture command","run_identity":"synthetic-validator-evidence"}})
}
#[test]
fn release_requires_complete_discovery_every_case_and_exact_independent_evidence() {
    let source = inventory();
    let complete = json!({"baseline_revision":BASELINE,"cases":[entry("Parent.test_one"),entry("Child.test_one")]});
    let success = _native::source_migration_coverage::validate(&source, &complete, true).unwrap();
    assert_eq!(success["verified"], 2);
    assert_eq!(success["metadata_complete"], true);
    assert_eq!(success["release_ready"], false);
    let mut partial = complete.clone();
    partial["cases"].as_array_mut().unwrap().pop();
    let report = _native::source_migration_coverage::validate(&source, &partial, false).unwrap();
    assert_eq!(
        report["unmapped"],
        json!(["tests/test_source.py::Child.test_one"])
    );
    assert!(
        _native::source_migration_coverage::validate(&source, &partial, true)
            .unwrap_err()
            .contains("unmapped")
    );
    partial["cases"][0]["status"] = json!("pending");
    let report = _native::source_migration_coverage::validate(&source, &partial, false).unwrap();
    assert_eq!(report["pending"].as_array().unwrap().len(), 1);
    assert_eq!(report["release_ready"], false);
    let mut incomplete = source.clone();
    incomplete["discovery_complete"] = json!(false);
    assert!(
        _native::source_migration_coverage::validate(&incomplete, &complete, true)
            .unwrap_err()
            .contains("discovery")
    );
    for (field, value) in [
        ("source_hash", json!("d".repeat(64))),
        ("target_file", json!("../foreign.rs")),
        ("fixture", json!("../private.json")),
        ("fixture_hash", json!("invalid")),
        ("source_case", json!("Other.test_wrong")),
        ("status", json!("removed")),
    ] {
        let mut broken = complete.clone();
        broken["cases"][0][field] = value;
        assert!(
            _native::source_migration_coverage::validate(&source, &broken, true).is_err(),
            "{field}"
        );
    }
    let mut duplicate = complete.clone();
    duplicate["cases"]
        .as_array_mut()
        .unwrap()
        .push(entry("Parent.test_one"));
    assert!(_native::source_migration_coverage::validate(&source, &duplicate, false).is_err());
    let mut unsupported_evidence = complete.clone();
    unsupported_evidence["cases"][0]["evidence"]["result"] = json!("authored");
    assert!(
        _native::source_migration_coverage::validate(&source, &unsupported_evidence, true).is_err()
    );
    let mut redundant = complete.clone();
    redundant["cases"][1]["status"] = json!("redundant");
    assert!(_native::source_migration_coverage::validate(&source, &redundant, true).is_err());
    redundant["cases"][1]["reason"] =
        json!("Inherited fixture variant retained in the same target seam");
    assert!(_native::source_migration_coverage::validate(&source, &redundant, true).is_ok());
    assert!(_native::source_migration_coverage::validate(&json!({}), &complete, false).is_err());
}

#[test]
fn original_bindings_can_map_to_observed_crate_private_unit_tests() {
    let source = inventory();
    let mut binding = entry("Parent.test_one");
    binding["target_file"] = json!("native/src/binding_intent_tests.rs");
    binding["target_case"] = json!(
        "binding_intent_tests::registered_native_intent_and_resume_preserve_original_public_json_assertions"
    );
    let mut coverage = json!({"baseline_revision":BASELINE,"cases":[binding]});
    let report = _native::source_migration_coverage::validate(&source, &coverage, false).unwrap();
    assert_eq!(report["verified"], 1);
    assert_eq!(report["release_ready"], false);
    for path in [
        "native/src/lib.rs",
        "native/src/intent.rs",
        "native/src/../outside_tests.rs",
        "native/src/binding_tests.rs/extra.rs",
        "other/src/binding_tests.rs",
    ] {
        coverage["cases"][0]["target_file"] = json!(path);
        assert!(
            _native::source_migration_coverage::validate(&source, &coverage, false).is_err(),
            "{path}"
        );
    }
}

#[test]
fn inline_private_connection_assertions_require_their_exact_module_test_namespace() {
    let source = inventory();
    let candidate = json!({"target_file":"native/src/catalog.rs",
        "target_case":"catalog::tests::original_wal_normal_and_empty_order_preserve_exact_connection_change_count",
        "fixture":"tests/fixtures/native/source-baseline.json","fixture_hash":"b".repeat(64),
        "remaining":"Original private connection assertions authored; execution unverified"});
    let mut coverage = json!({"baseline_revision":BASELINE,"cases":[{
        "source_file":"tests/test_source.py","source_case":"Parent.test_one",
        "source_hash":"a".repeat(64),"status":"pending","candidate":candidate
    }]});
    let report = _native::source_migration_coverage::validate(&source, &coverage, false).unwrap();
    assert_eq!(report["candidates"], 1);
    assert_eq!(report["verified"], 0);
    assert_eq!(report["release_ready"], false);
    for name in [
        "catalog::dispatch",
        "other::tests::case",
        "catalog::tests::",
        "catalog::tests",
        "",
    ] {
        coverage["cases"][0]["candidate"]["target_case"] = json!(name);
        assert!(
            _native::source_migration_coverage::validate(&source, &coverage, false).is_err(),
            "{name}"
        );
    }
}

#[test]
fn inherited_candidates_require_the_actual_definition_identity_and_assertion_span() {
    let text = "import unittest\nclass Parent(unittest.TestCase):\n def test_one(self): self.assertEqual(1, 1)\nclass Child(Parent): pass\n";
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let sources = [_native::source_git_inventory::Source {
        baseline_revision: BASELINE.into(),
        path: "tests/test_source.py".into(),
        sha256: digest.clone(),
        text: text.into(),
    }];
    let source = _native::source_git_inventory::inventory(&sources).unwrap();
    let origin = json!({"class_identity":"tests/test_source.py:4",
        "defined_in_identity":"tests/test_source.py:2","method_line":3});
    let mut coverage = json!({"baseline_revision":BASELINE,"cases":[{
        "source_file":"tests/test_source.py","source_case":"Child.test_one",
        "source_hash":digest,"status":"pending","origin":origin
    }]});
    let report = _native::source_migration_coverage::validate(&source, &coverage, false).unwrap();
    assert_eq!(
        report["pending"],
        json!(["tests/test_source.py::Child.test_one"])
    );
    assert_eq!(report["verified"], 0);
    assert_eq!(report["release_ready"], false);
    for (field, value) in [
        ("class_identity", json!("tests/test_source.py:2")),
        ("defined_in_identity", json!("tests/test_source.py:4")),
        ("method_line", json!(99)),
        ("method_line", json!("3")),
    ] {
        let mut changed = coverage.clone();
        changed["cases"][0]["origin"][field] = value;
        assert!(
            _native::source_migration_coverage::validate(&source, &changed, false)
                .unwrap_err()
                .contains("origin"),
            "{field}"
        );
    }
    let mut no_origin = coverage.clone();
    no_origin["cases"][0]
        .as_object_mut()
        .unwrap()
        .remove("origin");
    assert!(_native::source_migration_coverage::validate(&source, &no_origin, false).is_err());
    coverage["cases"][0]["origin"] = Value::Null;
    assert!(_native::source_migration_coverage::validate(&source, &coverage, false).is_err());
    let mut missing = source.clone();
    missing["files"][0]["case_assertions"]["cases"] = json!([]);
    coverage["cases"][0]["origin"] = origin;
    assert!(_native::source_migration_coverage::validate(&missing, &coverage, false).is_err());
    assert!(_native::source_migration_coverage::validate(&missing, &no_origin, false).is_err());
    let mut drift = source.clone();
    for case in drift["files"][0]["case_assertions"]["cases"]
        .as_array_mut()
        .unwrap()
    {
        if case["source_case"] == "Child.test_one" {
            case["defined_in_identity"] = json!("tests/test_source.py:4");
        }
    }
    assert!(_native::source_migration_coverage::validate(&drift, &no_origin, false).is_err());
    assert!(_native::source_migration_coverage::validate(&source, &coverage, true).is_err());
}

#[test]
fn collected_cases_keep_definition_coverage_and_unobserved_gaps_separate() {
    let source = json!({"baseline_revision":BASELINE,"discovery_complete":false,"files":[{
        "source":"tests/test_source.py","sha256":"a".repeat(64),
        "test_cases":["Parent.test_one","Child.test_one","Case.test_unobserved","Case.test_decorated"],
        "case_assertions":{"cases":[
            {"source_case":"Parent.test_one","defined_in_file":"tests/test_source.py","defined_in_hash":"a".repeat(64),"method_line":3,"code_first_line":3},
            {"source_case":"Child.test_one","defined_in_file":"tests/test_source.py","defined_in_hash":"a".repeat(64),"method_line":3,"code_first_line":3},
            {"source_case":"Case.test_decorated","defined_in_file":"tests/test_source.py","defined_in_hash":"a".repeat(64),"method_line":10,"code_first_line":10}
        ]}
    },{"source":"tests/reference/helper.py","sha256":"b".repeat(64),"test_cases":["Helper.test_reference"]}]});
    let coverage = json!({"baseline_revision":BASELINE,"cases":[
        {"source_file":"tests/test_source.py","source_case":"Parent.test_one","source_hash":"a".repeat(64),"status":"pending"},
        {"source_file":"tests/test_source.py","source_case":"Case.test_unobserved","source_hash":"a".repeat(64),"status":"pending"}
    ]});
    let observation = json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9",
    "collection_only":true,"tests_run":0,"guard_imported_first":true,"loader_errors":0,
    "case_fields":["id","defined_in_file","defined_in_hash","code_line","class_skipped","code_qualname","method_skipped"],
    "cases":[
        ["tests.test_source.Parent.test_one","tests/test_source.py","a".repeat(64),3,false,"Parent.test_one",false],
        ["tests.test_source.Child.test_one","tests/test_source.py","a".repeat(64),3,false,"Parent.test_one",true],
        ["tests.test_source.Case.test_decorated","tests/test_source.py","a".repeat(64),9,true,"Case.test_decorated",false],
        ["tests.test_source.Dynamic.test_created","tests/test_source.py","a".repeat(64),20,true,"Factory.test_created",true]
    ]});
    let report =
        _native::source_runtime_discovery::reconcile(&source, &coverage, &observation).unwrap();
    assert_eq!(report["runtime_cases"], 4);
    assert_eq!(report["static_candidates"], 4);
    assert_eq!(
        report["collection_skipped"],
        json!([
            "tests.test_source.Case.test_decorated",
            "tests.test_source.Child.test_one",
            "tests.test_source.Dynamic.test_created"
        ])
    );
    assert_eq!(
        report["definition_matches"],
        json!([
            "tests.test_source.Child.test_one",
            "tests.test_source.Parent.test_one"
        ])
    );
    assert_eq!(
        report["static_unobserved"],
        json!(["tests.test_source.Case.test_unobserved"])
    );
    assert_eq!(
        report["runtime_only"],
        json!(["tests.test_source.Dynamic.test_created"])
    );
    assert_eq!(
        report["runtime_without_coverage"].as_array().unwrap().len(),
        3
    );
    assert_eq!(
        report["unobserved_coverage"],
        json!(["tests.test_source.Case.test_unobserved"])
    );
    assert_eq!(
        report["observed_pending"],
        json!(["tests.test_source.Parent.test_one"])
    );
    assert_eq!(
        report["out_of_collection_scope"],
        json!(["tests/reference/helper.py::Helper.test_reference"])
    );
    assert_eq!(
        report["definition_gaps"][0]["id"],
        "tests.test_source.Case.test_decorated"
    );
    assert_eq!(
        report["definition_gaps"][0]["reason"],
        "definition span differs or requires decorator provenance"
    );
    assert_eq!(report["definition_gaps"][0]["observed_code_line"], 9);
    assert_eq!(report["definition_gaps"][0]["static_method_line"], 10);
    assert_eq!(report["static_discovery_complete"], false);
    assert_eq!(report["coverage_verified_by_collection"], 0);
    assert_eq!(report["release_ready"], false);
    let mut explicit_span = source.clone();
    for definition in explicit_span["files"][0]["case_assertions"]["cases"]
        .as_array_mut()
        .unwrap()
    {
        definition["code_first_line"] = definition["method_line"].clone();
    }
    explicit_span["files"][0]["case_assertions"]["cases"][2]["code_first_line"] = json!(9);
    let matched =
        _native::source_runtime_discovery::reconcile(&explicit_span, &coverage, &observation)
            .unwrap();
    assert!(matched["definition_gaps"].as_array().unwrap().is_empty());
    assert!(
        matched["definition_matches"]
            .as_array()
            .unwrap()
            .contains(&json!("tests.test_source.Case.test_decorated"))
    );
    assert_eq!(matched["coverage_verified_by_collection"], 0);
    let mut original_definitions = source.clone();
    original_definitions["files"][0]["assertions"] = json!({"methods":[
        {"class":"Parent","method":"test_one","assertions":[{"source":"self.assertTrue(True)"}]},
        {"class":"Case","method":"test_decorated","assertions":[{"source":"self.assertEqual(1, 1)"}]},
        {"class":"Factory","method":"test_created","assertions":[]}
    ],"method_origins":[
        {"class_identity":"tests/test_source.py:2","method":"test_one","method_line":3,"code_first_line":3},
        {"class_identity":"tests/test_source.py:8","method":"test_decorated","method_line":10,"code_first_line":9},
        {"class_identity":"tests/test_source.py:18","method":"test_created","method_line":20,"code_first_line":20}
    ]});
    let original = _native::source_runtime_discovery::reconcile(
        &original_definitions,
        &coverage,
        &observation,
    )
    .unwrap();
    assert_eq!(
        original["original_definition_matches"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert!(
        original["original_definition_gaps"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let decorated = original["original_definition_matches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "tests.test_source.Case.test_decorated")
        .unwrap();
    assert_eq!(decorated["class_identity"], "tests/test_source.py:8");
    assert_eq!(decorated["method_line"], 10);
    assert_eq!(decorated["code_first_line"], 9);
    assert_eq!(decorated["assertion_count"], 1);
    assert_eq!(original["runtime_only"], report["runtime_only"]);
    assert_eq!(original["coverage_verified_by_collection"], 0);
    let mut wrong_definition = observation.clone();
    wrong_definition["cases"][1][5] = json!("Child.test_one");
    let wrong = _native::source_runtime_discovery::reconcile(
        &original_definitions,
        &coverage,
        &wrong_definition,
    )
    .unwrap();
    assert_eq!(
        wrong["original_definition_gaps"],
        json!(["tests.test_source.Child.test_one"])
    );
    for malformed in ["length", "duplicate", "origin", "class"] {
        let mut changed = original_definitions.clone();
        match malformed {
            "length" => {
                changed["files"][0]["assertions"]["method_origins"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            "duplicate" => {
                let method = changed["files"][0]["assertions"]["methods"][0].clone();
                let origin = changed["files"][0]["assertions"]["method_origins"][0].clone();
                changed["files"][0]["assertions"]["methods"]
                    .as_array_mut()
                    .unwrap()
                    .push(method);
                changed["files"][0]["assertions"]["method_origins"]
                    .as_array_mut()
                    .unwrap()
                    .push(origin);
            }
            "origin" => {
                changed["files"][0]["assertions"]["method_origins"][0]["class_identity"] =
                    json!("tests/other.py:2")
            }
            _ => changed["files"][0]["assertions"]["methods"][0]["class"] = Value::Null,
        }
        assert!(
            _native::source_runtime_discovery::reconcile(&changed, &coverage, &observation)
                .is_err(),
            "{malformed}"
        );
    }
    for value in [Value::Null, json!(0), json!(11), json!("9")] {
        let mut invalid_span = explicit_span.clone();
        invalid_span["files"][0]["case_assertions"]["cases"][2]["code_first_line"] = value;
        assert!(
            _native::source_runtime_discovery::reconcile(&invalid_span, &coverage, &observation)
                .is_err()
        );
    }
    let mut absent_span = explicit_span.clone();
    absent_span["files"][0]["case_assertions"]["cases"][0]
        .as_object_mut()
        .unwrap()
        .remove("code_first_line");
    assert!(
        _native::source_runtime_discovery::reconcile(&absent_span, &coverage, &observation)
            .is_err()
    );
    let mut missing_assertions = source.clone();
    missing_assertions["files"][0]["case_assertions"]["cases"] = json!([]);
    let gaps =
        _native::source_runtime_discovery::reconcile(&missing_assertions, &coverage, &observation)
            .unwrap();
    assert_eq!(gaps["definition_gaps"].as_array().unwrap().len(), 3);
    assert!(gaps["definition_matches"].as_array().unwrap().is_empty());
    for (field, value) in [
        ("source_revision", json!("0".repeat(40))),
        ("source_runtime", json!("CPython3.12.0")),
        ("collection_only", json!(false)),
        ("tests_run", json!(1)),
        ("guard_imported_first", json!(false)),
        ("loader_errors", json!(1)),
        ("case_fields", json!(["id"])),
        ("cases", json!([])),
    ] {
        let mut changed = observation.clone();
        changed[field] = value;
        assert!(
            _native::source_runtime_discovery::reconcile(&source, &coverage, &changed).is_err(),
            "{field}"
        );
    }
    for (index, value) in [
        (0, json!("")),
        (1, json!("../outside.py")),
        (2, json!("b".repeat(64))),
        (3, json!(0)),
        (3, json!("3")),
        (4, json!("false")),
        (5, json!("")),
        (5, Value::Null),
        (6, Value::Null),
        (6, json!("false")),
        (6, json!(0)),
    ] {
        let mut changed = observation.clone();
        changed["cases"][0][index] = value;
        assert!(
            _native::source_runtime_discovery::reconcile(&source, &coverage, &changed).is_err(),
            "{index}"
        );
    }
    let mut old_schema = observation.clone();
    old_schema["case_fields"].as_array_mut().unwrap().pop();
    for row in old_schema["cases"].as_array_mut().unwrap() {
        row.as_array_mut().unwrap().pop();
    }
    assert!(_native::source_runtime_discovery::reconcile(&source, &coverage, &old_schema).is_err());
    let mut missing_flag = observation.clone();
    missing_flag["cases"][0].as_array_mut().unwrap().pop();
    assert!(
        _native::source_runtime_discovery::reconcile(&source, &coverage, &missing_flag).is_err()
    );
    let mut duplicate = observation.clone();
    duplicate["cases"]
        .as_array_mut()
        .unwrap()
        .push(observation["cases"][0].clone());
    assert!(_native::source_runtime_discovery::reconcile(&source, &coverage, &duplicate).is_err());
    let mut unadmitted = source.clone();
    unadmitted["files"][0]["case_assertions"]["cases"][0]["defined_in_hash"] =
        json!("b".repeat(64));
    assert!(
        _native::source_runtime_discovery::reconcile(&unadmitted, &coverage, &observation).is_err()
    );
}

#[test]
fn pending_candidates_require_realistic_targets_but_never_count_as_verified() {
    let source = inventory();
    let candidate = json!({"target_file":"native/tests/intent_parity.rs",
        "target_case":"original_intent_cases_preserve_results_refusals_and_all_seven_invalid_targets",
        "fixture":"tests/fixtures/backend/intent.json","fixture_hash":"b".repeat(64),
        "remaining":"Native execution and source regeneration unverified"});
    let pending = json!({"baseline_revision":BASELINE,"cases":[{
        "source_file":"tests/test_source.py","source_case":"Parent.test_one",
        "source_hash":"a".repeat(64),"status":"pending","candidate":candidate
    }]});
    let report = _native::source_migration_coverage::validate(&source, &pending, false).unwrap();
    assert_eq!(report["verified"], 0);
    assert_eq!(report["candidates"], 1);
    assert_eq!(report["pending"].as_array().unwrap().len(), 1);
    assert_eq!(report["release_ready"], false);
    assert!(_native::source_migration_coverage::validate(&source, &pending, true).is_err());
    for (field, value) in [
        ("target_file", json!("../foreign.rs")),
        ("target_case", json!("")),
        ("fixture", json!("tests/fixtures/../private.json")),
        ("fixture", json!("tests/fixtures-sibling/private.json")),
        ("fixture_hash", json!("not-a-hash")),
        ("remaining", json!("")),
    ] {
        let mut broken = pending.clone();
        broken["cases"][0]["candidate"][field] = value;
        assert!(
            _native::source_migration_coverage::validate(&source, &broken, false).is_err(),
            "{field}"
        );
    }
    let mut null = pending.clone();
    null["cases"][0]["candidate"] = json!(null);
    assert!(_native::source_migration_coverage::validate(&source, &null, false).is_err());
}
