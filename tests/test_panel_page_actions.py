"""The panel page's editing gestures: drag-and-drop, undo/redo, selection mode.

Served-text contracts: they supplement the browser tests, which verify the
actual gestures, timing, layout and persisted results.
"""

from __future__ import annotations

import unittest

from tests._panel_page_fixtures import PAGE, SCRIPT, block, p

class DragAndDropOrdering(unittest.TestCase):
    """The model is edited AS you drag; the drop only keeps what it finds."""

    def test_the_drop_keeps_the_model_instead_of_recomputing_an_index(self):
        """Two bugs came from drop-time index arithmetic; there is none now.

        It once fell back to `ITEMS.length-1` when the release was not on a
        card, so a drop in a grid gap threw the emoji to the end. Then it used
        `splice(from,1)` followed by `splice(to,0)` with a `to` measured BEFORE
        the removal -- which lands the card one slot past the tile you aimed at
        when dragging DOWN and exactly on it when dragging UP.

        Every dragover now moves the carried items inside ITEMS and re-projects
        the grid, so the translucent tile IS where they land. The drop compares
        the order with the snapshot taken at dragstart and records it.
        """
        drop = block(SCRIPT, "grid.addEventListener('drop'", "grid.addEventListener('dragend'")
        self.assertIn("commitDrag()", drop)
        for arithmetic in ("findIndex", "splice", "ITEMS.length"):
            self.assertNotIn(arithmetic, drop, f"the drop handler must not reach for {arithmetic}")
        commit = block(SCRIPT, "function commitDrag(){", "function endDrag")
        self.assertIn("order.every((k,i)=>k===dragSnap.order[i])", commit)
        self.assertIn("selSig(ITEMS.filter(x=>x.included).map(x=>x.key))===selSig(dragSnap.included)",
                      commit,
                      "a tray release changes inclusion even when its position stays the same")
        self.assertIn("packs===JSON.stringify(dragSnap.packs)) return;", commit,
                      "and a cross-pack drop changes the stamp even when the order holds")
        self.assertLess(commit.index("remember(dragSnap);"), commit.index("saveOrder();"))

    def test_a_cancelled_drag_restores_the_order_taken_at_dragstart(self):
        """The model moves with the pointer, so an escape or a drop outside
        must put the dragstart order back, or the page shows an order that is
        not the one it saved."""
        start = block(SCRIPT, "grid.addEventListener('dragstart'", "grid.addEventListener('dragover'")
        self.assertIn("dragSnap=snapshot();", start)
        end = block(SCRIPT, "function endDrag(committed){", "// --- Auto-scroll")
        self.assertIn("if(!committed && dragKey !== null){ applySnapshot(dragSnap,false); }", end)

    def test_the_pointer_side_decides_before_or_after(self):
        """Without it the last slot of a row is unreachable: every hover would
        mean "before this card"."""
        over = block(SCRIPT, "grid.addEventListener('dragover'", "function moveCarried")
        self.assertIn("getBoundingClientRect", over)
        self.assertIn("(e.clientX > r.left + r.width/2) ? j + 1 : j", over)

    def test_hovering_the_slot_the_run_already_holds_changes_nothing(self):
        """dragover fires continuously; a relayout per event that moves nothing
        would be the old full-grid cost back under another name."""
        move = block(SCRIPT, "function moveCarried(slot){", "grid.addEventListener('drop'")
        self.assertIn("if(next.every((x,i)=>x===ITEMS[i])) return;", move)
        self.assertLess(move.index("return;"), move.index("relayout();"))

    def test_dragging_to_an_edge_scrolls_the_page(self):
        """Without this the drag is trapped in the current viewport.

        With 200 cards there is otherwise no way to carry #200 up to #10.
        """
        self.assertIn("function edgeScroll(", SCRIPT)
        self.assertIn("requestAnimationFrame", SCRIPT)
        # On the document: at the top of the window the pointer sits over the
        # sticky header, where a grid-only listener never fires.
        doc_over = SCRIPT.index("document.addEventListener('dragover'")
        self.assertIn("edgeScroll(e.clientY)", SCRIPT[doc_over:doc_over + 300])

    def test_a_carried_card_is_parked_never_unmounted(self):
        """The browser delivers dragend to the SOURCE node. A carried card whose
        row scrolls out of the window used to be a candidate for removal, and a
        removed source leaves the gesture stuck with no drop and no dragend."""
        retire = block(SCRIPT, "function retire(node){", "function relayout")
        self.assertIn("if(carried.has(node.dataset.key)) park.appendChild(node);", retire)
        self.assertIn("else unmountCard(node);", retire)
        self.assertIn('id="park"', PAGE)
        # And the park is emptied once nothing carries them any more.
        end = block(SCRIPT, "function endDrag(committed){", "// --- Auto-scroll")
        self.assertIn("while(park.firstChild) unmountCard(park.firstChild);", end)

    def test_the_position_number_is_projected_not_stored(self):
        """A number written at build time is right once and wrong after a drag.

        render() writes the index of every mounted card each time the grid is
        projected -- ITEMS *is* the order, so there is no second copy to drift.
        """
        card = block(SCRIPT, "function makeCard(it){", "function packStarts")
        self.assertIn("hdr.appendChild(el('span','pos',''));", card)
        self.assertNotIn("el('span','pos', n", SCRIPT)   # never filled at build time
        render = block(SCRIPT, "function render(){", "function retire")
        self.assertIn("if(pos.textContent !== String(displayPos[k])) pos.textContent = displayPos[k];", render)

    def test_the_logo_IS_numbered_because_it_takes_a_real_slot(self):
        """It leads every set it is added to, so it costs one of the 200.

        build_collection reserves it -- `capacity = per_set - 1` -- so leaving
        it out of the panel's numbering made the panel disagree with what ships:
        the owner read "200" and the pack was 201.
        """
        hdr = block(SCRIPT, "const hdr = el('div','hdr');", "card.appendChild(hdr);")
        self.assertIn("hdr.appendChild(el('span','pos',''));", hdr)
        # The tick is the one thing the logo does NOT get: it is not toggleable.
        self.assertIn("if(!it.isLogo) hdr.appendChild(el('span','tick'", hdr)
        render = block(SCRIPT, "function render(){", "function retire")
        self.assertNotIn("isLogo", render, "skipping the logo is what made the count wrong")

    def test_a_selection_that_cannot_be_one_pack_says_so(self):
        """200 chosen emoji plus the logo is 201, over Telegram's per-set cap.

        Surfaced in the header rather than discovered as a surprise second set.

        The count is the number of runs ``packStarts()`` produces -- the same
        function that draws the boundaries -- and NOT a division of the total.
        ``Math.ceil(included / PER_SET)`` counted the logo once for the whole
        selection when every pack is led by one, so 399 emoji plus a logo was
        reported as two packs over a grid already drawing three. That the two
        agree at every boundary is asserted in ``tests/test_panel_browser.py``.
        """
        count = block(SCRIPT, "function updateCount(){", chr(10) + "}")
        self.assertIn("packStarts().starts.length", count)
        self.assertNotIn("Math.ceil(included / PER_SET)", SCRIPT,
                         "a second, wrong copy of the pack planner")
        self.assertIn("PER_SET - (logo ? 1 : 0)", count, "every pack is led by the logo")
        # The limit comes from build_collection, not a second copy that drifts.
        self.assertIn("__PER_SET__", PAGE)
        self.assertEqual(p.PER_SET, 200)

    def test_the_card_header_cannot_overlap_itself(self):
        """The badge, the number and the tick shared one ~120px strip.

        Absolutely positioned at left / centre / right, "animated" ran straight
        under the number and both were unreadable. They are now a centred
        COLUMN -- number over format -- with only the tick pinned to the corner,
        which is what keeps it from pushing the stack off centre.
        """
        rule = block(PAGE, ".hdr{", "}")
        self.assertIn("display:flex", rule)
        self.assertIn("flex-direction:column", rule)
        self.assertIn("align-items:center", rule)
        # In the column flow, so they cannot be placed on top of each other.
        for cls in (".badge{", ".pos{"):
            self.assertNotIn("position:absolute", block(PAGE, cls, "}"), cls)
        # The tick is the one exception, and deliberately so.
        self.assertIn("position:absolute", block(PAGE, ".tick{", "}"))

    def test_the_scroll_loop_cannot_outlive_the_drag(self):
        """A drag released outside the window fires no drop.

        An unguarded rAF loop would then scroll the page forever.
        """
        step = SCRIPT[SCRIPT.index("function stepEdge("):]
        self.assertIn("if(dragKey === null || !edgeSpeed) return;", step[:step.index("}") + 400])
        self.assertIn("function stopEdgeScroll(", SCRIPT)
        self.assertIn("cancelAnimationFrame", SCRIPT)


class UndoRedoAndFormatColours(unittest.TestCase):
    """The header controls, the history stack, and telling formats apart."""

    def test_every_mutation_records_the_state_to_return_to(self):
        """remember() must hold the state from BEFORE the change, at all three
        sites. A drag mutates from its first dragover, so it records the
        snapshot taken at dragstart rather than one taken at the drop.

        A missed site is invisible until someone undoes past it and gets the
        wrong state back, which is worse than having no undo at all.
        """
        for label, marker, end, mutation, call in (
            ("select all / invert", "function setAll(fn){", "// Reduced motion", "setIncluded(it, fn(it))", "remember()"),
            ("card toggle", "grid.addEventListener('click'", "// Plain document scrolling", "setIncluded(ITEMS[i], !ITEMS[i].included)", "remember()"),
            ("drag reorder", "function commitDrag(){", "function endDrag", "saveOrder();", "remember(dragSnap)"),
        ):
            body = block(SCRIPT, marker, end)
            self.assertLess(body.index(call), body.index(mutation), f"{label} does not record history first")

    def test_the_history_is_bounded(self):
        """A long curation session must not grow the stack without limit."""
        self.assertIn("HISTORY_MAX", SCRIPT)
        body = SCRIPT[SCRIPT.index("function remember(snap){"):]
        self.assertIn("past.shift()", body[:body.index("}") + 200])

    def test_a_new_action_drops_the_redo_branch(self):
        body = SCRIPT[SCRIPT.index("function remember(snap){"):]
        self.assertIn("future.length = 0", body[:body.index("updateHistoryButtons")])

    def test_undo_re_projects_the_grid_instead_of_rebuilding_it(self):
        """A rebuild would re-request every thumbnail and preview on screen.

        relayout() reconciles the mounted nodes: a node already in the document
        RELOCATES on insertBefore, so the loaded media survives an undo.
        """
        body = block(SCRIPT, "function applySnapshot(", "function undo()")
        self.assertIn("relayout();", body)
        self.assertIn("saveOrder()", body, "order auto-saves, so an undone reorder must reach the catalog")
        self.assertNotIn("cards.clear()", SCRIPT, "nothing may throw the mounted cards away wholesale")
        render = block(SCRIPT, "function render(){", "function retire")
        self.assertIn("if(n === cur){ cur = cur.nextSibling; continue; }", render)
        self.assertIn("grid.insertBefore(n, cur);", render)

    def test_each_format_has_its_own_accent(self):
        colours = {}
        for fmt in ("static", "animated", "video"):
            rule = PAGE[PAGE.index(f".card.fmt-{fmt}"):]
            colours[fmt] = rule[rule.index("--fmt:") + 6:rule.index(";")]
        self.assertEqual(len(set(colours.values())), 3, colours)
        # The drag affordance must not be any format's colour, or the card
        # being carried reads as "this one is animated".
        drag = block(PAGE, ".card.drag{", "}")
        for fmt, c in colours.items():
            self.assertNotIn(c, drag, f"the dragged card uses the {fmt} colour")

    def test_two_frames_answer_two_questions(self):
        """Inner frame = the format, outer frame = whether it is selected.

        One frame carrying both is what the owner rejected: a per-format card
        border striped the whole dark grid.

        The format frame is an OUTLINE, not a border. A border is drawn inside
        the box, so it ate two pixels off every thumbnail and sat flush against
        the artwork; an outline is painted outside and resizes nothing.
        """
        rule = block(PAGE, ".thumb{", "}")
        self.assertIn("outline:.17em solid var(--fmt", rule)
        self.assertIn("outline-offset:", rule, "without an offset the frame still touches the artwork")
        self.assertNotIn("border:", rule.replace("border-radius", ""),
                         "an inner border shrinks the thumbnail it frames")
        self.assertIn("#22c55e", block(PAGE, ".card.on{", "}"), "the selected frame must be green")

    def test_the_tick_offers_a_click_not_a_grab(self):
        """The card is cursor:grab because it is the drag handle, and cursor
        inherits -- so the switch you are aiming at offered a hand for a drag."""
        self.assertIn("cursor:pointer", block(PAGE, ".tick{", "}"))

    def test_scrolling_holds_every_card_on_frame_zero(self):
        """The one moment the decoding is pure waste: the frames go past too
        fast to read while the compositor is already busy."""
        self.assertIn("function freezeAll(){", SCRIPT)
        scroll = block(SCRIPT, "addEventListener('scroll'", "}, {passive:true});")
        self.assertIn("freezeAll()", scroll)
        self.assertIn("applyAnim()", scroll, "it must thaw again when you stop")
        self.assertIn("scheduleRender()", scroll, "the window follows the scroll")
        self.assertIn("passive", SCRIPT[SCRIPT.index("addEventListener('scroll'"):][:600],
                      "a non-passive scroll listener blocks the scroll it watches")
        # The playback observer has NO margin: a band beyond the viewport
        # animated a row nobody was looking at, above AND below.
        self.assertIn("}, {root: null, rootMargin: '0px'})", block(SCRIPT, "const animIO", "function setPlaying"))

    def test_a_switch_shows_its_own_state(self):
        """On/off shown by the control itself, not only by its label.

        The rule is bound to `.switch`, not to one id: selection mode is a
        second switch and an id-bound rule would have left its knob dead.
        """
        self.assertIn('aria-pressed', PAGE)
        self.assertIn('.switch[aria-pressed="true"]  .knob{background:#22c55e}', PAGE)
        self.assertIn('.switch[aria-pressed="false"] .knob{background:#f43f5e}', PAGE)
        for control in ('id="anim" class="switch"', 'id="animAll" class="switch"',
                        'id="selmode" class="switch"'):
            self.assertIn(control, PAGE)
        self.assertIn("--btn:#34ebc6", PAGE)

    def test_the_card_header_stacks_number_over_format(self):
        rule = block(PAGE, ".hdr{", "}")
        self.assertIn("flex-direction:column", rule)
        self.assertIn("align-items:center", rule)
        # The number is appended before the format badge, so it sits on top.
        markup = block(SCRIPT, "const hdr = el('div','hdr');", "card.appendChild(hdr);")
        self.assertLess(markup.index("'pos'"), markup.index("'badge'"))

    def test_only_save_selection_sits_outside_the_centre_group(self):
        actions = block(PAGE, '<div class="actions">', "</div>")
        for btn in ("undo", "redo", "top", "bot", "zoomOut", "zoomReset", "zoomIn",
                    "all", "none", "inv", "bg", "anim", "animAll", "selmode"):
            self.assertIn(f'id="{btn}"', actions, btn)
        self.assertNotIn('id="save"', actions, "Save writes; it stays out of the centre group")
        # The expanding toolbar must wrap rather than cover the title; the real
        # rendered rectangles are checked in test_panel_curation.
        self.assertIn("grid-template-columns:auto minmax(0,1fr) auto", PAGE)

class SelectionModeCarriesARun(unittest.TestCase):
    """Picking several emoji and moving them in one gesture.

    Arranging a 200-card pack one card at a time is the slow part. The risk it
    introduces is the reason these are pinned: a selection that silently changes
    what SHIPS, or a cancelled group drag that leaves half the run somewhere new.
    """

    def test_the_pick_box_is_not_the_tick(self):
        """They answer different questions -- "does this ship" and "does this
        move with the others". One control for both would let arranging drop an
        emoji from the pack by accident."""
        self.assertIn("el('span','pick'", SCRIPT)
        self.assertIn("el('span','tick'", SCRIPT)
        # Invisible until the mode is on, so the card is unchanged until asked.
        self.assertIn("display:none", block(PAGE, ".pick{", "}"))
        self.assertIn("body.selmode .pick{display:flex}", PAGE)

    def test_only_a_picked_box_draws_a_check(self):
        """It used to be written into the element, so both states rendered the
        same glyph and only the colour changed: an unselected box looked
        checked, and two states were told apart by colour alone. The glyph is
        the signal now, and it comes from one place."""
        for built in ("el('span','pick')", "el('span','pick');"):
            if built in SCRIPT:
                break
        else:
            self.fail("the pick box must be built with no glyph of its own")
        self.assertNotIn("el('span','pick','✓')", SCRIPT)
        self.assertNotIn("el('span','pick', '✓')", SCRIPT)
        rule = ".card.picked .pick::after,.hcard.picked .pick::after"
        self.assertIn(rule, PAGE, "the check must be drawn for the picked state")
        self.assertIn("✓", block(PAGE, rule + "{", "}"))

    def test_the_empty_box_is_still_visible_over_artwork(self):
        """An empty box with no fill would vanish over a light thumbnail, and
        the state would then be unreadable in the other direction."""
        base = block(PAGE, ".pick{", "}")
        self.assertIn("border:", base)
        self.assertIn("background:#0b1422", base)

    def test_leaving_the_mode_drops_the_picks(self):
        """A hidden selection that still moves cards on the next drag is worse
        than no selection at all."""
        self.assertIn("if(!selMode) clearPicked();", SCRIPT)

    def test_arranging_never_changes_what_ships(self):
        """The card click toggles `included`. In selection mode that click now
        PICKS instead -- it used to do nothing at all, which left the 1.7em box
        as the only way to select on a 140px card -- and either way a click
        while arranging must never quietly drop an emoji from the pack."""
        body = block(SCRIPT, "grid.addEventListener('click'", "// Plain document scrolling")
        self.assertIn("if(selMode){", body)
        self.assertIn("pickCardAt(i, e.shiftKey);", body)
        self.assertLess(body.index("if(selMode){"), body.index("remember();"),
                        "the branch has to come before anything mutates inclusion")

    def test_the_box_owns_its_own_click(self):
        """Its `pointerdown` has already toggled. Acting on the click as well
        would toggle twice and net to zero -- the same double-toggle that made
        the tray's fix a replacement rather than an addition."""
        body = block(SCRIPT, "grid.addEventListener('click'", "// Plain document scrolling")
        self.assertIn("if(!e.target.closest('.pick')) pickCardAt", body)

    def test_a_picked_card_is_marked_by_a_moving_ring_on_both_surfaces(self):
        """A flat inset outline among four format accents is what the owner
        could not find. The ring is the same control in the grid and the tray:
        two markers for one state would be a worse outcome than the bug."""
        rule = ".card.picked::before,.hcard.picked::before"
        self.assertIn(rule, PAGE)
        ring = block(PAGE, rule + "{", "}")
        self.assertIn("conic-gradient", ring)
        self.assertIn("animation:pickspin", ring)
        self.assertIn("@keyframes pickspin{to{transform:rotate(1turn)}}", PAGE,
                      "the rotation must be a transform: the compositor animates "
                      "that without repainting, and this grid is virtual because "
                      "repainting cost 14-17 ms per pointer move")
        self.assertNotIn(".card.picked{outline:2px solid #f0abfc;outline-offset:-2px}", PAGE,
                         "the flat outline it replaces must be gone, not doubled")

    def test_reduced_motion_keeps_the_ring_and_stops_it_moving(self):
        """The preference is about movement, not about the marker."""
        reduced = block(PAGE, "@media (prefers-reduced-motion:reduce){", "}}")
        self.assertIn(".card.picked::before,.hcard.picked::before{animation:none", reduced)

    def test_the_ring_pauses_while_the_page_moves(self):
        self.assertIn("body.moving .card.picked::before,body.moving .hcard.picked::before"
                      "{animation-play-state:paused}", PAGE)
        self.assertIn("markMoving();", block(SCRIPT, "addEventListener('scroll'", "{passive:true});"))

    def test_dragging_a_picked_card_carries_the_whole_set_even_off_screen(self):
        """The carried set is read from ITEMS, not from the DOM: a picked card
        whose row is not mounted must still travel, or the run splits in two."""
        body = block(SCRIPT, "grid.addEventListener('dragstart'", "grid.addEventListener('dragover'")
        self.assertIn("picked.has(dragKey)", body)
        self.assertIn("ITEMS.filter(x=>picked.has(x.key) && !x.isLogo)", body)
        self.assertNotIn("querySelectorAll('.card.picked')", body)
        # An unpicked card is still the single-card gesture that always worked.
        self.assertIn("[dragKey]", body)

    def test_a_pick_survives_the_card_being_unmounted(self):
        """Picks live in a Set keyed by content key; the card only PAINTS it,
        and a card built later for the same key paints it again."""
        self.assertIn("if(node) node.classList.toggle('picked', on);", SCRIPT)
        self.assertIn("c.classList.toggle('picked', picked.has(it.key));",
                      block(SCRIPT, "function mount(k){", "function unmountCard"))

    def test_dragging_back_shrinks_the_run(self):
        """Overshooting has to be correctable inside the same stroke; without a
        baseline the run only ever grows and the mode has to be left to fix it."""
        body = block(SCRIPT, "function applyStroke(", chr(10) + "}")
        self.assertIn("strokeBase.has(", body)
        self.assertIn("if(!now.has(k))", body)

if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_panel_page_actions -v")
