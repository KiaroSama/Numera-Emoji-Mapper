"""Public native live-roster entries preserve source, current and previous IDs."""
import json
import unittest
from tests.backend_roster_oracle import FIXTURE, SOURCE, fixtures
from tests.backend_source_oracle import validate_cases
RUNS_ON_NATIVE_WINDOWS = True


class NativeRosterRows(unittest.TestCase):
    def test_gallery_and_whole_roster_fixtures_are_source_generated(self):
        from tests.backend_gallery_oracle import FIXTURE as gallery_path, fixtures as gallery
        from tests.backend_roster_pack_oracle import FIXTURE as roster_path, fixtures as roster
        from tests.backend_archive_oracle import FIXTURE as archive_path, fixtures as archive
        from tests.backend_state_artifacts_oracle import FIXTURE as state_path, fixtures as state
        from tests.backend_migration_database_oracle import FIXTURE as database_path, fixtures as database
        from tests.backend_signature_oracle import FIXTURE as signature_path, fixtures as signature
        self.assertEqual(json.loads(gallery_path.read_text(encoding="utf-8")), gallery())
        self.assertEqual(json.loads(roster_path.read_text(encoding="utf-8")), roster())
        self.assertEqual(json.loads(archive_path.read_text(encoding="utf-8")), archive())
        self.assertEqual(json.loads(state_path.read_text(encoding="utf-8")), state())
        self.assertEqual(json.loads(database_path.read_text(encoding="utf-8")), database())
        self.assertEqual(json.loads(signature_path.read_text(encoding="utf-8")), signature())

    def test_ids_format_and_history_match_live_source_contract(self):
        from emojikit import _native
        document = json.loads(FIXTURE.read_text(encoding="utf-8"))
        validate_cases(document, source=SOURCE, count=4)
        self.assertEqual(document, fixtures())
        for case in document["cases"]:
            with self.subTest(name=case["name"]):
                self.assertEqual(json.loads(_native.roster_row_json(json.dumps(case))), case["expected"])
