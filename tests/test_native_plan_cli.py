"""Native plan dry-run reports intent without mutating catalog, state or plan."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

from tests.reference.catalog import Catalog
from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativePlanCommand(unittest.TestCase):
    def test_dry_run_reads_existing_data_without_creating_or_rewriting_files(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder).resolve()
            data = folder / "collection"
            with Catalog(data / "catalog.db") as catalog:
                with mock.patch("tests.reference.catalog.catalog_identity",
                                side_effect=lambda cat, key, fmt, path, phash: (key, True)):
                    for key in ("a", "b"):
                        catalog.add(content_key=key, fmt="static", file_path=folder / f"{key}.png")
                catalog.mark_uploaded("a", "111111111", base="fixture", set_name="fixture1_by_YourEmojiBot")
            state = {"base": "fixture", "sets": [
                {"name": "fixture1_by_YourEmojiBot", "title": "Fixture 1", "fmt": "mixed", "index": 1,
                 "keys": ["a"], "live": 1, "logo": False},
                {"name": "fixture2_by_YourEmojiBot", "title": "Fixture 2", "fmt": "mixed", "index": 2,
                 "keys": [], "live": 0, "logo": False}], "sent": [], "sent_full": [], "skipped": []}
            plan = {"version": 1, "targets": [["a", 2], ["b", 1]], "moves": [], "held": [],
                    "known": ["a", "b"], "excluded": [], "per_set": 200}
            (data / "publish_fixture.json").write_text(json.dumps(state), encoding="utf-8")
            (data / "pack_plan.json").write_text(json.dumps(plan), encoding="utf-8")
            from tests.reference.plan_apply import read_catalog
            # SQLite's source read-only connection may create WAL/SHM bookkeeping too.
            # Warm that same source path before asserting durable-byte parity.
            read_catalog(data / "catalog.db", "fixture")
            before = {p.name: p.read_bytes() for p in data.iterdir() if p.is_file()}
            result = subprocess.run([str(BINARY), "plan-apply", "--base", "fixture", "--data-dir", str(data)],
                cwd=folder, stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=10,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
            self.assertEqual(result.returncode, 3, result.stderr)
            from tests.reference.plan_apply import compute_steps, render
            items, ids = read_catalog(data / "catalog.db", "fixture")
            expected = render(compute_steps(plan, state, items, ids), ids, 20)
            self.assertEqual(result.stdout, expected + "\n")
            for variant in (
                    {**plan, "targets": [], "held": [{"key": "a", "from_pack": 1}], "excluded": ["a"]},
                    {**plan, "targets": [], "held": [], "excluded": []}):
                (data / "pack_plan.json").write_text(json.dumps(variant), encoding="utf-8")
                result = subprocess.run([str(BINARY), "plan-apply", "--base", "fixture", "--data-dir", str(data)],
                    cwd=folder, stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=10,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                expected_steps = compute_steps(variant, state, items, ids)
                self.assertEqual(result.returncode, 3 if expected_steps.pending else 0, result.stderr)
                self.assertEqual(result.stdout, render(expected_steps, ids, 20) + "\n")
            (data / "pack_plan.json").write_bytes(before["pack_plan.json"])
            for name, contents in before.items():
                if name in {"catalog.db-wal", "catalog.db-shm"}:
                    continue
                with self.subTest(artifact=name):
                    self.assertEqual((data / name).read_bytes(), contents)
            self.assertLessEqual({p.name for p in data.iterdir() if p.is_file()} - before.keys(),
                                 {"catalog.db-wal", "catalog.db-shm"})
