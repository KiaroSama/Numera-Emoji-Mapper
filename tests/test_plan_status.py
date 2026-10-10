"""`python -m emojikit.plan_status`: what the saved pack plan would change.

Read-only: fixture plan and publisher state only, no Telegram, no writes.
"""

from __future__ import annotations

import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock


from tests.reference import plan_status

STATE = {"sets": [{"index": 1, "name": "mine1_by_bot", "live": 4,
                   "keys": ["s:a", "s:b", "s:c"]},
                  {"index": 2, "name": "mine2_by_bot", "live": 2, "keys": ["s:d"]}]}


def _plan(**kw) -> dict:
    plan = {"version": 1, "per_set": 200, "targets": [["s:a", 1], ["s:c", 1], ["s:d", 2]],
            "counts": {"1": 2, "2": 1}, "logo_slots": {"1": 1, "2": 1},
            "over_capacity": {}, "moves": [], "held": [], "excluded": []}
    plan.update(kw)
    return plan


class WhatThePlanWouldChange(unittest.TestCase):

    def test_a_plan_that_matches_the_packs_is_not_pending(self):
        summary = plan_status.summarize(_plan(targets=[["s:a", 1], ["s:b", 1], ["s:c", 1],
                                                       ["s:d", 2]]), STATE)
        self.assertFalse(summary["pending"])

    def test_moves_held_and_candidates_are_counted_per_pack(self):
        plan = _plan(targets=[["s:a", 2], ["s:c", 1], ["s:d", 2], ["s:new", 2]],
                     moves=[{"key": "s:a", "label": "a", "from_pack": 1, "to_pack": 2}],
                     held=[{"key": "s:b", "label": "b", "from_pack": 1}],
                     excluded=["s:b"])
        packs = plan_status.summarize(plan, STATE)["packs"]
        self.assertEqual(packs[1]["move_out"], ["s:a"])
        self.assertEqual(packs[2]["move_in"], ["s:a"])
        self.assertEqual(packs[1]["held_live"], ["s:b"],
                         "a held emoji still in a pack would have to be removed")
        self.assertEqual(packs[2]["candidates"], ["s:new"])

    def test_over_capacity_is_pending(self):
        plan = _plan(targets=[["s:a", 1], ["s:b", 1], ["s:c", 1], ["s:d", 2]],
                     counts={"1": 200, "2": 1})
        self.assertTrue(plan_status.summarize(plan, STATE)["pending"])

    def test_the_command_reports_the_ids_it_would_retire_and_exits_3(self):
        with tempfile.TemporaryDirectory() as tmp:
            data = Path(tmp)
            (data / "pack_plan.json").write_text(json.dumps(_plan(
                moves=[{"key": "s:a", "label": "a", "from_pack": 1, "to_pack": 2}])),
                encoding="utf-8")
            (data / "publish_mine.json").write_text(json.dumps(STATE), encoding="utf-8")
            before = sorted((p.name, p.read_bytes()) for p in data.iterdir())
            out = io.StringIO()
            with mock.patch.object(plan_status, "setup_logging", lambda *a, **k: None), \
                    mock.patch.object(plan_status, "load_env", lambda: None), \
                    mock.patch.object(plan_status, "_catalog_ids",
                                      lambda d, b: {"s:a": "5000000000000000001"}), \
                    contextlib.redirect_stdout(out):
                code = plan_status.main(["--data-dir", str(data), "--base", "mine"])
            after = sorted((p.name, p.read_bytes()) for p in data.iterdir())
        self.assertEqual(code, plan_status.EXIT_PENDING)
        self.assertIn("5000000000000000001", out.getvalue())
        self.assertIn("RETIRE", out.getvalue())
        self.assertEqual(after, before, "the report must write nothing")

    def test_no_saved_plan_is_nothing_pending(self):
        with tempfile.TemporaryDirectory() as tmp, \
                mock.patch.object(plan_status, "setup_logging", lambda *a, **k: None), \
                mock.patch.object(plan_status, "load_env", lambda: None), \
                contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(plan_status.main(["--data-dir", tmp, "--base", "mine"]), 0)


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_plan_status -v")
