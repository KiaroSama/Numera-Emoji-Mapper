// Real animated-WebP decode, frame probes, screenshot pixels and model equivalence.
// Timings are reported, never asserted; the stable count budgets remain assertions.
import { PNG } from 'pngjs';
import { test, expect, synth, PROBE, REFERENCE } from './browser-fixture.js';
test.use({ catalogSize: 4 });
test.skip(process.env.NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS === '1', 'Explicit browser opt-out');
const id = name => `test_panel_perf.AnimationCost.${name}`;
const report = row => console.log('PANEL_PERF ' + JSON.stringify(row));
const round = n => Math.round(n * 10) / 10;
const pct = (values, q) => round([...values].sort((a, b) => a - b)[Math.min(values.length - 1, Math.floor(q * values.length))]);
async function open(panel) {
  const page = await panel.open(synth(600, { fmt: 'animated' }), { init: PROBE, media: 'animated' });
  page.setDefaultTimeout(15_000);
  return page;
}
async function settle(page) {
  await page.waitForFunction('window.__settled()');
  await page.evaluate('new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))');
}
async function compact(panel) {
  const page = await open(panel);
  await page.evaluate('setZoom(0.4)');
  await settle(page);
  return page;
}
async function scrollScenario(panel, zoom) {
  const page = await open(panel);
  await page.evaluate(`setZoom(${zoom})`);
  await settle(page);
  await page.evaluate('window.__perf.swaps = 0; window.__perf.loaf = []');
  await page.evaluate('window.__keepScrolling(1500)');
  await page.waitForTimeout(1600);
  await settle(page);
  const frames = await page.evaluate('window.__frames(1000)');
  const perf = await page.evaluate('window.__perf');
  const row = { scenario: `scroll-${zoom}`, swaps: perf.swaps,
    playing: await page.evaluate('window.__animating()'),
    visible: await page.evaluate('window.__visibleAnimated()'),
    loaf_count: perf.loaf.length, loaf_blocking_ms: round(perf.loaf.reduce((a, b) => a + b, 0)),
    frame_p50: pct(frames, 0.5), frame_p95: pct(frames, 0.95) };
  report(row);
  expect(frames.length, 'the frame probe did not run').toBeGreaterThanOrEqual(10);
  expect(row.playing, 'nothing animates after settling').toBeGreaterThanOrEqual(1);
  return row;
}

test(id('test_nothing_outside_the_viewport_animates_once_settled'), async ({ panel }) => {
  const page = await open(panel);
  for (const zoom of [1, 0.4]) {
    await page.evaluate(`setZoom(${zoom})`);
    await settle(page);
    const outside = await page.evaluate(`[...document.querySelectorAll('#grid img[data-anim]')]
      .filter(n => n.getAttribute('src') === n.dataset.anim)
      .filter(n => { const r = n.getBoundingClientRect(); return !(r.bottom > headerH && r.top < innerHeight); }).length`);
    expect(outside, `zoom ${zoom}: off-screen cards still animate`).toBe(0);
  }
});

test(id('test_a_scroll_gesture_reports_its_cost_at_compact_zoom'), async ({ panel }) => {
  const row = await scrollScenario(panel, 0.4);
  expect(row.playing).toBeLessThanOrEqual(24);
  expect(row.swaps, 'a scroll must touch only what was playing').toBeLessThanOrEqual(48);
});

test(id('test_light_mode_animates_at_most_the_budget'), async ({ panel }) => {
  const page = await compact(panel);
  const visible = await page.evaluate('window.__visibleAnimated()');
  expect(visible, 'the scenario must have more on screen than the budget').toBeGreaterThan(24);
  expect(await page.evaluate('window.__animating()')).toBe(Math.min(24, visible));
});

test(id('test_the_budget_goes_to_the_cards_nearest_the_centre'), async ({ panel }) => {
  const page = await compact(panel);
  const [farPlaying, nearIdle] = await page.evaluate(`(() => {
    const cx = innerWidth / 2, cy = headerH + (innerHeight - headerH) / 2;
    const d = n => { const r = n.getBoundingClientRect(); const x = r.left + r.width / 2 - cx, y = r.top + r.height / 2 - cy; return x * x + y * y; };
    const vis = [...document.querySelectorAll('#grid img[data-anim]')].filter(n => { const r = n.getBoundingClientRect(); return r.bottom > headerH && r.top < innerHeight; });
    const on = vis.filter(n => n.getAttribute('src') === n.dataset.anim).map(d);
    const off = vis.filter(n => n.getAttribute('src') !== n.dataset.anim).map(d);
    return [Math.max(...on), Math.min(...off)];
  })()`);
  expect(farPlaying).toBeLessThanOrEqual(nearIdle + 1);
});

test(id('test_all_visible_brings_back_every_animation'), async ({ panel }) => {
  const page = await compact(panel);
  await page.click('#animAll');
  await settle(page);
  const visible = await page.evaluate('window.__visibleAnimated()');
  expect(await page.evaluate('window.__animating()')).toBe(visible);
  expect(visible).toBeGreaterThan(24);
  await page.click('#animAll');
  await settle(page);
  expect(await page.evaluate('window.__animating()')).toBeLessThanOrEqual(24);
});

test(id('test_a_drag_storm_writes_no_unchanged_css_variable'), async ({ panel }) => {
  const counter = `window.__varWrites = 0;
    const _set = CSSStyleDeclaration.prototype.setProperty;
    CSSStyleDeclaration.prototype.setProperty = function(name) {
      if (String(name).startsWith('--')) window.__varWrites++;
      return _set.apply(this, arguments);
    };`;
  const page = await panel.open(synth(300), { init: counter });
  await page.evaluate('window.__varWrites = 0');
  const ms = await page.evaluate('(() => { const t = performance.now(); for (let i = 0; i < 30; i++) window.__reorder(i, i + 5); return performance.now() - t; })()');
  report({ scenario: 'reorder-30', ms: round(ms), var_writes: await page.evaluate('window.__varWrites') });
  expect(await page.evaluate('window.__varWrites')).toBe(0);
});

test(id('test_zoomed_out_mounts_a_bounded_window_of_slim_cards'), async ({ panel }) => {
  const page = await panel.open(synth(1000), { init: PROBE });
  const count = `(() => { const cards = [...document.querySelectorAll('#grid .card')];
    const seen = cards.filter(c => { const r = c.getBoundingClientRect(); return r.bottom > headerH && r.top < innerHeight; });
    return [cards.length, seen.length, G.cols, document.querySelectorAll('#grid .card .lbl').length]; })()`;
  const full = await page.evaluate(count);
  await page.evaluate('setZoom(0.4)');
  await settle(page);
  const [mounted, seen, cols, labels] = await page.evaluate(count);
  report({ scenario: 'mounted', zoom1: full[0], 'zoom0.4': mounted, 'visible0.4': seen });
  expect(mounted, 'the buffer must be a few rows, not a screen').toBeLessThanOrEqual(seen + 6 * cols);
  expect(labels, 'a compact card carries no hidden text rows').toBe(0);
  await page.evaluate('setZoom(1)');
  await settle(page);
  expect((await page.evaluate(count))[3], 'normal cards keep their labels').toBeGreaterThan(0);
});

test(id('test_the_pick_ring_pauses_while_scrolling'), async ({ panel }) => {
  const page = await compact(panel);
  await page.click('#selmode');
  await page.click('#all');
  const state = "getComputedStyle(document.querySelector('#grid .card.picked'), '::before').animationPlayState";
  await page.waitForFunction(`${state} === 'running'`);
  await page.evaluate('window.__keepScrolling(1000)');
  await page.waitForFunction(`${state} === 'paused'`);
  await settle(page);
  await page.waitForFunction(`${state} === 'running'`);
});

test(id('test_the_checkerboard_keeps_its_pixels'), async ({ panel }) => {
  const page = await open(panel);
  const light = [130, 140, 154], dark = [70, 78, 90];
  for (let i = 0; i < 3; i++) {
    const im = PNG.sync.read(await page.locator('#grid .thumb').nth(i).screenshot());
    const got = [[4, 4], [12, 4], [4, 12], [12, 12]].map(([x, y]) => {
      const at = (y * im.width + x) * 4;
      return Array.from(im.data.subarray(at, at + 3));
    });
    for (const [j, want] of [light, dark, dark, light].entries()) {
      expect(got[j].every((c, k) => Math.abs(c - want[k]) <= 2), JSON.stringify({ i, got })).toBe(true);
    }
  }
});

test(id('test_bulk_hold_and_release_report_their_cost'), async ({ panel }) => {
  const packs = Array.from({ length: 1000 }, (_, i) => 1 + Math.floor(i / 200));
  const page = await panel.open(synth(1000, { packs }), { init: PROBE });
  const timed = id => { const t = performance.now(); document.getElementById(id).click(); return performance.now() - t; };
  const hold = await page.evaluate(timed, 'none');
  const held = await page.evaluate('ITEMS.filter(x => !x.isLogo && !x.included).length');
  const release = await page.evaluate(timed, 'unholdAll');
  report({ scenario: 'bulk-1000', deselect_all_ms: round(hold), unhold_all_ms: round(release) });
  expect(held).toBe(1000);
  expect(await page.evaluate('ITEMS.filter(x => !x.isLogo && !x.included).length')).toBe(0);
});

test(id('test_bulk_hold_matches_the_item_by_item_result'), async ({ panel }) => {
  const page = await panel.open(synth(4), { init: REFERENCE });
  const mismatches = await page.evaluate('window.__bulkEquivalence(150, 20260929)');
  expect(mismatches, 'the batch disagrees with the item-by-item path').toEqual([]);
});

test(id('test_undo_restores_the_all_visible_switch'), async ({ panel }) => {
  const page = await compact(panel);
  await page.click('#animAll');
  expect(await page.getAttribute('#animAll', 'aria-pressed')).toBe('true');
  await page.keyboard.press('Control+z');
  expect(await page.getAttribute('#animAll', 'aria-pressed')).toBe('false');

  const hover = page.locator('#animHover');
  await hover.click();
  await expect(hover).toHaveAttribute('aria-pressed', 'true');
  await page.keyboard.press('Control+z');
  await expect(hover).toHaveAttribute('aria-pressed', 'false');
  await page.keyboard.press('Control+y');
  await expect(hover).toHaveAttribute('aria-pressed', 'true');
  expect(await page.evaluate("localStorage.getItem('animHover')")).toBe('1');
  await page.click('#resetAll');
  await expect(hover).toHaveAttribute('aria-pressed', 'false');
  await expect(page.locator('#animAll')).toBeEnabled();
});

test(id('test_a_scroll_gesture_reports_its_cost_at_full_zoom'), async ({ panel }) => {
  await scrollScenario(panel, 1);
});
