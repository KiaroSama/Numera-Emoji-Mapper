
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
