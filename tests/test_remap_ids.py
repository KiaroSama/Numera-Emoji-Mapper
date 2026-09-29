"""Regression tests for the coin remap / pack audit tools.

Both tools cache expensive live downloads, and both used to trust that cache
blindly:
  1. ``remap_ids.download_live`` marked a set "done" even when stickers failed,
     so a set could be cached as complete with no signatures at all.
  2. The cache key was the set INDEX, so a replaced/appended/deleted sticker was
     never re-read, and ids of deleted stickers stayed match candidates.
  3. ``--apply`` accepted every nearest neighbour however poor or ambiguous and
     overwrote the canonical ticker->custom_emoji_id map with it.
  4. (``check_all_packs``'s own cache defects are in test_check_all_packs.)
  5. ``--apply`` computed its whole-map replacement from a live read taken
     BEFORE any lock and took the map lock only around the write, so a provider
     that appended a sticker AND its map entry in between had that entry erased
     by a replacement that was, formally, correctly locked.

Everything here uses fakes: no network, no Telegram, no real sleeping.
"""

from __future__ import annotations

import base64
import importlib.util
import json
import os
import re
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "coins"))

import numpy as np  # noqa: E402
from PIL import Image  # noqa: E402

from emojikit.build_pack import (EXIT_FAILED, EXIT_OK, EXIT_PARTIAL, EXIT_USAGE)  # noqa: E402
from emojikit.packstate import (LockBusy, exclusive_lock, pack_family_lock_path, write_json_atomic)  # noqa: E402

from coins import _dedup_plan as rd_cfg  # noqa: E402  - imported for its BASE, see PackFamilyLockTest
import remap_ids  # noqa: E402
from tests._coin_fixtures import (FakeSession, _png,  # noqa: E402
                                  _provider_top_up, _sticker)
from tests._coin_fixtures import LiveSetsTelegram as FakeTelegram  # noqa: E402


class DownloadLiveTest(unittest.TestCase):
    """download_live: completeness, manifest identity, pruning."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.cache_path = Path(self.tmp.name) / "cache.json"
        self.sleep = mock.patch.object(remap_ids.time, "sleep", lambda s: None)
        self.sleep.start()

    def tearDown(self):
        self.sleep.stop()
        self.tmp.cleanup()

    def _run(self, tg, session, cache):
        tg.session = session
        return remap_ids.download_live(tg, [{"index": 1, "name": "s1"}],
                                       cache, self.cache_path)

    def test_failed_sticker_leaves_set_incomplete_and_is_retried(self):
        tg = FakeTelegram({"s1": [_sticker("a"), _sticker("b")]})
        blobs = {"a": _png((200, 20, 20, 255)), "b": _png((20, 200, 20, 255))}
        cache = remap_ids.load_cache(self.cache_path)

        incomplete = self._run(tg, FakeSession(blobs, fail={"b"}), cache)

        self.assertEqual(incomplete, ["s1"])
        self.assertNotIn("s1", cache["sets"])          # NOT cached as complete
        self.assertEqual(list(cache["sigs"]), ["a"])
        self.assertIn("b", cache["errors"])
        self.assertNotIn("TOKEN123", json.dumps(cache))  # token never persisted

        # A re-run retries only the failed sticker and then completes the set.
        sess = FakeSession(blobs)
        incomplete = self._run(tg, sess, remap_ids.load_cache(self.cache_path))
        self.assertEqual(incomplete, [])
        self.assertEqual(sess.fetched, ["b"])
        after = remap_ids.load_cache(self.cache_path)
        self.assertIn("s1", after["sets"])
        self.assertEqual(after["errors"], {})

    def test_changed_manifest_invalidates_cache_and_prunes_stale_ids(self):
        blobs = {"a": _png((200, 20, 20, 255)), "b": _png((20, 200, 20, 255)),
                 "c": _png((20, 20, 200, 255))}
        tg = FakeTelegram({"s1": [_sticker("a"), _sticker("b")]})
        cache = remap_ids.load_cache(self.cache_path)
        self.assertEqual(self._run(tg, FakeSession(blobs), cache), [])
        digest_before = cache["sets"]["s1"]

        # "b" is replaced by "c" (the classic re-upload): the set index is
        # unchanged, so the old cache skipped it forever.
        tg.sets["s1"] = [_sticker("a"), _sticker("c")]
        sess = FakeSession(blobs)
        self.assertEqual(self._run(tg, sess, cache), [])

        self.assertEqual(sess.fetched, ["c"])                 # re-read, not skipped
        self.assertNotEqual(cache["sets"]["s1"], digest_before)
        self.assertEqual(sorted(cache["sigs"]), ["a", "c"])   # stale "b" pruned

    def test_appended_sticker_is_picked_up(self):
        blobs = {"a": _png((200, 20, 20, 255)), "b": _png((20, 200, 20, 255))}
        tg = FakeTelegram({"s1": [_sticker("a")]})
        cache = remap_ids.load_cache(self.cache_path)
        self._run(tg, FakeSession(blobs), cache)
        tg.sets["s1"].append(_sticker("b"))
        sess = FakeSession(blobs)
        self._run(tg, sess, cache)
        self.assertEqual(sess.fetched, ["b"])
        self.assertEqual(sorted(cache["sigs"]), ["a", "b"])

    def test_legacy_cache_keeps_signatures_but_re_verifies_sets(self):
        """Old LAYOUT migrates; the signatures themselves are still good.

        The placeholder has to be a real-length signature. A short stand-in used
        to pass here, but load_cache now also drops signatures written by an
        older signature() -- a separate rule, covered by
        ACacheFromAnOlderSignatureIsDiscarded -- and a 4-byte stub would trip
        that one instead, testing the wrong thing.
        """
        # Old layout: {"done_sets": [1], "sigs": {...}} keyed by set index.
        sig = base64.b64encode(bytes(remap_ids.SIG_LEN)).decode()
        self.cache_path.write_text(json.dumps({"done_sets": [1], "sigs": {"a": sig}}),
                                   encoding="utf-8")
        cache = remap_ids.load_cache(self.cache_path)
        self.assertEqual(cache["sets"], {})
        self.assertEqual(list(cache["sigs"]), ["a"])


class NearestTest(unittest.TestCase):
    def test_identical_rows_never_produce_negative_or_nan_distance(self):
        row = np.full((1, 768), 255.0, dtype=np.float32)
        live = np.repeat(row, 3, axis=0)
        _, d2, second = remap_ids.nearest(row, live)
        self.assertTrue((d2 >= 0).all())
        self.assertFalse(np.isnan(np.sqrt(d2)).any())
        self.assertTrue((second >= 0).all())      # runner-up is a real candidate

    def test_single_candidate_has_infinite_margin(self):
        local = np.zeros((1, 768), dtype=np.float32)
        live = np.ones((1, 768), dtype=np.float32)
        idx, _, second = remap_ids.nearest(local, live)
        self.assertEqual(idx.tolist(), [0])
        self.assertTrue(np.isinf(second).all())   # nothing to be ambiguous with


class MainApplyTest(unittest.TestCase):
    """End-to-end main(): what --apply is and is not allowed to overwrite."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        self.emoji = self.dir / "emoji"
        self.emoji.mkdir()
        self.out = self.dir / "ticker_to_id.json"
        self.cand = self.dir / "ticker_to_id.candidate.json"
        # The real family lock lives in the repo; keep the suite off it.
        self.pack_lock = self.dir / "pack_cryptoemoji.lock"
        (self.dir / "state.json").write_text(
            json.dumps({"sets": [{"index": 1, "name": "s1"}]}), encoding="utf-8")

    def tearDown(self):
        self.tmp.cleanup()

    def _local(self, name: str, png: bytes) -> None:
        (self.emoji / f"{name}.png").write_bytes(png)

    def _main(self, tg, session, *extra):
        tg.session = session
        argv = ["remap_ids", "--emoji-dir", str(self.emoji),
                "--state", str(self.dir / "state.json"),
                "--cache", str(self.dir / "cache.json"),
                "--out", str(self.out), "--candidates", str(self.cand), *extra]
        with mock.patch.object(remap_ids, "Telegram", lambda token: tg), \
                mock.patch.object(remap_ids, "PACK_LOCK", self.pack_lock), \
                mock.patch.object(remap_ids, "load_env", lambda: None), \
                mock.patch.object(remap_ids, "setup_logging", lambda *a, **k: None), \
                mock.patch.object(remap_ids.time, "sleep", lambda s: None), \
                mock.patch.dict(os.environ, {"TELEGRAM_BOT_TOKEN": "TOKEN123"}), \
                mock.patch.object(sys, "argv", argv):
            return remap_ids.main()

    def test_apply_refuses_to_overwrite_canonical_map_from_incomplete_data(self):
        self.out.write_text(json.dumps({"btc": "GOOD-OLD"}), encoding="utf-8")
        self._local("btc", _png((200, 20, 20, 255)))
        tg = FakeTelegram({"s1": [_sticker("a"), _sticker("b")]})
        blobs = {"a": _png((200, 20, 20, 255)), "b": _png((20, 200, 20, 255))}

        rc = self._main(tg, FakeSession(blobs, fail={"b"}), "--max-distance", "100", "--apply")

        self.assertEqual(rc, EXIT_PARTIAL)
        self.assertEqual(json.loads(self.out.read_text("utf-8")), {"btc": "GOOD-OLD"})
        self.assertEqual(json.loads(self.cand.read_text("utf-8")), {"btc": "a"})
        self.assertFalse(self.out.with_suffix(".prebroken.json").exists())

    def test_apply_refuses_ambiguous_matches(self):
        # Two live stickers that differ by a hair: the winner is not meaningful.
        self._local("btc", _png((200, 20, 20, 255)))
        tg = FakeTelegram({"s1": [_sticker("a"), _sticker("b")]})
        blobs = {"a": _png((200, 20, 20, 255)), "b": _png((201, 20, 20, 255))}

        rc = self._main(tg, FakeSession(blobs), "--max-distance", "100", "--apply")

        self.assertEqual(rc, EXIT_PARTIAL)
        self.assertFalse(self.out.exists())              # canonical map untouched
        self.assertEqual(json.loads(self.cand.read_text("utf-8")), {})

    def test_apply_writes_map_when_complete_and_unambiguous(self):
        self.out.write_text(json.dumps({"btc": "STALE"}), encoding="utf-8")
        self._local("btc", _png((200, 20, 20, 255)))
        self._local("eth", _png((20, 200, 20, 255)))
        tg = FakeTelegram({"s1": [_sticker("a"), _sticker("b")]})
        blobs = {"a": _png((200, 20, 20, 255)), "b": _png((20, 200, 20, 255))}

        rc = self._main(tg, FakeSession(blobs), "--max-distance", "100", "--apply")

        self.assertEqual(rc, EXIT_OK)
        self.assertEqual(json.loads(self.out.read_text("utf-8")), {"btc": "a", "eth": "b"})
        self.assertEqual(json.loads(self.out.with_suffix(".prebroken.json").read_text("utf-8")),
                         {"btc": "STALE"})

    def test_far_match_is_omitted_not_mapped(self):
        self._local("btc", _png((200, 20, 20, 255)))
        self._local("ghost", _png((20, 20, 200, 255)))   # never uploaded
        tg = FakeTelegram({"s1": [_sticker("a")]})
        rc = self._main(tg, FakeSession({"a": _png((200, 20, 20, 255))}),
                        "--max-distance", "50", "--apply")
        self.assertEqual(rc, EXIT_OK)
        self.assertEqual(json.loads(self.out.read_text("utf-8")), {"btc": "a"})

    def test_apply_without_max_distance_is_a_usage_error(self):
        self._local("btc", _png((200, 20, 20, 255)))
        tg = FakeTelegram({"s1": [_sticker("a")]})
        rc = self._main(tg, FakeSession({"a": _png((200, 20, 20, 255))}), "--apply")
        self.assertEqual(rc, EXIT_USAGE)
        self.assertFalse(self.out.exists())

    def test_no_local_logos_fails_instead_of_crashing(self):
        tg = FakeTelegram({"s1": [_sticker("a")]})
        rc = self._main(tg, FakeSession({"a": _png((200, 20, 20, 255))}))
        self.assertEqual(rc, EXIT_FAILED)

    def test_a_short_result_from_nearest_raises_instead_of_mapping_fewer(self):
        """nearest() returns four arrays that MUST line up with `tickers`.

        zip() truncating to the shortest of them would drop every ticker past
        the gap, and --apply would then replace the canonical map with fewer
        coins than the run examined -- reporting success either way.
        """
        self._local("btc", _png((200, 20, 20, 255)))
        self._local("eth", _png((20, 200, 20, 255)))
        tg = FakeTelegram({"s1": [_sticker("a"), _sticker("b")]})
        blobs = {"a": _png((200, 20, 20, 255)), "b": _png((20, 200, 20, 255))}
        real = remap_ids.nearest

        def truncated(local, live):
            idx, d2, second = real(local, live)
            return idx[:1], d2[:1], second[:1]

        with mock.patch.object(remap_ids, "nearest", truncated), \
                self.assertRaises(ValueError):
            self._main(tg, FakeSession(blobs), "--max-distance", "100", "--apply")
        self.assertFalse(self.out.exists(), "a short map must not be written")


class ApplyIsSerialisedAgainstThePackFamily(unittest.TestCase):
    """5: the VALUE written has to come from a pack that cannot move.

    ``--apply`` replaces the whole map, so locking only the write is not enough:
    the replacement was computed from a live read taken before any lock, and a
    provider that appended a sticker and its map entry in between was erased by
    it. Merging instead is not the answer either -- ``--apply`` exists to throw a
    corrupted map away, and a merge would carry the corruption back in. So the
    guarantee tested here is the stricter one: the window does not exist,
    because the pack-family lock is held from the live read through the write.
    """

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        self.emoji = self.dir / "emoji"
        self.emoji.mkdir()
        (self.emoji / "btc.png").write_bytes(_png((200, 20, 20, 255)))
        self.out = self.dir / "ticker_to_id.json"
        self.out.write_text(json.dumps({"btc": "STALE"}), encoding="utf-8")
        self.pack_lock = self.dir / "pack_cryptoemoji.lock"
        (self.dir / "state.json").write_text(
            json.dumps({"sets": [{"index": 1, "name": "s1"}]}), encoding="utf-8")

    def tearDown(self):
        self.tmp.cleanup()

    def _main(self, tg, session):
        tg.session = session
        argv = ["remap_ids", "--emoji-dir", str(self.emoji),
                "--state", str(self.dir / "state.json"),
                "--cache", str(self.dir / "cache.json"),
                "--out", str(self.out),
                "--candidates", str(self.dir / "cand.json"),
                "--max-distance", "100", "--apply"]
        with mock.patch.object(remap_ids, "Telegram", lambda token: tg), \
                mock.patch.object(remap_ids, "PACK_LOCK", self.pack_lock), \
                mock.patch.object(remap_ids, "load_env", lambda: None), \
                mock.patch.object(remap_ids, "setup_logging", lambda *a, **k: None), \
                mock.patch.object(remap_ids.time, "sleep", lambda s: None), \
                mock.patch.dict(os.environ, {"TELEGRAM_BOT_TOKEN": "TOKEN123"}), \
                mock.patch.object(sys, "argv", argv):
            return remap_ids.main()

    def test_a_provider_cannot_land_a_map_entry_during_the_live_read(self):
        landed = []

        def provider():
            landed.append(_provider_top_up(self.pack_lock, self.out, "new", "z"))

        tg = FakeTelegram({"s1": [_sticker("a")]}, on_read=provider)
        rc = self._main(tg, FakeSession({"a": _png((200, 20, 20, 255))}))

        self.assertEqual(rc, EXIT_OK)
        # Refused, not raced: the provider never got to write an entry that this
        # run's whole-file replacement would then have thrown away.
        self.assertIsInstance(landed[0], LockBusy)
        self.assertEqual(json.loads(self.out.read_text("utf-8")), {"btc": "a"})

        # And the lock is released afterwards, so the provider's retry lands on
        # the freshly rebuilt map instead of fighting it.
        self.assertIsNone(_provider_top_up(self.pack_lock, self.out, "new", "z"))
        self.assertEqual(json.loads(self.out.read_text("utf-8")),
                         {"btc": "a", "new": "z"})

    def test_a_held_pack_lock_stops_apply_before_it_reads_anything(self):
        """LockBusy is an exit, not a traceback, and not a partial write."""
        sess = FakeSession({"a": _png((200, 20, 20, 255))})
        tg = FakeTelegram({"s1": [_sticker("a")]})
        with exclusive_lock(self.pack_lock):
            rc = self._main(tg, sess)

        self.assertEqual(rc, EXIT_FAILED)
        self.assertEqual(sess.fetched, [])       # the lock precedes the live read
        self.assertEqual(json.loads(self.out.read_text("utf-8")), {"btc": "STALE"})
        self.assertFalse(self.out.with_suffix(".prebroken.json").exists())


class PackFamilyLockTest(unittest.TestCase):
    def test_it_is_the_same_lock_the_pack_mutators_take(self):
        """A lock of our own would serialise nothing.

        rebuild_dedup, the fetchers and verify_logos --fix all key their lock on
        this same base name; a drifted copy here reads as locked and excludes
        nobody.
        """
        self.assertEqual(remap_ids.SET_BASE, rd_cfg.BASE)
        self.assertEqual(remap_ids.PACK_LOCK, rd_cfg.LOCK)
        self.assertEqual(remap_ids.PACK_LOCK,
                         pack_family_lock_path(remap_ids.SET_BASE))


class NumpyIsAnExtra(unittest.TestCase):
    """numpy is imported by exactly one first-party file: this one.

    Making it a hard requirement of the whole project meant every user of the
    emoji-pack builder installed a ~20 MB wheel for a coin-reconciliation tool
    they may never run.
    """

    def _reload_without(self, missing: str):
        spec = importlib.util.spec_from_file_location(
            f"remap_ids_no_{missing}", ROOT / "coins" / "remap_ids.py")
        mod = importlib.util.module_from_spec(spec)
        # None in sys.modules is the documented way to make `import x` fail.
        with mock.patch.dict(sys.modules, {missing: None}), \
                mock.patch.object(sys, "argv", ["remap_ids.py"]):
            spec.loader.exec_module(mod)

    def test_a_missing_numpy_says_which_file_to_install(self):
        with self.assertRaises(SystemExit) as caught:
            self._reload_without("numpy")
        self.assertIn("requirements-coins.txt", str(caught.exception))

    def test_nothing_else_first_party_imports_numpy(self):
        """The premise of the split: if it spreads, the extra has to come back."""
        roots = [ROOT / "coins", ROOT / "emojikit"]
        users = sorted(p.name for d in roots for p in d.glob("*.py")
                       if re.search(r"^\s*import numpy", p.read_text(encoding="utf-8"),
                                    re.MULTILINE))
        users += sorted(p.name for p in ROOT.glob("*.py")
                        if re.search(r"^\s*import numpy", p.read_text(encoding="utf-8"),
                                     re.MULTILINE))
        self.assertEqual(users, ["remap_ids.py"])


class TheSignatureMustNotBeBlindToBlackOnTransparent(unittest.TestCase):
    """A black mark on transparency must not collapse onto every other one.

    signature() composited on black and kept only RGB, so any logo drawn in
    black on a transparent background flattened to a uniformly black square:
    all-zero signature, pairwise distance 0.0, every such coin indistinguishable
    from every other. 135 of the 5875 real coin logos are exactly that shape --
    Aptos, Arkham, NEAR, Worldcoin, Bittensor all ship a black wordmark.

    It cost an entire wrong diagnosis: 129 tickers "provably shared one picture"
    and were written up as corrupt source art, when the files were fine and the
    METRIC was blind. A distance of 0.0 between two files is a claim about the
    measure before it is a claim about the files.
    """

    @staticmethod
    def _black_on_transparent(shape) -> Image.Image:
        im = Image.new("RGBA", (100, 100), (0, 0, 0, 0))
        px = im.load()
        for x, y in shape:
            px[x, y] = (0, 0, 0, 255)          # pure black, fully opaque
        return im

    def _two_distinct_marks(self):
        left = [(x, y) for x in range(10, 45) for y in range(10, 90)]
        ring = [(x, y) for x in range(20, 80) for y in range(20, 80)
                if 22 <= ((x - 50) ** 2 + (y - 50) ** 2) ** 0.5 <= 29]
        return (self._black_on_transparent(left),
                self._black_on_transparent(ring))

    def test_two_different_black_marks_are_not_identical(self):
        a, b = self._two_distinct_marks()
        d = float(np.linalg.norm(remap_ids.signature(a) - remap_ids.signature(b)))
        self.assertGreater(
            d, 0.0,
            "two visibly different black-on-transparent marks measured as the "
            "same image; the signature cannot see shape")

    def test_a_black_mark_carries_information_at_all(self):
        a, _ = self._two_distinct_marks()
        sig = remap_ids.signature(a)
        self.assertEqual(len(sig), remap_ids.SIG_LEN)
        self.assertGreater(
            float(sig.max()), 0.0,
            "an all-zero signature says 'I cannot see this image', and the "
            "matcher would read it as 'this image equals that one'")

    def test_colour_still_separates_two_marks_of_the_same_shape(self):
        """The silhouette must not swamp colour: same shape, different colour."""
        shape = [(x, y) for x in range(20, 80) for y in range(20, 80)]
        red, blue = Image.new("RGBA", (100, 100), (0, 0, 0, 0)), \
            Image.new("RGBA", (100, 100), (0, 0, 0, 0))
        for im, colour in ((red, (220, 30, 30, 255)), (blue, (30, 30, 220, 255))):
            px = im.load()
            for x, y in shape:
                px[x, y] = colour
        d = float(np.linalg.norm(remap_ids.signature(red) - remap_ids.signature(blue)))
        self.assertGreater(d, 0.0, "colour information was lost")


class ACacheFromAnOlderSignatureIsDiscarded(unittest.TestCase):
    """Mixing two signature formats matches coins against noise, silently.

    The cached vectors are raw bytes with no format marker, so a shorter one
    from a previous definition does not raise -- it just compares wrongly.
    """

    def test_signatures_of_the_wrong_length_are_dropped(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "cache.json"
            old = base64.b64encode(bytes(remap_ids.SIG_LEN // 2)).decode()
            new = base64.b64encode(bytes(remap_ids.SIG_LEN)).decode()
            write_json_atomic(path, {"sets": {"s1": "digest"},
                                     "sigs": {"old": old, "new": new},
                                     "errors": {}})
            cache = remap_ids.load_cache(path)
            self.assertNotIn("old", cache["sigs"], "a stale-format signature survived")
            self.assertIn("new", cache["sigs"])
            self.assertEqual(cache["sets"], {},
                             "completeness marks must be cleared too, or the "
                             "dropped sets are never re-read")


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_remap_ids -v")
