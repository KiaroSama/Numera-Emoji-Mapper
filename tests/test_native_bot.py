"""Public native bot pure contracts and hash-verified source rendering fixtures."""
import json
import unittest

from tests.backend_bot_oracle import FIXTURE, SOURCE, fixtures
from tests.backend_source_oracle import validate_cases

RUNS_ON_NATIVE_WINDOWS = True


class NativeBotPayloads(unittest.TestCase):
    def test_render_parse_extract_and_copy_batches_match_source(self):
        from emojikit import _native

        document = json.loads(FIXTURE.read_text(encoding="utf-8"))
        validate_cases(document, source=SOURCE, count=6)
        self.assertEqual(document, fixtures())
        for case in document["cases"]:
            with self.subTest(name=case["name"]):
                self.assertEqual(json.loads(_native.bot_json(json.dumps(case))), case["expected"])
