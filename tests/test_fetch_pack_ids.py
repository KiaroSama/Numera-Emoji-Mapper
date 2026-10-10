"""`fetch_pack` takes an emoji id and fetches the pack it belongs to.

The documented way from an id to its pack was a `python -c` one-liner around
the client's private call. Ids and names can be mixed; each pack is fetched
once; an id that names no pack is reported and counted as a failure.
"""

from __future__ import annotations

import contextlib
import io
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock


from tests.reference import fetch_pack
from tests.reference.build_pack import EXIT_OK, EXIT_PARTIAL
from tests.test_fetch_pack_limit import _png_bytes

KNOWN = {"5000000000000000001": "alpha_by_bot", "5000000000000000002": "alpha_by_bot",
         "5000000000000000003": "beta_by_bot"}


class _Telegram:
    def __init__(self):
        self.fetched: list[str] = []
        self.lookups: list[list[str]] = []

    def get_me(self):
        return {"username": "bot"}

    def call(self, method, *, data=None, **kw):
        assert method == "getCustomEmojiStickers", method
        ids = json.loads(data["custom_emoji_ids"])
        self.lookups.append(ids)
        return [{"custom_emoji_id": i, "set_name": KNOWN[i]} for i in ids if i in KNOWN]

    def get_sticker_set(self, name):
        self.fetched.append(name)
        n = len(self.fetched)
        return {"title": name, "stickers": [
            {"file_id": str(n), "file_unique_id": f"FU-{name}", "emoji": "\U0001F600"}]}

    def download_file(self, file_id, dest: Path):
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(_png_bytes(int(file_id)))


class APackCanBeNamedByOneOfItsEmoji(unittest.TestCase):

    def _run(self, *args: str) -> tuple[int, _Telegram]:
        tg = _Telegram()
        with tempfile.TemporaryDirectory() as tmp, \
                mock.patch.object(fetch_pack, "Telegram", lambda token: tg), \
                mock.patch.object(fetch_pack, "setup_logging", lambda *a, **k: None), \
                mock.patch.object(fetch_pack, "load_env", lambda: None), \
                mock.patch.dict(os.environ, {"GENERAL_BOT_TOKEN": "x"}), \
                contextlib.redirect_stdout(io.StringIO()):
            code = fetch_pack.main([*args, "--data-dir", tmp])
        return code, tg

    def test_two_ids_of_one_pack_fetch_it_once(self):
        code, tg = self._run("5000000000000000001", "premium-id:5000000000000000002")
        self.assertEqual(code, EXIT_OK)
        self.assertEqual(tg.fetched, ["alpha_by_bot"])

    def test_names_and_ids_mix_and_nothing_is_fetched_twice(self):
        code, tg = self._run("https://t.me/addemoji/beta_by_bot",
                             "5000000000000000003", "5000000000000000001")
        self.assertEqual(code, EXIT_OK)
        self.assertEqual(tg.fetched, ["beta_by_bot", "alpha_by_bot"])

    def test_an_id_that_names_no_pack_is_reported_and_partial(self):
        with self.assertLogs("fetch_pack", level="ERROR") as logs:
            code, tg = self._run("5000000000000000001", "5999999999999999999")
        self.assertEqual(code, EXIT_PARTIAL)
        self.assertEqual(tg.fetched, ["alpha_by_bot"])
        self.assertTrue(any("5999999999999999999" in line for line in logs.output))


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_fetch_pack_ids -v")
