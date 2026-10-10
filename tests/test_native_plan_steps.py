"""Execution-preview parity and reproducible pinned-source fixture checks."""
import copy
import json
from pathlib import Path
import re
import unittest
from unittest import mock

from tests.backend_source_oracle import SOURCE_HASHES, validate_cases, verify_sources
from tests.backend_view_steps_oracle import STEPS_SOURCE, steps_fixtures

RUNS_ON_NATIVE_WINDOWS = True
FIXTURE = Path(__file__).parent / "fixtures/backend/steps.json"


class NativePlanSteps(unittest.TestCase):
    def test_moves_holding_reorder_and_capacity_match_source(self):
        from emojikit import _native

        data = json.loads(FIXTURE.read_text(encoding="utf-8"))
        validate_cases(data, source=STEPS_SOURCE, count=6)
        for case in data["cases"]:
            with self.subTest(name=case["name"]):
                request = json.dumps({key: value for key, value in case.items()
                                      if key not in {"name", "expected", "error"}})
                if "error" in case:
                    with self.assertRaisesRegex(ValueError, r"\A" + re.escape(case["error"]) + r"\Z"):
                        _native.plan_steps_json(request)
                else:
                    self.assertEqual(json.loads(_native.plan_steps_json(request)), case["expected"])

    def test_fixture_drift(self):
        self.assertEqual(json.loads(FIXTURE.read_text(encoding="utf-8")), steps_fixtures())

    def test_pinned_source_drift_fails_before_execution(self):
        verify_sources()
        with mock.patch.dict(SOURCE_HASHES, {"plan_apply.py": "0" * 64}):
            with self.assertRaisesRegex(ValueError, "Pinned oracle source drift"):
                steps_fixtures()

    def test_empty_duplicated_or_unannotated_cases_cannot_pass(self):
        data = json.loads(FIXTURE.read_text(encoding="utf-8"))
        for mutation in ("empty", "duplicate", "unannotated", "both", "wrong-source"):
            broken = copy.deepcopy(data)
            if mutation == "empty":
                broken["cases"] = []
            elif mutation == "duplicate":
                broken["cases"][1]["name"] = broken["cases"][0]["name"]
            elif mutation == "unannotated":
                broken["cases"][0].pop("expected")
            elif mutation == "both":
                broken["cases"][0]["error"] = "not a source result"
            else:
                broken["source"] = "unverified"
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                validate_cases(broken, source=STEPS_SOURCE, count=6)
