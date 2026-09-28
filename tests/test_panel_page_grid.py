"""The panel page's grid: off-screen cost, the virtual window, zoom, shipping, pack splits.

Served-text contracts: they supplement the browser tests, which verify the
actual gestures, timing, layout and persisted results.
"""

from __future__ import annotations

import re
import unittest

from tests._panel_fixtures import ROOT
from tests._panel_page_fixtures import PAGE, SCRIPT, block, p

class OffScreenCostsNothing(unittest.TestCase):
    """A card you cannot see must not be decoding frames or holding a player.

    Four hundred and forty-one of the 1 068 cards are animated and 56 are
    video: if leaving the viewport did not stop them the grid would decode
    every one of them forever, which is what "the page is heavy" first meant.
    """

    def test_leaving_the_viewport_stops_video_AND_animation(self):
        io_block = block(SCRIPT, "const animIO", "}, {root:")
        # One flag decides both media kinds, and it is driven by intersection.
        self.assertIn("e.isIntersecting", io_block)
        self.assertIn("setPlaying(t, live)", io_block, "video must be paused when it scrolls away, not muted")
        self.assertIn("inView.delete(t)", io_block)
        self.assertIn("showStill(t)", io_block, "an animated image must fall back to its single frame")
        # Coming back must restore it -- a one-way stop would leave a dead grid.
        self.assertIn("inView.add(t)", io_block)
        allocate = block(SCRIPT, "function allocate(){", chr(10) + "}")
        self.assertIn("showMotion(", allocate)
        self.assertIn("ANIM_BUDGET", allocate, "the animation budget must bound what plays")

    def test_a_mounted_card_is_observed_and_an_unmounted_one_released(self):
        """mount() creates the nodes, so it is what must start observing; the
        window moves on every scroll, so unmounting must stop it again or the
        observer keeps a reference to every card that ever scrolled past."""
        mount = block(SCRIPT, "function mount(k){", "function unmountCard")
        self.assertIn("animIO.observe(n)", mount)
        self.assertIn("videoIO.observe(n)", mount)
        unmount = block(SCRIPT, "function unmountCard(c){", "const videoIO")
        self.assertIn("animIO.unobserve(n)", unmount)
        self.assertIn("forgetNode(n)", unmount, "an unmounted card must leave the budget's sets")
        self.assertIn("videoIO.unobserve(n)", unmount)
        self.assertIn("cards.delete(c.dataset.key);", unmount)

    def test_a_video_holds_a_player_only_near_the_viewport(self):
        """A <video> costs a media player from the moment it has a source, and
        creating or tearing one down was the 60-140 ms frame that survived the
        virtual grid. The card carries the URL; the observer attaches it."""
        video = block(SCRIPT, "if(it.fmt === 'video'){", "} else if(it.fmt === 'animated'){")
        self.assertIn("v.dataset.src = src", video)
        self.assertNotIn("v.src =", video, "a mounted card must not create a player by itself")
        attach = block(SCRIPT, "function attachVideo(v, on){", "function setPlaying")
        self.assertIn("v.src = v.dataset.src", attach)
        self.assertIn("v.removeAttribute('src'); v.load();", attach,
                      "removing the attribute alone leaves the player alive")
        self.assertIn("rootMargin: '0px'", block(SCRIPT, "const videoIO", "function attachVideo"))
        self.assertIn("v.poster=", video, "a still poster is visible before a decoder is attached")
        # Playing implies a source, whichever observer fired first.
        self.assertIn("if (on) attachVideo(v, true);", block(SCRIPT, "function setPlaying(v, on){", "}"))

    def test_the_sticky_header_does_not_blur_its_backdrop(self):
        """backdrop-filter re-blurs everything behind it on every scroll frame.

        It is the most expensive thing a sticky bar can do, and over an opaque
        background it buys nothing.
        """
        # Strip CSS comments first. Twice now a check like this has passed or
        # failed on the COMMENT explaining the rule rather than the rule.
        rule = re.sub(r"/\*.*?\*/", "", block(PAGE, "header{", "}"), flags=re.S)
        self.assertNotIn("backdrop-filter", rule)


class TheGridIsVirtual(unittest.TestCase):
    """Only the rows near the viewport exist. Measured on the 1 063-card
    catalog: every drag step re-laid-out 1 062 cards (24-46 ms per pointer
    move, 40 % of frames over 32 ms) and a scroll sweep spent 802 ms in long
    tasks. With the window it is 16-24 ms, none over 32 ms, and 113 ms.
    """

    def test_only_rows_near_the_viewport_are_in_the_document(self):
        render = block(SCRIPT, "function render(){", "function retire")
        self.assertIn("const pad = innerHeight / 2;", render)
        self.assertIn("let first = rowAt(viewTop - pad);", render)
        # Two spacers carry the height of everything above and below, so the
        # scrollbar and the jump buttons see the whole grid.
        self.assertIn("setSpacer(padTop, first > 0 ? rows[first].top - G.gap : 0);", render)
        self.assertIn("setSpacer(padBot, bottom < total ? total - bottom - G.gap : 0);", render)
        for spacer in ('id="padTop"', 'id="padBot"'):
            self.assertIn(spacer, PAGE)
        self.assertIn(".spacer{grid-column:1/-1", PAGE)

    def test_the_layout_is_arithmetic_on_whole_pixels(self):
        """Every offset is a sum of row heights JavaScript chose, handed to CSS
        as variables and rounded to whole pixels; a fractional row height would
        drift the window by a pixel per row over a long grid. A card is a
        fixed height for the same reason: a row whose height depends on its
        longest label cannot be positioned without rendering it first."""
        layout = block(SCRIPT, "function layoutRows(){", "function measure")
        self.assertIn("Math.round(BASE.gap * zoom)", layout)
        self.assertIn("Math.round((compact ? BASE.cardHCompact : BASE.cardH) * zoom)", layout)
        self.assertIn("grid.style.setProperty('--cardH', G.cardH + 'px');", layout)
        card = block(PAGE, ".card{", "}")
        self.assertIn("height:var(--cardH", card)
        self.assertIn("overflow:hidden", card)
        # Line-anchored: `.card.logo .lbl{` comes first in the sheet and would match.
        self.assertIn("-webkit-line-clamp:2", block(PAGE, "\n.lbl{", "}"), "the label cannot decide the height")
        # The window IS the lazy loading; the old per-card hint has no job left.
        self.assertNotIn("content-visibility", PAGE)
        self.assertNotIn("contain-intrinsic-size", PAGE)

    def test_a_separator_starts_a_fresh_row_like_css_would(self):
        """Card rows are chunked exactly as CSS grid auto-placement would place
        them: a full-width marker takes its own row and the next card row
        starts under it. If the arithmetic and the browser ever disagreed, the
        spacers would be wrong by one row for every pack."""
        layout = block(SCRIPT, "function layoutRows(){", "function measure")
        self.assertIn("if(sepAt.has(i)){", layout)
        self.assertIn("indices.length < G.cols && !sepAt.has(visible[v])", layout)

    def test_scrolling_within_the_same_rows_costs_a_binary_search(self):
        """render() runs once per frame while scrolling. Between row
        transitions it must find the same window and stop; a full reconcile
        per frame measured 4 ms of JavaScript on every scroll frame."""
        render = block(SCRIPT, "function render(){", "function retire")
        self.assertIn("if(first === win.first && last === win.last) return;", render)
        # But a changed model must always project, whatever the window was.
        layout = block(SCRIPT, "function layoutRows(){", "function measure")
        self.assertIn("win = {first: -1, last: -1};", layout)
        self.assertIn("requestAnimationFrame", block(SCRIPT, "function scheduleRender(){", "}"))

    def test_reconciling_moves_nodes_and_retires_only_the_stale_ones(self):
        render = block(SCRIPT, "function render(){", "function retire")
        self.assertIn("if(n === cur){ cur = cur.nextSibling; continue; }", render)
        self.assertIn("grid.insertBefore(n, cur);", render)
        self.assertIn("retire(stale)", render)


class ZoomFitsMoreOrLess(unittest.TestCase):
    """Zoom out to see more emoji per screen, in to inspect one."""

    def test_the_header_offers_zoom_in_out_and_reset(self):
        for btn in ('id="zoomOut"', 'id="zoomReset"', 'id="zoomIn"'):
            self.assertIn(btn, PAGE)
        self.assertIn("document.getElementById('zoomIn').onclick = ()=>setZoom(zoom * ZOOM_STEP);", SCRIPT)
        self.assertIn("document.getElementById('zoomOut').onclick = ()=>setZoom(zoom / ZOOM_STEP);", SCRIPT)
        # zoomReset is a typeable percentage (panel-holding.js), not a plain
        # reset button any more -- Enter applies it, double-click still resets.
        self.assertIn('<input id="zoomReset"', PAGE)
        self.assertIn("if(e.key==='Enter'){e.preventDefault();applyZoomInput();}", SCRIPT)
        self.assertIn("zoomInput.addEventListener('dblclick',()=>setZoom(1));", SCRIPT)

    def test_ctrl_wheel_and_ctrl_keys_are_taken_over_from_the_browser(self):
        """Ctrl+wheel is the browser's own page zoom. Left alone, both would
        fire: the grid re-zooms AND the whole page scales."""
        wheel = block(SCRIPT, "addEventListener('wheel'", "{passive: false});")
        self.assertIn("if(!e.ctrlKey) return;", wheel)
        self.assertIn("e.preventDefault();", wheel, "without it the browser zooms too")
        keys = block(SCRIPT, "addEventListener('keydown', e=>{", "});")
        for key in ("'='", "'-'", "'0'"):
            self.assertIn(key, keys)

    def test_the_level_is_clamped_and_remembered(self):
        """Persistence goes through the guarded adapter, never straight at
        localStorage: a browser that blocks site data THROWS on the property,
        and the unguarded read this replaced took the rest of the file with it.
        That the level really does survive a reload is asserted against a real
        browser in ``tests/test_panel_browser.py``."""
        z = block(SCRIPT, "function setZoom(z){", "function paintZoom")
        self.assertIn("Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, z))", z)
        self.assertIn("prefs.set('panelZoom'", z)
        self.assertIn("prefs.get('panelZoom'", block(SCRIPT, "function loadZoom(){", "}"))
        # Every preference, not just this one -- one raw call is the whole bug.
        raw = re.findall(r"localStorage\.(?:get|set)Item", SCRIPT)
        self.assertEqual(len(raw), 2, f"unguarded localStorage access: {raw}")
        self.assertIn("const prefs = {", SCRIPT)

    def test_zooming_keeps_the_top_item_on_screen(self):
        """Zooming is for seeing more or less of the same place, not for losing
        it: the item at the top of the screen is re-scrolled to the top."""
        z = block(SCRIPT, "function setZoom(z){", "function paintZoom")
        self.assertLess(z.index("const anchor = firstVisibleIndex();"), z.index("zoom = z;"))
        self.assertLess(z.index("layoutRows();"),
                        z.index("scrollTo(0, gridTop + rows[rowOfItem[anchor]].top - headerH)"))

    def test_far_out_the_text_rows_are_dropped(self):
        """Zoomed far out the text under a thumbnail is unreadable anyway;
        dropping it lets the rows pack tighter, which is what zooming out is
        for. The height model follows: a compact card has its own base height."""
        self.assertIn("const COMPACT_BELOW = 0.75;", SCRIPT)
        self.assertIn("document.body.classList.toggle('compact', compact);", SCRIPT)
        self.assertIn("body.compact .glyph,body.compact .lbl,body.compact .sub{display:none}", PAGE)

    def test_everything_in_a_card_scales_from_one_font_size(self):
        """One `font-size` on the card, everything inside in em: the zoom
        factor scales thumbnail, badges and label as one unit. A pixel size
        left inside would stay put while the card around it grew."""
        card = block(PAGE, ".card{", "}")
        self.assertIn("font-size:calc(12px*var(--z", card)
        self.assertIn("width:9em;height:9em", block(PAGE, ".thumb{", "}"))
        self.assertIn("width:8.67em;height:8.67em", block(PAGE, ".thumb img,.thumb video{", "}"))
        self.assertNotIn("104px", PAGE)
        self.assertNotIn("img.width = 104", SCRIPT, "an attribute size would pin the artwork at 100 %")


class ThePanelPageActuallyShips(unittest.TestCase):
    """The page moved out of panel.py into assets/panel.html.

    Every other test in this module reads `p.PAGE` and so would still pass if
    the asset were missing from a checkout -- the import would simply blow up
    first, somewhere unrelated. That is not hypothetical: the brand logo was an
    absolute `F:\\` path once, so on every machine but one the "mandatory" logo
    silently vanished and nothing failed. Same shape, same guard.
    """

    def test_the_page_is_a_real_file_inside_the_repo(self):
        asset = p.ASSET_DIR / "panel.html"
        self.assertTrue(asset.is_file(), f"the panel page is missing: {asset}")
        self.assertTrue(
            str(asset.resolve()).startswith(str(ROOT.resolve())),
            "the page must ship in the repo, not point at a machine-specific path")

    def test_the_loaded_page_is_the_document_the_handler_expects(self):
        """Loaded, not just present: an empty or truncated file must not pass."""
        self.assertTrue(PAGE.startswith("<!doctype html>"))
        self.assertTrue(PAGE.rstrip().endswith("</html>"))
        # Every placeholder the handler substitutes must survive extraction --
        # a page missing one renders the literal token to the browser.
        for token in ("__ITEMS__", "__TOKEN__", "__PREVIEW_FPS__",
                      "__PER_SET__", "__HIDDEN__", "__ASSET_VER__", "__ICON_VER__"):
            self.assertIn(token, PAGE, f"{token} lost in the asset")

class PackSplitsAndJumpButtons(unittest.TestCase):
    """The pack-boundary markers, and the two jump buttons beside them."""

    def test_a_separator_is_never_a_card(self):
        """The drop handler resolves its target with closest('.card').

        A separator carrying that class would sit between cards, swallow a drop
        aimed past it and do nothing -- the same shape as the bug where a drop
        on a grid gap silently threw the emoji to the end. It is `.packsep`.
        """
        self.assertIn("grid-column:1/-1", block(PAGE, ".packsep{", "}"))
        body = block(SCRIPT, "function sepNode(", chr(10) + "function ")
        self.assertIn("el('div','packsep')", body)
        self.assertNotIn("'card'", body)
        self.assertNotIn("packsep card", SCRIPT)
        # And the drop handler still keys off .card, so the two cannot meet.
        self.assertIn("e.target.closest('.card')", SCRIPT)

    def test_the_splits_are_counted_from_included_items_only(self):
        """An unticked card never ships, so it cannot push the boundary."""
        body = block(SCRIPT, "function packStarts(){", "function layoutRows")
        self.assertIn("!it.isLogo && it.included", body)
        # capacity leaves a slot for the logo, exactly as build_collection does.
        self.assertIn("PER_SET - (logo ? 1 : 0)", body)

    def test_a_candidate_no_longer_splits_the_pack_it_was_dropped_into(self):
        """Dragging one emoji into a pack used to open a "Not in a pack yet"
        run: a full-width marker plus the empty rest of its row, for every
        single card moved. The owner arranges by dropping candidates into a
        pack, so that made the grid unusable exactly when it was being used.
        """
        self.assertNotIn("'Not in a pack yet'", SCRIPT)
        body = block(SCRIPT, "function packStarts(){", "function layoutRows")
        # A null pack leaves `cur` alone, so the run it was dropped into
        # continues -- which is also the pack it will publish into.
        self.assertIn("pk !== null && (!starts.length || pk !== cur)", body)

    def test_the_logo_opens_its_own_packs_run(self):
        """The logo IS emoji 0 of its pack. While it was excluded from run
        detection the marker landed one card below it, so pack 5's logo drew
        above pack 5's header and read as part of the pack before it."""
        body = block(SCRIPT, "function packStarts(){", "function layoutRows")
        i_mem = body.index("if(byMembership){")
        i_cap = body.index("} else if(!it.isLogo && it.included){")
        self.assertLess(i_mem, i_cap, "membership mode must not filter logos out")
        self.assertNotIn("isLogo", body[i_mem:i_cap], "a logo has to be able to start its pack's run")

    def test_selection_changes_recompute_the_splits(self):
        """Both inclusion paths must relayout, not just update the counter.

        Unticking enough cards genuinely moves a boundary, so a counter-only
        refresh left the markers lying.
        """
        # `markSelDirty()` rides along on both paths now -- a selection change
        # is also the moment the page learns it differs from what the server
        # acknowledged -- so this asserts the relayout, not the exact tail.
        self.assertIn("relayout(); updateCount(); markSelDirty(); }", SCRIPT)
        self.assertIn("lastIdx=i; relayout(); updateCount(); markSelDirty();",
                      SCRIPT)

    def test_separators_are_part_of_the_row_model_not_accumulated(self):
        """Rebuilt from the pack starts on every layout and reused by title, so
        they can neither pile up nor go stale."""
        layout = block(SCRIPT, "function layoutRows(){", "function measure")
        self.assertIn("const sepAt = new Map();", layout)
        self.assertIn("rows.push({sep: sepAt.get(i), at: i, top: y, h: G.sepH});", layout)

    def test_one_pack_needs_no_divider(self):
        self.assertIn("if(starts.length >= 2){", block(SCRIPT, "function layoutRows(){", "function measure"))

    def test_the_header_offers_top_and_bottom(self):
        self.assertIn('id="top"', PAGE)
        self.assertIn('id="bot"', PAGE)
        # Document scrolling, NOT scrollIntoView: that aligns with the top of
        # the viewport, which sits behind the sticky header, so Top stopped one
        # header short of the Pack 1 marker and Bottom stopped short too. The
        # bottom spacer makes scrollHeight exact, so Bottom really is the end.
        jump = SCRIPT[SCRIPT.index("document.getElementById('top').onclick"):][:400]
        self.assertIn("window.scrollTo({top:0})", jump)
        self.assertIn("document.documentElement.scrollHeight", jump)
        self.assertNotIn("scrollIntoView", jump)

if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_panel_page_grid -v")
