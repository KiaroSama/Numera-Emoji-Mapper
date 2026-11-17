// Faithful served-text contracts; test identity matches the retired Python suite.
import { test } from "@playwright/test";
import assert from "node:assert/strict";
import { join } from "node:path";
import { statSync, realpathSync } from "node:fs";
import { PAGE, SCRIPT, ROOT, ASSET_DIR, PER_SET, block, index } from "./source-fixture.js";
const p = { PER_SET, ASSET_DIR };

test("test_panel_page_actions.DragAndDropOrdering.test_the_drop_keeps_the_model_instead_of_recomputing_an_index", () => {
  let drop = block(SCRIPT, "grid.addEventListener('drop'", "grid.addEventListener('dragend'");
  assert.ok(drop.includes("commitDrag()"));
  for (const arithmetic of ["findIndex", "splice", "ITEMS.length"]) {
    assert.ok(!drop.includes(arithmetic), `the drop handler must not reach for ${arithmetic}`);
  }
  let commit = block(SCRIPT, "function commitDrag(){", "function endDrag");
  assert.ok(commit.includes("order.every((k,i)=>k===dragSnap.order[i])"));
  assert.ok(commit.includes("selSig(ITEMS.filter(x=>x.included).map(x=>x.key))===selSig(dragSnap.included)"), "a tray release changes inclusion even when its position stays the same");
  assert.ok(commit.includes("packs===JSON.stringify(dragSnap.packs)) return;"), "and a cross-pack drop changes the stamp even when the order holds");
  assert.ok(index(commit, "remember(dragSnap);") < index(commit, "saveOrder();"));
});

test("test_panel_page_actions.DragAndDropOrdering.test_a_cancelled_drag_restores_the_order_taken_at_dragstart", () => {
  let start = block(SCRIPT, "grid.addEventListener('dragstart'", "grid.addEventListener('dragover'");
  assert.ok(start.includes("dragSnap=snapshot();"));
  let end = block(SCRIPT, "function endDrag(committed){", "// --- Auto-scroll");
  assert.ok(end.includes("if(!committed && dragKey !== null){ applySnapshot(dragSnap,false); }"));
});

test("test_panel_page_actions.DragAndDropOrdering.test_the_pointer_side_decides_before_or_after", () => {
  let over = block(SCRIPT, "grid.addEventListener('dragover'", "function moveCarried");
  assert.ok(over.includes("getBoundingClientRect"));
  assert.ok(over.includes("(e.clientX > r.left + r.width/2) ? j + 1 : j"));
});

test("test_panel_page_actions.DragAndDropOrdering.test_hovering_the_slot_the_run_already_holds_changes_nothing", () => {
  let move = block(SCRIPT, "function moveCarried(slot){", "grid.addEventListener('drop'");
  assert.ok(move.includes("if(next.every((x,i)=>x===ITEMS[i])) return;"));
  assert.ok(index(move, "return;") < index(move, "relayout();"));
});

test("test_panel_page_actions.DragAndDropOrdering.test_dragging_to_an_edge_scrolls_the_page", () => {
  assert.ok(SCRIPT.includes("function edgeScroll("));
  assert.ok(SCRIPT.includes("requestAnimationFrame"));
  let doc_over = index(SCRIPT, "document.addEventListener('dragover'");
  assert.ok(SCRIPT.slice(doc_over, (doc_over + 300)).includes("edgeScroll(e.clientY)"));
});

test("test_panel_page_actions.DragAndDropOrdering.test_a_carried_card_is_parked_never_unmounted", () => {
  let retire = block(SCRIPT, "function retire(node){", "function relayout");
  assert.ok(retire.includes("if(carried.has(node.dataset.key)) park.appendChild(node);"));
  assert.ok(retire.includes("else unmountCard(node);"));
  assert.ok(PAGE.includes("id=\"park\""));
  let end = block(SCRIPT, "function endDrag(committed){", "// --- Auto-scroll");
  assert.ok(end.includes("while(park.firstChild) unmountCard(park.firstChild);"));
});

test("test_panel_page_actions.DragAndDropOrdering.test_the_position_number_is_projected_not_stored", () => {
  let card = block(SCRIPT, "function makeCard(it){", "function packStarts");
  assert.ok(card.includes("hdr.appendChild(el('span','pos',''));"));
  assert.ok(!SCRIPT.includes("el('span','pos', n"));
  let render = block(SCRIPT, "function render(){", "function retire");
  assert.ok(render.includes("if(pos.textContent !== String(displayPos[k])) pos.textContent = displayPos[k];"));
});

test("test_panel_page_actions.DragAndDropOrdering.test_the_logo_IS_numbered_because_it_takes_a_real_slot", () => {
  let hdr = block(SCRIPT, "const hdr = el('div','hdr');", "card.appendChild(hdr);");
  assert.ok(hdr.includes("hdr.appendChild(el('span','pos',''));"));
  assert.ok(hdr.includes("if(!it.isLogo) hdr.appendChild(el('span','tick'"));
  let render = block(SCRIPT, "function render(){", "function retire");
  assert.ok(!render.includes("isLogo"), "skipping the logo is what made the count wrong");
});

test("test_panel_page_actions.DragAndDropOrdering.test_a_selection_that_cannot_be_one_pack_says_so", () => {
  let count = block(SCRIPT, "function updateCount(){", (String.fromCharCode(10) + "}"));
  assert.ok(count.includes("packStarts().starts.length"));
  assert.ok(!SCRIPT.includes("Math.ceil(included / PER_SET)"), "a second, wrong copy of the pack planner");
  assert.ok(count.includes("PER_SET - (logo ? 1 : 0)"), "every pack is led by the logo");
  assert.ok(PAGE.includes("__PER_SET__"));
  assert.deepEqual(p.PER_SET, 200);
});

test("test_panel_page_actions.DragAndDropOrdering.test_the_card_header_cannot_overlap_itself", () => {
  let rule = block(PAGE, ".hdr{", "}");
  assert.ok(rule.includes("display:flex"));
  assert.ok(rule.includes("flex-direction:column"));
  assert.ok(rule.includes("align-items:center"));
  for (const cls of [".badge{", ".pos{"]) {
    assert.ok(!block(PAGE, cls, "}").includes("position:absolute"), cls);
  }
  assert.ok(block(PAGE, ".tick{", "}").includes("position:absolute"));
});

test("test_panel_page_actions.DragAndDropOrdering.test_the_scroll_loop_cannot_outlive_the_drag", () => {
  let step = SCRIPT.slice(index(SCRIPT, "function stepEdge("));
  assert.ok(step.slice(0, (index(step, "}") + 400)).includes("if(dragKey === null || !edgeSpeed) return;"));
  assert.ok(SCRIPT.includes("function stopEdgeScroll("));
  assert.ok(SCRIPT.includes("cancelAnimationFrame"));
});

test("test_panel_page_actions.UndoRedoAndFormatColours.test_every_mutation_records_the_state_to_return_to", () => {
  for (const [label, marker, end, mutation, call] of [["select all / invert", "function setAll(fn){", "// Reduced motion", "setIncludedMany(", "remember()"], ["card toggle", "grid.addEventListener('click'", "// Plain document scrolling", "setIncluded(ITEMS[i], !ITEMS[i].included)", "remember()"], ["drag reorder", "function commitDrag(){", "function endDrag", "saveOrder();", "remember(dragSnap)"]]) {
    let body = block(SCRIPT, marker, end);
    assert.ok(index(body, call) < index(body, mutation), `${label} does not record history first`);
  }
});

test("test_panel_page_actions.UndoRedoAndFormatColours.test_the_history_is_bounded", () => {
  assert.ok(SCRIPT.includes("HISTORY_MAX"));
  let body = SCRIPT.slice(index(SCRIPT, "function remember(snap){"));
  assert.ok(body.slice(0, (index(body, "}") + 200)).includes("past.shift()"));
});

test("test_panel_page_actions.UndoRedoAndFormatColours.test_a_new_action_drops_the_redo_branch", () => {
  let body = SCRIPT.slice(index(SCRIPT, "function remember(snap){"));
  assert.ok(body.slice(0, index(body, "updateHistoryButtons")).includes("future.length = 0"));
});

test("test_panel_page_actions.UndoRedoAndFormatColours.test_undo_re_projects_the_grid_instead_of_rebuilding_it", () => {
  let body = block(SCRIPT, "function applySnapshot(", "function undo()");
  assert.ok(body.includes("relayout();"));
  assert.ok(body.includes("saveOrder()"), "order auto-saves, so an undone reorder must reach the catalog");
  assert.ok(!SCRIPT.includes("cards.clear()"), "nothing may throw the mounted cards away wholesale");
  let render = block(SCRIPT, "function render(){", "function retire");
  assert.ok(render.includes("if(n === cur){ cur = cur.nextSibling; continue; }"));
  assert.ok(render.includes("grid.insertBefore(n, cur);"));
});

test("test_panel_page_actions.UndoRedoAndFormatColours.test_each_format_has_its_own_accent", () => {
  let colours = {};
  for (const fmt of ["static", "animated", "video"]) {
    let rule = PAGE.slice(index(PAGE, `.card.fmt-${fmt}`));
    colours[fmt] = rule.slice((index(rule, "--fmt:") + 6), index(rule, ";"));
  }
  assert.deepEqual((new Set(Object.values(colours))).size, 3, colours);
  let drag = block(PAGE, ".card.drag{", "}");
  for (const [fmt, c] of Object.entries(colours)) {
    assert.ok(!drag.includes(c), `the dragged card uses the ${fmt} colour`);
  }
});

test("test_panel_page_actions.UndoRedoAndFormatColours.test_two_frames_answer_two_questions", () => {
  let rule = block(PAGE, ".thumb{", "}");
  assert.ok(rule.includes("outline:.17em solid var(--fmt"));
  assert.ok(rule.includes("outline-offset:"), "without an offset the frame still touches the artwork");
  assert.ok(!rule.replaceAll("border-radius", "").includes("border:"), "an inner border shrinks the thumbnail it frames");
  assert.ok(block(PAGE, ".card.on{", "}").includes("#22c55e"), "the selected frame must be green");
});

test("test_panel_page_actions.UndoRedoAndFormatColours.test_the_tick_offers_a_click_not_a_grab", () => {
  assert.ok(block(PAGE, ".tick{", "}").includes("cursor:pointer"));
});

test("test_panel_page_actions.UndoRedoAndFormatColours.test_scrolling_holds_every_card_on_frame_zero", () => {
  assert.ok(SCRIPT.includes("function freezeAll(){"));
  let scroll = block(SCRIPT, "addEventListener('scroll'", "}, {passive:true});");
  assert.ok(scroll.includes("freezeAll()"));
  assert.ok(scroll.includes("applyAnim()"), "it must thaw again when you stop");
  assert.ok(scroll.includes("scheduleRender()"), "the window follows the scroll");
  assert.ok(SCRIPT.slice(index(SCRIPT, "addEventListener('scroll'")).slice(0, 600).includes("passive"), "a non-passive scroll listener blocks the scroll it watches");
  assert.ok(block(SCRIPT, "const animIO", "function setPlaying").includes("}, {root: null, rootMargin: '0px'})"));
});

test("test_panel_page_actions.UndoRedoAndFormatColours.test_a_switch_shows_its_own_state", () => {
  assert.ok(PAGE.includes("aria-pressed"));
  assert.ok(PAGE.includes(".switch[aria-pressed=\"true\"]  .knob{background:#22c55e}"));
  assert.ok(PAGE.includes(".switch[aria-pressed=\"false\"] .knob{background:#f43f5e}"));
  for (const control of ["id=\"anim\" class=\"switch\"", "id=\"animAll\" class=\"switch\"", "id=\"selmode\" class=\"switch\""]) {
    assert.ok(PAGE.includes(control));
  }
  assert.ok(PAGE.includes("--btn:#34ebc6"));
});

test("test_panel_page_actions.UndoRedoAndFormatColours.test_the_card_header_stacks_number_over_format", () => {
  let rule = block(PAGE, ".hdr{", "}");
  assert.ok(rule.includes("flex-direction:column"));
  assert.ok(rule.includes("align-items:center"));
  let markup = block(SCRIPT, "const hdr = el('div','hdr');", "card.appendChild(hdr);");
  assert.ok(index(markup, "'pos'") < index(markup, "'badge'"));
});

test("test_panel_page_actions.UndoRedoAndFormatColours.test_only_save_selection_sits_outside_the_centre_group", () => {
  let actions = block(PAGE, "<div class=\"actions\">", "</div>");
  for (const btn of ["undo", "redo", "top", "bot", "zoomOut", "zoomReset", "zoomIn", "all", "none", "inv", "bg", "anim", "animAll", "selmode"]) {
    assert.ok(actions.includes(`id="${btn}"`), btn);
  }
  assert.ok(!actions.includes("id=\"save\""), "Save writes; it stays out of the centre group");
  assert.ok(PAGE.includes("grid-template-columns:auto minmax(0,1fr) auto"));
});

test("test_panel_page_actions.SelectionModeCarriesARun.test_the_pick_box_is_not_the_tick", () => {
  assert.ok(SCRIPT.includes("el('span','pick'"));
  assert.ok(SCRIPT.includes("el('span','tick'"));
  assert.ok(block(PAGE, ".pick{", "}").includes("display:none"));
  assert.ok(PAGE.includes("body.selmode .pick{display:flex}"));
});

test("test_panel_page_actions.SelectionModeCarriesARun.test_only_a_picked_box_draws_a_check", () => {
  assert.ok(["el('span','pick')", "el('span','pick');"].some(built => SCRIPT.includes(built)), "the pick box must be built with no glyph of its own");
  assert.ok(!SCRIPT.includes("el('span','pick','✓')"));
  assert.ok(!SCRIPT.includes("el('span','pick', '✓')"));
  let rule = ".card.picked .pick::after,.hcard.picked .pick::after";
  assert.ok(PAGE.includes(rule), "the check must be drawn for the picked state");
  assert.ok(block(PAGE, (rule + "{"), "}").includes("✓"));
});

test("test_panel_page_actions.SelectionModeCarriesARun.test_the_empty_box_is_still_visible_over_artwork", () => {
  let base = block(PAGE, ".pick{", "}");
  assert.ok(base.includes("border:"));
  assert.ok(base.includes("background:#0b1422"));
});

test("test_panel_page_actions.SelectionModeCarriesARun.test_leaving_the_mode_drops_the_picks", () => {
  assert.ok(SCRIPT.includes("if(!selMode) clearPicked();"));
});

test("test_panel_page_actions.SelectionModeCarriesARun.test_arranging_never_changes_what_ships", () => {
  let body = block(SCRIPT, "grid.addEventListener('click'", "// Plain document scrolling");
  assert.ok(body.includes("if(selMode){"));
  assert.ok(body.includes("pickCardAt(i, e.shiftKey);"));
  assert.ok(index(body, "if(selMode){") < index(body, "remember();"), "the branch has to come before anything mutates inclusion");
});

test("test_panel_page_actions.SelectionModeCarriesARun.test_the_box_owns_its_own_click", () => {
  let body = block(SCRIPT, "grid.addEventListener('click'", "// Plain document scrolling");
  assert.ok(body.includes("if(!e.target.closest('.pick')) pickCardAt"));
});

test("test_panel_page_actions.SelectionModeCarriesARun.test_a_picked_card_is_marked_by_a_moving_ring_on_both_surfaces", () => {
  let rule = ".card.picked::before,.hcard.picked::before";
  assert.ok(PAGE.includes(rule));
  let ring = block(PAGE, (rule + "{"), "}");
  assert.ok(ring.includes("conic-gradient"));
  assert.ok(ring.includes("animation:pickspin"));
  assert.ok(PAGE.includes("@keyframes pickspin{to{transform:rotate(1turn)}}"), "the rotation must be a transform: the compositor animates that without repainting, and this grid is virtual because repainting cost 14-17 ms per pointer move");
  assert.ok(!PAGE.includes(".card.picked{outline:2px solid #f0abfc;outline-offset:-2px}"), "the flat outline it replaces must be gone, not doubled");
});

test("test_panel_page_actions.SelectionModeCarriesARun.test_reduced_motion_keeps_the_ring_and_stops_it_moving", () => {
  let reduced = block(PAGE, "@media (prefers-reduced-motion:reduce){", "}}");
  assert.ok(reduced.includes(".card.picked::before,.hcard.picked::before{animation:none"));
});

test("test_panel_page_actions.SelectionModeCarriesARun.test_the_ring_pauses_while_the_page_moves", () => {
  assert.ok(PAGE.includes("body.moving .card.picked::before,body.moving .hcard.picked::before{animation-play-state:paused}"));
  assert.ok(block(SCRIPT, "addEventListener('scroll'", "{passive:true});").includes("markMoving();"));
});

test("test_panel_page_actions.SelectionModeCarriesARun.test_dragging_a_picked_card_carries_the_whole_set_even_off_screen", () => {
  let body = block(SCRIPT, "grid.addEventListener('dragstart'", "grid.addEventListener('dragover'");
  assert.ok(body.includes("picked.has(dragKey)"));
  assert.ok(body.includes("ITEMS.filter(x=>picked.has(x.key) && !x.isLogo)"));
  assert.ok(!body.includes("querySelectorAll('.card.picked')"));
  assert.ok(body.includes("[dragKey]"));
});

test("test_panel_page_actions.SelectionModeCarriesARun.test_a_pick_survives_the_card_being_unmounted", () => {
  assert.ok(SCRIPT.includes("if(node) node.classList.toggle('picked', on);"));
  assert.ok(block(SCRIPT, "function mount(k){", "function unmountCard").includes("c.classList.toggle('picked', picked.has(it.key));"));
});

test("test_panel_page_actions.SelectionModeCarriesARun.test_dragging_back_shrinks_the_run", () => {
  let body = block(SCRIPT, "function applyStroke(", (String.fromCharCode(10) + "}"));
  assert.ok(body.includes("strokeBase.has("));
  assert.ok(body.includes("if(!now.has(k))"));
});
