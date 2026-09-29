"""`python -m emojikit.plan_apply`: the saved pack plan, applied to live packs.

Everything runs against an in-memory Telegram; no test here can reach a live
pack (and tests/__init__.py blocks the network regardless). The fixture builds
a real catalog, real publisher state and a fake live family that agree with
each other, so the applier is exercised through the publisher's own verified
add path, not a stub of it.
"""

from __future__ import annotations

import contextlib
import io
import json
import os
import random
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

from PIL import Image

from emojikit import build_collection, identity, plan_apply, sync_order
from emojikit.catalog import Catalog
from emojikit.collection_state import _lock_path, _state_path, load_state, save_json
from emojikit.packstate import exclusive_lock
from emojikit.panel_plan import read_plan
from emojikit.errors import FloodWaitTooLong
from emojikit.telegram_api import LiveStateUnknown, SetState, Telegram

BASE, BOT = "cryptoemoji", "YourEmojiBot"


def _name(n: int) -> str:
    return f"{BASE}{n}_by_{BOT}"


def _image(path: Path, seed: int) -> Path:
    """A 100x100 grid of random opaque cells: no two seeds look alike."""
    rnd = random.Random(seed)
    im = Image.new("RGBA", (100, 100), (0, 0, 0, 0))
    for gx in range(10):
        for gy in range(10):
            if rnd.random() < 0.6:
                c = (rnd.randrange(256), rnd.randrange(256), rnd.randrange(256), 255)
                im.paste(c, (gx * 10, gy * 10, gx * 10 + 10, gy * 10 + 10))
    path.parent.mkdir(parents=True, exist_ok=True)
    im.save(path, "PNG")
    return path


class Crash(BaseException):
    """The process dying right after Telegram applied a delete."""


class FakeTelegram:
    """Live sets whose stickers carry file_id / file_unique_id / custom_emoji_id."""

    def __init__(self):
        self.sets: dict[str, list[dict]] = {}
        self.bodies: dict[str, bytes] = {}
        self.writes: list[tuple[str, str]] = []
        self.unreadable: set[str] = set()
        self.crash_after_delete = False
        self.flood_on: set[str] = set()   # ops refused once with a long retry_after
        self.n = 0

    def _sticker(self, path, fmt, emojis) -> dict:
        self.n += 1
        fid = f"FID-new-{self.n}"
        self.bodies[fid] = Path(path).read_bytes()
        return {"custom_emoji_id": f"cid-new-{self.n}", "file_id": fid,
                "file_unique_id": f"FU-new-{self.n}", "emojis": list(emojis), "fmt": fmt}

    def get_me(self):
        return {"username": BOT}

    def probe_set_state(self, name):
        if name in self.unreadable:
            return SetState.UNKNOWN, None
        if name not in self.sets:
            return SetState.MISSING, None
        return SetState.EXISTS, {"stickers": [dict(s) for s in self.sets[name]]}

    def get_sticker_set(self, name):
        state, sset = self.probe_set_state(name)
        if state is SetState.UNKNOWN:
            raise RuntimeError("getStickerSet failed after 5 attempts")
        if sset is None:
            raise RuntimeError("getStickerSet failed: STICKERSET_INVALID")
        return sset

    def download_file(self, file_id, dest, retries=5):
        Path(dest).write_bytes(self.bodies[str(file_id)])
        return Path(dest)

    def create_emoji_set(self, user_id, name, title, path, fmt, emoji_list, keywords,
                         *, needs_repainting=False):
        self.writes.append(("create", name))
        self.sets[name] = [self._sticker(path, fmt, emoji_list)]

    def add_emoji(self, user_id, name, path, fmt, emoji_list, keywords, *,
                  expected_before=None):
        self._flood("add")
        self.writes.append(("add", name))
        self.sets[name].append(self._sticker(path, fmt, emoji_list))

    def set_sticker_position(self, file_id, position):
        self.writes.append(("move", file_id))
        for stickers in self.sets.values():
            for i, st in enumerate(stickers):
                if st["file_id"] == file_id:
                    stickers.insert(position, stickers.pop(i))
                    return

    def call(self, method, *, data=None, files=None, retries=5, applied_check=None):
        assert method == "deleteStickerFromSet", method
        self._flood("delete")
        self.writes.append(("delete", data["sticker"]))
        for stickers in self.sets.values():
            stickers[:] = [st for st in stickers if st["file_id"] != data["sticker"]]
        if self.crash_after_delete:
            self.crash_after_delete = False
            raise Crash()
        return True

    def send_message(self, chat_id, text, *, disable_preview=False):
        pass

    def _flood(self, op: str) -> None:
        # What the real client raises over its ceiling: the request was
        # REFUSED with retry_after, so nothing was applied.
        if op in self.flood_on:
            self.flood_on.discard(op)
            raise FloodWaitTooLong(op, 900)


class Family:
    """A catalog, publisher state, saved plan and fake live packs that agree."""

    def __init__(self, data: Path, packs: dict[int, int], extra: int = 0,
                 fmt: str = "mixed"):
        self.data, self.tg = data, FakeTelegram()
        self.by_pack: dict[int, list[str]] = {}
        self.extra: list[str] = []
        seed = 0
        state = {"base": BASE, "sets": [], "sent": [], "sent_full": [], "skipped": []}
        with Catalog(data / "catalog.db") as cat:
            order = []

            def new_item() -> str:
                nonlocal seed
                seed += 1
                p = _image(data / "media" / "static" / f"item{seed}.png", seed)
                key = identity.content_key(p, "static")
                cat.add(content_key=key, fmt="static", file_path=p, emojis=["😀"],
                        keywords=[f"item{seed}"])
                order.append(key)
                return key

            for n, count in sorted(packs.items()):
                name = _name(n)
                stickers = [{"custom_emoji_id": f"logo-{n}", "file_id": f"FID-logo-{n}",
                             "file_unique_id": f"FU-logo-{n}"}]
                keys = [new_item() for _ in range(count)]
                for key in keys:
                    i = len(stickers)
                    st = {"custom_emoji_id": f"cid-{n}-{i}", "file_id": f"FID-{n}-{i}",
                          "file_unique_id": f"FU-{n}-{i}"}
                    stickers.append(st)
                    cat.mark_uploaded(key, st["custom_emoji_id"], base=BASE, set_name=name)
                    cat.record_file_unique_id(st["file_unique_id"], key)
                self.tg.sets[name] = stickers
                self.by_pack[n] = keys
                state["sets"].append({"fmt": fmt, "index": n, "name": name,
                                      "title": f"Test Pack {n}", "live": len(stickers),
                                      "logo": True, "keys": list(keys)})
                state["sent"].append(name)
            self.extra = [new_item() for _ in range(extra)]
            cat.set_order(order)
        save_json(_state_path(data, BASE), state)

    def plan(self, targets: dict[str, int], *, excluded=(), per_set=200, **kw) -> None:
        packs = set(targets.values()) | set(self.by_pack)
        doc = {"version": 1, "per_set": per_set,
               "targets": [[k, n] for k, n in sorted(targets.items())],
               "excluded": sorted(excluded), "held": [], "moves": [], "counts": {},
               "over_capacity": {}, "logo_slots": {str(n): 1 for n in packs}}
        doc.update(kw)
        (self.data / "pack_plan.json").write_text(json.dumps(doc), encoding="utf-8")

    def current(self) -> dict[str, int]:
        """Every live key -> its pack, as the plan's starting point."""
        return {k: n for n, keys in self.by_pack.items() for k in keys}

    def apply(self, max_changes: int = 20, logo: Path | None = None) -> int:
        with mock.patch.object(build_collection, "notify"), \
                mock.patch.object(build_collection.time, "sleep"), \
                mock.patch.object(sync_order.time, "sleep"), \
                Catalog(self.data / "catalog.db") as cat, \
                contextlib.redirect_stdout(io.StringIO()):
            return plan_apply.apply_run(
                self.tg, cat, plan=read_plan(self.data), state=load_state(self.data, BASE),
                data_dir=self.data, base=BASE, user_id=111111111, bot=BOT,
                logo_bots=frozenset({BOT.lower()}) if logo else frozenset(),
                logo_path=str(logo) if logo else None, max_changes=max_changes)

    def steps(self) -> plan_apply.Steps:
        items, ids = plan_apply.read_catalog(self.data / "catalog.db", BASE)
        return plan_apply.compute_steps(read_plan(self.data), load_state(self.data, BASE),
                                        items, ids)

    def assert_records_match_live(self, case: unittest.TestCase) -> None:
        state = load_state(self.data, BASE)
        seen: set[str] = set()
        with Catalog(self.data / "catalog.db") as cat:
            for rec in state["sets"]:
                live = self.tg.sets[rec["name"]]
                case.assertEqual(rec["live"], len(live), rec["name"])
                case.assertEqual(len(live), 1 + len(rec["keys"]), rec["name"])
                for key, st in zip(rec["keys"], live[1:], strict=True):
                    case.assertEqual(cat.custom_emoji_id_for(BASE, key),
                                     st["custom_emoji_id"], key)
                    case.assertNotIn(key, seen, f"{key} is live twice")
                    seen.add(key)

    def snapshot(self) -> list[tuple[str, bytes]]:
        # SQLite's -wal/-shm sidecars are the reader's shared memory, created by
        # any read of a WAL database; the database itself must not change.
        return sorted((str(p.relative_to(self.data)), p.read_bytes())
                      for p in self.data.rglob("*")
                      if p.is_file() and not p.name.endswith(("-wal", "-shm")))


class _Case(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.data = Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()


class CatalogUnpublish(_Case):
    def test_forgets_one_item_in_one_family_only(self):
        fam = Family(self.data, {1: 2})
        a, b = fam.by_pack[1]
        with Catalog(self.data / "catalog.db") as cat:
            cat.mark_uploaded(a, "cid-other", base="otherbase", set_name="other1_by_x")
            self.assertTrue(cat.unpublish(BASE, a))
            self.assertFalse(cat.unpublish(BASE, a), "a second call finds nothing")
            self.assertFalse(cat.is_published(BASE, a))
            self.assertTrue(cat.is_published(BASE, b))
            self.assertTrue(cat.is_published("otherbase", a))


class DryRun(_Case):
    def _main(self, *extra) -> tuple[int, str]:
        out = io.StringIO()
        with mock.patch.object(plan_apply, "setup_logging", lambda *a, **k: None), \
                mock.patch.object(plan_apply, "load_env", lambda: None), \
                contextlib.redirect_stdout(out):
            code = plan_apply.main(["--data-dir", str(self.data), "--base", BASE, *extra])
        return code, out.getvalue()

    def test_lists_the_work_retires_ids_and_writes_nothing(self):
        fam = Family(self.data, {1: 3, 2: 3}, extra=1)
        targets = fam.current()
        targets[fam.by_pack[1][2]] = 2          # move 1 -> 2
        targets[fam.by_pack[2][0]] = 1          # move 2 -> 1
        targets[fam.extra[0]] = 2               # brand-new emoji
        fam.plan(targets)
        before = fam.snapshot()
        code, out = self._main()
        self.assertEqual(code, plan_apply.EXIT_PENDING)
        steps = fam.steps()
        self.assertEqual((len(steps.removals), sum(map(len, steps.adds.values())),
                          steps.counted), (2, 3, 5))
        self.assertIn("5 counted change(s): 1 run(s)", out)
        self.assertIn("cid-1-3", out)            # the ids a move retires
        self.assertIn("cid-2-1", out)
        self.assertEqual(fam.snapshot(), before, "a dry run must write nothing")
        self.assertEqual(fam.tg.writes, [])

    def test_a_plan_the_packs_already_match_exits_0(self):
        fam = Family(self.data, {1: 3})
        fam.plan(fam.current())
        code, out = self._main()
        self.assertEqual(code, 0)
        self.assertIn("nothing pending", out)

    def test_a_held_emoji_that_is_live_stays_and_is_not_a_removal(self):
        fam = Family(self.data, {1: 2, 2: 2})
        held = fam.by_pack[2][1]
        targets = fam.current()
        del targets[held]
        fam.plan(targets, excluded=[held])
        steps = fam.steps()
        self.assertEqual(steps.removals, [])
        self.assertEqual(steps.held_live, [(held, 2)])
        self.assertIn("held, stays live until placed", self._main()[1])

    def test_two_hundred_moves_plan_offline_in_seconds(self):
        keys = [f"s:{i:04d}" for i in range(400)]
        items = {k: {"included": True, "pos": i} for i, k in enumerate(keys)}
        state = {"sets": [{"fmt": "mixed", "index": 1, "name": "a", "logo": True,
                           "keys": keys[:199]},
                          {"fmt": "mixed", "index": 2, "name": "b", "logo": True,
                           "keys": keys[199:398]}]}
        plan = {"targets": [[k, 2] for k in keys[:199]] + [[k, 1] for k in keys[199:398]]}
        ids = {k: f"cid-{k}" for k in keys}
        started = time.monotonic()
        steps = plan_apply.compute_steps(plan, state, items, ids)
        self.assertLess(time.monotonic() - started, 10)
        self.assertEqual(len(steps.removals), 398)


class ApplyInBoundedRuns(_Case):
    def test_twenty_five_changes_take_two_runs_then_nothing(self):
        fam = Family(self.data, {1: 10, 2: 2}, extra=5)
        targets = {k: 2 for k in fam.by_pack[1] + fam.by_pack[2] + fam.extra}
        fam.plan(targets)
        self.assertEqual(fam.steps().counted, 25)

        self.assertEqual(fam.apply(), plan_apply.EXIT_PENDING)
        counted = [w for w in fam.tg.writes if w[0] in ("delete", "add", "create")]
        self.assertEqual(len(counted), 20, "the cap is 20 counted changes per run")
        fam.assert_records_match_live(self)

        self.assertEqual(fam.apply(), 0)
        kinds = [w[0] for w in fam.tg.writes]
        self.assertEqual((kinds.count("delete"), kinds.count("add")), (10, 15))
        fam.assert_records_match_live(self)
        # The pack now reads in the panel's order (logo first).
        with Catalog(self.data / "catalog.db") as cat:
            want = [cat.custom_emoji_id_for(BASE, k)
                    for k in fam.by_pack[1] + fam.by_pack[2] + fam.extra]
        self.assertEqual([st["custom_emoji_id"] for st in fam.tg.sets[_name(2)][1:]], want)
        self.assertEqual(len(fam.tg.sets[_name(1)]), 1, "only the logo stays in pack 1")
        retired = json.loads((self.data / f"plan_apply_{BASE}.json")
                             .read_text(encoding="utf-8"))["retired"]
        self.assertEqual(len(retired), 10)

        writes = len(fam.tg.writes)
        self.assertEqual(fam.apply(), 0)
        self.assertEqual(len(fam.tg.writes), writes, "a finished plan changes nothing")

    def test_the_next_new_pack_is_created_with_the_logo_first(self):
        fam = Family(self.data, {1: 1, 2: 1}, extra=2)
        fam.plan({**fam.current(), **{k: 3 for k in fam.extra}})
        self.assertEqual((fam.steps().create, fam.steps().counted), (3, 3))
        logo = _image(self.data / "brand-logo.png", 999)
        self.assertEqual(fam.apply(logo=logo), 0)
        self.assertEqual([w[0] for w in fam.tg.writes], ["create", "add", "add"])
        new = fam.tg.sets[_name(3)]
        self.assertEqual(new[0]["file_unique_id"], "FU-new-1", "the logo leads the pack")
        rec = load_state(self.data, BASE)["sets"][-1]
        self.assertEqual((rec["index"], rec["title"], rec["keys"]), (3, "Test Pack 3", fam.extra))
        fam.assert_records_match_live(self)

    def test_a_delete_applied_before_a_crash_is_settled_not_resent(self):
        fam = Family(self.data, {1: 2, 2: 1})
        moved = fam.by_pack[1][0]
        targets = fam.current()
        targets[moved] = 2
        fam.plan(targets)
        fam.tg.crash_after_delete = True
        with self.assertRaises(Crash):
            fam.apply()
        journal = json.loads((self.data / f"plan_apply_{BASE}.json").read_text(encoding="utf-8"))
        self.assertEqual(journal["intent"]["key"], moved)

        self.assertEqual(fam.apply(), 0)
        kinds = [w[0] for w in fam.tg.writes]
        self.assertEqual(kinds.count("delete"), 1, "the delete must not be sent twice")
        self.assertEqual(kinds.count("add"), 1)
        fam.assert_records_match_live(self)

    def test_an_intent_whose_delete_never_landed_is_cleared_and_redone_once(self):
        fam = Family(self.data, {1: 2, 2: 1})
        moved = fam.by_pack[1][0]
        targets = fam.current()
        targets[moved] = 2
        fam.plan(targets)
        save_json(self.data / f"plan_apply_{BASE}.json", {
            "version": 1, "base": BASE, "retired": [],
            "intent": {"op": "remove", "key": moved, "set": _name(1), "cid": "cid-1-1",
                       "at": "2026-09-29T00:00:00Z"}})
        self.assertEqual(fam.apply(), 0)
        self.assertEqual([w[0] for w in fam.tg.writes].count("delete"), 1)
        fam.assert_records_match_live(self)


class LongFloodWaitEndsTheRunCleanly(_Case):
    def _moved(self) -> Family:
        fam = Family(self.data, {1: 2, 2: 1})
        fam.plan({**fam.current(), fam.by_pack[1][0]: 2})
        return fam

    def test_on_a_delete_the_intent_is_cleared_and_nothing_is_recorded(self):
        fam = self._moved()
        fam.tg.flood_on.add("delete")
        self.assertEqual(fam.apply(), plan_apply.EXIT_PENDING)
        journal = json.loads((self.data / f"plan_apply_{BASE}.json").read_text(encoding="utf-8"))
        self.assertEqual((journal["intent"], journal["retired"]), (None, []))
        self.assertEqual(fam.tg.writes, [])
        fam.assert_records_match_live(self)
        self.assertEqual(fam.apply(), 0, "the next run does the work")
        fam.assert_records_match_live(self)

    def test_on_an_add_the_removal_stays_recorded_and_the_next_run_adds(self):
        fam = self._moved()
        fam.tg.flood_on.add("add")
        self.assertEqual(fam.apply(), plan_apply.EXIT_PENDING)
        self.assertEqual([w[0] for w in fam.tg.writes], ["delete"])
        fam.assert_records_match_live(self)
        self.assertEqual(fam.apply(), 0)
        counted = [w[0] for w in fam.tg.writes if w[0] != "move"]   # reorder is not counted
        self.assertEqual(counted, ["delete", "add"])
        fam.assert_records_match_live(self)


class ClientFloodCeiling(unittest.TestCase):
    """Telegram.call: over the ceiling it raises instead of sleeping."""

    def _client(self, retry_after: int, ceiling):
        tg = Telegram("123:test")
        tg.max_flood_wait = ceiling
        answers = iter([{"ok": False, "description": f"Too Many Requests: retry after "
                         f"{retry_after}", "parameters": {"retry_after": retry_after}},
                        {"ok": True, "result": True}])
        tg.s = mock.Mock(post=lambda *a, **k: mock.Mock(json=lambda: next(answers)))
        return tg

    def test_a_wait_over_the_ceiling_raises_without_sleeping(self):
        tg = self._client(900, 300)
        with mock.patch("emojikit.telegram_api.time.sleep") as sleep, \
                contextlib.redirect_stdout(io.StringIO()), \
                self.assertRaises(FloodWaitTooLong) as ctx:
            tg.call("deleteStickerFromSet", data={"sticker": "x"})
        self.assertEqual(ctx.exception.seconds, 900)
        sleep.assert_not_called()

    def test_a_wait_under_the_ceiling_is_still_honoured(self):
        tg = self._client(20, 300)
        with mock.patch("emojikit.telegram_api.time.sleep") as sleep, \
                contextlib.redirect_stdout(io.StringIO()):
            self.assertTrue(tg.call("deleteStickerFromSet", data={"sticker": "x"}))
        sleep.assert_called_once_with(21)


class RefuseBeforeTouchingAnything(_Case):
    def _refused(self, fam: Family, needle: str) -> None:
        state_file = _state_path(self.data, BASE)
        before = state_file.read_bytes()
        with self.assertRaises(plan_apply.Refusal) as ctx:
            fam.apply()
        self.assertIn(needle, str(ctx.exception))
        self.assertEqual(fam.tg.writes, [])
        self.assertEqual(state_file.read_bytes(), before)
        self.assertFalse((self.data / f"plan_apply_{BASE}.json").exists())

    def test_a_pack_over_its_cap(self):
        fam = Family(self.data, {1: 3}, extra=2)
        fam.plan({**fam.current(), **{k: 1 for k in fam.extra}}, per_set=5)
        self._refused(fam, "over the cap of 5")

    def test_the_panel_flagged_over_capacity(self):
        fam = Family(self.data, {1: 1})
        fam.plan(fam.current(), over_capacity={"1": 201})
        self._refused(fam, "overfills")

    def test_a_key_the_catalog_no_longer_holds(self):
        fam = Family(self.data, {1: 1})
        fam.plan({**fam.current(), "s:gone": 1})
        self._refused(fam, "s:gone")

    def test_a_per_format_family(self):
        fam = Family(self.data, {1: 1}, fmt="static")
        fam.plan(fam.current())
        self._refused(fam, "per format")

    def test_a_gap_in_the_pack_numbers(self):
        fam = Family(self.data, {1: 2, 2: 1})
        fam.plan({**fam.current(), fam.by_pack[1][0]: 9})
        self._refused(fam, "[9]")

    def test_an_unreadable_live_set_stops_before_changing_it(self):
        fam = Family(self.data, {1: 2, 2: 1})
        fam.plan({**fam.current(), fam.by_pack[1][0]: 2})
        fam.tg.unreadable.add(_name(1))
        with self.assertRaises(LiveStateUnknown):
            fam.apply()
        self.assertEqual(fam.tg.writes, [])

    def test_the_family_lock_held_by_another_writer(self):
        fam = Family(self.data, {1: 2, 2: 1})
        fam.plan({**fam.current(), fam.by_pack[1][0]: 2})
        env = {"GENERAL_BOT_TOKEN": "123:test"}
        with exclusive_lock(_lock_path(self.data, BASE)), \
                mock.patch.dict(os.environ, env), \
                mock.patch.object(plan_apply, "setup_logging", lambda *a, **k: None), \
                mock.patch.object(plan_apply, "load_env", lambda: None), \
                mock.patch.object(plan_apply.operator_config, "brand_logo",
                                  lambda *a: (frozenset(), None)), \
                mock.patch.object(plan_apply, "Telegram",
                                  side_effect=AssertionError("must not reach Telegram")), \
                contextlib.redirect_stdout(io.StringIO()):
            code = plan_apply.main(["--data-dir", str(self.data), "--base", BASE,
                                    "--user-id", "111111111", "--apply"])
        self.assertEqual(code, 2)
        self.assertEqual(fam.tg.writes, [])


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_plan_apply -v")
