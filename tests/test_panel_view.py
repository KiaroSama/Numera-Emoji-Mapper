"""What ``emojikit.panel_view.build_view`` makes of the catalog.

The brand-logo preview card (shown, never counted, never saved), the
similarity order, the tap-to-copy emoji id, published packs leaving the grid,
and one pack unhidden on request. The server side lives in test_panel.py.
"""

from __future__ import annotations

import json
import os
import random
import sys
import tempfile
import unittest
from dataclasses import dataclass
from pathlib import Path
from unittest import mock

from tests._panel_fixtures import ROOT, _make_png

sys.path.insert(0, str(ROOT))

from emojikit import panel as p
from emojikit import panel_view as pv
from emojikit.catalog import Catalog
from emojikit.identity import hamming


class BrandLogoPreview(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.data = Path(self.tmp.name)
        with Catalog(self.data / "catalog.db") as cat:
            for i in range(3):
                img = self.data / "media" / "static" / f"i{i}.png"
                _make_png(img)
                cat.add(content_key=f"s:item{i:030d}", fmt="static", file_path=img,
                        emojis=["😀"], keywords=[f"item{i}"])
            self.cat_path = self.data / "catalog.db"

    def tearDown(self):
        self.tmp.cleanup()

    def _view(self, bot_username: str, logo_file: Path | None):
        operator = {"BRAND_LOGO_BOTS": "YourEmojiBot",
                    "BRAND_LOGO_PATH": str(logo_file) if logo_file else ""}
        with mock.patch.dict(os.environ, operator):
            with Catalog(self.cat_path) as cat:
                view, by_key, _hidden = p.build_view(cat, bot_username)
                return view, by_key

    def test_logo_shown_first_for_a_listed_bot(self):
        logo = self.data / "logo.png"
        _make_png(logo)
        view, by_key = self._view("YourEmojiBot", logo)
        self.assertTrue(view[0]["isLogo"])
        self.assertEqual(view[0]["key"], pv.LOGO_KEY)
        self.assertEqual(by_key[pv.LOGO_KEY], logo)
        # The 3 real catalog items still follow, none marked as logo.
        self.assertEqual(len(view), 4)
        self.assertTrue(all(not v.get("isLogo") for v in view[1:]))

    def test_logo_hidden_for_coin_bot(self):
        logo = self.data / "logo.png"
        _make_png(logo)
        view, _ = self._view("YourCoinEmojiBot", logo)
        self.assertEqual(len(view), 3)  # no logo card injected
        self.assertTrue(all(not v.get("isLogo") for v in view))

    def test_logo_hidden_when_file_missing(self):
        missing = self.data / "does_not_exist.png"
        view, _ = self._view("YourEmojiBot", missing)
        self.assertEqual(len(view), 3)

    def test_logo_hidden_when_bot_unknown(self):
        logo = self.data / "logo.png"
        _make_png(logo)
        view, _ = self._view("", logo)
        self.assertEqual(len(view), 3)

@dataclass
class _Fake:
    fmt: str
    phash: int | None


def _reference_order(items: list) -> list:
    """The original greedy walk, written against ``identity.hamming``."""
    out: list = []
    for fmt in sorted({it.fmt for it in items}, key=lambda f: pv.FMT_ORDER.get(f, 9)):
        group = [it for it in items if it.fmt == fmt]
        hashed = [it for it in group if it.phash is not None]
        plain = [it for it in group if it.phash is None]
        if hashed:
            remaining = hashed[:]
            ordered = [remaining.pop(0)]
            while remaining:
                last = ordered[-1].phash
                j = min(range(len(remaining)),
                        key=lambda i: hamming(remaining[i].phash, last))
                ordered.append(remaining.pop(j))
            out.extend(ordered)
        out.extend(plain)
    return out


class SimilarityOrder(unittest.TestCase):
    """The look-alike grouping must not shift when the walk gets faster.

    ``order_by_similarity`` seeds the saved publish order, so a different
    ordering is a different pack. The inlined popcount is only allowed to be
    identity.hamming's exact result, first-minimum tie-break included.
    """

    def _items(self, seed: int) -> list:
        rnd = random.Random(seed)
        items = [_Fake("static", rnd.getrandbits(64)) for _ in range(150)]
        # Duplicate hashes exercise the distance-0 shortcut, and equal distances
        # exercise the tie-break: min() keeps the FIRST minimum.
        items += [_Fake("static", items[3].phash) for _ in range(6)]
        items += [_Fake("video", rnd.getrandbits(64) & 0xFF) for _ in range(60)]
        items += [_Fake("animated", None) for _ in range(4)]
        rnd.shuffle(items)
        return items

    def test_matches_the_reference_walk(self):
        for seed in (1, 1234, 99999):
            with self.subTest(seed=seed):
                items = self._items(seed)
                self.assertEqual(
                    [id(x) for x in pv.order_by_similarity(items)],
                    [id(x) for x in _reference_order(items)])

    def test_unhashed_items_follow_their_format_group(self):
        items = [_Fake("animated", None), _Fake("static", 1), _Fake("static", 2)]
        out = pv.order_by_similarity(items)
        self.assertEqual([it.fmt for it in out], ["static", "static", "animated"])

class CopyTheEmojiId(unittest.TestCase):
    """Clicking the id under a card copies it, and does not toggle the card."""

    def test_only_a_whole_premium_id_label_is_copyable(self):
        self.assertEqual(pv.copy_id_for("premium-id:5406926593698312391"),
                         "5406926593698312391")
        # Anchored: a label that merely contains the prefix or trails junk is
        # not an id, and offering it would put the wrong thing on the clipboard.
        for junk in ("xpremium-id:12", "premium-id:12x", "premium-id:",
                     "premium-id:12 34", "", "coin logo", "premium-id:abc"):
            self.assertEqual(pv.copy_id_for(junk), "", junk)

    def test_the_view_carries_the_id_for_the_page(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        data = Path(tmp.name)
        img = data / "media" / "static" / "a.png"
        _make_png(img)
        other = data / "media" / "static" / "b.png"
        _make_png(other)
        with Catalog(data / "catalog.db") as cat:
            cat.add(content_key="s:" + "a" * 30, fmt="static", file_path=img,
                    emojis=["😀"], keywords=["premium-id:5406926593698312391"])
            cat.add(content_key="s:" + "b" * 30, fmt="static", file_path=other,
                    emojis=["😀"], keywords=["hand drawn"])
        with Catalog(data / "catalog.db") as cat:
            view, _by_key, _hidden = p.build_view(cat, "")
        by_label = {v["label"]: v["copyId"] for v in view}
        self.assertEqual(by_label["premium-id:5406926593698312391"],
                         "5406926593698312391")
        self.assertEqual(by_label["hand drawn"], "")

    def test_the_page_stops_the_click_before_the_toggle(self):
        """The label sits inside the card, so without this a copy also toggles.

        Asserted against the served scripts because the ordering lives in the
        click handler, not in any Python function.
        """
        page = p.SCRIPT
        copy_at = page.index("closest('.copyable')")
        toggle_at = page.index("closest('.card')", copy_at - 400)
        self.assertLess(copy_at, toggle_at,
                        "the copy branch must run before the toggle branch")
        self.assertIn("e.stopPropagation()", page[copy_at:copy_at + 200])

    def test_video_plays_without_hover(self):
        """Hover-only playback was rejected: a grid of stills cannot be curated.

        Video is bounded the same way the animated cards are -- by the viewport
        observer and the Animation switch -- not by the mouse.
        """
        page = p.SCRIPT
        self.assertIn("data-play", page.replace("dataset.play", "data-play"))
        self.assertIn("video[data-play]", page)
        # The remaining hover handlers exist only for prefers-reduced-motion.
        self.assertIn("if (RM) {", page)

class PublishedEmojiLeaveTheGrid(unittest.TestCase):
    """The panel arranges the pack being BUILT: anything live is out.

    The rule was "hide only a FULL set" for one round. `--new-set` broke that
    theory -- a pack can be left half-empty deliberately, so a set that is not
    full is not therefore unfinished, and the owner kept being shown an
    abandoned pack's emoji while curating the next one. Published is the
    property that decides it, and it needs no publish_*.json and no capacity
    arithmetic.
    """

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.data = Path(self.tmp.name)
        self.db = self.data / "catalog.db"
        with Catalog(self.db) as cat:
            for i in range(5):
                img = self.data / "media" / "static" / f"i{i}.png"
                _make_png(img)
                cat.add(content_key=f"s:item{i:030d}", fmt="static", file_path=img,
                        emojis=["😀"], keywords=[f"item{i}"])
            for i in (0, 1):                      # in the FULL set
                cat.mark_uploaded(f"s:item{i:030d}", f"cid{i}",
                                  base="pk", set_name="pk1_by_bot")
            cat.mark_uploaded(f"s:item{2:030d}", "cid2",   # in the HALF-EMPTY set
                              base="pk", set_name="pk2_by_bot")

    def tearDown(self):
        self.tmp.cleanup()

    def _view(self, show_published=False):
        with Catalog(self.db) as cat:
            return p.build_view(cat, "", show_published)

    def test_a_half_empty_pack_is_hidden_too_once_it_is_published(self):
        """The regression the owner reported twice: pack 2 kept coming back."""
        view, _by_key, hidden = self._view()
        keys = [v["key"] for v in view]
        self.assertEqual(hidden, 3, "two in the full set AND the half-empty one")
        self.assertNotIn(f"s:item{2:030d}", keys,
                         "published into a 3/200 set is still published")
        for i in (3, 4):
            self.assertIn(f"s:item{i:030d}", keys, "never published at all")

    def test_no_publish_state_file_is_needed(self):
        """The old rule read publish_*.json; this one asks the catalog."""
        self.assertEqual(list(self.data.glob("publish_*.json")), [])
        _view, _bk, hidden = self._view()
        self.assertEqual(hidden, 3)

    def test_a_row_with_no_recorded_set_name_still_counts_as_published(self):
        """Unknown WHERE is not unknown WHETHER -- it must not be offered up."""
        with Catalog(self.db) as cat:
            cat.mark_uploaded(f"s:item{3:030d}", "cid3", base="pk",
                              set_name=None)
        view, _bk, _h = self._view()
        self.assertNotIn(f"s:item{3:030d}", [v["key"] for v in view])

    def test_all_shows_everything(self):
        view, _bk, hidden = self._view(show_published=True)
        self.assertEqual(hidden, 0)
        self.assertEqual(len(view), 5)

    def test_hiding_deletes_nothing(self):
        self._view()
        with Catalog(self.db) as cat:
            self.assertEqual(len(cat.all_items()), 5, "no row was removed")
            self.assertTrue(cat.is_published("pk", f"s:item{0:030d}"))

class OnePackCanBeUnhidden(unittest.TestCase):
    """`--with-pack N` re-opens ONE published set, not all of them.

    `--all` is the wrong tool for arranging a half-full pack: it also brings
    back every finished pack, which on the real catalog is hundreds of cards
    nothing can be done with.
    """

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.data = Path(self.tmp.name)
        self.db = self.data / "catalog.db"
        # The logo cards need a configured operator logo: none ships in the repo.
        logo = self.data / "brand.png"
        _make_png(logo)
        operator = mock.patch.dict(os.environ, {"BRAND_LOGO_BOTS": "YourEmojiBot",
                                                "BRAND_LOGO_PATH": str(logo)})
        operator.start()
        self.addCleanup(operator.stop)
        with Catalog(self.db) as cat:
            for i in range(6):
                img = self.data / "media" / "static" / f"i{i}.png"
                _make_png(img)
                cat.add(content_key=f"s:item{i:030d}", fmt="static", file_path=img,
                        emojis=["😀"], keywords=[f"item{i}"])
            for i in (0, 1):
                cat.mark_uploaded(f"s:item{i:030d}", f"cid{i}",
                                  base="pk", set_name="pk1_by_bot")
            for i in (2, 3):
                cat.mark_uploaded(f"s:item{i:030d}", f"cid{i}",
                                  base="pk", set_name="pk2_by_bot")
            # 4 and 5 stay unpublished: the new candidates.
        (self.data / "publish_pk.json").write_text(json.dumps({
            "sets": [{"name": "pk1_by_bot", "index": 1, "live": 3, "logo": True},
                     {"name": "pk2_by_bot", "index": 2, "live": 3, "logo": True}],
        }), encoding="utf-8")

    def tearDown(self):
        self.tmp.cleanup()

    def _keys(self, keep=None):
        with Catalog(self.db) as cat:
            view, _bk, hidden = p.build_view(cat, "", False, keep)
        return {v["key"] for v in view if not v.get("isLogo")}, hidden

    def test_an_index_resolves_through_the_publishers_own_state(self):
        """Not by rebuilding '<base><n>_by_<bot>' -- the file records it."""
        self.assertEqual(p.packs_named(self.data, {2}), {"pk2_by_bot": 2})
        self.assertEqual(p.packs_named(self.data, {1, 2}),
                         {"pk1_by_bot": 1, "pk2_by_bot": 2})
        self.assertEqual(p.packs_named(self.data, {9}), {}, "no such pack")

    def test_the_index_travels_with_the_name_onto_the_card(self):
        """The grid cannot find the seam between two packs without it.

        Their real sizes are whatever they happen to be, so counting to the
        per-set capacity never lands on the boundary.
        """
        with Catalog(self.db) as cat:
            view, _bk, _h = p.build_view(cat, "", False,
                                         p.packs_named(self.data, {1, 2}))
        packs = {v["key"]: v.get("pack") for v in view if not v.get("isLogo")}
        self.assertEqual(packs[f"s:item{0:030d}"], 1)
        self.assertEqual(packs[f"s:item{2:030d}"], 2)
        # A candidate is in no pack yet, and saying "1" would be a lie the grid
        # would then draw a boundary from.
        self.assertIsNone(packs[f"s:item{4:030d}"])

    def test_every_shown_pack_gets_its_own_live_logo_card(self):
        """One card for the whole grid put the logo on whichever pack happened
        to be shown first and left the other looking like it had none -- pack 5
        was accused of exactly that. Each of these packs really does carry the
        logo as its emoji 0; it went up when the pack was created."""
        with Catalog(self.db) as cat:
            view, by_key, _h = p.build_view(cat, "YourEmojiBot", False,
                                            p.packs_named(self.data, {1, 2}))
        logos = [v for v in view if v.get("isLogo")]
        self.assertEqual([v["pack"] for v in logos], [1, 2])
        self.assertNotIn(pv.LOGO_KEY, {v["key"] for v in logos},
                         "the 'auto-added on publish' card is for a NEW pack")
        for v in logos:
            self.assertIn(v["key"], by_key, "the card needs art to show")
            # It has to open its pack's run, or the grid draws the boundary
            # BELOW it and the logo reads as the previous pack's.
            self.assertLess(view.index(v),
                            min(i for i, c in enumerate(view)
                                if c.get("pack") == v["pack"] and not c.get("isLogo")))

    def test_without_with_pack_the_single_publish_preview_is_unchanged(self):
        """The normal flow builds ONE new pack, and its logo is not live yet."""
        with Catalog(self.db) as cat:
            view, _bk, _h = p.build_view(cat, "YourEmojiBot", False, None)
        logos = [v for v in view if v.get("isLogo")]
        self.assertEqual([v["key"] for v in logos], [pv.LOGO_KEY])
        self.assertEqual(view[0]["key"], pv.LOGO_KEY, "always first")

    def test_an_unnamed_pack_tags_nothing(self):
        """Without --with-pack there is no membership to draw, by design."""
        with Catalog(self.db) as cat:
            view, _bk, _h = p.build_view(cat, "", False, None)
        self.assertEqual([v for v in view if v.get("pack") is not None], [])

    def test_only_the_named_pack_comes_back(self):
        keys, hidden = self._keys(p.packs_named(self.data, {2}))
        self.assertEqual(hidden, 2, "pack 1 stays hidden")
        for i in (2, 3):
            self.assertIn(f"s:item{i:030d}", keys, "pack 2 is visible")
        for i in (0, 1):
            self.assertNotIn(f"s:item{i:030d}", keys, "pack 1 is not")
        for i in (4, 5):
            self.assertIn(f"s:item{i:030d}", keys, "candidates always show")

    def test_without_the_flag_every_published_pack_stays_hidden(self):
        keys, hidden = self._keys(None)
        self.assertEqual(hidden, 4)
        self.assertEqual(keys, {f"s:item{i:030d}" for i in (4, 5)})

    def test_a_row_with_no_recorded_set_is_not_unhidden_by_guesswork(self):
        """Unknown WHERE must not be answered with "probably that one"."""
        with Catalog(self.db) as cat:
            cat.mark_uploaded(f"s:item{4:030d}", "cid4", base="pk", set_name=None)
        keys, _h = self._keys(p.packs_named(self.data, {2}))
        self.assertNotIn(f"s:item{4:030d}", keys)

if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_panel_view -v")
