"""`python -m emojikit.status`: one offline answer to "is everything current?"."""

from __future__ import annotations

import contextlib
import io
import sqlite3
import tempfile
import unittest
from pathlib import Path
from unittest import mock


from emojikit import pack_archive, pack_manifest, status


class TheStatusCommand(unittest.TestCase):

    def _main(self, *, roster, archive=(False, []), catalog=None) -> tuple[int, str]:
        out = io.StringIO()
        with mock.patch.object(pack_manifest, "check_stale", lambda: roster), \
                mock.patch.object(pack_archive, "check", lambda: archive), \
                mock.patch.object(pack_manifest, "CATALOG",
                                  catalog or Path(tempfile.gettempdir()) / "absent.db"), \
                mock.patch.object(status, "setup_logging", lambda *a, **k: None), \
                mock.patch.object(status, "load_env", lambda: None), \
                contextlib.redirect_stdout(out):
            code = status.main([])
        return code, out.getvalue()

    def test_a_stale_roster_exits_3_and_names_the_fix(self):
        code, text = self._main(roster=(True, "changed since the roster was written: catalog.db"))
        self.assertEqual(code, status.EXIT_STALE)
        self.assertIn("pack_manifest --refresh", text)

    def test_everything_current_exits_0(self):
        code, text = self._main(roster=(False, "roster matches its inputs"))
        self.assertEqual(code, 0)
        self.assertNotIn("STALE", text)

    def test_a_stale_archive_is_stale_too(self):
        code, text = self._main(roster=(False, "fine"),
                                archive=(True, ["Pack 1: full but never archived"]))
        self.assertEqual(code, status.EXIT_STALE)
        self.assertIn("pack_archive --sync", text)

    def test_unpublished_emoji_are_counted_without_writing_the_catalog(self):
        with tempfile.TemporaryDirectory() as tmp:
            db = Path(tmp) / "catalog.db"
            con = sqlite3.connect(db)
            con.executescript(
                "CREATE TABLE items(content_key TEXT, included INTEGER);"
                "CREATE TABLE publications(base TEXT, content_key TEXT);"
                "INSERT INTO items VALUES ('s:a', 1), ('s:b', 1), ('s:c', 0);"
                "INSERT INTO publications VALUES ('mypacks', 's:a');")
            con.commit()
            con.close()
            before = db.read_bytes()
            with mock.patch.dict("os.environ", {"COLLECTION_PACK_BASE": "mypacks"}):
                code, text = self._main(roster=(False, "fine"), catalog=db)
            self.assertEqual(db.read_bytes(), before, "status must write nothing")
        self.assertEqual(code, 0, "waiting to publish is a to-do, not stale")
        self.assertIn("1 included emoji not yet published to mypacks", text)


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_status -v")
