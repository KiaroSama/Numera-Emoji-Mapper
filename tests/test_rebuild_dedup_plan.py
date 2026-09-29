"""coins/rebuild_dedup.py: the frozen plan, the cursor that walks it, the state.

Split from test_rebuild_dedup_state.py, plus the `build` command's exit. That
exit drives an external restart loop -- partial means "run me again", OK means
"stop" -- and is decided from the SAVED cursor and the in-flight marker, so both
have to describe the walk exactly. Also pinned here:

* a truncated plan file was read as the whole (frozen) plan (M-13),
* a retryable upload failure consuming the plan position forever (12),
* a legacy in-flight marker accepted at validation and then unresolvable
  forever at recovery (B).

No network and no real sleeps: Telegram is a fake object.
"""

from __future__ import annotations

import contextlib
import io
import json
from unittest import mock


from emojikit import build_pack as bp
from emojikit import packstate as ps
from coins import rebuild_dedup as rd
from coins import _dedup_plan as cfg
from tests._rebuild_fixtures import FakeTelegram, RebuildCase, _png


class BuildFinishesWhenThePlanIsWalked(RebuildCase):
    """A skip advanced the cursor in memory only.

    When the LAST plan entries were skips, the saved cursor never reached the
    end, `build` answered "partial" forever and the restart loop never stopped.
    The reverse was wrong too: the cursor moves before an upload is sent, so a
    crash on the final upload read as "done" while that upload was unresolved.
    """

    def setUp(self):
        super().setUp()
        self.write_state(sets=[{"index": 1, "name": "s1", "title": "T 1"}],
                         order=["seed"], cursor=1)
        self.tg = FakeTelegram(live={"s1": 1})

    def _build_command(self) -> int:
        with contextlib.redirect_stdout(io.StringIO()), \
             contextlib.redirect_stderr(io.StringIO()):
            return rd.build_command(self.tg, "bot")

    def test_trailing_skips_are_saved_and_the_walk_is_done(self):
        self.write_plan(["seed", "aaa", "bbb"])
        (self.emoji / "aaa.png").unlink()
        (self.emoji / "bbb.png").write_bytes(b"")
        self.assertEqual(self._build_command(), bp.EXIT_OK)
        self.assertEqual(self.saved()["cursor"], 3)
        self.assertEqual(self.tg.mutations, 0)

    def test_a_crash_on_the_last_upload_is_partial_and_keeps_its_marker(self):
        self.write_plan(["seed", "aaa"])
        self.tg.add_error = ValueError("the process died mid-request")
        self.assertEqual(self._build_command(), bp.EXIT_PARTIAL)
        saved = self.saved()
        self.assertEqual(saved["cursor"], 2, "the cursor already walked the plan")
        self.assertEqual((saved["in_flight"] or {}).get("key"), "aaa",
                         "the unresolved upload must stay recorded")

    def test_a_clean_walk_is_done(self):
        self.write_plan(["seed", "aaa"])
        self.assertEqual(self._build_command(), bp.EXIT_OK)
        self.assertEqual(self.saved()["order"], ["seed", "aaa"])


class CursorOrderAndPlanMustDescribeOneWalk(RebuildCase):
    """5: cursor, order and the plan were only ever checked for shape.

    cursor=0 with order=["aaa"] and one live sticker satisfied every existing
    invariant, and the build then started at plan[0] and uploaded "aaa" a
    SECOND time. The mirror case is a cursor at the end with an order that
    cannot have come from that prefix: plan entries are skipped and the run
    reports a completed rebuild.
    """

    def setUp(self):
        super().setUp()
        self.write_plan(["aaa", "bbb", "ccc"])
        ps.write_json_atomic(self.old_state, {"sets": [{"name": "old1"}]})

    def _rejects(self, **state) -> str:
        self.write_state(**{"deleted_old": False, **state})
        tg = FakeTelegram(live={"old1": 3, "s1": 1})
        with self.assertRaises(SystemExit) as caught:
            rd.build(tg, "bot")
        self.assertEqual(tg.deleted, [],
                         "an inconsistent state must be caught BEFORE the "
                         "destructive delete phase")
        self.assertEqual(tg.mutations, 0)
        return str(caught.exception.code)

    def test_an_upload_the_cursor_never_reached_is_rejected(self):
        # THE case: one live sticker, one recorded upload, cursor still at 0.
        # Resuming here re-uploads plan[0] into a set that already holds it.
        problem = self._rejects(
            cursor=0, order=["aaa"],
            sets=[{"index": 1, "name": "s1", "title": "T 1", "live": 1}])
        self.assertIn("'order' records 'aaa'", problem)

    def test_an_order_that_is_not_in_plan_order_is_rejected(self):
        self.assertIn("'order' records 'aaa'", self._rejects(
            cursor=3, order=["ccc", "aaa"],
            sets=[{"index": 1, "name": "s1", "title": "T 1", "live": 2}]))

    def test_an_upload_that_is_not_a_plan_entry_at_all_is_rejected(self):
        self.assertIn("'order' records 'zzz'", self._rejects(
            cursor=3, order=["zzz"],
            sets=[{"index": 1, "name": "s1", "title": "T 1", "live": 1}]))

    def test_uploads_with_no_set_to_live_in_are_rejected(self):
        self.assertIn("no set holds them",
                      self._rejects(cursor=1, order=["aaa"], sets=[]))

    def test_a_marker_for_a_different_plan_entry_is_rejected(self):
        # The cursor is moved past an entry before its mutation is recorded, so
        # a marker for anything else reconciles one entry's outcome onto another.
        self.assertIn("is not plan entry", self._rejects(
            cursor=1, order=[], sets=[{"index": 1, "name": "s1", "live": 0}],
            in_flight={"key": "ccc", "operation": "add", "set_name": "s1",
                       "set_index": 1, "expected_before": 0}))

    def test_a_marker_with_no_cursor_behind_it_is_rejected(self):
        self.assertIn("is not plan entry", self._rejects(
            cursor=0, order=[], sets=[{"index": 1, "name": "s1", "live": 0}],
            in_flight={"key": "aaa", "operation": "add", "set_name": "s1",
                       "set_index": 1, "expected_before": 0}))

    def test_live_drift_stops_before_the_old_packs_are_deleted(self):
        """The one inconsistency only LIVE state can see, checked in time.

        Nothing on disk is wrong here, so the schema cannot catch it: the set
        simply holds a sticker no upload accounts for. The reconciliation that
        does catch it used to run after the delete phase, so the old packs were
        already destroyed by the time the run refused to continue.
        """
        self.write_state(deleted_old=False, cursor=1, order=["aaa"],
                         sets=[{"index": 1, "name": "s1", "title": "T 1",
                                "live": 1}])
        tg = FakeTelegram(live={"old1": 3, "s1": 2})   # one sticker too many
        with self.assertRaises(SystemExit) as caught:
            rd.build(tg, "bot")
        self.assertIn("uploads are recorded", str(caught.exception.code))
        self.assertEqual(tg.deleted, [],
                         "the old packs were destroyed on a state that could "
                         "not be reconciled")
        self.assertEqual(tg.mutations, 0)

    def test_a_walk_with_skipped_entries_is_still_accepted(self):
        """Skips are legitimate: order is a SUBSEQUENCE, not a copy."""
        self.write_state(order=["aaa", "ccc"], cursor=3,
                         sets=[{"index": 1, "name": "s1", "title": "T 1",
                                "live": 2}])
        tg = FakeTelegram(live={"s1": 2})
        rd.build(tg, "bot")                       # bbb was skipped; nothing left
        self.assertEqual(tg.mutations, 0)
        self.assertEqual(self.saved()["cursor"], 3)


class RetryableFailureKeepsThePlanPosition(RebuildCase):
    """12: the cursor moves past an entry BEFORE the upload is attempted.

    A failure that definitely did not apply must put it back, or that image is
    skipped forever -- silently, because the run still ends "successfully".
    """

    def setUp(self):
        super().setUp()
        self.write_plan(["seed", "aaa", "bbb"])
        self.write_state(sets=[{"index": 1, "name": "s1", "title": "T 1"}],
                         order=["seed"], cursor=1)
        self.tg = FakeTelegram(live={"s1": 1})

    def test_a_transport_failure_leaves_the_cursor_on_the_item(self):
        self.tg.add_error = RuntimeError(
            "addStickerToSet failed after 5 attempts")
        code = self.run_build(self.tg)
        saved = self.saved()
        self.assertEqual(saved["cursor"], 1,
                         "a not-applied failure must not consume the position")
        self.assertEqual(saved["order"], ["seed"])
        self.assertEqual(self.tg.mutations, 1,
                         "the run must stop; walking on with a rolled-back "
                         "cursor re-uploads everything after it")
        self.assertEqual(code, bp.EXIT_PARTIAL)

    def test_a_media_rejection_is_still_skipped_for_good(self):
        self.tg.add_error = RuntimeError(
            "addStickerToSet failed: Bad Request: STICKER_PNG_NOPNG")
        code = self.run_build(self.tg)
        saved = self.saved()
        self.assertEqual(saved["cursor"], 3, "both images are permanently bad")
        self.assertEqual(saved["order"], ["seed"])
        self.assertEqual(self.tg.mutations, 2, "each entry is tried once")
        self.assertIsNone(code, "a permanent skip is not a retryable stop")


class TruncatedPlanFailsClosed(RebuildCase):
    """M-13: the frozen plan must never be half-read or silently regenerated."""

    def test_a_truncated_plan_refuses_to_build(self):
        self.plan.write_text('[{"rep": "aaa", "tickers": ["aa',
                             encoding="utf-8")
        self.write_state()
        tg = FakeTelegram()
        with self.assertRaises(SystemExit) as caught:
            rd.build(tg, "bot")
        self.assertIn(self.plan.name, str(caught.exception.code))
        self.assertEqual(tg.mutations, 0)

    def test_a_plan_entry_missing_its_fields_is_rejected(self):
        ps.write_json_atomic(self.plan, [{"rep": "aaa", "tickers": ["aaa"],
                                          "kw": "aaa"},
                                         {"rep": "bbb"}])
        self.write_state()
        tg = FakeTelegram()
        with self.assertRaises(SystemExit):
            rd.build(tg, "bot")
        self.assertEqual(tg.mutations, 0)

    def test_a_sound_plan_still_loads(self):
        self.write_plan(["aaa", "bbb"])
        self.assertEqual([g["rep"] for g in cfg.load_plan()], ["aaa", "bbb"])

    def test_the_plan_and_the_report_are_written_atomically(self):
        """A killed process cannot be staged in-process; the writer is the contract.

        write_text truncates the live file first, so an interrupted generation
        destroys the frozen plan. write_json_atomic renames a finished temp file
        over it, so the previous plan survives any crash before the rename.
        """
        _png(self.emoji / "aaa.png")
        with mock.patch.object(cfg, "write_json_atomic",
                               side_effect=ps.write_json_atomic) as writer:
            cfg.build_plan()
        self.assertEqual({c.args[0] for c in writer.call_args_list},
                         {self.plan, self.groups})
        self.assertEqual([g["rep"] for g in cfg.load_plan()], ["aaa"])
        self.assertEqual(sorted(p.name for p in self.dir.glob("*.tmp")), [])


class StateSchemaIsValidatedBeforeAnyMutation(RebuildCase):
    """7: load_state() parsed JSON and checked no invariant at all.

    cursor=-1 makes plan[-1] the first upload AND leaves it to be uploaded
    again at the end; a cursor past the plan reports the rebuild finished
    without ever walking it. Both had to be caught before the delete phase.
    """

    def setUp(self):
        super().setUp()
        self.write_plan(["aaa", "bbb"])
        ps.write_json_atomic(self.old_state, {"sets": [{"name": "old1"}]})

    def _rejects(self, **state) -> str:
        self.write_state(**{"deleted_old": False, **state})
        tg = FakeTelegram(live={"old1": 3})
        with self.assertRaises(SystemExit) as caught:
            rd.build(tg, "bot")
        self.assertEqual(tg.deleted, [],
                         "an untrusted state must not destroy the old packs")
        self.assertEqual(tg.mutations, 0)
        self.assertEqual(tg.messages, [], "and must not announce anything")
        return str(caught.exception.code)

    def test_a_negative_cursor_is_rejected_before_any_deletion(self):
        self.assertIn("cursor -1", self._rejects(cursor=-1))

    def test_an_oversized_cursor_is_rejected_instead_of_reporting_done(self):
        self.assertIn("past the end", self._rejects(cursor=3))

    def test_a_non_integer_cursor_is_rejected(self):
        self.assertIn("cursor", self._rejects(cursor="1"))

    def test_a_boolean_cursor_is_not_read_as_a_number(self):
        self.assertIn("cursor", self._rejects(cursor=True))

    def test_an_order_that_repeats_an_entry_is_rejected(self):
        self.assertIn("more than once", self._rejects(order=["aaa", "aaa"]))

    def test_a_repeated_set_name_is_rejected(self):
        self.assertIn("repeats the set name", self._rejects(sets=[
            {"index": 1, "name": "s1"}, {"index": 2, "name": "s1"}]))

    def test_set_indexes_must_ascend(self):
        self.assertIn("does not ascend", self._rejects(sets=[
            {"index": 2, "name": "s2"}, {"index": 1, "name": "s1"}]))

    def test_a_live_count_over_the_pack_limit_is_rejected(self):
        self.assertIn("outside 0..", self._rejects(sets=[
            {"index": 1, "name": "s1", "live": cfg.PER_SET + 1}]))

    def test_an_in_flight_marker_missing_its_target_is_rejected(self):
        self.assertIn("set_name", self._rejects(in_flight={
            "key": "aaa", "operation": "add"}))

    def test_an_unknown_in_flight_operation_is_rejected(self):
        self.assertIn("unknown", self._rejects(in_flight={
            "key": "aaa", "operation": "delete", "set_name": "s1",
            "set_index": 1}))

    def test_wrong_types_are_rejected(self):
        self.assertIn("'order'", self._rejects(order="aaa"))
        self.assertIn("'deleted_old'", self._rejects(deleted_old="no"))

    def test_the_legacy_bare_key_marker_is_rejected_here_not_at_recovery(self):
        """B: it used to pass validation and then never be resolvable.

        A bare key names no set, so _reconcile_in_flight had nothing to probe
        and stopped with EXIT_PARTIAL -- every run, at the same point, with no
        way to clear it. It cannot be resolved by guessing either: the version
        that wrote it (5d4669b) recorded the key before choosing between add
        and create, so the set may never have reached state["sets"].
        """
        problem = self._rejects(in_flight="aaa")
        self.assertIn("legacy bare key", problem)
        self.assertIn("cursor", problem, "the operator needs the exact repair")
        self.assertIn("'order'", problem,
                      "the IS-live branch also has to name 'order', or the "
                      "repair walks into the cum/order gate and stops again")

    def test_following_the_legacy_repair_actually_clears_the_stop(self):
        """A prescribed repair that does not work is a second permanent stop.

        The message is only worth what executing it achieves, so execute it. The
        cursor is advanced past an entry BEFORE its mutation is recorded and
        'order' gains the key only on success -- so in the IS-live branch,
        clearing the marker alone leaves one more live sticker than recorded
        uploads, and build()'s own consistency gate refuses. Asserting on the
        wording of the message could never have caught that.
        """
        self.write_plan(["seed", "aaa", "bbb"])
        self.write_state(deleted_old=True, cursor=2, order=["seed"],
                         in_flight="aaa",
                         sets=[{"index": 1, "name": "s1", "title": "T 1"}])
        tg = FakeTelegram(live={"s1": 2})       # seed AND aaa are both live
        with self.assertRaises(SystemExit) as caught:
            rd.build(tg, "bot")                 # refused, as it must be

        # Read the repair OUT of the message before performing it. Hard-coding
        # the two edits proved that THIS repair works, not that the prescribed
        # one does -- so the message could drift into prescribing something else
        # and the test would still pass while the instructions went wrong.
        prescribed = str(caught.exception)
        self.assertIn("'in_flight' to null", prescribed)
        self.assertIn("append 'aaa' to 'order'", prescribed)

        state = json.loads(self.state.read_text("utf-8"))
        state["in_flight"] = None
        state["order"].append("aaa")
        ps.write_json_atomic(self.state, state)

        rd.build(tg, "bot")                     # must now run to completion
        after = json.loads(self.state.read_text("utf-8"))
        self.assertIsNone(after["in_flight"])
        self.assertIn("aaa", after["order"])
        sent = [str(c) for c in tg.add_calls + tg.create_calls]
        self.assertFalse([c for c in sent if "aaa" in c],
                         "the repair must not re-upload an image already live")

    def test_a_marker_that_is_neither_object_nor_null_is_rejected(self):
        self.assertIn("'in_flight'", self._rejects(in_flight=["aaa"]))

    def test_a_sound_state_still_builds(self):
        self.write_state(deleted_old=False, cursor=0)
        tg = FakeTelegram(live={"old1": 3})
        rd.build(tg, "bot")
        self.assertEqual(tg.deleted, ["old1"])
        self.assertEqual([c[1] for c in tg.create_calls], ["aaa"])
        self.assertEqual(self.saved()["cursor"], 2)


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_rebuild_dedup_plan -v")
