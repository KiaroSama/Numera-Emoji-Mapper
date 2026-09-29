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

import json
import unittest

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
