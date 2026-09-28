"""coins/rebuild_dedup.py: resuming after an interruption, decided by identity.

Split from test_rebuild_dedup_state.py. Each test pins one way the rebuild kept
mutating packs after it had stopped knowing what was live:

* an ambiguous upload was followed by the next upload, whose in-flight marker
  overwrote the record of the unresolved one (C-05),
* a failed live read counted as "0 stickers", which rolls the cursor back onto
  an entry that already landed (C-06),
* an in-flight upload judged "landed" by a count any stranger's sticker
  satisfies (13),
* a create recovery adopting a set of the wrong SHAPE, after it had already
  written the adoption to disk (C).

No network and no real sleeps: Telegram is a fake object.
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))

from emojikit import build_pack as bp  # noqa: E402
from emojikit import telegram_api as tg_api  # noqa: E402
from coins import rebuild_dedup as rd  # noqa: E402
from coins import _dedup_plan as cfg  # noqa: E402
from tests._rebuild_fixtures import (  # noqa: E402
    FakeTelegram, RebuildCase, _png_bytes)


class AmbiguousUploadStopsTheRun(RebuildCase):
    """C-05: an unresolved mutation must be the LAST mutation of the run.

    Without the stop the loop continued, and the next entry's in-flight marker
    overwrote the unresolved one -- so nothing recorded which upload might have
    landed, and the reconcile on the next run attributed it to the wrong image.
    """

    def setUp(self):
        super().setUp()
        # "seed" is plan[0], already uploaded: order and cursor have to describe
        # the same walk of the plan, so a live sticker needs a plan entry that
        # accounts for it.
        self.write_plan(["seed", "aaa", "bbb"])
        self.write_state(sets=[{"index": 1, "name": "s1", "title": "T 1"}],
                         order=["seed"], cursor=1)
        self.tg = FakeTelegram(live={"s1": 1})
        self.tg.add_error = tg_api.AmbiguousUploadError("addStickerToSet: unknown")

    def test_the_second_entry_is_never_uploaded(self):
        code = self.run_build(self.tg)
        self.assertEqual(self.tg.mutations, 1,
                         "no mutation may follow an unresolved one")
        self.assertEqual([c[1] for c in self.tg.add_calls], ["aaa"])
        self.assertEqual(code, bp.EXIT_PARTIAL,
                         "an unresolved upload must ask the loop to resume")

    def test_the_unresolved_mutation_keeps_its_identity(self):
        self.run_build(self.tg)
        marker = self.saved()["in_flight"]
        self.assertEqual(marker["key"], "aaa",
                         "a later mutation overwrote the unresolved one")
        self.assertEqual(marker["operation"], "add")
        self.assertEqual(marker["set_name"], "s1")
        self.assertEqual(marker["set_index"], 1)
        self.assertEqual(marker["expected_before"], 1)
        self.assertEqual(marker["phase"], "upload")

    def test_an_ambiguous_create_records_the_set_it_may_have_made(self):
        # Empty state: the first entry has to create a set.
        self.write_state(sets=[], order=[], cursor=0)
        tg = FakeTelegram()
        tg.create_error = tg_api.AmbiguousUploadError("createNewStickerSet: unknown")
        code = self.run_build(tg)
        self.assertEqual(tg.mutations, 1)
        self.assertEqual(code, bp.EXIT_PARTIAL)
        marker = self.saved()["in_flight"]
        self.assertEqual(marker["operation"], "create")
        self.assertEqual(marker["set_name"], f"{cfg.BASE}1_by_bot")
        self.assertEqual(marker["set_index"], 1)
        self.assertEqual(self.saved()["sets"], [],
                         "the set is unconfirmed; it must not be recorded as ours")


class ResumeReconcilesTheMarker(RebuildCase):
    """The structured marker is what makes an ambiguous create recoverable."""

    def test_a_create_that_landed_is_adopted_from_the_marker(self):
        self.write_plan(["aaa"])
        created = f"{cfg.BASE}1_by_bot"
        self.write_state(sets=[], order=[], cursor=1, in_flight={
            "key": "aaa", "operation": "create", "set_name": created,
            "set_index": 1, "expected_before": 0, "phase": "upload"})
        tg = FakeTelegram()                    # the create did land...
        tg.append(created, (self.emoji / "aaa.png").read_bytes())  # ...with OUR image
        rd.build(tg, "bot")
        saved = self.saved()
        self.assertEqual(saved["order"], ["aaa"], "the upload must be recorded")
        self.assertEqual([s["name"] for s in saved["sets"]], [created])
        self.assertIsNone(saved["in_flight"])
        self.assertEqual(tg.mutations, 0, "it already landed; do not re-send it")

    def test_a_create_that_did_not_land_is_retried_once(self):
        self.write_plan(["aaa"])
        created = f"{cfg.BASE}1_by_bot"
        self.write_state(sets=[], order=[], cursor=1, in_flight={
            "key": "aaa", "operation": "create", "set_name": created,
            "set_index": 1, "expected_before": 0, "phase": "upload"})
        tg = FakeTelegram()                    # nothing exists live
        rd.build(tg, "bot")
        self.assertEqual([c[1] for c in tg.create_calls], ["aaa"])
        self.assertEqual(self.saved()["order"], ["aaa"])

    def test_an_unreadable_set_stops_instead_of_deciding(self):
        self.write_plan(["aaa"])
        created = f"{cfg.BASE}1_by_bot"
        self.write_state(sets=[], order=[], cursor=1, in_flight={
            "key": "aaa", "operation": "create", "set_name": created,
            "set_index": 1, "expected_before": 0, "phase": "upload"})
        tg = FakeTelegram()
        tg.unknown.add(created)
        before = self.saved()
        with self.assertRaises(SystemExit) as caught:
            rd.build(tg, "bot")
        self.assertEqual(caught.exception.code, bp.EXIT_PARTIAL)
        self.assertEqual(tg.mutations, 0)
        self.assertEqual(self.saved(), before, "state must be untouched")


class InFlightIsResolvedByIdentity(RebuildCase):
    """13: "the set grew by one" is not proof that OUR upload grew it.

    A sticker added by hand, or by a concurrent tool, satisfies the count just
    as well -- and the resume then records the plan entry as uploaded, so the
    image is never sent and its ticker is mapped onto a stranger's sticker.
    """

    MARKER = {"key": "aaa", "operation": "add", "set_name": "s1",
              "set_index": 1, "expected_before": 1, "phase": "upload"}

    def setUp(self):
        super().setUp()
        # plan[0]="seed" is already live; the marker is plan[1], the entry the
        # cursor was moved past to run it.
        self.write_plan(["seed", "aaa"])
        self.write_state(sets=[{"index": 1, "name": "s1", "title": "T 1"}],
                         order=["seed"], cursor=2, in_flight=dict(self.MARKER))

    def test_a_manual_sticker_cannot_satisfy_the_marker(self):
        tg = FakeTelegram(live={"s1": 1})
        tg.append("s1", _png_bytes("added-by-hand"))   # not our image
        self.run_build(tg)
        saved = self.saved()
        self.assertNotIn("aaa", saved["order"],
                         "a stranger's sticker was accepted as our upload")
        self.assertEqual(saved["cursor"], 1, "the entry must be retried")
        self.assertIsNone(saved["in_flight"])
        self.assertEqual(tg.mutations, 0,
                         "an unexplained sticker must stop the run, not be "
                         "built upon")

    def test_our_own_image_is_recognised(self):
        tg = FakeTelegram(live={"s1": 1})
        tg.append("s1", (self.emoji / "aaa.png").read_bytes())  # it did land
        rd.build(tg, "bot")
        saved = self.saved()
        self.assertEqual(saved["order"], ["seed", "aaa"])
        self.assertEqual(tg.mutations, 0, "it landed; do not send it again")
        self.assertIsNone(saved["in_flight"])

    def test_an_upload_that_never_landed_is_retried(self):
        tg = FakeTelegram(live={"s1": 1})            # nothing was added
        rd.build(tg, "bot")
        self.assertEqual([c[1] for c in tg.add_calls], ["aaa"])
        self.assertEqual(self.saved()["order"], ["seed", "aaa"])

    def test_an_unreadable_set_stops_instead_of_deciding(self):
        tg = FakeTelegram(live={"s1": 1})
        tg.unknown.add("s1")
        before = self.saved()
        code = self.run_build(tg)
        self.assertEqual(code, bp.EXIT_PARTIAL)
        self.assertEqual(tg.mutations, 0)
        self.assertEqual(self.saved(), before, "state must be untouched")


class CreateIsAdoptedOnlyOnImageIdentity(RebuildCase):
    """4: EXISTS is not proof that OUR create made the set.

    The reconcile treated "a set of that name is live" as "the in-flight create
    landed" and adopted it. A same-named set can pre-exist -- a leftover family,
    a hand-made pack, another bot's -- and adopting it attaches this rebuild's
    state, its cursor and every later add to a pack it never built. Only the
    first sticker's IMAGE can tell the two apart.
    """

    CREATED = f"{cfg.BASE}1_by_bot"

    def setUp(self):
        super().setUp()
        self.write_plan(["aaa", "bbb"])
        self.write_state(sets=[], order=[], cursor=1, in_flight={
            "key": "aaa", "operation": "create", "set_name": self.CREATED,
            "set_index": 1, "expected_before": 0, "phase": "upload"})

    def test_a_foreign_first_image_is_never_adopted(self):
        tg = FakeTelegram()
        tg.append(self.CREATED, _png_bytes("someone-elses-pack"))
        before = self.saved()
        code = self.run_build(tg)
        self.assertEqual(code, bp.EXIT_PARTIAL,
                         "an unidentifiable set must stop the run")
        self.assertEqual(tg.mutations, 0, "nothing may be built on it")
        self.assertEqual(self.saved(), before,
                         "the set is not ours: it must not enter the state")

    def test_a_first_sticker_that_cannot_be_read_stops_the_run(self):
        tg = FakeTelegram()
        tg.append(self.CREATED, b"not an image at all")
        before = self.saved()
        code = self.run_build(tg)
        self.assertEqual(code, bp.EXIT_PARTIAL,
                         "unverifiable is UNKNOWN, never 'close enough'")
        self.assertEqual(tg.mutations, 0)
        self.assertEqual(self.saved(), before)

    def test_an_empty_set_of_that_name_is_not_treated_as_ours(self):
        """A create leaves exactly one sticker; nothing else is our create."""
        tg = FakeTelegram()
        tg.images[self.CREATED] = []
        tg.live[self.CREATED] = 0
        before = self.saved()
        self.assertEqual(self.run_build(tg), bp.EXIT_PARTIAL)
        self.assertEqual(tg.mutations, 0)
        self.assertEqual(self.saved(), before)

    def test_our_image_beside_a_foreign_one_is_not_our_create(self):
        """C: the SHAPE has to be checked before anything is written.

        A create leaves EXACTLY one sticker, so [OURS, FOREIGN] is somebody
        else's set -- and it satisfies the first-sticker check. The count
        disagreement only surfaced afterwards, in the cum/order comparison,
        with the adoption, the cursor and the order record already saved.
        """
        tg = FakeTelegram()
        tg.append(self.CREATED, (self.emoji / "aaa.png").read_bytes())   # ours
        tg.append(self.CREATED, _png_bytes("someone-elses-second"))      # not
        before = self.saved()
        code = self.run_build(tg)
        self.assertEqual(code, bp.EXIT_PARTIAL,
                         "a two-sticker set is not the one our create made")
        self.assertEqual(tg.mutations, 0)
        saved = self.saved()
        self.assertEqual(saved["sets"], [],
                         "a set of the wrong shape must not be adopted")
        self.assertEqual(saved["cursor"], before["cursor"])
        self.assertEqual(saved["order"], [])
        self.assertEqual(saved["in_flight"], before["in_flight"],
                         "the intent must survive to be resolved next run")
        self.assertEqual(saved, before, "the refusal half-applied itself")

    def test_our_own_image_is_still_adopted(self):
        tg = FakeTelegram()
        tg.append(self.CREATED, (self.emoji / "aaa.png").read_bytes())
        rd.build(tg, "bot")
        saved = self.saved()
        self.assertEqual([s["name"] for s in saved["sets"]], [self.CREATED])
        self.assertEqual(saved["order"], ["aaa", "bbb"],
                         "the adopted upload counts; the run continues from it")
        self.assertEqual([c[1] for c in tg.add_calls], ["bbb"],
                         "aaa is already live and must not be sent again")


class LiveStateUnknownStopsTheRun(RebuildCase):
    """C-06: a failed live read is not "the set is empty"."""

    def setUp(self):
        super().setUp()
        self.write_plan(["seed", "aaa", "bbb"])
        self.write_state(sets=[{"index": 1, "name": "s1", "title": "T 1"}],
                         order=["seed"], cursor=1)
        self.tg = FakeTelegram(live={"s1": 1})
        self.tg.unknown.add("s1")

    def test_cursor_order_and_marker_are_left_untouched(self):
        before = self.saved()
        with self.assertRaises(SystemExit) as caught:
            rd.build(self.tg, "bot")
        self.assertEqual(caught.exception.code, bp.EXIT_PARTIAL)
        self.assertEqual(self.tg.mutations, 0,
                         "an unknown live count must not drive an upload")
        after = self.saved()
        self.assertEqual(after["cursor"], before["cursor"])
        self.assertEqual(after["order"], before["order"])
        self.assertIsNone(after["in_flight"])
        self.assertEqual(after, before)


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_rebuild_dedup_resume -v")
