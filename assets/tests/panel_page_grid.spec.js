// Faithful served-text contracts; test identity matches the retired Python suite.
import { test } from "@playwright/test";
import assert from "node:assert/strict";
import { join } from "node:path";
import { statSync, realpathSync } from "node:fs";
import { PAGE, SCRIPT, ROOT, ASSET_DIR, PER_SET, block, index } from "./source-fixture.js";
const p = { PER_SET, ASSET_DIR };

test("test_panel_page_grid.OffScreenCostsNothing.test_leaving_the_viewport_stops_video_AND_animation", () => {
  let io_block = block(SCRIPT, "const animIO", "}, {root:");
  assert.ok(io_block.includes("e.isIntersecting"));
  assert.ok(io_block.includes("setPlaying(t, live)"), "video must be paused when it scrolls away, not muted");
  assert.ok(io_block.includes("inView.delete(t)"));
  assert.ok(io_block.includes("showStill(t)"), "an animated image must fall back to its single frame");
  assert.ok(io_block.includes("inView.add(t)"));
  let allocate = block(SCRIPT, "function allocate(){", (String.fromCharCode(10) + "}"));
  assert.ok(allocate.includes("showMotion("));
  assert.ok(allocate.includes("ANIM_BUDGET"), "the animation budget must bound what plays");
});

test("test_panel_page_grid.OffScreenCostsNothing.test_a_mounted_card_is_observed_and_an_unmounted_one_released", () => {
  let mount = block(SCRIPT, "function mount(k){", "function unmountCard");
  assert.ok(mount.includes("animIO.observe(n)"));
  assert.ok(mount.includes("videoIO.observe(n)"));
  let unmount = block(SCRIPT, "function unmountCard(c){", "const videoIO");
  assert.ok(unmount.includes("animIO.unobserve(n)"));
  assert.ok(unmount.includes("forgetNode(n)"), "an unmounted card must leave the budget's sets");
  assert.ok(unmount.includes("videoIO.unobserve(n)"));
  assert.ok(unmount.includes("cards.delete(c.dataset.key);"));
});

test("test_panel_page_grid.OffScreenCostsNothing.test_a_video_holds_a_player_only_near_the_viewport", () => {
  let preview = block(SCRIPT, "if(it.fmt === 'video' && !VIDEO_REAL){", "} else if(it.fmt === 'video'){");
  assert.ok(preview.includes("previewSources("), "by default a video card is a preview image");
  let video = block(SCRIPT, "} else if(it.fmt === 'video'){", "} else if(it.fmt === 'animated'){");
  assert.ok(video.includes("v.dataset.src = src"));
  assert.ok(!video.includes("v.src ="), "a mounted card must not create a player by itself");
  let attach = block(SCRIPT, "function attachVideo(v, on){", "function setPlaying");
  assert.ok(attach.includes("v.src = v.dataset.src"));
  assert.ok(attach.includes("v.removeAttribute('src'); v.load();"), "removing the attribute alone leaves the player alive");
  assert.ok(block(SCRIPT, "const videoIO", "function attachVideo").includes("rootMargin: '0px'"));
  assert.ok(video.includes("v.poster="), "a still poster is visible before a decoder is attached");
  assert.ok(block(SCRIPT, "function setPlaying(v, on){", "}").includes("if (on) attachVideo(v, true);"));
});

test("test_panel_page_grid.OffScreenCostsNothing.test_the_sticky_header_does_not_blur_its_backdrop", () => {
  let rule = block(PAGE, "header{", "}").replace(new RegExp("/\\*.*?\\*/", "gs"), "");
  assert.ok(!rule.includes("backdrop-filter"));
});

test("test_panel_page_grid.TheGridIsVirtual.test_only_rows_near_the_viewport_are_in_the_document", () => {
  let render = block(SCRIPT, "function render(){", "function retire");
  assert.ok(render.includes("const pad = Math.min(innerHeight / 2, BUFFER_ROWS * (G.cardH + G.gap));"));
  assert.ok(SCRIPT.includes("const BUFFER_ROWS = 3;"));
  assert.ok(render.includes("let first = rowAt(viewTop - pad);"));
  assert.ok(render.includes("setSpacer(padTop, first > 0 ? rows[first].top - G.gap : 0);"));
  assert.ok(render.includes("setSpacer(padBot, bottom < total ? total - bottom - G.gap : 0);"));
  for (const spacer of ["id=\"padTop\"", "id=\"padBot\""]) {
    assert.ok(PAGE.includes(spacer));
  }
  assert.ok(PAGE.includes(".spacer{grid-column:1/-1"));
});

test("test_panel_page_grid.TheGridIsVirtual.test_the_layout_is_arithmetic_on_whole_pixels", () => {
  let layout = block(SCRIPT, "function layoutRows(){", "function measure");
  assert.ok(layout.includes("Math.round(BASE.gap * zoom)"));
  assert.ok(layout.includes("Math.round((compact ? BASE.cardHCompact : BASE.cardH) * zoom)"));
  assert.ok(layout.includes("setVar('--cardH', G.cardH + 'px');"));
  let card = block(PAGE, ".card{", "}");
  assert.ok(card.includes("height:var(--cardH"));
  assert.ok(card.includes("overflow:hidden"));
  assert.ok(block(PAGE, "\n.lbl{", "}").includes("-webkit-line-clamp:2"), "the label cannot decide the height");
  assert.ok(!PAGE.includes("content-visibility"));
  assert.ok(!PAGE.includes("contain-intrinsic-size"));
});

test("test_panel_page_grid.TheGridIsVirtual.test_a_separator_starts_a_fresh_row_like_css_would", () => {
  let layout = block(SCRIPT, "function layoutRows(){", "function measure");
  assert.ok(layout.includes("if(sepAt.has(i)){"));
  assert.ok(layout.includes("indices.length < G.cols && !sepAt.has(visible[v])"));
});

test("test_panel_page_grid.TheGridIsVirtual.test_scrolling_within_the_same_rows_costs_a_binary_search", () => {
  let render = block(SCRIPT, "function render(){", "function retire");
  assert.ok(render.includes("if(first === win.first && last === win.last) return;"));
  let layout = block(SCRIPT, "function layoutRows(){", "function measure");
  assert.ok(layout.includes("win = {first: -1, last: -1};"));
  assert.ok(block(SCRIPT, "function scheduleRender(){", "}").includes("requestAnimationFrame"));
});

test("test_panel_page_grid.TheGridIsVirtual.test_reconciling_moves_nodes_and_retires_only_the_stale_ones", () => {
  let render = block(SCRIPT, "function render(){", "function retire");
  assert.ok(render.includes("if(n === cur){ cur = cur.nextSibling; continue; }"));
  assert.ok(render.includes("grid.insertBefore(n, cur);"));
  assert.ok(render.includes("retire(stale)"));
});

test("test_panel_page_grid.ZoomFitsMoreOrLess.test_the_header_offers_zoom_in_out_and_reset", () => {
  for (const btn of ["id=\"zoomOut\"", "id=\"zoomReset\"", "id=\"zoomIn\""]) {
    assert.ok(PAGE.includes(btn));
  }
  assert.ok(SCRIPT.includes("document.getElementById('zoomIn').onclick = ()=>setZoom(zoom * ZOOM_STEP);"));
  assert.ok(SCRIPT.includes("document.getElementById('zoomOut').onclick = ()=>setZoom(zoom / ZOOM_STEP);"));
  assert.ok(PAGE.includes("<input id=\"zoomReset\""));
  assert.ok(SCRIPT.includes("if(e.key==='Enter'){e.preventDefault();applyZoomInput();}"));
  assert.ok(SCRIPT.includes("zoomInput.addEventListener('dblclick',()=>setZoom(1));"));
});

test("test_panel_page_grid.ZoomFitsMoreOrLess.test_ctrl_wheel_and_ctrl_keys_are_taken_over_from_the_browser", () => {
  let wheel = block(SCRIPT, "addEventListener('wheel'", "{passive: false});");
  assert.ok(wheel.includes("if(!e.ctrlKey) return;"));
  assert.ok(wheel.includes("e.preventDefault();"), "without it the browser zooms too");
  let keys = block(SCRIPT, "addEventListener('keydown', e=>{", "});");
  for (const key of ["'='", "'-'", "'0'"]) {
    assert.ok(keys.includes(key));
  }
});

test("test_panel_page_grid.ZoomFitsMoreOrLess.test_the_level_is_clamped_and_remembered", () => {
  let z = block(SCRIPT, "function setZoom(z){", "function paintZoom");
  assert.ok(z.includes("Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, z))"));
  assert.ok(z.includes("prefs.set('panelZoom'"));
  assert.ok(block(SCRIPT, "function loadZoom(){", "}").includes("prefs.get('panelZoom'"));
  let raw = (SCRIPT.match(new RegExp("localStorage\\.(?:get|set)Item", "g")) ?? []);
  assert.deepEqual((raw).length, 2, `unguarded localStorage access: ${raw}`);
  assert.ok(SCRIPT.includes("const prefs = {"));
});

test("test_panel_page_grid.ZoomFitsMoreOrLess.test_zooming_keeps_the_top_item_on_screen", () => {
  let z = block(SCRIPT, "function setZoom(z){", "function paintZoom");
  assert.ok(index(z, "const anchor = firstVisibleIndex();") < index(z, "zoom = z;"));
  assert.ok(index(z, "layoutRows();") < index(z, "scrollTo(0, gridTop + rows[rowOfItem[anchor]].top - headerH)"));
});

test("test_panel_page_grid.ZoomFitsMoreOrLess.test_far_out_the_text_rows_are_dropped", () => {
  assert.ok(SCRIPT.includes("const COMPACT_BELOW = 0.75;"));
  assert.ok(SCRIPT.includes("document.body.classList.toggle('compact', compact);"));
  assert.ok(PAGE.includes("body.compact .glyph,body.compact .lbl,body.compact .sub{display:none}"));
});

test("test_panel_page_grid.ZoomFitsMoreOrLess.test_everything_in_a_card_scales_from_one_font_size", () => {
  let card = block(PAGE, ".card{", "}");
  assert.ok(card.includes("font-size:calc(12px*var(--z"));
  assert.ok(block(PAGE, ".thumb{", "}").includes("width:9em;height:9em"));
  assert.ok(block(PAGE, ".thumb img,.thumb video{", "}").includes("width:8.67em;height:8.67em"));
  assert.ok(!PAGE.includes("104px"));
  assert.ok(!SCRIPT.includes("img.width = 104"), "an attribute size would pin the artwork at 100 %");
});

test("test_panel_page_grid.ThePanelPageActuallyShips.test_the_page_is_a_real_file_inside_the_repo", () => {
  let asset = join(p.ASSET_DIR, "panel.html");
  assert.ok(statSync(asset).isFile(), `the panel page is missing: ${asset}`);
  assert.ok(String(realpathSync(asset)).startsWith(String(realpathSync(ROOT))), "the page must ship in the repo, not point at a machine-specific path");
});

test("test_panel_page_grid.ThePanelPageActuallyShips.test_the_loaded_page_is_the_document_the_handler_expects", () => {
  assert.ok(PAGE.startsWith("<!doctype html>"));
  assert.ok(PAGE.trimEnd().endsWith("</html>"));
  for (const token of ["__ITEMS__", "__TOKEN__", "__PREVIEW_TIERS__", "__PER_SET__", "__HIDDEN__", "__STALE__", "__ASSET_VER__", "__ICON_VER__"]) {
    assert.ok(PAGE.includes(token), `${token} lost in the asset`);
  }
});

test("test_panel_page_grid.PackSplitsAndJumpButtons.test_a_separator_is_never_a_card", () => {
  assert.ok(block(PAGE, ".packsep{", "}").includes("grid-column:1/-1"));
  let body = block(SCRIPT, "function sepNode(", (String.fromCharCode(10) + "function "));
  assert.ok(body.includes("el('div','packsep')"));
  assert.ok(!body.includes("'card'"));
  assert.ok(!SCRIPT.includes("packsep card"));
  assert.ok(SCRIPT.includes("e.target.closest('.card')"));
});

test("test_panel_page_grid.PackSplitsAndJumpButtons.test_the_splits_are_counted_from_included_items_only", () => {
  let body = block(SCRIPT, "function packStarts(){", "function layoutRows");
  assert.ok(body.includes("!it.isLogo && it.included"));
  assert.ok(body.includes("PER_SET - (logo ? 1 : 0)"));
});

test("test_panel_page_grid.PackSplitsAndJumpButtons.test_a_candidate_no_longer_splits_the_pack_it_was_dropped_into", () => {
  assert.ok(!SCRIPT.includes("'Not in a pack yet'"));
  let body = block(SCRIPT, "function packStarts(){", "function layoutRows");
  assert.ok(body.includes("pk !== null && (!starts.length || pk !== cur)"));
});

test("test_panel_page_grid.PackSplitsAndJumpButtons.test_the_logo_opens_its_own_packs_run", () => {
  let body = block(SCRIPT, "function packStarts(){", "function layoutRows");
  let i_mem = index(body, "if(byMembership){");
  let i_cap = index(body, "} else if(!it.isLogo && it.included){");
  assert.ok(i_mem < i_cap, "membership mode must not filter logos out");
  assert.ok(!body.slice(i_mem, i_cap).includes("isLogo"), "a logo has to be able to start its pack's run");
});

test("test_panel_page_grid.PackSplitsAndJumpButtons.test_selection_changes_recompute_the_splits", () => {
  assert.ok(SCRIPT.includes("relayout(); updateCount(); markSelDirty(); }"));
  assert.ok(SCRIPT.includes("lastIdx=i; relayout(); updateCount(); markSelDirty();"));
});

test("test_panel_page_grid.PackSplitsAndJumpButtons.test_separators_are_part_of_the_row_model_not_accumulated", () => {
  let layout = block(SCRIPT, "function layoutRows(){", "function measure");
  assert.ok(layout.includes("const sepAt = new Map();"));
  assert.ok(layout.includes("rows.push({sep: sepAt.get(i), at: i, top: y, h: G.sepH});"));
});

test("test_panel_page_grid.PackSplitsAndJumpButtons.test_one_pack_needs_no_divider", () => {
  assert.ok(block(SCRIPT, "function layoutRows(){", "function measure").includes("if(starts.length >= 2){"));
});

test("test_panel_page_grid.PackSplitsAndJumpButtons.test_the_header_offers_top_and_bottom", () => {
  assert.ok(PAGE.includes("id=\"top\""));
  assert.ok(PAGE.includes("id=\"bot\""));
  let jump = SCRIPT.slice(index(SCRIPT, "document.getElementById('top').onclick")).slice(0, 400);
  assert.ok(jump.includes("window.scrollTo({top:0})"));
  assert.ok(jump.includes("document.documentElement.scrollHeight"));
  assert.ok(!jump.includes("scrollIntoView"));
});
