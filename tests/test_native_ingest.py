"""Public native identity seam against hash-verified pre-port source decisions."""
import json
from pathlib import Path
import re
import tempfile
import unittest

from tests.backend_ingest_oracle import FIXTURE, SOURCE, fixtures
from tests.backend_source_oracle import validate_cases

RUNS_ON_NATIVE_WINDOWS = True
ROOT = Path(__file__).resolve().parent.parent


class NativeIngestParity(unittest.TestCase):
    def test_nominations_unknown_ambiguity_and_collision_match_source(self):
        from emojikit import _native

        document = json.loads(FIXTURE.read_text(encoding="utf-8"))
        validate_cases(document, source=SOURCE, count=8)
        self.assertEqual(document, fixtures())
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as directory:
            for index, case in enumerate(document["cases"]):
                with self.subTest(name=case["name"]):
                    catalog = _native.NativeCatalog(str(Path(directory) / str(index) / "catalog.db"),
                                                    "fixture UTC", str(ROOT))
                    calls = []
                    names = {f"rival-{n}": item["content_key"]
                             for n, item in enumerate(case["items"])}
                    def compare(a, b, fmt, names=names, calls=calls, case=case):
                        key = names[Path(a).name]
                        calls.append(key)
                        return case["comparison"][key]
                    try:
                        for n, item in enumerate(case["items"]):
                            catalog.call(json.dumps({"operation": "insert", "item": {
                                **item, "file_path": str(Path(directory) / f"rival-{n}")},
                                "utc": "fixture UTC"}))
                        request = json.dumps({"incoming": case["incoming"],
                                              "threshold": case["threshold"]})
                        if "error" in case:
                            with self.assertRaisesRegex(ValueError, r"\A" + re.escape(case["error"]) + r"\Z"):
                                catalog.identify(request, compare, lambda path, key, case=case: case["collision"])
                        else:
                            self.assertEqual(json.loads(catalog.identify(
                                request, compare, lambda path, key, case=case: case["collision"])), case["expected"])
                        self.assertEqual(calls, case["calls"])
                    finally:
                        catalog.close()
