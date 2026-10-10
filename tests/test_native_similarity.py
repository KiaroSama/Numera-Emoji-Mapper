"""Exact source-oracle parity at the native computation seam."""

from __future__ import annotations

import json
from pathlib import Path
import unittest

from unittest import mock
from concurrent.futures import ThreadPoolExecutor

from tests._similarity_oracle import fixtures

RUNS_ON_NATIVE_WINDOWS = True


class NativeSimilarity(unittest.TestCase):
    def test_greedy_matches_source_fixtures(self):
        from emojikit import _native

        data = json.loads((Path(__file__).parent / "fixtures" / "similarity.json")
                          .read_text(encoding="utf-8"))
        self.assertTrue(data["greedy"])
        for case in data["greedy"]:
            with self.subTest(hashes=case["hashes"][:5]):
                self.assertEqual(_native.greedy_indices(case["hashes"]), case["expected"])

    def test_near_matches_source_fixtures(self):
        from emojikit import _native

        data = json.loads((Path(__file__).parent / "fixtures" / "similarity.json")
                          .read_text(encoding="utf-8"))
        self.assertTrue(data["near"])
        for case in data["near"]:
            with self.subTest(query=case["query"], threshold=case["threshold"]):
                self.assertEqual(_native.near_indices(case["hashes"], case["query"],
                                                       case["threshold"]), case["expected"])

    def test_source_fixtures_do_not_drift(self):
        data = json.loads((Path(__file__).parent / "fixtures" / "similarity.json")
                          .read_text(encoding="utf-8"))
        self.assertEqual(data, fixtures())

    def test_invalid_native_inputs_fail_before_computation(self):
        from emojikit import _native

        for value, error in ((-1, OverflowError), (2**64, OverflowError),
                             (1.5, TypeError), ("1", TypeError)):
            with self.subTest(value=value), self.assertRaises(error):
                _native.greedy_indices([value])
        for threshold in (-2, 17):
            with self.subTest(threshold=threshold), self.assertRaises(ValueError):
                _native.near_indices([0], 0, threshold)

    def test_missing_module_never_uses_a_python_fallback(self):
        from emojikit import similarity

        with mock.patch.object(similarity.importlib, "import_module", side_effect=ImportError):
            with self.assertRaisesRegex(RuntimeError, "scripts/build_native.py"):
                similarity.greedy_indices([0, 1])

    def test_missing_native_cannot_create_catalog_state(self):
        import tempfile
        from emojikit import similarity
        from tests.reference.catalog import Catalog

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "catalog.db"
            with mock.patch.object(similarity.importlib, "import_module", side_effect=ImportError):
                with self.assertRaisesRegex(RuntimeError, "scripts/build_native.py"):
                    Catalog(path)
            self.assertFalse(path.exists())

    def test_installer_fails_usefully_without_rust(self):
        import contextlib
        import io
        from scripts import build_native

        with mock.patch.object(build_native, "setup_logging"), \
                mock.patch.object(build_native.shutil, "which", return_value=None), \
                contextlib.redirect_stderr(io.StringIO()):
            with self.assertLogs("build_native", level="ERROR") as captured:
                self.assertEqual(build_native.main(), 2)
            self.assertIn("Rust 1.99+", captured.output[0])

    def test_independent_concurrent_calls_have_no_shared_state(self):
        from emojikit import _native

        cases = fixtures()["greedy"]
        with ThreadPoolExecutor(max_workers=2) as pool:
            calls = [pool.submit(_native.greedy_indices, case["hashes"]) for case in cases]
            self.assertEqual([call.result(timeout=5) for call in calls],
                             [case["expected"] for case in cases])

