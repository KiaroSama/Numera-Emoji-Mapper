"""coins/_keywords: the one writer of coins/keywords.csv.

The file is tracked, curated input for the coin packs' search keywords. Two
scripts used to rewrite it their own way; these tests pin what the shared
writer guarantees to both of them.
"""

from __future__ import annotations

import contextlib
import csv
import io
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock


from PIL import Image

from coins import _keywords

OLD = ("ticker,name,format,file,keywords\n"
       "btc,Bitcoin,svg,logos/svg/btc.svg,\"btc, Bitcoin\"\n")


class KeywordsFileHasOneWriter(unittest.TestCase):

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.csv = self.tmp / "keywords.csv"
        self.png = self.tmp / "logos" / "png"
        self.png.mkdir(parents=True)

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def rows(self) -> dict[str, dict]:
        with open(self.csv, encoding="utf-8", newline="") as fh:
            return {r["ticker"]: r for r in csv.DictReader(fh)}

    def _write(self, rows: dict) -> None:
        with contextlib.redirect_stdout(io.StringIO()):
            _keywords.write_keywords_csv(rows, self.csv)

    def test_an_invalid_cached_png_is_not_listed(self):
        (self.png / "bad.png").write_bytes(b"<html>429 Too Many Requests</html>")
        Image.new("RGBA", (8, 8), (1, 2, 3, 255)).save(self.png / "ok.png")
        self._write({
            t: {"ticker": t, "name": "", "format": "png",
                "file": f"logos/png/{t}.png"} for t in ("bad", "ok")})
        self.assertEqual(set(self.rows()), {"ok"})

    def test_an_empty_new_name_keeps_the_old_one(self):
        self.csv.write_text(OLD, encoding="utf-8")
        self._write({"btc": {"ticker": "btc", "name": "", "format": "svg",
                             "file": "logos/svg/btc.svg"}})
        row = self.rows()["btc"]
        self.assertEqual(row["name"], "Bitcoin",
                         "a failed lookup must not erase an earlier name")
        self.assertEqual(row["keywords"], "btc, Bitcoin")

    def test_a_crash_before_the_swap_leaves_the_old_file_intact(self):
        self.csv.write_text(OLD, encoding="utf-8")
        with mock.patch.object(_keywords.os, "replace",
                               side_effect=OSError("interrupted")), \
                self.assertRaises(OSError):
            self._write({})
        self.assertEqual(self.csv.read_text(encoding="utf-8"), OLD)
        self.assertEqual(list(self.tmp.glob("*.tmp")), [],
                         "the half-written temp file must not be left behind")


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_coin_keywords -v")
