"""Pure native view parity; no HTTP-server cutover claim from these fixtures."""
import json
from pathlib import Path
import sys
import unittest
from unittest import mock

from tests.backend_source_oracle import validate_cases
from tests.backend_view_steps_oracle import VIEW_SOURCE, view_fixtures

RUNS_ON_NATIVE_WINDOWS = True
FIXTURE = Path(__file__).parent / "fixtures/backend/view.json"


class NativePanelView(unittest.TestCase):
    def test_visibility_membership_and_labels_match_source(self):
        from emojikit import _native

        data = json.loads(FIXTURE.read_text(encoding="utf-8"))
        validate_cases(data, source=VIEW_SOURCE, count=3)
        for case in data["cases"]:
            with self.subTest(name=case["name"]):
                self.assertEqual(json.loads(_native.panel_view_json(json.dumps(case["request"]))),
                                 case["expected"])

    def test_fixture_drift(self):
        self.assertEqual(json.loads(FIXTURE.read_text(encoding="utf-8")), view_fixtures())

    def test_generators_need_no_native_or_live_application_module(self):
        from tests.backend_oracle import generated_fixtures

        modules = {name: None for name in (
            "emojikit._native", "emojikit.catalog", "emojikit.operator_config",
            "emojikit.packstate", "emojikit.panel_plan", "emojikit.panel_view",
            "emojikit.plan_apply", "emojikit.collection_state")}
        with mock.patch.dict(sys.modules, modules):
            documents = generated_fixtures()
        self.assertEqual(set(documents), {"intent.json", "resume.json", "steps.json", "view.json"})
        for filename, document in documents.items():
            with self.subTest(fixture=filename):
                self.assertEqual(json.loads(FIXTURE.with_name(filename).read_text(encoding="utf-8")),
                                 document)
