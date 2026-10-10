"""Catalog public operations compared with the unchanged source implementation."""
import dataclasses
import json
from pathlib import Path
import tempfile
import unittest

from tests.reference.catalog import Catalog
from emojikit.maintenance import writer

RUNS_ON_NATIVE_WINDOWS = True
ROOT = Path(__file__).resolve().parent.parent
UTC = "2026-10-07 00:00:00 UTC"


class NativeCatalogParity(unittest.TestCase):
    def test_order_inclusion_signed_hash_and_family_publications_match_source(self):
        from emojikit import _native

        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as directory:
            folder = Path(directory)
            expected = []
            with Catalog(folder / "old" / "catalog.db") as old:
                # Known distinct keys avoid media decoding: this seam is storage,
                # not the separately inventoried ingest/content identity decision.
                from unittest import mock
                with mock.patch("tests.reference.catalog.catalog_identity",
                                side_effect=lambda cat, key, fmt, path, phash: (key, True)):
                    for k, phash in (("a", 2**64-1), ("b", 0), ("c", None)):
                        old.add(content_key=k, fmt="static", file_path=folder / (k+".png"),
                                emojis=["😀"], phash=phash)
                expected.append(old.set_order(["c", "a", "missing"]))
                expected.append(list(old.set_inclusion({"b"})))
                old.mark_uploaded("a", "111111111", base="fixture", set_name="fixture1")
                old.mark_uploaded("a", None, base="fixture", set_name=None)
                expected.append([dataclasses.asdict(x) for x in old.all_items()])
                expected.append([dataclasses.asdict(x) for x in old.pending(base="fixture")])
                expected.append([dataclasses.asdict(x) for x in old.pending(base="other")])
                expected.append(old.custom_emoji_id_for("fixture", "a"))
                expected.append(old.stats())
            new = _native.NativeCatalog(str(folder / "new" / "catalog.db"), UTC, str(ROOT))
            def call(operation, **kw):
                return json.loads(new.call(json.dumps({"operation": operation, "utc": UTC, **kw})))
            try:
                for k, phash in (("a", 2**64-1), ("b", 0), ("c", None)):
                    self.assertTrue(call("insert", item={"content_key": k, "fmt": "static",
                        "file_path": str(folder / (k+".png")), "emojis": ["😀"], "phash": phash}))
                actual = [call("order", keys=["c", "a", "missing"]),
                          call("inclusion", keys=["b"])]
                call("uploaded", content_key="a", custom_emoji_id="111111111",
                     base="fixture", set_name="fixture1")
                call("uploaded", content_key="a", custom_emoji_id=None, base="fixture", set_name=None)
                actual.extend([call("all"), call("pending", base="fixture"),
                               call("pending", base="other"), call("custom_id", base="fixture",
                               content_key="a"), call("stats")])
                self.assertEqual(actual, expected)
                with self.assertRaises(RuntimeError):
                    with writer(folder / "new"):
                        self.fail("native catalog did not hold writer ownership")
            finally:
                new.close()
            with writer(folder / "new"):
                pass
