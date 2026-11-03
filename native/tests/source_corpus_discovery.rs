//! Imported classes are candidates from pinned syntax, never permission to execute test modules.
use _native::source_git_inventory::Source;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";

fn source(path: &str, text: &str) -> Source {
    Source {
        baseline_revision: BASELINE.into(),
        path: path.into(),
        sha256: Sha256::digest(text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        text: text.into(),
    }
}

#[test]
fn original_inheritance_observations_survive_a_module_import_boundary() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-discovery.json"
    ))
    .unwrap();
    // Keep the independently observed class bodies and outcomes unchanged. Only move the base
    // definitions across a module boundary; the loader still visits the same class identities.
    for (name, split, imported) in [
        ("inherited override", "class Child", "Parent"),
        ("diamond follows C3", "class Diamond", "Left, Right"),
        ("mixin and plain helper", "class Concrete", "Mixin, Case"),
        (
            "noncallable shadows inherited method",
            "class Child",
            "Parent",
        ),
    ] {
        let case = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap();
        let text = case["source"].as_str().unwrap();
        let at = text.find(split).unwrap();
        let sources = [
            source("tests/shared.py", &text[..at]),
            source(
                "tests/test_child.py",
                &format!("from .shared import {imported}\n{}", &text[at..]),
            ),
        ];
        let found = _native::source_module_discovery::discover(&sources).unwrap();
        let child = &found["tests/test_child.py"];
        let class = split.strip_prefix("class ").unwrap();
        let expected = case["expected"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["class"] == class)
            .cloned()
            .collect::<Vec<_>>();
        let actual = child["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["class"] == class)
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{name}");
        assert_eq!(
            child["discovery_complete"], false,
            "imports were not executed"
        );
        assert!(
            child["unresolved"]
                .as_array()
                .unwrap()
                .iter()
                .any(|issue| { issue.as_str().unwrap().contains("import") })
        );
    }
}

#[test]
fn imported_test_classes_are_collected_but_not_imported_module_objects() {
    let base = source(
        "tests/shared.py",
        "import unittest\nclass Parent(unittest.TestCase):\n def test_one(self): pass\n",
    );
    for (text, expected) in [
        (
            "from tests.shared import Parent\n",
            json!([{"class":"Parent","method":"test_one","defined_in":"Parent"}]),
        ),
        ("import tests.shared\n", json!([])),
        ("from . import shared\n", json!([])),
        (
            "from . import shared as fixtures\nclass Child(fixtures.Parent): pass\n",
            json!([{"class":"Child","method":"test_one","defined_in":"Parent"}]),
        ),
        (
            "import tests.shared as fixtures\nclass Child(fixtures.Parent): pass\n",
            json!([{"class":"Child","method":"test_one","defined_in":"Parent"}]),
        ),
        (
            "import tests.shared\nclass Child(tests.shared.Parent): pass\n",
            json!([{"class":"Child","method":"test_one","defined_in":"Parent"}]),
        ),
    ] {
        let sources = [
            source(&base.path, &base.text),
            source("tests/test_child.py", text),
        ];
        let found = _native::source_module_discovery::discover(&sources).unwrap();
        assert_eq!(found["tests/test_child.py"]["cases"], expected, "{text}");
        assert_eq!(found["tests/test_child.py"]["discovery_complete"], false);
    }
}

#[test]
fn definition_identity_is_not_confused_by_reexports_or_same_named_classes() {
    let sources = [
        source(
            "tests/left.py",
            "import unittest\nclass Base(unittest.TestCase):\n def test_left(self): pass\n",
        ),
        source(
            "tests/right.py",
            "import unittest\nclass Base(unittest.TestCase):\n def test_right(self): pass\n",
        ),
        source(
            "tests/exports/__init__.py",
            "from ..left import Base as Left\nfrom ..right import Base as Right\n",
        ),
        source(
            "tests/test_child.py",
            "from .exports import Left, Right\nclass Child(Left, Right): pass\nclass Left: pass\n",
        ),
    ];
    let found = _native::source_module_discovery::discover(&sources).unwrap();
    assert_eq!(
        found["tests/test_child.py"]["cases"],
        json!([
            {"class":"Child","method":"test_left","defined_in":"Base"},
            {"class":"Child","method":"test_right","defined_in":"Base"},
            {"class":"Base","method":"test_right","defined_in":"Base"}
        ])
    );
    assert_eq!(
        found["tests/test_child.py"]["case_origins"],
        json!([
            {"binding":"Child","class_identity":"tests/test_child.py:2","method":"test_left","defined_in_identity":"tests/left.py:2","method_line":3},
            {"binding":"Child","class_identity":"tests/test_child.py:2","method":"test_right","defined_in_identity":"tests/right.py:2","method_line":3},
            {"binding":"Right","class_identity":"tests/right.py:2","method":"test_right","defined_in_identity":"tests/right.py:2","method_line":3}
        ])
    );
}

#[test]
fn inherited_assertions_follow_definition_identity_not_class_name_or_final_rebinding() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-assertions.json"
    ))
    .unwrap();
    let captured = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["source"].is_string())
        .unwrap();
    // The observed assertion spans stay byte-for-byte unchanged in the original class body.
    // A later class with the same name must not steal Saved's inherited method evidence.
    let shared = format!(
        "{}class Saved(Case): pass\nclass Case:\n def test_unicode(self): self.fail('later uncollected definition')\n",
        captured["source"].as_str().unwrap()
    );
    let sources = [
        source("tests/shared.py", &shared),
        source(
            "tests/test_child.py",
            "import unittest\nfrom .shared import Saved\nclass Child(Saved, unittest.TestCase): pass\n",
        ),
    ];
    let inventory = _native::source_git_inventory::inventory(&sources).unwrap();
    let file = inventory["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["source"] == "tests/test_child.py")
        .unwrap();
    let evidence = &file["case_assertions"];
    assert_eq!(evidence["cases"].as_array().unwrap().len(), 1);
    assert_eq!(evidence["unresolved"], json!([]));
    assert_eq!(evidence["runtime_variants_complete"], false);
    let case = &evidence["cases"][0];
    assert_eq!(case["source_case"], "Child.test_unicode");
    assert_eq!(case["class_identity"], "tests/test_child.py:3");
    assert_eq!(case["defined_in_identity"], "tests/shared.py:1");
    assert_eq!(case["defined_in_file"], "tests/shared.py");
    assert_eq!(case["defined_in_hash"], sources[0].sha256);
    assert_eq!(case["method_line"], 2);
    assert_eq!(case["method"], captured["methods"][0]);
    assert_eq!(inventory["discovery_complete"], false);

    // Repeated methods in one class bind to the last declaration, not the first.
    let replacement = source(
        "tests/test_replaced.py",
        "import unittest\nclass Case(unittest.TestCase):\n def test_one(self): self.fail('first')\n def test_one(self): self.fail('last')\n",
    );
    let replaced = _native::source_git_inventory::inventory(&[replacement]).unwrap();
    let case = &replaced["files"][0]["case_assertions"]["cases"][0];
    assert_eq!(case["method_line"], 4);
    assert_eq!(
        case["method"]["assertions"][0]["source"],
        "self.fail('last')"
    );

    let mut changed = inventory["files"].as_array().unwrap().clone();
    let child = changed
        .iter_mut()
        .find(|file| file["source"] == "tests/test_child.py")
        .unwrap();
    child["discovery"]["case_origins"][0]["defined_in_identity"] = json!("tests/shared.py:999");
    let unresolved = _native::source_assertions::resolve_cases(&changed).unwrap();
    assert_eq!(unresolved["tests/test_child.py"]["cases"], json!([]));
    assert_eq!(
        unresolved["tests/test_child.py"]["unresolved"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let bindings: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-bindings.json"
    ))
    .unwrap();
    let fallback = bindings["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "noncallable runTest still collected")
        .unwrap();
    let text = fallback["source"].as_str().unwrap().replace(
        " runTest = None", " def runTest(self): self.fail('replaced method must not supply evidence')\n runTest = None",
    );
    let masked =
        _native::source_git_inventory::inventory(&[source("tests/test_masked.py", &text)]).unwrap();
    let file = &masked["files"][0];
    assert_eq!(file["discovery"]["cases"], fallback["expected"]);
    assert_eq!(file["case_assertions"]["cases"], json!([]));
    assert_eq!(
        file["case_assertions"]["unresolved"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // An evidence-index mismatch is invalid input, not a silently truncated zip/join.
    changed[0]["assertions"]["method_origins"] = json!([]);
    assert!(_native::source_assertions::resolve_cases(&changed).is_err());
}

#[test]
fn missing_cyclic_dynamic_and_ambiguous_modules_never_certify_complete_discovery() {
    for sources in [
        vec![source(
            "tests/test_child.py",
            "from .missing import Parent\nclass Child(Parent): pass\n",
        )],
        vec![
            source("tests/left.py", "from .right import Parent\n"),
            source("tests/right.py", "from .left import Parent\n"),
            source(
                "tests/test_child.py",
                "from .left import Parent\nclass Child(Parent): pass\n",
            ),
        ],
        vec![
            source(
                "tests/shared.py",
                "raise RuntimeError('must never execute')\n",
            ),
            source(
                "tests/test_child.py",
                "from .shared import Parent\nclass Child(Parent): pass\n",
            ),
        ],
        vec![source(
            "test_child.py",
            "from ..missing import Parent\nclass Child(Parent): pass\n",
        )],
    ] {
        let found = _native::source_module_discovery::discover(&sources).unwrap();
        let child = sources.last().unwrap();
        assert_eq!(found[&child.path]["discovery_complete"], false);
        assert!(
            !found[&child.path]["unresolved"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    assert!(
        _native::source_module_discovery::discover(&[
            source("tests/shared.py", "pass\n"),
            source("tests/shared/__init__.py", "pass\n"),
        ])
        .unwrap_err()
        .contains("ambiguous")
    );
    let shared_unknown = [
        source(
            "tests/shared.py",
            "class Unknown(External): pass\nclass Left(Unknown): pass\nclass Right(Unknown): pass\n",
        ),
        source(
            "tests/test_child.py",
            "from .shared import Left, Right\nclass Child(Left, Right): pass\n",
        ),
    ];
    let unresolved = _native::source_module_discovery::discover(&shared_unknown).unwrap();
    assert_eq!(
        unresolved["tests/test_child.py"]["discovery_complete"],
        false
    );
    let mut forged = source("tests/test_child.py", "pass\n");
    forged.sha256 = "0".repeat(64);
    assert!(_native::source_module_discovery::discover(&[forged]).is_err());
}
