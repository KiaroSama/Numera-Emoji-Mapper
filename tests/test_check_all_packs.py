"""coins/check_all_packs: the final integrity audit across every coin pack.

Split from test_remap_ids.py -- a different tool. The audit caches expensive
live downloads, and it used to trust that cache blindly: it counted cached ids
that are no longer live and cached an analyse failure as ``blank=true``
forever, reporting healthy coins as blank.

Everything here uses fakes: no network, no Telegram, no real sleeping.
"""

from __future__ import annotations

import contextlib
import io
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "coins"))

from emojikit.build_pack import (EXIT_FAILED, EXIT_OK, EXIT_PARTIAL)  # noqa: E402
from emojikit import media  # noqa: E402

import check_all_packs as cap  # noqa: E402
from tests._coin_fixtures import (FakeSession, ThrottledTelegram, _blank_png,  # noqa: E402
                                  _png, _sticker)
from tests._coin_fixtures import LiveSetsTelegram as FakeTelegram  # noqa: E402


class CheckAllPacksTest(unittest.TestCase):
    """The audit report must describe the LIVE packs, not the cache."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        (self.dir / "state.json").write_text(
            json.dumps({"sets": [{"index": 1, "name": "s1"}]}), encoding="utf-8")
        self.audit = self.dir / "audit.json"
        self.report = self.dir / "report.txt"

    def tearDown(self):
        self.tmp.cleanup()

    def _main(self, tg, session):
        tg.session = session
        with mock.patch.object(cap, "STATE", self.dir / "state.json"), \
                mock.patch.object(cap, "AUDIT", self.audit), \
                mock.patch.object(cap, "REPORT", self.report), \
                mock.patch.object(cap, "Telegram", lambda token: tg), \
                mock.patch.object(cap, "load_env", lambda: None), \
                mock.patch.object(cap, "setup_logging", lambda *a, **k: None), \
                mock.patch.object(cap.time, "sleep", lambda s: None), \
                mock.patch.dict(os.environ, {"TELEGRAM_BOT_TOKEN": "TOKEN123"}), \
                contextlib.redirect_stdout(io.StringIO()):
            return cap.main()

    def test_deleted_sticker_leaves_totals_and_duplicate_groups(self):
        # Cache remembers three ids; only two are still live, and the surviving
        # pair is no longer a duplicate group.
        same = _png((200, 20, 20, 255))
        self.audit.write_text(json.dumps({
            "a": {"hash": "H1", "blank": False},
            "b": {"hash": "H2", "blank": False},
            "gone": {"hash": "H1", "blank": False},
        }), encoding="utf-8")
        tg = FakeTelegram({"s1": [_sticker("a"), _sticker("b")]})

        rc = self._main(tg, FakeSession({"a": same, "b": same}))

        self.assertEqual(rc, EXIT_OK)
        report = self.report.read_text("utf-8")
        self.assertIn("live stickers: 2", report)
        self.assertIn("DUPLICATE image groups: 0", report)
        self.assertNotIn("gone", self.audit.read_text("utf-8"))

    def test_analyse_failure_is_an_error_that_is_retried_not_a_blank(self):
        tg = FakeTelegram({"s1": [_sticker("a"), _sticker("b")]})
        blobs = {"a": _png((200, 20, 20, 255)), "b": _png((20, 200, 20, 255))}

        rc = self._main(tg, FakeSession(blobs, fail={"b"}))

        self.assertEqual(rc, EXIT_PARTIAL)          # partial run, retry me
        report = self.report.read_text("utf-8")
        self.assertIn("FAILED (retried on next run): 1", report)
        self.assertIn("BLANK stickers: 0", report)  # NOT reported as blank
        cached = json.loads(self.audit.read_text("utf-8"))
        self.assertIn("error", cached["b"])
        self.assertNotIn("TOKEN123", self.audit.read_text("utf-8"))

        # Second run retries the failed sticker instead of skipping it forever.
        sess = FakeSession(blobs)
        self.assertEqual(self._main(tg, sess), EXIT_OK)
        self.assertEqual(sess.fetched, ["b"])
        self.assertIn("FAILED (retried on next run): 0", self.report.read_text("utf-8"))

    def test_blank_sticker_is_still_reported_with_live_position(self):
        tg = FakeTelegram({"s1": [_sticker("a"), _sticker("b")]})
        blobs = {"a": _png((200, 20, 20, 255)), "b": _blank_png()}
        self.assertEqual(self._main(tg, FakeSession(blobs)), EXIT_OK)
        self.assertIn("blank cid=b set=1 pos=1", self.report.read_text("utf-8"))

    def test_blank_is_the_pipeline_wide_rule_not_a_local_copy(self):
        """A second definition of "empty" is how a coin gets called blank here
        and usable by the converter (or the reverse).

        The substitution is the proof: importing the shared rule while still
        answering from an inlined copy would satisfy an identity check.
        """
        self.assertIs(cap.is_blank_image, media.is_blank_image)
        verdict = object()
        with mock.patch.object(cap, "is_blank_image", lambda im: verdict):
            self.assertIs(cap.analyze(_png((200, 20, 20, 255)))[1], verdict,
                          "analyze answered from its own copy of the rule")


class CheckAllPacksListingRetry(unittest.TestCase):
    """The per-set LISTING had no retry while the per-sticker download had four.

    That listing is the call most likely to be throttled when walking many sets,
    and ``save_audit`` only fires every 100 stickers -- so one 429 there killed
    the run and discarded up to 99 analysed stickers, which then had to be
    downloaded again, making the next throttle likelier.
    """

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dir = Path(self.tmp.name)
        self.audit = self.dir / "audit.json"
        self.report = self.dir / "report.txt"

    def _state(self, *names):
        path = self.dir / "state.json"
        path.write_text(json.dumps({"sets": [
            {"index": i, "name": n} for i, n in enumerate(names, start=1)]}),
            encoding="utf-8")
        return path

    def _main(self, tg, session, state):
        tg.session = session
        self.out = io.StringIO()
        with mock.patch.object(cap, "STATE", state), \
                mock.patch.object(cap, "AUDIT", self.audit), \
                mock.patch.object(cap, "REPORT", self.report), \
                mock.patch.object(cap, "Telegram", lambda token: tg), \
                mock.patch.object(cap, "load_env", lambda: None), \
                mock.patch.object(cap, "setup_logging", lambda *a, **k: None), \
                mock.patch.object(cap.time, "sleep", lambda s: None), \
                mock.patch.dict(os.environ, {"TELEGRAM_BOT_TOKEN": "TOKEN123"}), \
                contextlib.redirect_stdout(self.out):
            return cap.main()

    def test_a_throttled_listing_is_retried_instead_of_ending_the_run(self):
        tg = ThrottledTelegram({"s1": [_sticker("a")]}, fail_times={"s1": 2})
        rc = self._main(tg, FakeSession({"a": _png((200, 20, 20, 255))}),
                        self._state("s1"))
        self.assertEqual(rc, EXIT_OK)
        self.assertEqual(tg.listings, 3, "the first two 429s must not be fatal")
        self.assertIn("live stickers: 1", self.report.read_text("utf-8"))
        self.assertNotIn("TOKEN123", self.out.getvalue(),
                         "the retry line printed the raw error, token and all")

    def test_work_already_analysed_survives_a_set_that_will_not_list(self):
        """The audit is the resume cache: losing it is what causes the re-download.

        Reporting is refused too -- ``live`` would be missing a whole set, so the
        reconcile step would prune its cached analyses and call healthy coins
        deleted.
        """
        tg = ThrottledTelegram({"s1": [_sticker("a")], "s2": [_sticker("b")]},
                               fail_times={"s2": 99})
        rc = self._main(tg, FakeSession({"a": _png((200, 20, 20, 255))}),
                        self._state("s1", "s2"))

        self.assertEqual(rc, EXIT_FAILED)
        self.assertEqual(list(json.loads(self.audit.read_text("utf-8"))), ["a"],
                         "set 1's analysis was thrown away with the run")
        self.assertFalse(self.report.exists(),
                         "a report built from a partial live read is wrong")

    def test_each_set_boundary_checkpoints_the_audit(self):
        """A run killed during set 2 must not lose set 1.

        save_audit fired only every 100 stickers, so any set smaller than that
        was analysed and then thrown away by an interruption -- and had to be
        downloaded all over again, which is what makes the next throttle likelier.
        Killing the listing outright leaves the boundary checkpoint as the only
        thing that can have written the file.
        """
        tg = ThrottledTelegram({"s1": [_sticker("a")], "s2": [_sticker("b")]},
                               kill_on="s2")
        with self.assertRaises(KeyboardInterrupt):
            self._main(tg, FakeSession({"a": _png((200, 20, 20, 255))}),
                       self._state("s1", "s2"))
        self.assertTrue(self.audit.is_file(), "set 1 was never checkpointed")
        self.assertEqual(list(json.loads(self.audit.read_text("utf-8"))), ["a"])


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_check_all_packs -v")
