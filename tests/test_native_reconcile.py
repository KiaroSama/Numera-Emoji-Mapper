"""Live identity nomination uses exact source evidence, not catalog-wide guessing."""
import json
import unittest

from tests.backend_reconcile_oracle import FIXTURE, SOURCE, fixtures
from tests.backend_source_oracle import validate_cases

RUNS_ON_NATIVE_WINDOWS = True


class NativeReconcileParity(unittest.TestCase):
    def test_near_candidates_and_unknown_results_match_source(self):
        from emojikit import _native

        document = json.loads(FIXTURE.read_text(encoding="utf-8"))
        validate_cases(document, source=SOURCE, count=8)
        self.assertEqual(document, fixtures())
        for case in document["cases"]:
            calls = []
            def compare(key, case=case, calls=calls):
                calls.append(key)
                return case["comparison"][key]
            with self.subTest(name=case["name"]):
                actual = json.loads(_native.near_catalog_json(json.dumps(case), compare))
                self.assertEqual(actual, case["expected"])
                self.assertEqual(calls, case["calls"])
