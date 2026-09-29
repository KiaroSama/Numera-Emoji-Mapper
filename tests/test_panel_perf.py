"""What animating the grid costs, measured in a real browser.

The owner felt the panel go heavy while scrolling six packs, and the CPU jump
the moment Animation was switched on. The other browser suites serve one tiny
PNG for every preview, so nothing there ever decodes an animation. Here the
previews are real animated WebP, and each scenario prints one ``PANEL_PERF``
line: live animations after settling, ``src`` swaps per scroll gesture, long
animation frames and frame intervals. Timings are printed, never asserted (CI
runners are small and noisy); counts are stable, and later changes assert them.
"""
from __future__ import annotations

import io
import json
import unittest

from PIL import Image

from tests import _panel_browser_fixtures as fx

H = fx.Harness()


def setUpModule():
    H.start()


def tearDownModule():
    H.stop()


PROBE = """
window.__perf = {swaps: 0, loaf: []};
new MutationObserver(list => { window.__perf.swaps += list.length; })
  .observe(document, {subtree: true, attributes: true, attributeFilter: ['src']});
try {
  new PerformanceObserver(l => {
    for (const e of l.getEntries()) window.__perf.loaf.push(e.blockingDuration || 0);
  }).observe({type: 'long-animation-frame', buffered: true});
} catch (_) {}
window.__frames = (ms) => new Promise(done => {
  const out = []; let last = performance.now(); const end = last + ms;
  const step = (t) => { out.push(t - last); last = t;
                        if (t < end) requestAnimationFrame(step); else done(out); };
  requestAnimationFrame(step);
});
window.__settled = () => scrollThaw === null;
window.__visibleAnimated = () => [...document.querySelectorAll('#grid img[data-anim]')]
  .filter(n => { const r = n.getBoundingClientRect();
                 return r.bottom > headerH && r.top < innerHeight; }).length;
"""


REFERENCE = """
window.__refOriginFor = function(it){
  const index=ITEMS.indexOf(it);
  let pack=it.pack ?? null;
  if(pack===null){
    const starts=packStarts().starts;
    const run=starts.filter(s=>s.index<=index).pop();
    pack=run ? (run.pack ?? 'new:'+starts.indexOf(run)) : 'new:0';
  }
  const starts=packStarts().starts;
  const run=starts.filter(s=>s.index<=index).pop();
  const start=run?run.index:0;
  return {index,pack,slot:ITEMS.slice(start,index).filter(x=>x.included||x.isLogo).length,
          anchor:ITEMS[start]?.key};
};
window.__bulkEquivalence = function(cases, seed){
  let a = seed >>> 0;
  const rnd = () => { a = (a + 0x6D2B79F5) >>> 0; let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1); t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
  const bad = [];
  for (let c = 0; c < cases; c++) {
    const n = 20 + Math.floor(rnd() * 281), withPacks = rnd() < 0.67;
    const excluded = rnd() * 0.3, model = [];
    if (rnd() < 0.5) model.push({key: '__logo', isLogo: true, included: true, fmt: 'static'});
    for (let i = 0; i < n; i++) {
      const item = {key: 'k' + i, fmt: 'static', included: rnd() >= excluded};
      if (withPacks && rnd() < 0.8) item.pack = 1 + Math.floor(rnd() * 4);
      model.push(item);
    }
    const want = new Map(model.filter(x => !x.isLogo).map(x => [x.key, rnd() < 0.5]));
    const load = () => { ITEMS.length = 0; for (const x of model) ITEMS.push({...x}); };
    const dump = m => JSON.stringify([[...m.entries()].sort(), ITEMS.map(x => x.included)]);

    load(); const ref = new Map();
    for (const it of ITEMS) {
      const on = want.get(it.key);
      if (it.isLogo || it.included === on) continue;
      if (!on) ref.set(it.key, __refOriginFor(it)); else ref.delete(it.key);
      it.included = on;
    }
    const expected = dump(ref);
    load(); holdOrigins.clear();
    setIncludedMany(ITEMS.filter(x => !x.isLogo).map(x => [x, want.get(x.key)]));
    if (dump(holdOrigins) !== expected) bad.push(['sequential', c]);

    load();
    const picks = ITEMS.filter(x => !x.isLogo && x.included && rnd() < 0.3);
    const snap = JSON.stringify([...new Map(picks.map(it => [it.key, __refOriginFor(it)])).entries()].sort());
    if (JSON.stringify([...originsFor(picks).entries()].sort()) !== snap) bad.push(['together', c]);
  }
  return bad;
};
"""


def _pct(values, q):
    ordered = sorted(values)
    return round(ordered[min(len(ordered) - 1, int(q * len(ordered)))], 1)


class AnimationCost(unittest.TestCase):
    def open(self):
        self.errors = []
        page = H.open(fx.synth(600, fmt="animated"), init=PROBE, media="animated",
                      on_error=self.errors.append, cleanup=self.addCleanup)
        self.addCleanup(lambda: self.assertEqual(self.errors, []))
        page.set_default_timeout(15000)
        return page

    def settle(self, page):
        page.wait_for_function("window.__settled()")
        page.evaluate("new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))")

    def test_nothing_outside_the_viewport_animates_once_settled(self):
        page = self.open()
        for zoom in (1, 0.4):
            page.evaluate(f"setZoom({zoom})")
            self.settle(page)
            outside = page.evaluate("""[...document.querySelectorAll('#grid img[data-anim]')]
                .filter(n => n.getAttribute('src') === n.dataset.anim)
                .filter(n => { const r = n.getBoundingClientRect();
                               return !(r.bottom > headerH && r.top < innerHeight); }).length""")
            self.assertEqual(outside, 0, f"zoom {zoom}: off-screen cards still animate")

    def _scroll_scenario(self, zoom):
        page = self.open()
        page.evaluate(f"setZoom({zoom})")
        self.settle(page)
        page.evaluate("window.__perf.swaps = 0; window.__perf.loaf = []")
        page.evaluate("window.__keepScrolling(1500)")
        page.wait_for_timeout(1600)
        self.settle(page)
        frames = page.evaluate("window.__frames(1000)")
        perf = page.evaluate("window.__perf")
        row = {"scenario": f"scroll-{zoom:g}", "swaps": perf["swaps"],
               "playing": page.evaluate("window.__animating()"),
               "visible": page.evaluate("window.__visibleAnimated()"),
               "loaf_count": len(perf["loaf"]),
               "loaf_blocking_ms": round(sum(perf["loaf"]), 1),
               "frame_p50": _pct(frames, 0.5), "frame_p95": _pct(frames, 0.95)}
        print("PANEL_PERF " + json.dumps(row), flush=True)
        self.assertGreaterEqual(len(frames), 10, "the frame probe did not run")
        self.assertGreaterEqual(row["playing"], 1, "nothing animates after settling")
        return row

    def test_a_scroll_gesture_reports_its_cost_at_compact_zoom(self):
        row = self._scroll_scenario(0.4)
        # Owner decision 2026-09-28: the 24 nearest the centre, not every card.
        self.assertLessEqual(row["playing"], 24)
        self.assertLessEqual(row["swaps"], 48, "a scroll must touch only what was playing")

    def _compact(self):
        page = self.open()
        page.evaluate("setZoom(0.4)")
        self.settle(page)
        return page

    def test_light_mode_animates_at_most_the_budget(self):
        page = self._compact()
        visible = page.evaluate("window.__visibleAnimated()")
        self.assertGreater(visible, 24, "the scenario must have more on screen than the budget")
        self.assertEqual(page.evaluate("window.__animating()"), min(24, visible))

    def test_the_budget_goes_to_the_cards_nearest_the_centre(self):
        page = self._compact()
        far_playing, near_idle = page.evaluate("""(() => {
            const cx = innerWidth / 2, cy = headerH + (innerHeight - headerH) / 2;
            const d = n => { const r = n.getBoundingClientRect();
                             const x = r.left + r.width / 2 - cx, y = r.top + r.height / 2 - cy;
                             return x * x + y * y; };
            const vis = [...document.querySelectorAll('#grid img[data-anim]')].filter(n => {
                const r = n.getBoundingClientRect(); return r.bottom > headerH && r.top < innerHeight; });
            const on = vis.filter(n => n.getAttribute('src') === n.dataset.anim).map(d);
            const off = vis.filter(n => n.getAttribute('src') !== n.dataset.anim).map(d);
            return [Math.max(...on), Math.min(...off)];
        })()""")
        # Ties at the boundary are equally near, so only "no idle card is nearer".
        self.assertLessEqual(far_playing, near_idle + 1)

    def test_all_visible_brings_back_every_animation(self):
        page = self._compact()
        page.click("#animAll")
        self.settle(page)
        visible = page.evaluate("window.__visibleAnimated()")
        self.assertEqual(page.evaluate("window.__animating()"), visible)
        self.assertGreater(visible, 24)
        page.click("#animAll")
        self.settle(page)
        self.assertLessEqual(page.evaluate("window.__animating()"), 24)

    def test_a_drag_storm_writes_no_unchanged_css_variable(self):
        """A drag relayouts on every step; each --var write restyled every card."""
        counter = """
            window.__varWrites = 0;
            const _set = CSSStyleDeclaration.prototype.setProperty;
            CSSStyleDeclaration.prototype.setProperty = function(name) {
              if (String(name).startsWith('--')) window.__varWrites++;
              return _set.apply(this, arguments);
            };"""
        self.errors = []
        page = H.open(fx.synth(300), init=counter, on_error=self.errors.append,
                      cleanup=self.addCleanup)
        self.addCleanup(lambda: self.assertEqual(self.errors, []))
        page.evaluate("window.__varWrites = 0")
        ms = page.evaluate("""(() => { const t = performance.now();
            for (let i = 0; i < 30; i++) window.__reorder(i, i + 5);
            return performance.now() - t; })()""")
        print("PANEL_PERF " + json.dumps({"scenario": "reorder-30", "ms": round(ms, 1),
                                          "var_writes": page.evaluate("window.__varWrites")}),
              flush=True)
        self.assertEqual(page.evaluate("window.__varWrites"), 0)

    def test_zoomed_out_mounts_a_bounded_window_of_slim_cards(self):
        self.errors = []
        page = H.open(fx.synth(1000), init=PROBE, on_error=self.errors.append,
                      cleanup=self.addCleanup)
        self.addCleanup(lambda: self.assertEqual(self.errors, []))
        count = """(() => { const cards = [...document.querySelectorAll('#grid .card')];
            const seen = cards.filter(c => { const r = c.getBoundingClientRect();
                                             return r.bottom > headerH && r.top < innerHeight; });
            return [cards.length, seen.length, G.cols,
                    document.querySelectorAll('#grid .card .lbl').length]; })()"""
        full = page.evaluate(count)
        page.evaluate("setZoom(0.4)")
        self.settle(page)
        mounted, seen, cols, labels = page.evaluate(count)
        print("PANEL_PERF " + json.dumps({"scenario": "mounted", "zoom1": full[0],
                                          "zoom0.4": mounted, "visible0.4": seen}), flush=True)
        self.assertLessEqual(mounted, seen + 6 * cols, "the buffer must be a few rows, not a screen")
        self.assertEqual(labels, 0, "a compact card carries no hidden text rows")
        page.evaluate("setZoom(1)")
        self.settle(page)
        self.assertGreater(page.evaluate(count)[3], 0, "normal cards keep their labels")

    def test_the_pick_ring_pauses_while_scrolling(self):
        page = self._compact()
        page.click("#selmode")
        page.click("#all")
        state = ("getComputedStyle(document.querySelector('#grid .card.picked'), '::before')"
                 ".animationPlayState")
        # The zoom above scrolled, so the ring may still be paused for up to one
        # settle delay: wait for the condition rather than reading it once.
        page.wait_for_function(f"{state} === 'running'")
        page.evaluate("window.__keepScrolling(1000)")
        page.wait_for_function(f"{state} === 'paused'")
        self.settle(page)
        page.wait_for_function(f"{state} === 'running'")

    def test_the_checkerboard_keeps_its_pixels(self):
        """One gradient layer replaced four; the pattern must not change."""
        page = self.open()
        light, dark = (130, 140, 154), (70, 78, 90)
        for i in range(3):
            with Image.open(io.BytesIO(page.locator("#grid .thumb").nth(i).screenshot())) as im:
                rgb = im.convert("RGB")
                got = [rgb.getpixel(pt) for pt in ((4, 4), (12, 4), (4, 12), (12, 12))]
            for colour, want in zip(got, (light, dark, dark, light), strict=True):
                self.assertTrue(all(abs(c - w) <= 2 for c, w in zip(colour, want, strict=True)),
                                (i, got))

    def test_bulk_hold_and_release_report_their_cost(self):
        self.errors = []
        packs = [1 + i // 200 for i in range(1000)]
        page = H.open(fx.synth(1000, packs=packs), init=PROBE, on_error=self.errors.append,
                      cleanup=self.addCleanup)
        self.addCleanup(lambda: self.assertEqual(self.errors, []))
        timed = """(id) => { const t = performance.now();
                              document.getElementById(id).click();
                              return performance.now() - t; }"""
        hold = page.evaluate(timed, "none")
        held = page.evaluate("ITEMS.filter(x => !x.isLogo && !x.included).length")
        release = page.evaluate(timed, "unholdAll")
        print("PANEL_PERF " + json.dumps({"scenario": "bulk-1000", "deselect_all_ms": round(hold, 1),
                                          "unhold_all_ms": round(release, 1)}), flush=True)
        self.assertEqual(held, 1000)
        self.assertEqual(page.evaluate("ITEMS.filter(x => !x.isLogo && !x.included).length"), 0)

    def test_bulk_hold_matches_the_item_by_item_result(self):
        """BulkHoldEquivalence: 150 random models, the batch against the loop.

        REFERENCE is a verbatim copy of originFor()/setIncluded() as they were
        before the batch existed, so it survives any later rewrite of either.
        """
        self.errors = []
        page = H.open(fx.synth(4), init=REFERENCE, on_error=self.errors.append,
                      cleanup=self.addCleanup)
        self.addCleanup(lambda: self.assertEqual(self.errors, []))
        mismatches = page.evaluate("window.__bulkEquivalence(150, 20260929)")
        self.assertEqual(mismatches, [], "the batch disagrees with the item-by-item path")

    def test_undo_restores_the_all_visible_switch(self):
        page = self._compact()
        page.click("#animAll")
        self.assertEqual(page.get_attribute("#animAll", "aria-pressed"), "true")
        page.keyboard.press("Control+z")
        self.assertEqual(page.get_attribute("#animAll", "aria-pressed"), "false")

    def test_a_scroll_gesture_reports_its_cost_at_full_zoom(self):
        self._scroll_scenario(1)


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_panel_perf -v")
