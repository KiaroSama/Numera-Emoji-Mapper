"""Local-only real catalog replay uses online copies; the operator database is read-only."""
from contextlib import closing
import dataclasses
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import unittest

from emojikit.sqlite_snapshot import backup
from tests.reference.catalog import Catalog

RUNS_ON_NATIVE_WINDOWS = True
ROOT = Path(__file__).resolve().parent.parent


class RealCatalogReplay(unittest.TestCase):
    def test_copied_operator_rows_replay_and_python_reads_native_changes(self):
        raw = os.environ.get("NUMERA_REAL_CATALOG_FIXTURE")
        if not raw:
            self.skipTest("private catalog is local-by-design; public native catalog parity runs in CI")
        source_path = Path(raw).resolve()
        self.assertTrue(source_path.is_file())
        before = hashlib.sha256(source_path.read_bytes()).hexdigest()
        from emojikit import _native
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder).resolve()
            copied = folder / "catalog.db"
            with closing(sqlite3.connect(source_path.as_uri() + "?mode=ro", uri=True)) as source, \
                    closing(sqlite3.connect(copied)) as destination:
                backup(source, destination)
            with closing(sqlite3.connect(copied)) as db:
                # Bind relative media to the original location in this detached
                # snapshot only. No media is decoded, moved, or rewritten.
                from emojikit.media_paths import resolve
                for key, stored in db.execute("SELECT content_key,file_path FROM items").fetchall():
                    db.execute("UPDATE items SET file_path=? WHERE content_key=?",
                               (str(resolve(source_path.parent, stored)), key))
                db.commit()
            with Catalog(copied) as legacy:
                rows = [dataclasses.asdict(item) for item in legacy.all_items()]
                self.assertGreater(len(rows), 0, "real fixture cannot be empty")
                stats = legacy.stats()
                bases = legacy.publication_bases()
                mappings = {base: {item.content_key: legacy.custom_emoji_id_for(base, item.content_key)
                                   for item in legacy.all_items() if legacy.is_published(base, item.content_key)}
                            for base in bases}
                expected_order = [item.content_key for item in legacy.all_items()][::-1]
            native = _native.NativeCatalog(str(copied), "2026-10-09 00:00:00 UTC", str(ROOT))
            def call(operation, **fields):
                return json.loads(native.call(json.dumps({"operation": operation, **fields})))
            try:
                self.assertEqual(call("all"), rows)
                self.assertEqual(call("stats"), stats)
                for base, ids in mappings.items():
                    for key, value in ids.items():
                        self.assertEqual(call("custom_id", base=base, content_key=key), value)
                self.assertEqual(call("order", keys=expected_order), len(expected_order))
                self.assertEqual(call("inclusion", keys=[expected_order[0]]), [len(expected_order) - 1, 1])
            finally:
                native.close()
            with Catalog(copied) as legacy:
                self.assertEqual([item.content_key for item in legacy.all_items()], expected_order)
                self.assertFalse(legacy.get(expected_order[0]).included)
                for base, ids in mappings.items():
                    for key, value in ids.items():
                        self.assertEqual(legacy.custom_emoji_id_for(base, key), value)
        self.assertEqual(hashlib.sha256(source_path.read_bytes()).hexdigest(), before,
                         "operator source database changed during detached replay")
