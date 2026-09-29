"""coins/rebuild_dedup.py: the delete phase and the run's own settings.

The other halves of the mutation walk live beside this one:
`test_rebuild_dedup_resume` (interruptions, decided by identity),
`test_rebuild_dedup_plan` (cursor, plan and state schema, the `build` exit),
`test_rebuild_dedup_locks` (one run per pack family). The canonical map phase
is `test_rebuild_dedup_map`.

Pinned here: an old pack that survived deletion still completed the delete
phase (H-15); the link message's retries; the owner id parsed at import.

No network and no real sleeps: Telegram is a fake object.
"""

from __future__ import annotations

import importlib
import os
import unittest
from unittest import mock


from emojikit import build_pack as bp
from emojikit import packstate as ps
from coins import rebuild_dedup as rd
from coins import _dedup_plan as cfg
from tests._rebuild_fixtures import (
    FakeTelegram, RebuildCase)


class LinkMessagesAreBounded(RebuildCase):
    """19: sendMessage has no dedup key, so five retries can post five links."""

    def test_the_link_message_retries_at_most_twice(self):
        self.write_plan(["aaa"])
        self.write_state()
        tg = FakeTelegram()
        rd.build(tg, "bot")
        self.assertEqual(len(tg.messages), 1)
        self.assertEqual(tg.message_retries, [2],
                         "the default retry count multiplies accepted posts")
        self.assertEqual(tg.message_previews, [True],
                         "a preview card per addemoji link buries the list; "
                         "the coin path had this before the shared announcer "
                         "and must not have lost it")


class OwnerIdIsParsedSafely(unittest.TestCase):
    """18: int() on a .env typo raised before argparse could explain anything."""

    def test_a_typo_falls_back_instead_of_killing_the_import(self):
        # The owner id is resolved at IMPORT of the config module, so the reload
        # has to be of that module -- reloading the tool re-reads nothing.
        self.addCleanup(importlib.reload, cfg)
        with mock.patch.dict(os.environ, {"PACK_OWNER_USER_ID": "42abc"},
                             clear=False):
            self.assertEqual(importlib.reload(cfg).USER_ID, 0)

    def test_a_valid_value_is_still_used(self):
        self.addCleanup(importlib.reload, cfg)
        with mock.patch.dict(os.environ, {"PACK_OWNER_USER_ID": "12345"},
                             clear=False):
            self.assertEqual(importlib.reload(cfg).USER_ID, 12345)


class OldPackDeletionMustBeConfirmed(RebuildCase):
    """H-15: a pack that survived deletion used to complete the phase."""

    def setUp(self):
        super().setUp()
        self.write_plan(["aaa"])
        self.write_state(deleted_old=False)
        ps.write_json_atomic(self.old_state,
                             {"sets": [{"name": "old1"}, {"name": "old2"}]})

    def test_a_surviving_pack_leaves_the_phase_open(self):
        tg = FakeTelegram(live={"old1": 5, "old2": 5})
        tg.delete_error = RuntimeError("deleteStickerSet failed: BOT_ACCESS_DENIED")
        with self.assertRaises(SystemExit) as caught:
            rd.build(tg, "bot")
        self.assertEqual(caught.exception.code, bp.EXIT_PARTIAL)
        saved = self.saved()
        self.assertFalse(saved["deleted_old"],
                         "the phase must stay open while a pack is still live")
        self.assertEqual(saved.get("deleted_old_packs", []), [])
        self.assertEqual(tg.mutations, 0, "never build beside surviving packs")

    def test_partial_deletion_records_only_what_is_gone(self):
        tg = FakeTelegram(live={"old1": 5, "old2": 5})
        real_call = tg.call

        def only_first(method, *, data=None, **kw):
            if method == "deleteStickerSet" and data["name"] == "old2":
                raise RuntimeError("deleteStickerSet failed: BOT_ACCESS_DENIED")
            return real_call(method, data=data, **kw)

        tg.call = only_first
        with self.assertRaises(SystemExit) as caught:
            rd.build(tg, "bot")
        self.assertEqual(caught.exception.code, bp.EXIT_PARTIAL)
        saved = self.saved()
        self.assertEqual(saved.get("deleted_old_packs"), ["old1"],
                         "only the confirmed-gone pack may be checked off")
        self.assertFalse(saved["deleted_old"])
        self.assertEqual(tg.mutations, 0)

    def test_an_unreadable_pack_stops_the_run(self):
        tg = FakeTelegram(live={"old1": 5, "old2": 5})
        tg.unknown.add("old1")
        with self.assertRaises(SystemExit) as caught:
            rd.build(tg, "bot")
        self.assertEqual(caught.exception.code, bp.EXIT_PARTIAL)
        self.assertEqual(tg.deleted, ["old1"], "stop at the first unknown answer")
        self.assertFalse(self.saved()["deleted_old"])

    def test_a_confirmed_deletion_completes_the_phase(self):
        tg = FakeTelegram(live={"old1": 5, "old2": 5})
        rd.build(tg, "bot")
        saved = self.saved()
        self.assertTrue(saved["deleted_old"])
        self.assertEqual(saved.get("deleted_old_packs"), ["old1", "old2"])
        self.assertEqual([c[1] for c in tg.create_calls], ["aaa"])

    def test_deletion_is_not_repeated_after_a_resume(self):
        tg = FakeTelegram(live={"old1": 5, "old2": 5})
        tg.unknown.add("old2")
        with self.assertRaises(SystemExit):
            rd.build(tg, "bot")
        self.assertEqual(tg.deleted, ["old1", "old2"])
        tg.unknown.clear()
        rd.build(tg, "bot")
        self.assertEqual(tg.deleted, ["old1", "old2", "old2"],
                         "old1 was confirmed gone and must not be re-deleted")


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_rebuild_dedup_state -v")
