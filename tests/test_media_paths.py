"""Where a catalog row's media lives, and how that survives a folder move.

`items.file_path` used to be an absolute path. Renaming the project folder left
68 rows pointing at a folder that no longer existed, and every one of them
showed a broken thumbnail in the panel. A path inside the data folder is now
stored relative to it; everything reading the table resolves it the same way.
"""

from __future__ import annotations

import shutil
import sqlite3
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

from emojikit import media_paths  # noqa: E402
from tests.reference.catalog import MEDIA_PATHS_DONE, Catalog  # noqa: E402

# Case-insensitive path containment is a Windows property; the Linux matrix
# skips it, so the windows-safety job has to run this module.
RUNS_ON_NATIVE_WINDOWS = True


class MediaPathTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.data = self.tmp / "collection"
        (self.data / "media" / "static").mkdir(parents=True)
        self.outside = self.tmp / "archive"
        self.outside.mkdir()

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _file(self, path: Path, data: bytes = b"x") -> Path:
        path.write_bytes(data)
        return path

    def _raw_paths(self, data_dir: Path) -> dict[str, str]:
        with sqlite3.connect(data_dir / "catalog.db") as con:
            return dict(con.execute("SELECT content_key, file_path FROM items"))

    def test_inside_the_data_folder_is_stored_relative(self):
        inside = self._file(self.data / "media" / "static" / "a.webp")
        with Catalog(self.data / "catalog.db") as cat:
            cat.add(content_key="s:a", fmt="static", file_path=inside)
            self.assertEqual(Path(cat.get("s:a").file_path), inside)
        self.assertEqual(self._raw_paths(self.data)["s:a"], "./media/static/a.webp")

    def test_outside_the_data_folder_stays_absolute(self):
        archived = self._file(self.outside / "001_static.webp")
        with Catalog(self.data / "catalog.db") as cat:
            cat.add(content_key="s:b", fmt="static", file_path=archived)
        self.assertEqual(Path(self._raw_paths(self.data)["s:b"]), archived)

    def test_a_moved_data_folder_still_finds_every_file(self):
        """The reported bug: a renamed folder left every in-folder row dangling."""
        inside = self._file(self.data / "media" / "static" / "a.webp", b"inside")
        archived = self._file(self.outside / "b.webp", b"archive")
        with Catalog(self.data / "catalog.db") as cat:
            cat.add(content_key="s:a", fmt="static", file_path=inside)
            cat.add(content_key="s:b", fmt="static", file_path=archived)
        moved = self.tmp / "renamed project" / "collection"
        moved.parent.mkdir()
        shutil.move(str(self.data), str(moved))
        with Catalog(moved / "catalog.db") as cat:
            items = {it.content_key: Path(it.file_path) for it in cat.all_items()}
        self.assertEqual(items["s:a"].read_bytes(), b"inside")
        self.assertTrue(items["s:a"].is_relative_to(moved))
        self.assertEqual(items["s:b"], archived)

    def test_a_raw_reader_resolves_like_the_catalog(self):
        inside = self._file(self.data / "media" / "static" / "a.webp")
        with Catalog(self.data / "catalog.db") as cat:
            cat.add(content_key="s:a", fmt="static", file_path=inside)
        with sqlite3.connect(self.data / "catalog.db") as con:
            (stored,) = con.execute("SELECT file_path FROM items").fetchone()
        self.assertEqual(media_paths.resolve(self.data, stored), inside)

    def test_windows_case_does_not_make_a_path_outside(self):
        if sys.platform != "win32":
            self.skipTest("case-insensitive paths are a Windows property")
        inside = self.data / "media" / "static" / "a.webp"
        other_case = Path(str(self.data).upper())
        self.assertEqual(media_paths.store(other_case, inside), "./media/static/a.webp")


class LegacyConversionTests(unittest.TestCase):
    """A catalog written by the old code: absolute rows, maybe project-relative ones."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.data = self.tmp / "collection"
        (self.data / "media").mkdir(parents=True)
        self.inside = self.data / "media" / "a.webp"
        self.inside.write_bytes(b"a")
        rel_dir = ROOT / "tests" / "__media_paths_rel__"
        rel_dir.mkdir(exist_ok=True)
        self.addCleanup(shutil.rmtree, rel_dir, True)
        self.project_file = rel_dir / "p.webp"
        self.project_file.write_bytes(b"p")
        with Catalog(self.data / "catalog.db"):
            pass
        # Turn the fresh catalog back into what the old code left behind.
        with sqlite3.connect(self.data / "catalog.db") as con:
            con.execute("DELETE FROM meta WHERE key='media_paths'")
            for key, stored in (("s:a", str(self.inside)),
                                ("s:p", "tests/__media_paths_rel__/p.webp")):
                con.execute("INSERT INTO items(content_key, format, file_path, created_utc, "
                            "position) VALUES(?, 'static', ?, '2026-01-01', 1)", (key, stored))

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_opening_converts_once_and_keeps_every_file(self):
        with Catalog(self.data / "catalog.db") as cat:
            items = {it.content_key: Path(it.file_path) for it in cat.all_items()}
        self.assertEqual(items["s:a"], self.inside)
        self.assertEqual(items["s:p"].resolve(), self.project_file.resolve())
        with sqlite3.connect(self.data / "catalog.db") as con:
            raw = dict(con.execute("SELECT content_key, file_path FROM items"))
            flag = con.execute("SELECT value FROM meta WHERE key='media_paths'").fetchone()
        self.assertEqual(raw["s:a"], "./media/a.webp")
        self.assertTrue(Path(raw["s:p"]).is_absolute(), "outside the data folder: absolute")
        self.assertEqual(flag, (MEDIA_PATHS_DONE,))

    def test_a_converted_catalog_is_not_converted_again(self):
        with Catalog(self.data / "catalog.db"):
            pass
        with Catalog(self.data / "catalog.db") as cat:
            self.assertEqual(cat.converted_media_paths, 0)

    def test_a_catalog_marked_with_the_old_value_is_healed_on_open(self):
        """An identity migration wrote absolute in-folder rows AFTER the
        conversion had run and marked itself done, so nothing re-ran it."""
        with sqlite3.connect(self.data / "catalog.db") as con:
            con.execute("INSERT OR REPLACE INTO meta(key, value) "
                        "VALUES('media_paths', 'data-relative')")
        with Catalog(self.data / "catalog.db") as cat:
            self.assertGreaterEqual(cat.converted_media_paths, 1)
        with sqlite3.connect(self.data / "catalog.db") as con:
            raw = dict(con.execute("SELECT content_key, file_path FROM items"))
        self.assertEqual(raw["s:a"], "./media/a.webp")

    def test_a_legacy_relative_row_means_the_project_even_without_meta(self):
        """Each row says which rule it follows, so a lost meta record cannot flip it."""
        with sqlite3.connect(self.data / "catalog.db") as con:
            con.execute("DROP TABLE meta")
        self.assertEqual(
            media_paths.resolve(self.data, "tests/__media_paths_rel__/p.webp").resolve(),
            self.project_file.resolve())
        self.assertEqual(media_paths.resolve(self.data, "./media/a.webp"), self.inside)


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_media_paths -v")
