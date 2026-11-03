//! Artifact drift refuses metadata-only retirement, even when a ledger says verified.
use _native::source_git_inventory::Source;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::Path;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn write(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}
#[test]
fn coverage_binds_actual_source_fixture_target_and_case_receipt() {
    let owned = tooling_fixture::Root::new("source coverage artifacts");
    let root = &owned.0;
    let source = "import unittest\nclass Case(unittest.TestCase):\n def test_one(self): pass\n";
    let sources = vec![Source {
        baseline_revision: BASELINE.into(),
        path: "tests/test_fixture.py".into(),
        sha256: hash(source.as_bytes()),
        text: source.into(),
    }];
    let target = "native/tests/fixture_case.rs";
    let fixture = "tests/fixtures/native/fixture.json";
    let body = b"#[test]\nfn observed_fixture_case() {}\n";
    write(root, target, body);
    write(root, fixture, b"{\"independent\":true}\n");
    let revision = "a".repeat(40);
    let evidence = json!({"result":"verified","baseline_revision":BASELINE,"target_revision":revision,
        "platform":"synthetic-artifact-policy","command":"synthetic command","run_identity":"synthetic-run"});
    let coverage = json!({"baseline_revision":BASELINE,"cases":[{
        "source_file":"tests/test_fixture.py","source_case":"Case.test_one","source_hash":hash(source.as_bytes()),"status":"parity",
        "target_file":target,"target_case":"observed_fixture_case","target_hash":hash(body),
        "fixture":fixture,"fixture_hash":hash(b"{\"independent\":true}\n"),"evidence":evidence
    }]});
    let receipt = json!({"result":"verified","baseline_revision":BASELINE,"target_revision":revision,
        "platform":"synthetic-artifact-policy","command":"synthetic command","run_identity":"synthetic-run",
        "passed_cases":[{"target_file":target,"target_case":"observed_fixture_case","target_hash":hash(body),
            "fixture":fixture,"fixture_hash":hash(b"{\"independent\":true}\n")} ]});
    assert!(
        _native::source_coverage_artifacts::verify(
            root, &sources, &coverage, &receipt, &revision, true
        )
        .is_ok()
    );
    write(root, fixture, b"changed\n");
    assert!(
        _native::source_coverage_artifacts::verify(
            root, &sources, &coverage, &receipt, &revision, true
        )
        .unwrap_err()
        .contains("fixture")
    );
    write(root, fixture, b"{\"independent\":true}\n");
    write(root, target, b"changed\n");
    assert!(
        _native::source_coverage_artifacts::verify(
            root, &sources, &coverage, &receipt, &revision, true
        )
        .unwrap_err()
        .contains("target")
    );
    write(root, target, body);
    let mut failed = receipt.clone();
    failed["result"] = json!("authored");
    assert!(
        _native::source_coverage_artifacts::verify(
            root, &sources, &coverage, &failed, &revision, true
        )
        .is_err()
    );
    let mut absent = receipt.clone();
    absent["passed_cases"] = json!([]);
    assert!(
        _native::source_coverage_artifacts::verify(
            root, &sources, &coverage, &absent, &revision, true
        )
        .is_err()
    );
    assert!(
        _native::source_coverage_artifacts::verify(
            root,
            &sources,
            &coverage,
            &receipt,
            &"b".repeat(40),
            true
        )
        .is_err()
    );
    owned.clean().unwrap();
}
