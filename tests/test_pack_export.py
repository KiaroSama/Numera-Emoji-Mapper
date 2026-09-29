"""`pack_archive --export`: one pack as a zip, in slot order, sources untouched."""

from __future__ import annotations

import contextlib
import hashlib
import io
import json
import os
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock


from emojikit import pack_archive, pack_export, pack_manifest
from tests._media_fixtures import make_png

KA = "s:" + "a" * 32
KB = "v:" + "b" * 32


class APackExportsAsAZip(unittest.TestCase):

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.dir = Path(self._tmp.name)
        self.packs = self.dir / "packs"
        self.packs.mkdir()
        self.logo = make_png(self.dir / "logo.png")
        self.a = make_png(self.dir / "media" / "a.webp")
        self.b = self.dir / "media" / "b.webm"
        self.b.write_bytes(b"\x1a\x45\xdf\xa3 a video")
        doc = {"set_name": "mine2_by_bot", "title": "Mine 2", "family": "general",
               "pack_index": 2, "emoji": [
                   {"slot": 1, "role": "brand-logo", "custom_emoji_id": "900",
                    "history_key": "logo:mine2_by_bot"},
                   {"slot": 2, "role": "emoji", "custom_emoji_id": "901",
                    "history_key": f"ck:{KB}"},
                   {"slot": 3, "role": "emoji", "custom_emoji_id": "902",
                    "history_key": f"ck:{KA}"}]}
        (self.packs / "mine2_by_bot.json").write_text(json.dumps(doc), encoding="utf-8")
        self.items = {KA: {"path": self.a, "fmt": "static", "keywords": ["alpha"]},
                      KB: {"path": self.b, "fmt": "video", "keywords": ["beta"]}}

    def _export(self, pack="2", *, stale=False, items=None) -> tuple[int, str, Path]:
        out = self.dir / "out.zip"
        buf = io.StringIO()
        with mock.patch.object(pack_manifest, "OUT_DIR", self.packs), \
                mock.patch.object(pack_manifest, "check_stale",
                                  lambda: (stale, "changed: catalog.db")), \
                mock.patch.object(pack_archive, "_catalog",
                                  lambda: (self.items if items is None else items, {})), \
                mock.patch.dict(os.environ, {"BRAND_LOGO_PATH": str(self.logo)}), \
                contextlib.redirect_stdout(buf):
            code = pack_export.export(pack, out)
        return code, buf.getvalue(), out

    def _digests(self):
        return [hashlib.sha256(p.read_bytes()).hexdigest() for p in (self.logo, self.a, self.b)]

    def test_files_are_in_slot_order_with_the_manifest_and_sources_untouched(self):
        before = self._digests()
        code, _, out = self._export()
        self.assertEqual(code, 0)
        with zipfile.ZipFile(out) as zf:
            self.assertEqual(zf.namelist(), [
                "001_logo.png", f"002_video_{'b' * 12}.webm",
                f"003_static_{'a' * 12}.webp", "_manifest.md"])
            manifest = zf.read("_manifest.md").decode("utf-8")
        self.assertIn("beta", manifest)
        self.assertEqual(self._digests(), before, "an export must never touch a source")
        self.assertTrue(self.a.is_file() and self.b.is_file(), "nothing is moved")

    def test_the_set_name_works_as_well_as_the_number(self):
        self.assertEqual(self._export("mine2_by_bot")[0], 0)

    def test_a_stale_roster_is_refused(self):
        code, text, out = self._export(stale=True)
        self.assertEqual(code, pack_export.EXIT_STALE)
        self.assertIn("pack_manifest --refresh", text)
        self.assertFalse(out.exists())

    def test_a_missing_file_refuses_instead_of_a_partial_zip(self):
        code, text, out = self._export(items={KA: self.items[KA]})
        self.assertEqual(code, pack_export.EXIT_FAILED)
        self.assertIn("slot 2", text)
        self.assertFalse(out.exists(), "a partial zip would look like the whole pack")


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_pack_export -v")
