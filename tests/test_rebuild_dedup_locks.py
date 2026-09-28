"""coins/rebuild_dedup.py: one exclusive run per pack family.

Split from test_rebuild_dedup_state.py: two runs could mutate one pack family
at the same time (H-05), and the lock has to be the one every coin tool takes.

No network and no real sleeps: Telegram is a fake object.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))

from emojikit import telegram_api as tg_api  # noqa: E402
from emojikit import packstate as ps  # noqa: E402
from coins import rebuild_dedup as rd  # noqa: E402
from coins import _dedup_plan as cfg  # noqa: E402
from tests._rebuild_fixtures import (  # noqa: E402
    FakeTelegram, RebuildCase)


class RebuildTakesThePackFamilyLock(unittest.TestCase):
    """6: a lock named after this tool's state file excludes nobody else."""

    def test_the_lock_is_keyed_on_the_pack_base(self):
        self.assertEqual(cfg.LOCK, ps.pack_family_lock_path(cfg.BASE))


class _LockWatchingTelegram(FakeTelegram):
    """Records whether the pack-family lock was held at the first mutation.

    Without this, both release tests below assert only that the lock file is
    ABSENT when the run ends -- which an implementation that never takes the
    lock at all satisfies perfectly. Sampling at the mutation is what makes them
    prove acquire-and-release rather than merely "no leak".
    """

    held_at_mutation: bool | None = None

    def _sample(self) -> None:
        if self.held_at_mutation is None:
            self.held_at_mutation = cfg.LOCK.exists()

    def create_set(self, *a, **kw):
        self._sample()
        return super().create_set(*a, **kw)

    def add_sticker(self, *a, **kw):
        self._sample()
        return super().add_sticker(*a, **kw)


class ConcurrentRunsAreLockedOut(RebuildCase):
    """H-05: two publishers on one state file upload the same entries twice."""

    def test_a_second_run_refuses_to_start(self):
        self.write_plan(["aaa"])
        self.write_state()
        tg = FakeTelegram()
        with ps.exclusive_lock(cfg.LOCK):
            with self.assertRaises(ps.LockBusy):
                rd.build(tg, "bot")
        self.assertEqual(tg.mutations, 0)

    def test_the_lock_is_released_after_a_run(self):
        """Released means AVAILABLE, not deleted: the file outlives the run on
        purpose, because an unlinked inode is a lock nobody else can see."""
        self.write_plan(["aaa"])
        self.write_state()
        tg = _LockWatchingTelegram()
        rd.build(tg, "bot")
        self.assertTrue(tg.held_at_mutation,
                        "the lock must be HELD while the packs are mutated")
        with ps.exclusive_lock(cfg.LOCK):
            pass

    def test_the_lock_is_released_after_a_stop(self):
        self.write_plan(["aaa"])
        self.write_state()
        tg = _LockWatchingTelegram()
        tg.create_error = tg_api.AmbiguousUploadError("createNewStickerSet: unknown")
        with self.assertRaises(SystemExit):
            rd.build(tg, "bot")
        self.assertTrue(tg.held_at_mutation,
                        "the lock must be HELD while the packs are mutated")
        with ps.exclusive_lock(cfg.LOCK):   # a stopped run must not block the retry
            pass


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_rebuild_dedup_locks -v")
