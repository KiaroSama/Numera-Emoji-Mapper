//! Independent CPython loader observations, never imports/execution of repository source.
use serde_json::Value;

#[test]
fn source_discovery_preserves_inheritance_c3_shadowing_and_loader_prefix() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-discovery.json"
    ))
    .unwrap();
    assert_eq!(fixture["source_runtime"], "CPython 3.11.9");
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 7);
    for case in cases {
        let source = case["source"].as_str().unwrap();
        let metadata =
            _native::source_test_inventory::parse("tests/test_fixture.py", source).unwrap();
        assert_eq!(metadata["sha256"], case["sha256"]);
        let result = _native::source_test_discovery::discover("tests/test_fixture.py", source);
        if let Some(error) = case.get("error") {
            assert!(
                result
                    .unwrap_err()
                    .contains(error["kind"].as_str().unwrap()),
                "{}",
                case["name"]
            );
        } else {
            let result = result.unwrap();
            // The old fixture loads each class separately in definition order; module loading sorts names.
            let mut expected = case["expected"].as_array().unwrap().clone();
            expected.sort_by_key(|case| {
                (
                    case["class"].as_str().unwrap().to_owned(),
                    case["method"].as_str().unwrap().to_owned(),
                )
            });
            assert_eq!(
                result["cases"],
                serde_json::json!(expected),
                "{}",
                case["name"]
            );
            assert_eq!(result["discovery_complete"], true, "{}", case["name"]);
        }
    }
}

#[test]
fn source_module_collection_matches_final_bindings_and_run_test_fallback() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-bindings.json"
    ))
    .unwrap();
    assert_eq!(fixture["source_runtime"], "CPython 3.11.9");
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    for case in cases {
        let source = case["source"].as_str().unwrap();
        assert_eq!(
            _native::source_test_inventory::parse("tests/test_fixture.py", source).unwrap()["sha256"],
            case["sha256"]
        );
        let result =
            _native::source_test_discovery::discover("tests/test_fixture.py", source).unwrap();
        assert_eq!(result["cases"], case["expected"], "{}", case["name"]);
        assert_eq!(result["discovery_complete"], true, "{}", case["name"]);
    }
}

#[test]
fn incomplete_source_discovery_is_never_an_empty_complete_collection() {
    for source in [
        "from tests.shared import Parent\nclass Child(Parent):\n def test_one(self): pass\n",
        "from tests.shared import ImportedCase\n",
        "from tests.shared import *\n",
        "import unknown_module\n",
        "import unittest.missing_submodule\n",
        "@decorate\ndef helper(): pass\n",
        "def helper(value=install_test()): pass\n",
        "def helper(value: install_test()): pass\n",
        "def __getattr__(name): return make_test(name)\n",
        "def __dir__(): return ['ImportedCase']\n",
        "import unittest\nclass Hook(unittest.TestCase):\n def __init_subclass__(cls): cls.test_injected = lambda self: None\nclass Child(Hook): pass\n",
        "import unittest\nclass Case(unittest.TestCase):\n def test_one(self, value=install_test()): pass\n",
        "import unittest\nclass Case(unittest.TestCase):\n test_one: install_test()\n",
        "from unittest import TestCase\ndef TestCase(): pass\nclass Child(TestCase):\n def test_one(self): pass\n",
        "import unittest\nclass unittest: pass\nclass Child(unittest.TestCase):\n def test_one(self): pass\n",
        "class Child(unittest.TestCase):\n def test_one(self): pass\n",
        "import unittest\ndef unittest(): pass\nclass Child(unittest.TestCase):\n def test_one(self): pass\n",
        "import unittest\nfrom tests.shared import unittest\nclass Child(unittest.TestCase):\n def test_one(self): pass\n",
        "import unittest\n@decorate\nclass Case(unittest.TestCase):\n def test_one(self): pass\n",
        "import unittest\nclass Case(unittest.TestCase):\n @property\n def test_one(self): pass\n",
        "import unittest\nclass Case(unittest.TestCase):\n test_dynamic = factory()\n",
        "import unittest\nclass Parent(unittest.TestCase):\n def test_one(self): pass\nclass Child(Parent):\n __slots__ = 'test_one'\n",
        "import unittest\nclass Parent(unittest.TestCase):\n def test_one(self): pass\nclass Child(Parent):\n __slots__: str = 'test_one'\n",
        "import unittest\ndef load_tests(loader, suite, pattern): return suite\n",
        "import unittest\nif flag:\n class Case(unittest.TestCase):\n  def test_one(self): pass\n",
    ] {
        let result =
            _native::source_test_discovery::discover("tests/test_fixture.py", source).unwrap();
        assert_eq!(result["discovery_complete"], false, "{source}");
        assert!(
            !result["unresolved"].as_array().unwrap().is_empty(),
            "{source}"
        );
    }
    let result = _native::source_test_discovery::discover(
        "tests/test_fixture.py",
        "raise RuntimeError('module must not execute')\nclass Plain: pass\n",
    )
    .unwrap();
    assert_eq!(result["cases"], serde_json::json!([]));
}
