//! Actual pinned source AST observations retain assertions and variant-bearing control contexts.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[test]
fn mock_assertions_retain_their_receiver_and_source_without_crediting_other_calls() {
    let evidence = _native::source_assertions::parse(
        "tests/test_mock_assertions.py",
        concat!(
            "class Case:\n",
            " def test_notification(self):\n",
            "  worker.assert_not_called()\n",
            "  tg.send_message.assert_called_once_with('message')\n",
            "  client.send('ordinary operation')\n",
            "  client.assert_unknown('not a known mock assertion')\n",
        ),
    )
    .unwrap();
    let assertions = evidence["methods"][0]["assertions"].as_array().unwrap();
    assert_eq!(assertions.len(), 2);
    assert_eq!(assertions[0]["source"], "worker.assert_not_called()");
    assert_eq!(
        assertions[1]["source"],
        "tg.send_message.assert_called_once_with('message')"
    );
}

#[test]
fn same_class_assertion_helpers_keep_exact_definition_and_call_evidence() {
    let source = concat!(
        "import unittest\nclass Case(unittest.TestCase):\n",
        " def _refused(self, value): self.fail('old helper')\n",
        " def _refused(self, value): self.assertEqual(value, 'denied')\n",
        " def test_refusal(self): self._refused('denied')\n",
    );
    let inspect = |text: &str| {
        let hash = Sha256::digest(text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let corpus = [_native::source_git_inventory::Source {
            baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
            path: "tests/test_helpers.py".into(),
            sha256: hash,
            text: text.into(),
        }];
        _native::source_git_inventory::inventory(&corpus).unwrap()
    };
    let inventory = inspect(source);
    let file = &inventory["files"][0];
    let case = &file["case_assertions"]["cases"][0];
    assert_eq!(case["source_case"], "Case.test_refusal");
    assert_eq!(case["method"]["assertions"], json!([]));
    let helpers = case["helper_assertions"].as_array().unwrap();
    assert_eq!(helpers.len(), 1);
    let helper = &helpers[0];
    assert_eq!(helper["defined_in_identity"], "tests/test_helpers.py:2");
    assert_eq!(helper["defined_in_hash"], file["sha256"]);
    assert_eq!(helper["method_line"], 4);
    assert_eq!(helper["method"]["method"], "_refused");
    assert_eq!(
        helper["method"]["assertions"][0]["source"],
        "self.assertEqual(value, 'denied')"
    );
    assert_eq!(helper["call"]["source"], "self._refused('denied')");
    assert_eq!(helper["call"]["line"], 5);
    assert_eq!(file["case_assertions"]["runtime_variants_complete"], false);
    let ordinary = inspect(&source.replace("_refused", "refuse"));
    assert_eq!(
        ordinary["files"][0]["case_assertions"]["cases"][0]["helper_assertions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for text in [
        "import unittest\nclass Other:\n def _refused(self, value): self.fail('not this class')\nclass Case(unittest.TestCase):\n def test_refusal(self): self._refused('denied')\n",
        "import unittest\nclass Case(unittest.TestCase):\n def _refused(self, value): return value\n def test_refusal(self): self._refused('denied')\n",
        "import unittest\nclass Case(unittest.TestCase):\n def _refused(self, value): self.fail('shadowed')\n _refused = None\n def test_refusal(self): self._refused('denied')\n",
        "import unittest\nclass Case(unittest.TestCase):\n @staticmethod\n def _refused(value): assert False\n def test_refusal(self): self._refused('denied')\n",
        "import unittest\nclass Parent(unittest.TestCase):\n def _refused(self, value): self.fail('inherited helper not resolved')\nclass Case(Parent):\n def test_refusal(self): self._refused('denied')\n",
        "import unittest\nclass Case(unittest.TestCase):\n def test_refusal(self):\n  self.test_refusal()\n  self.assertTrue(True)\n",
        "import unittest\nclass Case(unittest.TestCase):\n def _refused(self, value): self.fail('nested call is not executed')\n def test_refusal(self):\n  def never(): self._refused('denied')\n",
        "import unittest\nclass Case(unittest.TestCase):\n def _refused(self, value): self.fail('different receiver')\n def test_refusal(self):\n  self = object()\n  self._refused('denied')\n",
        "import unittest\nclass Case(unittest.TestCase):\n def _refused(self, value): self.fail('replaced instance helper')\n def test_refusal(self):\n  self._refused = lambda value: None\n  self._refused('denied')\n",
        "import unittest\nclass Case(unittest.TestCase):\n def _refused(self, value):\n  def never(): self.fail('unexecuted helper assertion')\n def test_refusal(self): self._refused('denied')\n",
    ] {
        let inventory = inspect(text);
        let cases = inventory["files"][0]["case_assertions"]["cases"]
            .as_array()
            .unwrap();
        let case = cases
            .iter()
            .find(|case| case["source_case"] == "Case.test_refusal")
            .unwrap();
        assert_eq!(case["helper_assertions"], json!([]), "{text}");
    }
}

#[test]
fn helper_evidence_uses_outer_assertions_without_crediting_nested_scopes() {
    let inspect = |source: &str| {
        let hash = Sha256::digest(source.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        _native::source_git_inventory::inventory(&[_native::source_git_inventory::Source {
            baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
            path: "tests/test_lexical_helpers.py".into(),
            sha256: hash,
            text: source.into(),
        }])
        .unwrap()
    };
    let source = concat!(
        "import unittest\nclass Case(unittest.TestCase):\n",
        " def _publication(self):\n",
        "  class Handler:\n",
        "   def request(self):\n",
        "    self.closed = True\n",
        "    self.assertFalse('nested assertion')\n",
        "  self.assertEqual('outer', 'outer')\n",
        " def test_publication(self): self._publication()\n",
    );
    let inventory = inspect(source);
    let file = &inventory["files"][0];
    let case = &file["case_assertions"]["cases"][0];
    assert_eq!(case["method"]["assertions"], json!([]));
    let helpers = case["helper_assertions"].as_array().unwrap();
    assert_eq!(helpers.len(), 1);
    let helper = &helpers[0];
    assert_eq!(helper["method"]["assertions"].as_array().unwrap().len(), 2);
    let direct = helper["direct_assertions"].as_array().unwrap();
    assert_eq!(direct.len(), 1);
    assert_eq!(direct[0]["source"], "self.assertEqual('outer', 'outer')");
    assert_eq!(direct[0]["line"], 8);
    assert_eq!(helper["call"]["source"], "self._publication()");
    assert_eq!(helper["defined_in_hash"], file["sha256"]);
    for body in [
        "  class Handler:\n   def request(self): self.assertTrue('nested only')\n",
        "  def never(): self.assertTrue('nested only')\n",
        "  never = lambda: self.assertTrue('nested only')\n",
        "  def _publication(): pass\n  self.assertTrue(True)\n",
        "  def self(): pass\n  self.assertTrue(True)\n",
        "  class self: pass\n  self.assertTrue(True)\n",
        "  self = object()\n  self.assertTrue(True)\n",
        "  self._publication = object()\n  self.assertTrue(True)\n",
        "  self._publication()\n  self.assertTrue(True)\n",
        "  import other as self\n  self.assertTrue(True)\n",
        "  from other import replacement as self\n  self.assertTrue(True)\n",
        "  try:\n   pass\n  except Exception as self:\n   pass\n  self.assertTrue(True)\n",
        "  def change():\n   nonlocal self\n   self = object()\n  change()\n  self.assertTrue(True)\n",
    ] {
        let text = format!(
            "import unittest\nclass Case(unittest.TestCase):\n def _publication(self):\n{body} def test_publication(self): self._publication()\n"
        );
        let inventory = inspect(&text);
        let case = &inventory["files"][0]["case_assertions"]["cases"][0];
        assert_eq!(case["helper_assertions"], json!([]), "{text}");
    }
}

#[test]
fn code_first_lines_follow_method_decorators_without_changing_definition_identity() {
    let source = concat!(
        "import unittest\n",
        "from unittest import mock\n",
        "class Parent(unittest.TestCase):\n",
        " def test_plain(self): self.assertTrue(True)\n",
        " @mock.patch(\n",
        "  'builtins.print'\n",
        " )\n",
        " @mock.patch('builtins.input')\n",
        " def test_decorated(self, *args): self.assertTrue(True)\n",
        " @\\\n",
        " mock.patch('builtins.print')\n",
        " async def test_async(self, *args): self.assertTrue(True)\n",
        " @mock.patch('builtins.print')\n",
        " def test_final(self, *args): self.fail('old')\n",
        " def test_final(self): self.assertEqual('final', 'final')\n",
        " def test_shadowed(self): self.fail('old')\n",
        " @mock.patch('builtins.print')\n",
        " def test_shadowed(self, *args): self.assertTrue(True)\n",
        "class Child(Parent): pass\n",
        "@mock.patch('builtins.print')\n",
        "class ClassPatched(unittest.TestCase):\n",
        " def test_class_patch(self, *args): self.assertTrue(True)\n",
    );
    let hash = Sha256::digest(source.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let corpus = [_native::source_git_inventory::Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: "tests/test_code_lines.py".into(),
        sha256: hash,
        text: source.into(),
    }];
    let inventory = _native::source_git_inventory::inventory(&corpus).unwrap();
    let file = &inventory["files"][0];
    let origins = file["assertions"]["method_origins"].as_array().unwrap();
    assert_eq!(
        origins
            .iter()
            .map(|origin| json!([
                origin["method"],
                origin["method_line"],
                origin["code_first_line"]
            ]))
            .collect::<Vec<_>>(),
        vec![
            json!(["test_plain", 4, 4]),
            json!(["test_decorated", 9, 5]),
            json!(["test_async", 12, 11]),
            json!(["test_final", 14, 13]),
            json!(["test_final", 15, 15]),
            json!(["test_shadowed", 16, 16]),
            json!(["test_shadowed", 18, 17]),
            json!(["test_class_patch", 22, 22]),
        ]
    );
    assert_eq!(origins[7]["class_identity"], "tests/test_code_lines.py:21");
    let cases = file["case_assertions"]["cases"].as_array().unwrap();
    for (name, class_line, owner_line, line) in [
        ("Parent.test_plain", 3, 3, 4),
        ("Child.test_plain", 19, 3, 4),
        ("Parent.test_final", 3, 3, 15),
        ("Child.test_final", 19, 3, 15),
        ("ClassPatched.test_class_patch", 21, 21, 22),
    ] {
        let case = cases
            .iter()
            .find(|case| case["source_case"] == name)
            .unwrap();
        assert_eq!(
            case["class_identity"],
            format!("tests/test_code_lines.py:{class_line}")
        );
        assert_eq!(
            case["defined_in_identity"],
            format!("tests/test_code_lines.py:{owner_line}")
        );
        assert_eq!(case["defined_in_file"], file["source"]);
        assert_eq!(case["defined_in_hash"], file["sha256"]);
        assert_eq!(case["method_line"], line);
        assert_eq!(case["code_first_line"], line);
        if name.ends_with(".test_final") {
            assert_eq!(
                case["method"]["assertions"][0]["source"],
                "self.assertEqual('final', 'final')"
            );
        }
    }
    // Syntax provenance never promotes a decorated callable from static Unknown.
    assert_eq!(cases.len(), 5);
    let declarations = file["case_assertions"]["unresolved_declarations"]
        .as_array()
        .unwrap();
    assert_eq!(declarations.len(), 5);
    for (name, line, first) in [
        ("Parent.test_decorated", 9, 5),
        ("Parent.test_async", 12, 11),
        ("Parent.test_final", 14, 13),
        ("Parent.test_shadowed", 16, 16),
        ("Parent.test_shadowed", 18, 17),
    ] {
        let declaration = declarations
            .iter()
            .find(|entry| entry["source_case"] == name && entry["method_line"] == line)
            .unwrap();
        assert_eq!(declaration["code_first_line"], first);
        assert_eq!(
            declaration["defined_in_identity"],
            "tests/test_code_lines.py:3"
        );
        assert_eq!(declaration["defined_in_hash"], file["sha256"]);
        assert_eq!(declaration["binding_resolved"], false);
        assert_eq!(declaration["evidence_kind"], "declaration-only");
        assert!(
            !declaration["method"]["assertions"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            declaration["helper_assertions"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(!cases.iter().any(|case| case["source_case"] == name
            && case["defined_in_identity"] == declaration["defined_in_identity"]
            && case["method_line"] == line));
    }
    assert_eq!(file["case_assertions"]["runtime_variants_complete"], false);
    assert_eq!(inventory["discovery_complete"], false);
    for invalid in [Value::Null, json!(0), json!(5)] {
        let mut files = inventory["files"].as_array().unwrap().clone();
        files[0]["assertions"]["method_origins"][0]["code_first_line"] = invalid;
        assert!(_native::source_assertions::resolve_cases(&files).is_err());
    }
}

#[test]
fn source_assertions_keep_exact_spans_subtests_and_control_expressions() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-assertions.json"
    ))
    .unwrap();
    assert_eq!(fixture["source_runtime"], "CPython 3.11.9");
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3);
    // The existing pinned-corpus test checks the two real modules without another Git scan.
    let synthetic = cases
        .iter()
        .filter(|case| case["source"].is_string())
        .collect::<Vec<_>>();
    assert_eq!(synthetic.len(), 1);
    for case in synthetic {
        let path = case["path"].as_str().unwrap();
        let source = case["source"].as_str().unwrap();
        let observed = _native::source_assertions::parse(path, source).unwrap();
        assert_eq!(observed["sha256"], case["sha256"], "{path}");
        assert_eq!(observed["methods"], case["methods"], "{path}");
        assert_eq!(observed["runtime_variants_complete"], false);
    }
    for path in ["../outside.py", "/outside.py", "tests\\bad.py", ""] {
        assert!(_native::source_assertions::parse(path, "pass\n").is_err());
    }
    assert!(_native::source_assertions::parse("tests/bad.py", "class Broken(\n").is_err());
    assert!(
        _native::source_assertions::parse("tests/large.py", &" ".repeat(2 * 1024 * 1024 + 1))
            .is_err()
    );
}
