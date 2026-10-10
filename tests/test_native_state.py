"""Public JSON parity for the native pack-intent domain."""

import json
import unittest

from tests.backend_oracle import FIXTURE, fixtures, resume_fixtures

RUNS_ON_NATIVE_WINDOWS = True


class NativeIntent(unittest.TestCase):
    def test_intent_matches_pinned_source(self):
        from emojikit import _native

        data = json.loads(FIXTURE.read_text(encoding="utf-8"))
        self.assertTrue(data["cases"])
        for case in data["cases"]:
            with self.subTest(name=case["name"]):
                request = {k: v for k, v in case.items()
                           if k not in ("name", "expected", "error")}
                request["written_utc"] = data["utc"]
                if "error" in case:
                    with self.assertRaisesRegex(ValueError, case["error"]):
                        _native.intent_json(json.dumps(request, ensure_ascii=False))
                else:
                    result = _native.intent_json(json.dumps(request, ensure_ascii=False))
                    self.assertEqual(json.loads(result), case["expected"])

    def test_fixture_drift(self):
        self.assertEqual(json.loads(FIXTURE.read_text(encoding="utf-8")), fixtures())
        self.assertEqual(json.loads(FIXTURE.with_name("resume.json").read_text(encoding="utf-8")),
                         resume_fixtures())

    def test_collection_resume_matches_source(self):
        from pathlib import Path
        from emojikit import _native

        data = json.loads((Path(__file__).parent / "fixtures/backend/resume.json")
                          .read_text(encoding="utf-8"))
        self.assertTrue(data["cases"])
        for case in data["cases"]:
            with self.subTest(name=case["name"]):
                request = json.dumps({"operation": "collection", "base": "fixture",
                                      "state": case["state"]})
                if "error" in case:
                    with self.assertRaises(ValueError):
                        _native.resume_json(request)
                else:
                    self.assertEqual(json.loads(_native.resume_json(request)), case["expected"])

    def test_invalid_targets_are_refused(self):
        from emojikit import _native

        for value in (True, False, 0, -1, 1.5, "1", None):
            with self.subTest(value=value), self.assertRaises(ValueError):
                _native.intent_json(json.dumps({"operation": "targets", "previous": {
                    "targets": [["a", value]], "moves": [], "held": []}}))
